//! The techniques beyond the basic effects: trills with their fret and speed, tremolo
//! picking speeds, fingering, left-hand taps, string numbers, pick slides, rolled chords
//! (arpeggios), rasgueado, dead slapped beats, fades and beat text.
//!
//! The types are this crate's own; the accessors extend [`BeatView`] and [`NoteView`], and
//! [`NoteEffect`](crate::NoteEffect) and [`BeatEffect`](crate::BeatEffect) set them.

use gscore_gp::{BeatStrokeDirection, Duration as GpDuration, Fingering, TremoloPickingEffect};

use crate::effects::StrokeDirection;
use crate::error::ModelError;
use crate::views::{BeatView, NoteView};

/// How fast a trill alternates between its two notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrillSpeed {
    /// Sixteenth notes.
    #[default]
    Sixteenth,
    /// 32nd notes.
    ThirtySecond,
    /// 64th notes.
    SixtyFourth,
}

impl TrillSpeed {
    /// Every speed, slowest first.
    pub const ALL: [TrillSpeed; 3] = [
        TrillSpeed::Sixteenth,
        TrillSpeed::ThirtySecond,
        TrillSpeed::SixtyFourth,
    ];

    /// The note value of one trill note (16, 32 or 64).
    pub fn note_value(self) -> u16 {
        match self {
            TrillSpeed::Sixteenth => 16,
            TrillSpeed::ThirtySecond => 32,
            TrillSpeed::SixtyFourth => 64,
        }
    }

    fn from_value(value: u16) -> Self {
        match value {
            0..=16 => TrillSpeed::Sixteenth,
            17..=32 => TrillSpeed::ThirtySecond,
            _ => TrillSpeed::SixtyFourth,
        }
    }
}

/// A trill: the note alternates quickly with another fret on its string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trill {
    /// The other fret. In an edit, `None` means two frets above the note (a whole step).
    /// Read from a note, it is always set.
    pub fret: Option<u8>,
    /// How fast the two notes alternate.
    pub speed: TrillSpeed,
}

/// How fast tremolo picking repeats the note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TremoloSpeed {
    /// Eighth notes (one stroke through the stem).
    Eighth,
    /// Sixteenth notes (two strokes).
    #[default]
    Sixteenth,
    /// 32nd notes (three strokes).
    ThirtySecond,
}

impl TremoloSpeed {
    /// Every speed, slowest first.
    pub const ALL: [TremoloSpeed; 3] = [
        TremoloSpeed::Eighth,
        TremoloSpeed::Sixteenth,
        TremoloSpeed::ThirtySecond,
    ];

    /// The note value of one repeated note (8, 16 or 32).
    pub fn note_value(self) -> u16 {
        match self {
            TremoloSpeed::Eighth => 8,
            TremoloSpeed::Sixteenth => 16,
            TremoloSpeed::ThirtySecond => 32,
        }
    }

    /// The strokes written through the stem (1 to 3).
    pub fn strokes(self) -> u8 {
        match self {
            TremoloSpeed::Eighth => 1,
            TremoloSpeed::Sixteenth => 2,
            TremoloSpeed::ThirtySecond => 3,
        }
    }

    fn from_value(value: u16) -> Self {
        match value {
            0..=8 => TremoloSpeed::Eighth,
            9..=16 => TremoloSpeed::Sixteenth,
            _ => TremoloSpeed::ThirtySecond,
        }
    }
}

/// Which hand a fingering is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    /// The fretting hand: 0 (open), 1 to 4, and t (thumb).
    Left,
    /// The picking hand: p, i, m, a, c.
    Right,
}

/// A finger of a fingering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finger {
    /// The open string (left hand only), written `0`.
    Open,
    /// Thumb: `t` for the left hand, `p` for the right.
    Thumb,
    /// Index: `1` or `i`.
    Index,
    /// Middle: `2` or `m`.
    Middle,
    /// Ring: `3` or `a`.
    Ring,
    /// Little: `4` or `c`.
    Little,
}

impl Finger {
    /// Every finger, in the order a dialog offers them.
    pub const ALL: [Finger; 6] = [
        Finger::Open,
        Finger::Index,
        Finger::Middle,
        Finger::Ring,
        Finger::Little,
        Finger::Thumb,
    ];

    /// How the finger is written for `hand`: `0`-`4` and `t` (left), `p i m a c` (right;
    /// the open string has no right-hand sign and is written `0`).
    pub fn sign(self, hand: Hand) -> &'static str {
        match (hand, self) {
            (_, Finger::Open) => "0",
            (Hand::Left, Finger::Thumb) => "t",
            (Hand::Left, Finger::Index) => "1",
            (Hand::Left, Finger::Middle) => "2",
            (Hand::Left, Finger::Ring) => "3",
            (Hand::Left, Finger::Little) => "4",
            (Hand::Right, Finger::Thumb) => "p",
            (Hand::Right, Finger::Index) => "i",
            (Hand::Right, Finger::Middle) => "m",
            (Hand::Right, Finger::Ring) => "a",
            (Hand::Right, Finger::Little) => "c",
        }
    }
}

/// A fade on a beat: its volume swells in, out, or in and out again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fade {
    /// From silence up to full.
    In,
    /// From full down to silence.
    Out,
    /// Up and back down.
    Swell,
}

/// Most characters a beat's text may have.
pub const MAX_BEAT_TEXT_CHARS: usize = 255;

