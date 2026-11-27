//! A plugin used as an effect: its main-thread side ([`Effect`]: parameters, settings,
//! activation) and its audio side ([`Processor`]: stereo sound in, stereo sound out).
//!
//! CLAP's thread rules: an [`Effect`] stays on the thread that made it (it is not `Send`);
//! the [`Processor`] goes to the audio thread and comes back to [`Effect::deactivate`].

use std::fmt;
use std::sync::atomic::Ordering;

use clack_extensions::audio_ports::AudioPortInfoBuffer;
use clack_extensions::params::{ParamInfoBuffer, ParamInfoFlags};
use clack_host::events::event_types::ParamValueEvent;
use clack_host::prelude::*;

use crate::host::{GsHost, MainThread, Shared};
use crate::{HOST_NAME, Library, PluginError};

/// A plugin's ports are capped at this many (a hostile plugin must not make us allocate
/// without bounds)...
const MAX_PORTS: u32 = 16;
/// ...each with at most this many channels.
const MAX_CHANNELS: u32 = 32;
/// Parameter changes queued between two blocks; more are dropped (the latest wins on the
/// next change).
const MAX_PENDING: usize = 256;
/// The longest settings blob read back from a plugin (64 MiB).
const MAX_STATE: usize = 64 << 20;

/// The sound a plugin is activated for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioConfig {
    pub sample_rate: u32,
    /// The longest block [`Processor::process`] hands the plugin at once; longer buffers
    /// are split.
    pub max_frames: u32,
}

/// One of a plugin's parameters, as its own description says.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub id: u32,
    pub name: String,
    /// The group the plugin puts it in ("Filter/Low"), empty for none.
    pub module: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// Whole steps only (a switch, a choice).
    pub stepped: bool,
    /// Not for users (the plugin's own bookkeeping).
    pub hidden: bool,
    pub read_only: bool,
}

/// The channels of each audio port, main port first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Ports {
    inputs: Vec<u32>,
    outputs: Vec<u32>,
}

/// A plugin made from a library, used as an effect. Drop it on the thread that made it.
pub struct Effect {
    id: String,
    pub(crate) instance: PluginInstance<GsHost>,
    ports: Ports,
    /// Its editor is open (see `editor.rs`).
    pub(crate) editor_open: bool,
}

impl fmt::Debug for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Effect")
            .field("id", &self.id)
            .field("ports", &self.ports)
            .finish_non_exhaustive()
    }
}

impl Effect {
    /// Makes the plugin `id` of `library`. Fails for a plugin without a stereo or mono
    /// input and output (an instrument, an analyzer).
    pub fn new(library: &Library, id: &str) -> Result<Self, PluginError> {
        let start = |reason: String| PluginError::Start {
            id: id.to_owned(),
            reason,
        };
        let c_id = std::ffi::CString::new(id).map_err(|_| PluginError::NotFound(id.to_owned()))?;
        if !library.plugins().iter().any(|p| p.id == id) {
            return Err(PluginError::NotFound(id.to_owned()));
        }
        let info = HostInfo::new(
            HOST_NAME,
            HOST_NAME,
            "https://github.com/Gerrit91/guitarscore",
            gscore_version_text(),
        )
        .map_err(|e| start(e.to_string()))?;
        let mut instance = PluginInstance::<GsHost>::new(
            |_| Shared::default(),
            |shared| MainThread::new(shared),
            library.entry(),
            &c_id,
            &info,
        )
        .map_err(|e| start(e.to_string()))?;
        let ports = ports(&mut instance);
        let usable = |channels: &[u32]| channels.first().is_some_and(|&c| c > 0);
        if !usable(&ports.inputs) || !usable(&ports.outputs) {
            return Err(PluginError::Unsupported {
                id: id.to_owned(),
                reason: "it has no audio input and output to put in a sound chain".into(),
            });
        }
        Ok(Self {
            id: id.to_owned(),
            instance,
            ports,
            editor_open: false,
        })
    }

