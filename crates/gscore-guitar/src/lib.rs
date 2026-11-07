//! Physically modelled guitar and bass strings (M22, `docs/guitar-modelling.md`).
//!
//! Each string is a digital waveguide ([`StringVoice`]): a delay line in a feedback loop
//! with a loss filter and stiffness allpasses, tuned exactly by a fractional delay. Nothing
//! is sampled; every technique is a change of the running model (bends and slides change
//! the delay, hammer-ons change it without a pluck, damping lowers the loop gain).
//!
//! Pure Rust with no dependencies, so it builds for wasm and runs in the browser's
//! AudioWorklet too. [`StringVoice::render`] never allocates or blocks; a broken parameter
//! mutes the string instead of failing.

#![warn(clippy::arithmetic_side_effects)]

mod amp;
mod chain;
mod compressor;
mod convolver;
mod delay;
mod drive;
mod fft;
mod filters;
mod impulse;
mod knobs;
mod model;
mod oversample;
mod pickup;
mod reverb;
mod string;
mod string_loop;
mod tail;

pub use amp::{AmpKnobs, AmpVoice};
pub use chain::{BLOCKS, BlockKind, Chain, ChainSpec, PreparedChain};
pub use compressor::{CompRatio, CompressorKnobs};
pub use drive::DriveKnobs;
pub use impulse::{ImpulseError, ImpulseResponse, MAX_SECONDS};
pub use knobs::{BlockSettings, ImpulseKnobs, ReverbKnobs, RoomKnobs, knob_ranges};
pub use model::{GuitarModel, StringModel, ranges};
pub use pickup::{Pickup, positions as pickup_positions};
pub use string::{Attack, Pluck, Release, StringVoice};
pub use tail::{LENGTH_RANGE, MAX_LENGTHENED_SECONDS};

/// The frequency of MIDI note `key` plus `cents`, in Hz (A4 = note 69 = 440 Hz).
pub fn frequency(key: u8, cents: f32) -> f32 {
    440.0 * ((f32::from(key) - 69.0 + cents / 100.0) / 12.0).exp2()
}

// The no-allocation test counts allocations with `stats_alloc`; this marks the test-only
// dependency for the library test target's unused-dependency check.
#[cfg(test)]
use stats_alloc as _;
