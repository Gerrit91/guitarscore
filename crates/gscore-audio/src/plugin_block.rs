//! A plugin as a block of a sound chain (M24, native builds only): [`PluginSource`] is the
//! chain's [`ExternalSource`], starting one plugin host process for each chain it is placed
//! in ([`gscore_plugin::remote`]); [`PluginControl`] is what the app keeps to change the
//! plugin's parameters while it plays and to hear back how it is doing.
//!
//! A plugin that crashes or hangs takes only its host process down: its place then passes
//! the sound unchanged, and the control says why. Making the chain again restarts it.

use std::hash::Hasher as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use gscore_guitar::{ExternalBlock, ExternalSource};
use gscore_plugin::AudioConfig;
use gscore_plugin::remote::{RemoteEffect, Status};

/// The app's side of a plugin block: parameter values to play with, and what the plugin
/// reported. Shared by every chain the block is placed in.
#[derive(Debug, Default)]
pub struct PluginControl {
    /// The parameters set away from the plugin's saved settings: (id, value).
    values: Mutex<Vec<(u32, f64)>>,
    /// Bumped on every change of `values`, so the audio thread looks only then.
    version: AtomicU64,
    /// Why the plugin could not be started (the place then plays the sound unchanged).
    error: Mutex<Option<String>>,
    /// The hosts running it, for why one stopped.
    hosts: Mutex<Vec<Status>>,
    /// Frames its sound comes out late (its own latency and its host's).
    latency: AtomicU32,
}

impl PluginControl {
    /// A control with these parameter values.
    pub fn new(values: Vec<(u32, f64)>) -> Self {
        Self {
            values: Mutex::new(values),
            ..Self::default()
        }
    }

    /// Plays with `values` from the next block on (every instance of the block).
    pub fn set_values(&self, values: &[(u32, f64)]) {
        let mut held = self.values.lock().unwrap_or_else(PoisonError::into_inner);
        if held.as_slice() != values {
            held.clear();
            held.extend_from_slice(values);
            self.version.fetch_add(1, Ordering::Release);
        }
    }

    /// Why the plugin does not play: it could not start, or it crashed or hung.
    pub fn problem(&self) -> Option<String> {
        if let Some(failure) = self
            .hosts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find_map(Status::failure)
        {
            return Some(failure);
        }
        self.error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Frames its sound comes out late (0 until it has started).
    pub fn latency(&self) -> u32 {
        self.latency.load(Ordering::Acquire)
    }
}

/// A plugin and its saved settings, as a chain block.
pub struct PluginSource {
    key: String,
    library: PathBuf,
    id: String,
    state: Vec<u8>,
    control: Arc<PluginControl>,
}

impl std::fmt::Debug for PluginSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginSource")
            .field("key", &self.key)
            .finish()
    }
}

impl PluginSource {
    /// Plugin `id` of the library at `library`, started with the settings `state` (empty:
    /// its defaults), playing with the parameter values `control` holds.
    pub fn new(library: &Path, id: &str, state: Vec<u8>, control: Arc<PluginControl>) -> Self {
        // The same plugin with the same settings is the same block: no new chain for it.
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        hash.write(&state);
        let key = format!("{}#{id}#{:016x}", library.display(), hash.finish());
        Self {
            key,
            library: library.to_owned(),
            id: id.to_owned(),
            state,
            control,
        }
    }
}

impl ExternalSource for PluginSource {
    fn key(&self) -> &str {
        &self.key
    }

    fn instance(&self, sample_rate: u32) -> Option<Box<dyn ExternalBlock>> {
        let config = AudioConfig {
            sample_rate,
            max_frames: gscore_plugin::remote::BLOCK as u32,
        };
        // Made off the audio thread: starting a host may take a while.
        match RemoteEffect::start(&self.library, &self.id, &self.state, config) {
            Ok(remote) => {
                self.control
                    .latency
                    .store(remote.latency(), Ordering::Release);
                *self
                    .control
                    .error
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = None;
                let mut hosts = self
                    .control
                    .hosts
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                // Hosts of chains made before (and dropped since) are forgotten.
                hosts.retain(|status| status.failure().is_none());
                hosts.push(remote.status());
                drop(hosts);
                Some(Box::new(PluginBlock {
                    remote,
                    control: Arc::clone(&self.control),
                    seen: u64::MAX,
                }))
            }
            Err(error) => {
                *self
                    .control
                    .error
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(error.to_string());
                None
            }
        }
    }
}

/// One running plugin in one chain (on the audio thread).
struct PluginBlock {
    remote: RemoteEffect,
    control: Arc<PluginControl>,
    /// The parameter version played with now.
    seen: u64,
}

impl ExternalBlock for PluginBlock {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let version = self.control.version.load(Ordering::Acquire);
        // Never wait for the lock here: a change missed now is taken on the next block.
        if version != self.seen
            && let Ok(values) = self.control.values.try_lock()
        {
            for &(id, value) in values.iter() {
                self.remote.set_param(id, value);
            }
            self.seen = version;
        }
        self.remote.process(left, right);
    }

    fn reset(&mut self) {
        self.remote.reset();
    }

    fn set_offline(&mut self, offline: bool) {
        self.remote.set_offline(offline);
    }

    fn latency(&self) -> u32 {
        // A plugin that stopped is bypassed: its sound is no longer late.
        if self.remote.failed() {
            0
        } else {
            self.remote.latency()
        }
    }
}
