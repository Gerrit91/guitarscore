//! Audio and MIDI output backends implementing `gscore_play::Player`. Only this crate may
//! use device crates: `midir` for MIDI output, `cpal` and `rustysynth` for the internal
//! synthesizer.
//!
//! Every backend runs on its own thread; the UI talks to it through a bounded channel, so a
//! slow or failing device never blocks editing (AGENTS.md section 2 and section 5,
//! decision 9). If output fails, playback stops with an error and editing continues.

// Native device backends. In a browser, `gscore-audio`'s synth engine is compiled into an
// AudioWorklet (see `docs/web.md`) and there is no MIDI port list.
#[cfg(not(target_arch = "wasm32"))]
mod midi_out;
// The metronome's own click (the synth engine plays it in a browser too).
mod chains;
mod click;
mod guitars;
mod limiter;
mod meters;
mod mixer;
pub use mixer::Bus;
#[cfg(not(target_arch = "wasm32"))]
mod clock;
mod sound_wire;
mod soundfont;
mod synth;
#[cfg(not(target_arch = "wasm32"))]
mod synth_player;
#[cfg(feature = "test-support")]
pub mod testing;
mod transport;
#[cfg(not(target_arch = "wasm32"))]
pub mod tuner;
#[cfg(target_arch = "wasm32")]
mod tuner_web;
mod wav;

pub use chains::{
    MAX_TONES, chain_is_empty, drum_room_knobs, drum_room_spec, prepare_chain, prepare_drum_room,
};
pub use meters::{Level, Meters};
#[cfg(not(target_arch = "wasm32"))]
pub use midi_out::{MidiOutPlayer, midi_output_ports};
pub use sound_wire::{SoundMessage, decode_sound, encode_sound};
pub use soundfont::{
    SOUNDFONT_ENV, available_soundfonts, available_soundfonts_in, find_soundfont, find_soundfont_in,
};
pub use synth::{BLOCK, SoundBank, SynthEngine};
#[cfg(not(target_arch = "wasm32"))]
pub use synth_player::SynthPlayer;

/// The sample rates the settings offer for the built-in synthesizer and audio export.
pub const SAMPLE_RATES: [u32; 4] = [44_100, 48_000, 88_200, 96_000];
#[cfg(target_arch = "wasm32")]
mod web_player;
#[cfg(target_arch = "wasm32")]
mod web_soundfont;
#[cfg(target_arch = "wasm32")]
pub use web_player::WebPlayer;

pub use chains::ChainKnobs;
#[cfg(not(target_arch = "wasm32"))]
pub use tuner::TunerPlayer;
#[cfg(target_arch = "wasm32")]
pub use tuner_web::TunerPlayer;
pub use wav::{
    BusChoice, BusChoices, ChainChoice, PartChoice, RoomChoice, StemPart, WAV_SAMPLE_RATE, WavInfo,
    write_wav, write_wav_at, write_wav_part_with, write_wav_with,
};

/// The preset string model of `kind`, before the user turns any knob.
pub fn default_string_model(kind: gscore_play::StringKind) -> gscore_guitar::StringModel {
    guitars::default_model(kind)
}

// The integration test builds songs with `gscore-model`'s test support; this marks the
// test-only dependency for the library test target's unused-dependency check.
#[cfg(test)]
use gscore_model as _;
// The integration tests use this crate with `test-support` (a dev-dependency on itself).
#[cfg(test)]
use gscore_audio as _;
