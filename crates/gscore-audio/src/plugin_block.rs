//! A plugin as a block of a sound chain (M24, native builds only): [`PluginSource`] is the
//! chain's [`ExternalSource`], starting one effect on the plugin thread for each chain it
//! is placed in; [`PluginControl`] is what the app keeps to change the plugin's
//! parameters while it plays and to hear back how it is doing.

use std::hash::Hasher as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use gscore_guitar::{ExternalBlock, ExternalSource};
use gscore_plugin::{AudioConfig, PluginHandle, Processor};

/// How long making a chain waits for a plugin to start.
const START_WAIT: Duration = Duration::from_secs(20);
/// The longest block a chain hands a plugin (the engine's are far shorter; export's too).
const MAX_FRAMES: u32 = 4096;

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
    /// The plugin failed while playing and is bypassed.
    failed: AtomicBool,
    /// Frames its sound comes out late.
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

    /// Why the plugin could not be started, or that it failed while playing.
    pub fn problem(&self) -> Option<String> {
        if self.failed.load(Ordering::Acquire) {
            return Some("the plugin failed while playing and is bypassed".into());
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
    handle: PluginHandle,
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
    pub fn new(
        handle: PluginHandle,
        library: &Path,
        id: &str,
        state: Vec<u8>,
        control: Arc<PluginControl>,
    ) -> Self {
        // The same plugin with the same settings is the same block: no new chain for it.
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        hash.write(&state);
        let key = format!("{}#{id}#{:016x}", library.display(), hash.finish());
        Self {
            key,
            handle,
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
            max_frames: MAX_FRAMES,
        };
        let answer = self
            .handle
            .start(&self.library, &self.id, &self.state, config)
            .recv_timeout(START_WAIT);
        let error = match answer {
            Ok(Ok(started)) => {
                self.control
                    .latency
                    .store(started.latency, Ordering::Release);
                *self
                    .control
                    .error
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = None;
                return Some(Box::new(PluginBlock {
                    processor: Some(started.processor),
                    instance: started.instance,
                    handle: self.handle.clone(),
                    control: Arc::clone(&self.control),
                    seen: u64::MAX,
                }));
            }
            Ok(Err(error)) => error.to_string(),
            Err(_) => format!("the plugin \"{}\" did not start in time", self.id),
        };
        *self
            .control
            .error
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(error);
        None
    }
}

/// One running plugin in one chain (on the audio thread).
struct PluginBlock {
    /// Taken when the block is dropped, to go back to the plugin thread.
    processor: Option<Processor>,
    instance: u64,
    handle: PluginHandle,
    control: Arc<PluginControl>,
    /// The parameter version played with now.
    seen: u64,
}

impl ExternalBlock for PluginBlock {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let Some(processor) = &mut self.processor else {
            return;
        };
        let version = self.control.version.load(Ordering::Acquire);
        // Never wait for the lock here: a change missed now is taken on the next block.
        if version != self.seen
            && let Ok(values) = self.control.values.try_lock()
        {
            for &(id, value) in values.iter() {
                processor.set_param(id, value);
            }
            self.seen = version;
        }
        if processor.process(left, right).is_err() {
            self.control.failed.store(true, Ordering::Release);
        }
    }

    fn reset(&mut self) {
        if let Some(processor) = &mut self.processor {
            processor.reset();
        }
    }
}

impl Drop for PluginBlock {
    fn drop(&mut self) {
        // Chains are freed off the audio thread: the plugin thread deactivates it.
        if let Some(processor) = self.processor.take() {
            self.handle.stop(self.instance, processor);
        }
    }
}