    /// The plugin's id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Starts the plugin for `config`; its sound runs through the [`Processor`] returned.
    /// One processor at a time: give it back to [`Self::deactivate`] before activating
    /// again (a new sample rate, a restart the plugin asked for).
    pub fn activate(&mut self, config: AudioConfig) -> Result<Processor, PluginError> {
        let max_frames = config.max_frames.clamp(1, 1 << 16);
        let configuration = PluginAudioConfiguration {
            sample_rate: f64::from(config.sample_rate),
            min_frames_count: 1,
            max_frames_count: max_frames,
        };
        let stopped = self
            .instance
            .activate(|_, _| (), configuration)
            .map_err(|e| PluginError::Start {
                id: self.id.clone(),
                reason: e.to_string(),
            })?;
        let frames = max_frames as usize;
        let buffers = |channels: &[u32]| -> Vec<Vec<Vec<f32>>> {
            channels
                .iter()
                .map(|&c| (0..c).map(|_| vec![0.0; frames]).collect())
                .collect()
        };
        let total = |channels: &[u32]| channels.iter().map(|&c| c as usize).sum::<usize>();
        self.instance.access_shared_handler(|h| {
            h.restart.store(false, Ordering::Release);
        });
        Ok(Processor {
            audio: stopped.into(),
            inputs: buffers(&self.ports.inputs),
            outputs: buffers(&self.ports.outputs),
            input_ports: AudioPorts::with_capacity(
                total(&self.ports.inputs),
                self.ports.inputs.len(),
            ),
            output_ports: AudioPorts::with_capacity(
                total(&self.ports.outputs),
                self.ports.outputs.len(),
            ),
            events: EventBuffer::with_capacity(MAX_PENDING),
            output_events: EventBuffer::with_capacity(MAX_PENDING),
            pending: Vec::with_capacity(MAX_PENDING),
            max_frames: frames,
            steady_time: 0,
            failed: false,
        })
    }

    /// Stops the plugin: `processor` must be the one [`Self::activate`] returned.
    pub fn deactivate(&mut self, processor: Processor) {
        let stopped = processor.audio.into_stopped();
        if stopped.matches(&self.instance) {
            self.instance.deactivate(stopped);
        }
    }

    /// The plugin's parameters.
    pub fn params(&mut self) -> Vec<Param> {
        let Some(params) = self
            .instance
            .access_shared_handler(|h| h.params.get().copied().flatten())
        else {
            return Vec::new();
        };
        let handle = self.instance.plugin_handle();
        let mut buffer = ParamInfoBuffer::new();
        let text = |bytes: &[u8]| {
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            String::from_utf8_lossy(bytes.get(..end).unwrap_or_default()).into_owned()
        };
        (0..params.count(&handle))
            .filter_map(|index| {
                let info = params.get_info(&handle, index, &mut buffer)?;
                Some(Param {
                    id: info.id.get(),
                    name: text(info.name),
                    module: text(info.module),
                    min: info.min_value,
                    max: info.max_value,
                    default: info.default_value,
                    stepped: info.flags.contains(ParamInfoFlags::IS_STEPPED),
                    hidden: info.flags.contains(ParamInfoFlags::IS_HIDDEN),
                    read_only: info.flags.contains(ParamInfoFlags::IS_READONLY),
                })
            })
            .collect()
    }

    /// A parameter's current value.
    pub fn value(&mut self, id: u32) -> Option<f64> {
        let params = self
            .instance
            .access_shared_handler(|h| h.params.get().copied().flatten())?;
        params.get_value(&self.instance.plugin_handle(), ClapId::new(id))
    }

    /// The plugin's own text for `value` of parameter `id` ("-6.0 dB").
    pub fn value_text(&mut self, id: u32, value: f64) -> Option<String> {
        let params = self
            .instance
            .access_shared_handler(|h| h.params.get().copied().flatten())?;
        let mut buffer = [0_u8; 128];
        let text = params
            .value_to_text(
                &self.instance.plugin_handle(),
                ClapId::new(id),
                value,
                &mut buffer,
            )
            .ok()?;
        Some(String::from_utf8_lossy(text).into_owned())
    }

