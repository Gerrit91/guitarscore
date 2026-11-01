//! The GuitarScore document model.
//!
//! [`Song`] wraps the `guitarpro` crate's song model, so every field of a loaded file is kept
//! and written back unchanged, including the ones GuitarScore does not understand yet.
//! The wrapped model is private: the rest of the code base sees only this crate's API —
//! read-only views ([`TrackView`], [`MeasureView`], [`VoiceView`], [`BeatView`],
//! [`NoteView`]), value types ([`Duration`], [`TimeSignature`], [`Fret`], ...) and index
//! types ([`TrackIndex`], [`BeatPosition`], ...). No public item takes or returns a
//! `guitarpro` type, except in [`interop`], which only `gscore-io` may use
//! (enforced by `cargo xtask layers`). Replacing the wrapped model later is therefore a
//! change inside this crate only.
//!
//! A song is validated when it is created ([`interop::from_guitarpro`]): values the
//! editor cannot handle (a note on a string the track does not have, a duration that is
//! not a power of two, ...) are rejected with a [`ModelError`]. Musical problems that real
//! files can have, such as a measure that is not completely filled, are reported by
//! [`Song::issues`] instead.

#![warn(clippy::arithmetic_side_effects)]

mod duration;
mod error;
pub mod interop;
mod issues;
mod position;
mod song;
mod validate;
mod views;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

pub use duration::{Duration, TimeSignature};
pub use error::ModelError;
pub use issues::{Issue, IssueKind};
pub use position::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, Pitch, StringNumber, TrackIndex, VoiceIndex,
};
pub use song::Song;
pub use views::{
    BeatKind, BeatView, MeasureView, NoteKind, NoteView, StringTuning, TrackView, VoiceView,
};
