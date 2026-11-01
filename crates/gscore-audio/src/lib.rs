//! Audio and MIDI output backends implementing `gscore_play::Player`. Only this crate may
//! use device crates (`midir` now; `cpal` and `rustysynth` with the internal synth, M9).
//!
//! Every backend runs on its own thread; the UI talks to it through a bounded channel, so a
//! slow or failing device never blocks editing (AGENTS.md section 2 and section 5,
//! decision 9). If output fails, playback stops with an error and editing continues.

mod midi_out;

pub use midi_out::{MidiOutPlayer, midi_output_ports};

// The integration test builds songs with `gscore-model`'s test support; this marks the
// test-only dependency for the library test target's unused-dependency check.
#[cfg(test)]
use gscore_model as _;