    /// The plugin's settings, as its own opaque blob (kept with the tone or the song).
    pub fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        let fail = |reason: String| PluginError::State {
            action: "saved",
            reason,
        };
        let state = self
            .instance
            .access_shared_handler(|h| h.state.get().copied().flatten())
            .ok_or_else(|| fail("the plugin cannot save its settings".into()))?;
        let mut blob = Capped(Vec::new());
        state
            .save(&self.instance.plugin_handle(), &mut blob)
            .map_err(|e| fail(e.to_string()))?;
        self.instance
            .access_shared_handler(|h| h.dirty.store(false, Ordering::Release));
        Ok(blob.0)
    }

    /// Gives the plugin back settings that [`Self::save_state`] made.
    pub fn load_state(&mut self, blob: &[u8]) -> Result<(), PluginError> {
        let fail = |reason: String| PluginError::State {
            action: "loaded",
            reason,
        };
        let state = self
            .instance
            .access_shared_handler(|h| h.state.get().copied().flatten())
            .ok_or_else(|| fail("the plugin cannot load settings".into()))?;
        state
            .load(
                &self.instance.plugin_handle(),
                &mut std::io::Cursor::new(blob),
            )
            .map_err(|e| fail(e.to_string()))
    }

    /// How many frames late the plugin's sound comes out (to be made up for).
    pub fn latency(&mut self) -> u32 {
        self.instance.access_shared_handler(|h| {
            h.latency_changed.store(false, Ordering::Release);
        });
        let Some(latency) = self
            .instance
            .access_shared_handler(|h| h.latency.get().copied().flatten())
        else {
            return 0;
        };
        latency.get(&self.instance.plugin_handle())
    }

    /// Handles what the plugin asked for since the last call (its main-thread callback);
    /// call it regularly on the effect's thread. Returns what needs the owner's action.
    pub fn idle(&mut self) -> Requests {
        let callback = self
            .instance
            .access_shared_handler(|h| h.callback.swap(false, Ordering::AcqRel));
        if callback {
            self.instance.call_on_main_thread_callback();
        }
        self.instance.access_shared_handler(|h| Requests {
            restart: h.restart.load(Ordering::Acquire),
            changed: h.dirty.load(Ordering::Acquire),
            params_changed: h.rescan.swap(false, Ordering::AcqRel),
            latency_changed: h.latency_changed.load(Ordering::Acquire),
            log: h.take_log(),
        })
    }
}

/// What a plugin asked its owner for (see [`Effect::idle`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Requests {
    /// Deactivate and activate it again.
    pub restart: bool,
    /// Its settings changed since they were last saved.
    pub changed: bool,
    /// Its parameters' list or ranges changed: read [`Effect::params`] again.
    pub params_changed: bool,
    /// Its latency changed: read [`Effect::latency`] again.
    pub latency_changed: bool,
    /// Lines it logged.
    pub log: Vec<String>,
}

/// The audio side of an active [`Effect`]. Every buffer it needs is made when the effect
/// is activated: processing allocates nothing (unless the plugin itself does).
pub struct Processor {
    audio: PluginAudioProcessor<GsHost>,
    /// Per port, per channel, `max_frames` samples.
    inputs: Vec<Vec<Vec<f32>>>,
    outputs: Vec<Vec<Vec<f32>>>,
    input_ports: AudioPorts,
    output_ports: AudioPorts,
    events: EventBuffer,
    output_events: EventBuffer,
    pending: Vec<(u32, f64)>,
    max_frames: usize,
    steady_time: u64,
    failed: bool,
}

impl fmt::Debug for Processor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Processor")
            .field("max_frames", &self.max_frames)
            .field("failed", &self.failed)
            .finish_non_exhaustive()
    }
}

impl Processor {
    /// Sets parameter `id` to `value` from the next block on.
    pub fn set_param(&mut self, id: u32, value: f64) {
        if !value.is_finite() {
            return;
        }
        if let Some(slot) = self.pending.iter_mut().find(|(pending, _)| *pending == id) {
            slot.1 = value;
        } else if self.pending.len() < MAX_PENDING {
            self.pending.push((id, value));
        }
    }

    /// Forgets the sound so far (a stop, a jump): tails and delay lines are cleared.
    pub fn reset(&mut self) {
        if let Ok(started) = self.audio.as_started_mut() {
            started.reset();
        }
    }

