//! Guitar Pro song data: measure.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::model::*;

pub const MAX_VOICES: usize = 2;

/// A measure header contains metadata for measures over multiple tracks.
#[derive(Debug, Clone)]
pub struct Measure {
    pub number: usize,
    pub start: i64,
    pub has_double_bar: bool,
    pub key_signature: KeySignature,
    pub time_signature: TimeSignature,
    pub track_index: usize,
    pub header_index: usize,
    pub clef: MeasureClef,
    /// GuitarScore addition: whether the file stored a clef for this measure (GP6/GP7 do;
    /// GP3-5 store none, so the clef is derived from the instrument there).
    pub has_clef: bool,
    /// Max voice count is 2
    pub voices: Vec<Voice>,
    pub line_break: LineBreak,
    /// Simile mark from GPIF (GP6/GP7)
    pub simile_mark: Option<String>,
}

impl Default for Measure {
    fn default() -> Self {
        Measure {
            number: 1,
            start: DURATION_QUARTER_TIME,
            has_double_bar: false,
            key_signature: KeySignature::default(), //Cmajor
            time_signature: TimeSignature::default(),
            track_index: 0,
            header_index: 0,
            clef: MeasureClef::Treble,
            has_clef: false,
            voices: Vec::with_capacity(2),
            line_break: LineBreak::None,
            simile_mark: None,
        }
    }
}
