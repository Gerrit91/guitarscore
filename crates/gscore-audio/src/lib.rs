//! Audio and MIDI output backends implementing `gscore_play::Player`. Only this crate may
//! use device crates: `midir` for MIDI output, `cpal` and `rustysynth` for the internal
//! synthesizer.
//!
//! Every backend runs on its own thread; the UI talks to it through a bounded channel, so a
//! slow or failing device never blocks editing (AGENTS.md section 2 and section 5,
//! decision 9). If output fails, playback stops with an error and editing continues.

mod midi_out;
mod soundfont;
mod synth;
mod synth_player;
#[cfg(feature = "test-support")]
pub mod testing;
mod transport;

pub use midi_out::{MidiOutPlayer, midi_output_ports};
pub use soundfont::{
    SOUNDFONT_ENV, available_soundfonts, available_soundfonts_in, find_soundfont, find_soundfont_in,
};
pub use synth::{BLOCK, SoundBank, SynthEngine};
pub use synth_player::SynthPlayer;

// The integration test builds songs with `gscore-model`'s test support; this marks the
// test-only dependency for the library test target's unused-dependency check.
#[cfg(test)]
use gscore_model as _;
// The integration tests use this crate with `test-support` (a dev-dependency on itself).
#[cfg(test)]
use gscore_audio as _;
