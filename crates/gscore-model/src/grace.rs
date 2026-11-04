//! Grace notes: a quick note on the same string just before (or on) the note it ornaments.
//!
//! The model keeps a grace note attached to its main note (as GP3-5 store it). GP6/GP7
//! grace beats are folded into their main note when a file is read, wherever nothing is
//! lost; the few that cannot be (a grace note on a string the main beat does not play) stay
//! grace beats, which are drawn and played but not edited ([`crate::BeatView::is_grace`]).

use gscore_gp::{GraceEffect, GraceEffectTransition, grace_value};

use crate::error::ModelError;
use crate::position::Fret;

/// How quick a grace note is: its written value. Guitar Pro offers these three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GraceSpeed {
    /// A 64th: a flick.
    Fast,
    /// A 32nd: the usual grace note.
    #[default]
    Normal,
    /// A 16th: clearly heard before the main note.
    Slow,
}

impl GraceSpeed {
    /// Every speed, fastest first.
    pub const ALL: [GraceSpeed; 3] = [GraceSpeed::Fast, GraceSpeed::Normal, GraceSpeed::Slow];

    /// The written value as a note value (64, 32 or 16).
    pub fn value(self) -> u8 {
        match self {
            GraceSpeed::Fast => 64,
            GraceSpeed::Normal => 32,
            GraceSpeed::Slow => 16,
        }
    }

    /// The speed of written value `value` (64, 32, 16; anything else is normal).
    pub fn from_value(value: u16) -> Self {
        match value {
            64 => GraceSpeed::Fast,
            16 => GraceSpeed::Slow,
            _ => GraceSpeed::Normal,
        }
    }
}

/// A grace note attached to a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Grace {
    /// The fret of the grace note on the main note's string.
    pub fret: Fret,
    /// How quick it is.
    pub speed: GraceSpeed,
    /// Played on the beat (an appoggiatura, delaying the main note) instead of just
    /// before it (an acciaccatura).
    pub on_beat: bool,
    /// A dead (muted) grace note.
    pub dead: bool,
}

impl Grace {
    /// An ordinary grace note on `fret`: normal speed, before the beat.
    pub fn before(fret: Fret) -> Self {
        Grace {
            fret,
            speed: GraceSpeed::Normal,
            on_beat: false,
            dead: false,
        }
    }

    /// The grace note Guitar Pro stores.
    pub(crate) fn from_gp(grace: &GraceEffect) -> Self {
        let dead = grace.is_dead || grace.fret < 0;
        Grace {
            fret: u8::try_from(grace.fret)
                .ok()
                .and_then(|fret| Fret::new(fret).ok())
                .unwrap_or(Fret::OPEN),
            speed: GraceSpeed::from_value(grace_value(grace.duration)),
            on_beat: grace.is_on_beat,
            dead,
        }
    }

    /// Writes `grace` onto `note` (or removes it), keeping the stored loudness and the
    /// transition into the main note of a grace note it replaces.
    pub(crate) fn apply(
        grace: Option<Grace>,
        note: &mut gscore_gp::Note,
    ) -> Result<(), ModelError> {
        let Some(grace) = grace else {
            note.effect.grace = None;
            return Ok(());
        };
        let fret = i8::try_from(grace.fret.get()).map_err(|_| {
            ModelError::Invalid(format!(
                "a grace note on fret {} does not fit (at most {})",
                grace.fret.get(),
                i8::MAX
            ))
        })?;
        let previous = note.effect.grace.take();
        note.effect.grace = Some(GraceEffect {
            fret,
            velocity: previous.as_ref().map_or(note.velocity, |p| p.velocity),
            duration: grace.speed.value(),
            is_dead: grace.dead,
            is_on_beat: grace.on_beat,
            transition: previous.map_or(GraceEffectTransition::None, |p| p.transition),
        });
        Ok(())
    }
}
