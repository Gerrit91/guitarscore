//! The GuitarScore document model.
//!
//! [`Song`] wraps the Guitar Pro song data of the `gscore-gp` crate, so every field of a loaded file is kept
//! and written back unchanged, including the ones GuitarScore does not understand yet.
//! The wrapped data is private: the rest of the code base sees only this crate's API —
//! read-only views ([`TrackView`], [`MeasureView`], [`VoiceView`], [`BeatView`],
//! [`NoteView`]), value types ([`Duration`], [`TimeSignature`], [`Fret`], ...) and index
//! types ([`TrackIndex`], [`BeatPosition`], ...). No public item takes or returns a
//! `gscore-gp` type, except in [`interop`], which only `gscore-io` may use; only this crate
//! and `gscore-io` may depend on `gscore-gp` (both enforced by `cargo xtask layers`).
//! Replacing the wrapped data later is therefore a change inside this crate only.
//!
//! A song is validated when it is created ([`interop::from_gp`]): structural
//! problems that break timing or navigation (a duration that is not a power of two, a
//! track with a different number of measures, ...) are rejected with a [`ModelError`].
//! Everything real files contain, such as unfilled measures or notes on strings a track
//! lacks, is reported by [`Song::issues`] instead, and kept unchanged.

#![warn(clippy::arithmetic_side_effects)]

mod compare;
mod duration;
mod edit;
mod effects;
mod error;
mod instrument;
pub mod interop;
mod issues;
mod playback;
mod position;
mod song;
#[cfg(feature = "test-support")]
pub mod testing;
mod tuning;
mod validate;
mod views;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

pub use compare::{Difference, differences};
pub use duration::{Duration, NoteValue, TimeSignature};
pub use edit::Change;
pub use effects::{
    AccentKind, Bend, BendKind, Harmonic, HarmonicKind, Legato, SlapKind, SlideKind,
    StrokeDirection, Vibrato, Whammy,
};
pub use error::ModelError;
pub use instrument::InstrumentKind;
pub use issues::{Issue, IssueKind};
pub use playback::{Repeat, TrackSound};
pub use position::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, Pitch, StringNumber, TrackIndex, VoiceIndex,
};
pub use song::Song;
pub use views::{
    BeatKind, BeatView, MeasureView, NoteKind, NoteView, StringTuning, TrackView, VoiceView,
};
