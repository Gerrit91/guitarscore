//! Guitar Pro song data: effects.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see LICENSE-guitarpro next to this file). Changes: docs/guitarpro.md.

use crate::gp::*;

/// A single point within the BendEffect
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BendPoint {
    pub position: u8,
    pub value: i8,
    pub vibrato: bool,
}

pub const BEND_EFFECT_MAX_POSITION: u8 = 12;

pub const GP_BEND_SEMITONE: f32 = 25.0;

pub const GP_BEND_POSITION: f32 = 60.0;

pub const GP_BEND_SEMITONE_LENGTH: f32 = 1.0;

/// This effect is used to describe string bends and tremolo bars
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BendEffect {
    pub kind: BendType,
    pub value: i16,
    pub points: Vec<BendPoint>,
    pub semitone_length: u8,
    pub max_position: u8,
    pub max_value: u8,
}

impl Default for BendEffect {
    fn default() -> Self {
        BendEffect {
            kind: BendType::None,
            value: 0,
            points: Vec::with_capacity(12),
            semitone_length: 1,
            max_position: BEND_EFFECT_MAX_POSITION,
            max_value: 12,
        }
    }
}

// A collection of velocities / dynamics
pub const MIN_VELOCITY: i16 = 15;

pub const VELOCITY_INCREMENT: i16 = 16;

pub const FORTE: i16 = MIN_VELOCITY + VELOCITY_INCREMENT * 5;

pub const DEFAULT_VELOCITY: i16 = FORTE;

/// A grace note effect
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraceEffect {
    pub duration: u8,
    pub fret: i8,
    pub is_dead: bool,
    pub is_on_beat: bool,
    pub transition: GraceEffectTransition,
    pub velocity: i16,
}

impl Default for GraceEffect {
    fn default() -> Self {
        GraceEffect {
            duration: 1,
            fret: 0,
            is_dead: false,
            is_on_beat: false,
            transition: GraceEffectTransition::None,
            velocity: DEFAULT_VELOCITY,
        }
    }
}

/// A harmonic note effect
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarmonicEffect {
    pub kind: HarmonicType,
    pub pitch: Option<PitchClass>,
    pub octave: Option<Octave>,
    pub fret: Option<i8>,
}

impl Default for HarmonicEffect {
    fn default() -> Self {
        HarmonicEffect {
            kind: HarmonicType::Natural,
            pitch: None,
            octave: None,
            fret: None,
        }
    }
}

/// A tremolo picking effect.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TremoloPickingEffect {
    pub duration: Duration,
}

/// A trill effect.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrillEffect {
    pub fret: i8,
    pub duration: Duration,
}
