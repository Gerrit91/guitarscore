//! Guitar Pro song data: note.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::model::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub value: i16,
    pub velocity: i16,
    pub string: i8,
    pub effect: NoteEffect,
    pub duration_percent: f32,
    pub swap_accidentals: bool,
    pub kind: NoteType,
    pub duration: Option<i8>,
    pub tuplet: Option<i8>,
}

impl Default for Note {
    fn default() -> Self {
        Note {
            value: 0,
            velocity: DEFAULT_VELOCITY,
            string: 0,
            effect: NoteEffect::default(),
            duration_percent: 1.0,
            swap_accidentals: false,
            kind: NoteType::Rest,
            duration: None,
            tuplet: None,
        }
    }
}

/// Contains all effects which can be applied to one note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteEffect {
    pub accentuated_note: bool,
    pub bend: Option<BendEffect>,
    pub ghost_note: bool,
    pub grace: Option<GraceEffect>,
    pub hammer: bool,
    pub harmonic: Option<HarmonicEffect>,
    pub heavy_accentuated_note: bool,
    pub left_hand_finger: Fingering,
    pub let_ring: bool,
    pub palm_mute: bool,
    pub right_hand_finger: Fingering,
    pub slides: Vec<SlideType>,
    pub staccato: bool,
    pub tremolo_picking: Option<TremoloPickingEffect>,
    pub trill: Option<TrillEffect>,
    pub vibrato: bool,
    /// Wide (as opposed to slight) vibrato. GuitarScore addition: GPIF stores the kind in
    /// `<Vibrato>`; the legacy flag alone lost the distinction.
    pub wide_vibrato: bool,
    /// Ornament type from GPIF (GP6/GP7)
    pub ornament: Option<String>,
    /// GuitarScore addition: GPIF's `LeftHandTapped` (a left-hand tap, no pick).
    pub left_hand_tapped: bool,
    /// GuitarScore addition: GPIF's `ShowStringNumber` (the string's circled number).
    pub show_string_number: bool,
}

impl Default for NoteEffect {
    fn default() -> Self {
        NoteEffect {
            accentuated_note: false,
            bend: None,
            ghost_note: false,
            grace: None,
            hammer: false,
            harmonic: None,
            heavy_accentuated_note: false,
            left_hand_finger: Fingering::Open,
            let_ring: false,
            palm_mute: false,
            right_hand_finger: Fingering::Open,
            slides: Vec::new(),
            staccato: false,
            tremolo_picking: None,
            trill: None,
            vibrato: false,
            wide_vibrato: false,
            ornament: None,
            left_hand_tapped: false,
            show_string_number: false,
        }
    }
}

impl NoteEffect {
    pub fn is_bend(&self) -> bool {
        self.bend.is_some()
    }
    pub fn is_harmonic(&self) -> bool {
        self.harmonic.is_some()
    }
    pub fn is_grace(&self) -> bool {
        self.grace.is_some()
    }
    pub fn is_trill(&self) -> bool {
        self.trill.is_some()
    }
    pub fn is_tremollo_picking(&self) -> bool {
        self.tremolo_picking.is_some()
    }
    pub fn is_default(&self) -> bool {
        let d = NoteEffect::default();
        self.left_hand_finger == d.left_hand_finger
            && self.right_hand_finger == d.right_hand_finger
            && self.bend == d.bend
            && self.harmonic == d.harmonic
            && self.grace == d.grace
            && self.trill == d.trill
            && self.tremolo_picking == d.tremolo_picking
            && self.vibrato == d.vibrato
            && self.wide_vibrato == d.wide_vibrato
            && self.slides == d.slides
            && self.hammer == d.hammer
            && self.palm_mute == d.palm_mute
            && self.staccato == d.staccato
            && self.let_ring == d.let_ring
            && self.left_hand_tapped == d.left_hand_tapped
            && self.show_string_number == d.show_string_number
    }
    pub fn is_fingering(&self) -> bool {
        self.left_hand_finger != Fingering::Open || self.right_hand_finger != Fingering::Open
    }
}
