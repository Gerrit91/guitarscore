//! GuitarScore's test plugin: a CLAP library the tests open as they would a user's plugin.
//! A gain effect with one parameter ("Gain", 0 to 2), settings (the gain) and a fixed
//! latency, so hosting can be tested without any plugin installed.
//!
//! UNSAFE EXCEPTION (owner's approval, 2026-11-26, design note `docs/plugin-hosting.md`):
//! a CLAP library must export its entry symbol `clap_entry` unmangled, which Rust counts as
//! unsafe code. The symbol comes from clack's `clack_export_entry!`; nothing else here is
//! unsafe. Built by `cargo test` as a cdylib example, never shipped.
#![allow(unsafe_code)]

use std::ffi::CStr;
use std::fmt::Write as _;
use std::io::{Read as _, Write as _};
use std::sync::atomic::{AtomicU64, Ordering};

use clack_extensions::latency::{PluginLatency, PluginLatencyImpl};
use clack_extensions::params::{
    ParamDisplayWriter, ParamInfo, ParamInfoFlags, ParamInfoWriter, PluginAudioProcessorParams,
    PluginMainThreadParams, PluginParams,
};
use clack_extensions::state::{PluginState, PluginStateImpl};
use clack_plugin::events::event_types::ParamValueEvent;
use clack_plugin::prelude::*;
use clack_plugin::stream::{InputStream, OutputStream};

/// The plugin's id, as the tests look for it.
const ID: &str = "org.guitarscore.test.gain";
/// The gain parameter's id.
const GAIN: u32 = 1;
/// The latency it reports, in frames.
const LATENCY: u32 = 64;

struct TestGain;

impl Plugin for TestGain {
    type AudioProcessor<'a> = Processor<'a>;
    type Shared<'a> = Shared;
    type MainThread<'a> = MainThread<'a>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&Shared>) {
        builder
            .register::<PluginParams>()
            .register::<PluginState>()
            .register::<PluginLatency>();
    }
}

impl DefaultPluginFactory for TestGain {
    fn get_descriptor() -> PluginDescriptor {
        use clack_plugin::plugin::features::{AUDIO_EFFECT, STEREO};
        PluginDescriptor::new(ID, "GuitarScore Test Gain")
            .with_vendor("GuitarScore")
            .with_version("1.0.0")
            .with_features([AUDIO_EFFECT, STEREO])
    }

    fn new_shared(_host: HostSharedHandle<'_>) -> Result<Shared, PluginError> {
        Ok(Shared {
            gain: AtomicU64::new(1.0_f64.to_bits()),
        })
    }

    fn new_main_thread<'a>(
        _host: HostMainThreadHandle<'a>,
        shared: &'a Shared,
    ) -> Result<MainThread<'a>, PluginError> {
        Ok(MainThread { shared })
    }
}

struct Shared {
    gain: AtomicU64,
}

impl Shared {
    fn gain(&self) -> f64 {
        f64::from_bits(self.gain.load(Ordering::Relaxed))
    }

    fn set_gain(&self, value: f64) {
        self.gain
            .store(value.clamp(0.0, 2.0).to_bits(), Ordering::Relaxed);
    }

    fn apply(&self, events: &InputEvents) {
        for event in events {
            if let Some(change) = event.as_event::<ParamValueEvent>()
                && change.param_id().map(|id| id.get()) == Some(GAIN)
            {
                self.set_gain(change.value());
            }
        }
    }
}

impl PluginShared<'_> for Shared {}

struct MainThread<'a> {
    shared: &'a Shared,
}

impl<'a> PluginMainThread<'a, Shared> for MainThread<'a> {}

impl PluginMainThreadParams for MainThread<'_> {
    fn count(&self) -> u32 {
        1
    }

    fn get_info(&self, index: u32, info: &mut ParamInfoWriter) {
        if index == 0 {
            info.set(&ParamInfo {
                id: ClapId::new(GAIN),
                flags: ParamInfoFlags::IS_AUTOMATABLE,
                cookie: Default::default(),
                name: b"Gain",
                module: b"Output",
                min_value: 0.0,
                max_value: 2.0,
                default_value: 1.0,
            });
        }
    }

    fn get_value(&self, id: ClapId) -> Option<f64> {
        (id.get() == GAIN).then(|| self.shared.gain())
    }

    fn value_to_text(
        &self,
        _id: ClapId,
        value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> std::fmt::Result {
        write!(writer, "{value:.2}x")
    }

    fn text_to_value(&self, _id: ClapId, text: &CStr) -> Option<f64> {
        text.to_str().ok()?.trim_end_matches('x').parse().ok()
    }

    fn flush(&self, input: &InputEvents, _output: &mut OutputEvents) {
        self.shared.apply(input);
    }
}

impl PluginStateImpl for MainThread<'_> {
    fn save(&self, output: &mut OutputStream) -> Result<(), PluginError> {
        output.write_all(&self.shared.gain().to_le_bytes())?;
        Ok(())
    }

    fn load(&self, input: &mut InputStream) -> Result<(), PluginError> {
        let mut bytes = [0; 8];
        input.read_exact(&mut bytes)?;
        self.shared.set_gain(f64::from_le_bytes(bytes));
        Ok(())
    }
}

impl PluginLatencyImpl for MainThread<'_> {
    fn get(&self) -> u32 {
        LATENCY
    }
}

struct Processor<'a> {
    shared: &'a Shared,
}

impl<'a> PluginAudioProcessor<'a, Shared, MainThread<'a>> for Processor<'a> {
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        _main_thread: &MainThread<'a>,
        shared: &'a Shared,
        _config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        Ok(Self { shared })
    }

    fn process(
        &mut self,
        _process: Process,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        self.shared.apply(events.input);
        let gain = self.shared.gain() as f32;
        for mut pair in &mut audio {
            let Some(channels) = pair.channels()?.into_f32() else {
                continue;
            };
            for channel in channels {
                match channel {
                    ChannelPair::InputOnly(_) => {}
                    ChannelPair::OutputOnly(out) => out.fill(0.0),
                    ChannelPair::InputOutput(input, out) => {
                        for (o, i) in out.iter_mut().zip(input) {
                            *o = i * gain;
                        }
                    }
                    ChannelPair::InPlace(buffer) => {
                        for s in buffer {
                            *s *= gain;
                        }
                    }
                }
            }
        }
        Ok(ProcessStatus::Continue)
    }
}

impl PluginAudioProcessorParams for Processor<'_> {
    fn flush(&mut self, input: &InputEvents, _output: &mut OutputEvents) {
        self.shared.apply(input);
    }
}

clack_export_entry!(SinglePluginEntry<TestGain>);