pub(crate) fn finger_to_gp(finger: Option<Finger>) -> Fingering {
    match finger {
        None => Fingering::Open,
        Some(Finger::Open) => Fingering::OpenString,
        Some(Finger::Thumb) => Fingering::Thumb,
        Some(Finger::Index) => Fingering::Index,
        Some(Finger::Middle) => Fingering::Middle,
        Some(Finger::Ring) => Fingering::Annular,
        Some(Finger::Little) => Fingering::Little,
    }
}

fn finger_from_gp(finger: &Fingering) -> Option<Finger> {
    match finger {
        Fingering::Open | Fingering::Unknown(_) => None,
        Fingering::OpenString => Some(Finger::Open),
        Fingering::Thumb => Some(Finger::Thumb),
        Fingering::Index => Some(Finger::Index),
        Fingering::Middle => Some(Finger::Middle),
        Fingering::Annular => Some(Finger::Ring),
        Fingering::Little => Some(Finger::Little),
    }
}

/// The trill as Guitar Pro stores it, on a note on fret `fret`.
pub(crate) fn trill_to_gp(trill: Trill, fret: i16) -> Result<gscore_gp::TrillEffect, ModelError> {
    let other = match trill.fret {
        Some(other) => i16::from(other),
        None => fret.saturating_add(2),
    };
    let other = i8::try_from(other).map_err(|_| {
        ModelError::Invalid(format!("the trill fret {other} of a note does not fit"))
    })?;
    Ok(gscore_gp::TrillEffect {
        fret: other,
        duration: GpDuration {
            value: trill.speed.note_value(),
            ..GpDuration::default()
        },
    })
}

/// Tremolo picking as Guitar Pro stores it.
pub(crate) fn tremolo_to_gp(speed: TremoloSpeed) -> TremoloPickingEffect {
    TremoloPickingEffect {
        duration: GpDuration {
            value: speed.note_value(),
            ..GpDuration::default()
        },
    }
}

/// The stroke direction as Guitar Pro stores it.
pub(crate) fn direction_from_gp(direction: &BeatStrokeDirection) -> Option<StrokeDirection> {
    match direction {
        BeatStrokeDirection::Up => Some(StrokeDirection::Up),
        BeatStrokeDirection::Down => Some(StrokeDirection::Down),
        BeatStrokeDirection::None => None,
    }
}

/// Sets the beat's fade, clearing the others (a beat has at most one).
pub(crate) fn set_fade(effects: &mut gscore_gp::BeatEffects, fade: Fade, on: bool) {
    if on {
        effects.fade_in = false;
        effects.fade_out = false;
        effects.volume_swell = false;
    }
    match fade {
        Fade::In => effects.fade_in = on,
        Fade::Out => effects.fade_out = on,
        Fade::Swell => effects.volume_swell = on,
    }
}

impl BeatView<'_> {
    /// The rolled chord (arpeggio) on this beat and its direction, if any. A brush stroke
    /// ([`BeatView::brush_stroke`]) is the quicker strum.
    pub fn arpeggio(&self) -> Option<StrokeDirection> {
        direction_from_gp(&self.inner.effect.arpeggio)
    }

    /// True if the beat is dead slapped: a percussive slap on muted strings, written as a
    /// large cross instead of its notes.
    pub fn is_dead_slapped(&self) -> bool {
        self.inner.effect.dead_slapped
    }

    /// True if the chord is played as a rasgueado (a flamenco roll of the fingers).
    pub fn has_rasgueado(&self) -> bool {
        self.inner.effect.has_rasgueado
    }

    /// The beat's fade, if it has one.
    pub fn fade(&self) -> Option<Fade> {
        let effects = &self.inner.effect;
        if effects.fade_in {
            Some(Fade::In)
        } else if effects.fade_out {
            Some(Fade::Out)
        } else if effects.volume_swell {
            Some(Fade::Swell)
        } else {
            None
        }
    }

    /// The text written above the beat (Guitar Pro's beat text); empty if none.
    pub fn text(&self) -> &str {
        &self.inner.text
    }
}

impl NoteView<'_> {
    /// The note's trill: the other fret and the speed.
    pub fn trill(&self) -> Option<Trill> {
        let trill = self.inner.effect.trill.as_ref()?;
        Some(Trill {
            fret: u8::try_from(trill.fret).ok(),
            speed: TrillSpeed::from_value(trill.duration.value),
        })
    }

    /// How fast the note is tremolo picked, if it is.
    pub fn tremolo_speed(&self) -> Option<TremoloSpeed> {
        let tremolo = self.inner.effect.tremolo_picking.as_ref()?;
        Some(TremoloSpeed::from_value(tremolo.duration.value))
    }

    /// The finger that plays the note with `hand`, if the score says.
    pub fn finger(&self, hand: Hand) -> Option<Finger> {
        match hand {
            Hand::Left => finger_from_gp(&self.inner.effect.left_hand_finger),
            Hand::Right => finger_from_gp(&self.inner.effect.right_hand_finger),
        }
    }

    /// True if the note is tapped with the fretting hand (no pick).
    pub fn is_left_hand_tapped(&self) -> bool {
        self.inner.effect.left_hand_tapped
    }

    /// True if the note's string number is written (a circled number).
    pub fn shows_string_number(&self) -> bool {
        self.inner.effect.show_string_number
    }
}
