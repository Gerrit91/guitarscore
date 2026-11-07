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

mod chord_name;
mod compare;
mod duration;
mod dynamics;
mod edit;
mod effects;
mod error;
mod grace;
mod instrument;
pub mod interop;
mod issues;
mod lyrics;
mod marks;
mod metadata;
mod page;
mod playback;
mod position;
mod song;
mod style;
mod techniques;
#[cfg(feature = "test-support")]
pub mod testing;
mod tuning;
mod validate;
mod views;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

pub use chord_name::chord_name;
pub use compare::{Difference, differences, settings_differences};
pub use duration::{Duration, NoteValue, TimeSignature};
pub use dynamics::Dynamic;
pub use edit::{Change, MAX_SECTION_TITLE_CHARS, MAX_STRINGS, MAX_TRACK_NAME_CHARS, MAX_TRACKS};
pub use effects::{
    AccentKind, BEND_MAX_POSITION, BEND_MAX_VALUE, BeatEffect, Bend, BendKind, BendPoint,
    BendShape, Harmonic, HarmonicKind, Legato, MAX_BEND_POINTS, NoteEffect, OctaveSign, SlapKind,
    SlideKind, Stroke, StrokeDirection, StrokeKind, Vibrato, Whammy, WhammyShape,
};
pub use error::ModelError;
pub use grace::{Grace, GraceSpeed};
pub use instrument::{InstrumentKind, InstrumentSymbol, program_name};
pub use issues::{Issue, IssueKind};
pub use lyrics::{LYRIC_LINES, LyricLine, LyricSyllable, MAX_LYRIC_LINE_CHARS};
pub use marks::{Direction, Feel, Fermata};
pub use metadata::{
    MAX_SONG_NOTES_CHARS, MAX_SONG_TEXT_CHARS, MAX_TEMPO, SONG_TEXT_FIELDS, SongInfo, SongTextField,
};
pub use page::{
    MAX_BARS_PER_SYSTEM, MAX_MARGIN_MM, Margins, PAGE_SIZE_MM, PageSetup, PaperSize, SCALE_RANGE,
    SystemBreak,
};
pub use playback::{MASTER_UNITY, MasterMix, Repeat, TrackSound};
pub use position::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, Pitch, StringNumber, TrackIndex, VoiceIndex,
};
pub use song::Song;
pub use style::{Clef, KeySignature, Section, SimileMark, TrackColor, TrackStaves};
pub use techniques::{
    ChordDiagram, Fade, Finger, Hand, MAX_BEAT_TEXT_CHARS, TremoloSpeed, Trill, TrillSpeed,
};
pub use views::{
    BeatKind, BeatView, MeasureView, NoteKind, NoteView, StringTuning, TrackView, VoiceView,
    track_name_in_binary_formats,
};
