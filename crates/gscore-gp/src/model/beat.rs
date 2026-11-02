//! Guitar Pro song data: beat.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::model::*;

/// Parameters of beat display
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeatDisplay {
    pub break_beam: bool,
    pub force_beam: bool,
    pub beam_direction: VoiceDirection,
    pub tuplet_bracket: TupletBracket,
    pub break_secondary: u8,
    pub break_secondary_tuplet: bool,
    pub force_bracket: bool,
}

impl Default for BeatDisplay {
    fn default() -> Self {
        BeatDisplay {
            break_beam: false,
            force_beam: false,
            beam_direction: VoiceDirection::None,
            tuplet_bracket: TupletBracket::None,
            break_secondary: 0,
            break_secondary_tuplet: false,
            force_bracket: false,
        }
    }
}

/// A stroke effect for beats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeatStroke {
    pub direction: BeatStrokeDirection,
    pub value: u16,
    pub swap: bool,
}

impl Default for BeatStroke {
    fn default() -> Self {
        BeatStroke {
            direction: BeatStrokeDirection::None,
            value: 0,
            swap: false,
        }
    }
}

impl BeatStroke {
    pub fn swap_direction(&mut self) {
        if self.direction == BeatStrokeDirection::Up {
            self.direction = BeatStrokeDirection::Down
        } else if self.direction == BeatStrokeDirection::Down {
            self.direction = BeatStrokeDirection::Up
        }
    }
}

/// A voice contains multiple beats
#[derive(Debug, Clone)]
pub struct Voice {
    pub measure_index: i16,
    pub beats: Vec<Beat>,
    pub directions: VoiceDirection,
}

impl Default for Voice {
    fn default() -> Self {
        Voice {
            measure_index: 0,
            beats: Vec::new(),
            directions: VoiceDirection::None,
        }
    }
}

/// A legato run written over a whole beat (GP6/GP7 `<Legato>`).
///
/// The origin and destination flags overlap in the middle of a run: every beat between
/// the first and the last one carries both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LegatoEffect {
    /// This beat starts a legato group.
    pub origin: bool,
    /// This beat continues a legato group.
    pub destination: bool,
}

/// This class contains all beat effects
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeatEffects {
    pub stroke: BeatStroke,
    pub has_rasgueado: bool,
    pub pick_stroke: BeatStrokeDirection,
    pub chord: Option<Chord>,
    pub fade_in: bool,
    pub tremolo_bar: Option<BendEffect>,
    pub mix_table_change: Option<MixTableChange>,
    pub slap_effect: SlapEffect,
    pub vibrato: bool,
    /// Wide (as opposed to slight) beat vibrato. GuitarScore addition: GPIF's
    /// `VibratoWTremBar` names the strength; the legacy flag alone lost it.
    pub wide_vibrato: bool,
    /// GuitarScore addition: GPIF's beat-level legato (not a slide or a hammer).
    pub legato: LegatoEffect,
}

impl Default for BeatEffects {
    fn default() -> Self {
        BeatEffects {
            stroke: BeatStroke::default(),
            has_rasgueado: false,
            pick_stroke: BeatStrokeDirection::None,
            chord: None,
            fade_in: false,
            tremolo_bar: None,
            mix_table_change: None,
            slap_effect: SlapEffect::None,
            vibrato: false,
            wide_vibrato: false,
            legato: LegatoEffect::default(),
        }
    }
}

impl BeatEffects {
    pub fn is_chord(&self) -> bool {
        self.chord.is_some()
    }
    pub fn is_tremolo_bar(&self) -> bool {
        self.tremolo_bar.is_some()
    }
    pub fn is_slap_effect(&self) -> bool {
        self.slap_effect != SlapEffect::None
    }
    pub fn has_pick_stroke(&self) -> bool {
        self.pick_stroke != BeatStrokeDirection::None
    }
    pub fn is_default(&self) -> bool {
        let d = BeatEffects::default();
        self.stroke == d.stroke
            && self.has_rasgueado == d.has_rasgueado
            && self.pick_stroke == d.pick_stroke
            && self.fade_in == d.fade_in
            && self.vibrato == d.vibrato
            && self.wide_vibrato == d.wide_vibrato
            && self.tremolo_bar == d.tremolo_bar
            && self.slap_effect == d.slap_effect
            && self.legato == d.legato
    }
}

/// A beat contains multiple notes
#[derive(Debug, Clone, PartialEq)]
pub struct Beat {
    pub notes: Vec<Note>,
    pub duration: Duration,
    pub text: String,
    pub start: Option<i64>,
    pub effect: BeatEffects,
    pub octave: Octave,
    pub display: BeatDisplay,
    pub status: BeatStatus,
}

impl Default for Beat {
    fn default() -> Self {
        Beat {
            notes: Vec::with_capacity(12),
            duration: Duration::default(),
            text: String::new(),
            start: None,
            effect: BeatEffects::default(),
            octave: Octave::None,
            display: BeatDisplay::default(),
            status: BeatStatus::Normal,
        }
    }
}

impl Beat {
    /// Whether a note of this beat has vibrato.
    pub fn has_vibrato(&self) -> bool {
        self.notes.iter().any(|note| note.effect.vibrato)
    }

    /// Whether a note of this beat is a harmonic.
    pub fn has_harmonic(&self) -> bool {
        self.notes.iter().any(|note| note.effect.is_harmonic())
    }
}
