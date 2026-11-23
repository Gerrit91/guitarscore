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
    /// GuitarScore addition: Guitar Pro's opaque layout properties for this bar (its
    /// design mode's width scale, and whatever else the file wrote), kept verbatim.
    pub x_properties: Vec<XProperty>,
    /// GuitarScore addition: the sound changes written in this bar (Guitar Pro's `Sound`
    /// automations), kept so a save does not lose a track's RSE sound changing during the
    /// song. They sit on the bar, so inserting, moving or deleting bars carries them
    /// along instead of leaving them pointing at the wrong bar.
    pub sound_changes: Vec<SoundChange>,
}

/// GuitarScore addition: a track's sound change at some point in a bar (Guitar Pro's
/// `Sound` automation): the RSE sound it switches to, as written
/// (`Stringed/Electric Guitars/Overdrive Guitar;Blues;Factory`).
#[derive(Debug, Clone, PartialEq)]
pub struct SoundChange {
    /// Where in the bar it happens: Guitar Pro's fraction of the bar (`0.0` to `1.0`).
    pub position: f32,
    /// The sound, exactly as the file wrote it.
    pub value: String,
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
            x_properties: Vec::new(),
            sound_changes: Vec::new(),
        }
    }
}

impl Measure {
    /// This bar's own width scale (Guitar Pro's design mode), `None` when it has none.
    pub fn display_scale(&self) -> Option<f32> {
        property_scale(&self.x_properties, BAR_SCALE)
    }

    /// Sets this bar's own width scale, or removes it with `None`.
    pub fn set_display_scale(&mut self, scale: Option<f32>) {
        set_property_scale(&mut self.x_properties, BAR_SCALE, scale);
    }
}
