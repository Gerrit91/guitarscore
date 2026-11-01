//! Effects on beats and notes: strokes, slap and tapping, slides, bends, harmonics, and the
//! simple on/off techniques (ghost notes, accents, palm mute, let ring, ...).
//!
//! The types are this crate's own; the accessors extend [`BeatView`] and [`NoteView`].

use gscore_gp::{BeatStrokeDirection, BendType, HarmonicType, SlapEffect, SlideType};

use crate::views::{BeatView, NoteView};

/// The direction of a pick or brush stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeDirection {
    /// Towards the higher strings' side (an upstroke).
    Up,
    /// Towards the lower strings' side (a downstroke).
    Down,
}

/// A tapping or slap technique played on a whole beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlapKind {
    /// Tapping (right-hand hammer-on).
    Tapping,
    /// Slap.
    Slapping,
    /// Pop.
    Popping,
}

/// How a slide gets to or from its note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlideKind {
    /// Slide in from a higher note.
    IntoFromAbove,
    /// Slide in from a lower note.
    IntoFromBelow,
    /// Shift slide to the next note.
    ShiftSlideTo,
    /// Legato slide to the next note.
    LegatoSlideTo,
    /// Slide out downwards, to no particular note.
    OutDownwards,
    /// Slide out upwards, to no particular note.
    OutUpwards,
}

/// What a bend does to the pitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BendKind {
    /// Bend the string up (including a prebend and a bend that is released again).
    Up,
    /// Release a bend back down.
    Release,
}

/// A bend on a note and how far it reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bend {
    /// Whether the bend goes up or releases back down.
    pub kind: BendKind,
    /// How far the bend reaches at its highest point, in quarter tones: a half-step bend
    /// is 2, a full (whole-step) bend 4. Zero when the file stores no bend points.
    pub quarter_tones: i16,
}

/// What kind of harmonic a note is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarmonicKind {
    /// A natural harmonic, written on the fretted note.
    Natural,
    /// An artificial harmonic.
    Artificial,
    /// A tapped harmonic.
    Tapped,
    /// A pinch (squeal) harmonic.
    Pinch,
    /// A semi-harmonic.
    Semi,
}

/// A harmonic on a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Harmonic {
    /// The kind of harmonic.
    pub kind: HarmonicKind,
    /// The node's fret when the file stores the harmonic fret. The file stores it as a
    /// distance from the fretted note, so a pinch harmonic on fret 7 with a stored 12
    /// sounds the node on fret 19.
    pub node: Option<u8>,
}

impl BeatView<'_> {
    /// The pick stroke on this beat, if any.
    pub fn pick_stroke(&self) -> Option<StrokeDirection> {
        match self.inner.effect.pick_stroke {
            BeatStrokeDirection::Up => Some(StrokeDirection::Up),
            BeatStrokeDirection::Down => Some(StrokeDirection::Down),
            BeatStrokeDirection::None => None,
        }
    }

    /// The brush stroke (strum direction) on this beat, if any.
    pub fn brush_stroke(&self) -> Option<StrokeDirection> {
        match self.inner.effect.stroke.direction {
            BeatStrokeDirection::Up => Some(StrokeDirection::Up),
            BeatStrokeDirection::Down => Some(StrokeDirection::Down),
            BeatStrokeDirection::None => None,
        }
    }

    /// The tapping or slap technique on this beat, if any.
    pub fn slap(&self) -> Option<SlapKind> {
        match self.inner.effect.slap_effect {
            SlapEffect::Tapping => Some(SlapKind::Tapping),
            SlapEffect::Slapping => Some(SlapKind::Slapping),
            SlapEffect::Popping => Some(SlapKind::Popping),
            SlapEffect::None => None,
        }
    }

    /// True if the beat fades in.
    pub fn has_fade_in(&self) -> bool {
        self.inner.effect.fade_in
    }

    /// True if the beat uses the whammy bar (tremolo bar).
    pub fn has_whammy_bar(&self) -> bool {
        self.inner.effect.tremolo_bar.is_some()
    }
}