    /// Whether the plugin failed while processing: it is bypassed from then on.
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// Runs `left` and `right` through the plugin, in place. Equally long slices; a mono
    /// plugin gets their mix and its sound goes to both. After a failure the sound passes
    /// unchanged.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) -> Result<(), PluginError> {
        if self.failed {
            return Ok(());
        }
        let frames = left.len().min(right.len());
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(self.max_frames);
            let end = done + n;
            let (Some(l), Some(r)) = (left.get_mut(done..end), right.get_mut(done..end)) else {
                break;
            };
            if let Err(error) = self.block(l, r) {
                self.failed = true;
                return Err(error);
            }
            done = end;
        }
        Ok(())
    }

    fn block(&mut self, left: &mut [f32], right: &mut [f32]) -> Result<(), PluginError> {
        let n = left.len();
        // The main input gets the sound (a mono one both sides' mix); further channels
        // and side-chain ports get silence.
        for (port, channels) in self.inputs.iter_mut().enumerate() {
            let mono = channels.len() == 1;
            for (index, channel) in channels.iter_mut().enumerate() {
                let samples = channel.get_mut(..n).unwrap_or_default();
                match (port, index) {
                    (0, 0) if mono => {
                        for ((m, l), r) in samples.iter_mut().zip(left.iter()).zip(right.iter()) {
                            *m = (l + r) * 0.5;
                        }
                    }
                    (0, 0) => samples.copy_from_slice(left),
                    (0, 1) => samples.copy_from_slice(right),
                    _ => samples.fill(0.0),
                }
            }
        }
        self.events.clear();
        for (id, value) in self.pending.drain(..) {
            self.events.push(&ParamValueEvent::new(
                0,
                ClapId::new(id),
                Pckn::match_all(),
                value,
            ));
        }
        self.output_events.clear();
        let input = self
            .input_ports
            .with_input_buffers(self.inputs.iter_mut().map(|port| AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_input_only(port.iter_mut().map(|channel| {
                    InputChannel::variable(channel.get_mut(..n).unwrap_or_default())
                })),
            }));
        let mut output = self
            .output_ports
            .with_output_buffers(self.outputs.iter_mut().map(|port| {
                AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::f32_output_only(
                        port.iter_mut()
                            .map(|channel| channel.get_mut(..n).unwrap_or_default()),
                    ),
                }
            }));
        let started = self
            .audio
            .ensure_processing_started()
            .map_err(|e| PluginError::Process(e.to_string()))?;
        started
            .process(
                &input,
                &mut output,
                &InputEvents::from_buffer(&self.events),
                &mut OutputEvents::from_buffer(&mut self.output_events),
                Some(self.steady_time),
                None,
            )
            .map_err(|e| PluginError::Process(e.to_string()))?;
        self.steady_time = self.steady_time.saturating_add(n as u64);
        let main = self.outputs.first();
        let first = main.and_then(|port| port.first());
        let second = main.and_then(|port| port.get(1)).or(first);
        if let (Some(a), Some(b)) = (first, second) {
            left.copy_from_slice(a.get(..n).unwrap_or_default());
            right.copy_from_slice(b.get(..n).unwrap_or_default());
        }
        Ok(())
    }
}

/// The ports of a plugin (one stereo port each way if it does not say).
fn ports(instance: &mut PluginInstance<GsHost>) -> Ports {
    let Some(audio_ports) =
        instance.access_shared_handler(|h| h.audio_ports.get().copied().flatten())
    else {
        return Ports {
            inputs: vec![2],
            outputs: vec![2],
        };
    };
    let handle = instance.plugin_handle();
    let mut buffer = AudioPortInfoBuffer::new();
    let mut side = |is_input: bool| -> Vec<u32> {
        let count = audio_ports.count(&handle, is_input).min(MAX_PORTS);
        (0..count)
            .map(|index| {
                audio_ports
                    .get(&handle, index, is_input, &mut buffer)
                    .map_or(0, |info| info.channel_count.min(MAX_CHANNELS))
            })
            .collect()
    };
    Ports {
        inputs: side(true),
        outputs: side(false),
    }
}

/// The version text plugins see.
fn gscore_version_text() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// A settings blob being written, refused past [`MAX_STATE`].
struct Capped(Vec<u8>);

impl std::io::Write for Capped {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(data.len()) > MAX_STATE {
            return Err(std::io::Error::other("the settings are too large"));
        }
        self.0.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