impl NoteView<'_> {
    /// True if this note is the origin of a hammer-on or pull-off.
    pub fn is_hammer(&self) -> bool {
        self.inner.effect.hammer
    }

    /// What the note's bend does, if it has one.
    pub fn bend(&self) -> Option<Bend> {
        let bend = self.inner.effect.bend.as_ref()?;
        let kind = match bend.kind {
            BendType::Bend
            | BendType::BendRelease
            | BendType::BendReleaseBend
            | BendType::Prebend
            | BendType::PrebendRelease => BendKind::Up,
            BendType::ReleaseUp | BendType::ReleaseDown => BendKind::Release,
            BendType::None
            | BendType::Dip
            | BendType::Dive
            | BendType::InvertedDip
            | BendType::Return => return None,
        };
        // Bend points are stored in quarter tones. The highest point is how far the bend
        // reaches, also for a bend that is released again (whose last point is 0).
        let quarter_tones = bend
            .points
            .iter()
            .map(|point| i16::from(point.value))
            .max_by_key(|value| value.unsigned_abs())
            .unwrap_or(0);
        Some(Bend {
            kind,
            quarter_tones,
        })
    }

    /// The slides on this note, in the order the file stores them.
    pub fn slides(&self) -> impl Iterator<Item = SlideKind> + '_ {
        self.inner
            .effect
            .slides
            .iter()
            .filter_map(|slide| match slide {
                SlideType::IntoFromAbove => Some(SlideKind::IntoFromAbove),
                SlideType::IntoFromBelow => Some(SlideKind::IntoFromBelow),
                SlideType::ShiftSlideTo => Some(SlideKind::ShiftSlideTo),
                SlideType::LegatoSlideTo => Some(SlideKind::LegatoSlideTo),
                SlideType::OutDownwards => Some(SlideKind::OutDownwards),
                SlideType::OutUpWards => Some(SlideKind::OutUpwards),
                SlideType::None => None,
            })
    }

    /// The harmonic on this note, with its kind and node.
    pub fn harmonic(&self) -> Option<Harmonic> {
        let harmonic = self.inner.effect.harmonic.as_ref()?;
        let kind = match harmonic.kind {
            HarmonicType::Natural => HarmonicKind::Natural,
            HarmonicType::Artificial => HarmonicKind::Artificial,
            HarmonicType::Tapped => HarmonicKind::Tapped,
            HarmonicType::Pinch => HarmonicKind::Pinch,
            HarmonicType::Semi => HarmonicKind::Semi,
        };
        let node = harmonic.fret.and_then(|offset| {
            i32::from(self.inner.value)
                .checked_add(i32::from(offset))
                .and_then(|node| u8::try_from(node).ok())
        });
        Some(Harmonic { kind, node })
    }

    /// True if the note is a ghost note.
    pub fn is_ghost(&self) -> bool {
        self.inner.effect.ghost_note
    }

    /// True if the note is accentuated.
    pub fn is_accented(&self) -> bool {
        self.inner.effect.accentuated_note
    }

    /// True if the note is palm muted.
    pub fn is_palm_muted(&self) -> bool {
        self.inner.effect.palm_mute
    }

    /// True if the note should let ring.
    pub fn is_let_ring(&self) -> bool {
        self.inner.effect.let_ring
    }

    /// True if the note is staccato.
    pub fn is_staccato(&self) -> bool {
        self.inner.effect.staccato
    }

    /// True if the note has a trill.
    pub fn has_trill(&self) -> bool {
        self.inner.effect.trill.is_some()
    }

    /// True if the note is tremolo picked.
    pub fn has_tremolo_picking(&self) -> bool {
        self.inner.effect.tremolo_picking.is_some()
    }

    /// True if the note has vibrato (slight or wide).
    pub fn has_vibrato(&self) -> bool {
        self.inner.effect.vibrato
    }
}
