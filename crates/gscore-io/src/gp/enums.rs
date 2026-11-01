//! Guitar Pro reading and writing: enums.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see LICENSE-guitarpro next to this file). Changes: docs/guitarpro.md.

use crate::gp::*;

pub(crate) fn get_beat_status(value: u8) -> BeatStatus {
    match value {
        0 => BeatStatus::Empty,
        1 => BeatStatus::Normal,
        2 => BeatStatus::Rest,
        _ => BeatStatus::Normal,
    }
}

pub(crate) fn from_beat_status(value: &BeatStatus) -> u8 {
    match value {
        BeatStatus::Empty => 0,
        BeatStatus::Normal => 1,
        BeatStatus::Rest => 2,
    }
}

pub(crate) fn get_octave(value: u8) -> GpResult<Octave> {
    match value {
        0 => Ok(Octave::None),
        1 => Ok(Octave::Ottava),
        2 => Ok(Octave::Quindicesima),
        3 => Ok(Octave::OttavaBassa),
        4 => Ok(Octave::QuindicesimaBassa),
        _ => Err(GpError::InvalidValue {
            context: "octave",
            value: value as i64,
        }),
    }
}

pub(crate) fn from_octave(value: &Octave) -> u8 {
    match value {
        Octave::None => 0,
        Octave::Ottava => 1,
        Octave::Quindicesima => 2,
        Octave::OttavaBassa => 3,
        Octave::QuindicesimaBassa => 4,
    }
}

pub(crate) fn get_beat_stroke_direction(value: i8) -> GpResult<BeatStrokeDirection> {
    match value {
        0 => Ok(BeatStrokeDirection::None),
        1 => Ok(BeatStrokeDirection::Up),
        2 => Ok(BeatStrokeDirection::Down),
        _ => Ok(BeatStrokeDirection::None),
    }
}

pub(crate) fn from_beat_stroke_direction(value: &BeatStrokeDirection) -> i8 {
    match value {
        BeatStrokeDirection::None => 0,
        BeatStrokeDirection::Up => 1,
        BeatStrokeDirection::Down => 2,
    }
}

pub(crate) fn get_slap_effect(value: u8) -> GpResult<SlapEffect> {
    match value {
        0 => Ok(SlapEffect::None),
        1 => Ok(SlapEffect::Tapping),
        2 => Ok(SlapEffect::Slapping),
        3 => Ok(SlapEffect::Popping),
        _ => Ok(SlapEffect::None),
    }
}

pub(crate) fn from_slap_effect(value: &SlapEffect) -> u8 {
    match value {
        SlapEffect::None => 0,
        SlapEffect::Tapping => 1,
        SlapEffect::Slapping => 2,
        SlapEffect::Popping => 3,
    }
}

pub(crate) fn get_chord_type(value: u8) -> ChordType {
    match value {
        0 => ChordType::Major,
        1 => ChordType::Seventh,
        2 => ChordType::MajorSeventh,
        3 => ChordType::Sixth,
        4 => ChordType::Minor,
        5 => ChordType::MinorSeventh,
        6 => ChordType::MinorMajor,
        7 => ChordType::MinorSixth,
        8 => ChordType::SuspendedSecond,
        9 => ChordType::SuspendedFourth,
        10 => ChordType::SeventhSuspendedSecond,
        11 => ChordType::SeventhSuspendedFourth,
        12 => ChordType::Diminished,
        13 => ChordType::Augmented,
        14 => ChordType::Power,
        _ => ChordType::Unknown(value),
    }
}

pub(crate) fn from_chord_type(value: &ChordType) -> u8 {
    match value {
        ChordType::Major => 0,
        ChordType::Seventh => 1,
        ChordType::MajorSeventh => 2,
        ChordType::Sixth => 3,
        ChordType::Minor => 4,
        ChordType::MinorSeventh => 5,
        ChordType::MinorMajor => 6,
        ChordType::MinorSixth => 7,
        ChordType::SuspendedSecond => 8,
        ChordType::SuspendedFourth => 9,
        ChordType::SeventhSuspendedSecond => 10,
        ChordType::SeventhSuspendedFourth => 11,
        ChordType::Diminished => 12,
        ChordType::Augmented => 13,
        ChordType::Power => 14,
        ChordType::Unknown(value) => *value,
    }
}

pub(crate) fn get_chord_alteration(value: u8) -> GpResult<ChordAlteration> {
    match value {
        0 => Ok(ChordAlteration::Perfect),
        1 => Ok(ChordAlteration::Diminished),
        2 => Ok(ChordAlteration::Augmented),
        _ => Ok(ChordAlteration::Perfect),
    }
}

pub(crate) fn from_chord_alteration(value: &ChordAlteration) -> u8 {
    match value {
        ChordAlteration::Perfect => 0,
        ChordAlteration::Diminished => 1,
        ChordAlteration::Augmented => 2,
    }
}

pub(crate) fn get_chord_extension(value: u8) -> ChordExtension {
    match value {
        0 => ChordExtension::None,
        1 => ChordExtension::Ninth,
        2 => ChordExtension::Eleventh,
        3 => ChordExtension::Thirteenth,
        _ => ChordExtension::Unknown(value),
    }
}

pub(crate) fn from_chord_extension(value: &ChordExtension) -> u8 {
    match value {
        ChordExtension::None => 0,
        ChordExtension::Ninth => 1,
        ChordExtension::Eleventh => 2,
        ChordExtension::Thirteenth => 3,
        ChordExtension::Unknown(value) => *value,
    }
}

pub(crate) fn get_bend_type(value: i8) -> GpResult<BendType> {
    match value {
        0 => Ok(BendType::None),
        1 => Ok(BendType::Bend),
        2 => Ok(BendType::BendRelease),
        3 => Ok(BendType::BendReleaseBend),
        4 => Ok(BendType::Prebend),
        5 => Ok(BendType::PrebendRelease),
        6 => Ok(BendType::Dip),
        7 => Ok(BendType::Dive),
        8 => Ok(BendType::ReleaseUp),
        9 => Ok(BendType::InvertedDip),
        10 => Ok(BendType::Return),
        11 => Ok(BendType::ReleaseDown),
        _ => Ok(BendType::None),
    }
}

pub(crate) fn from_bend_type(value: &BendType) -> i8 {
    match value {
        BendType::None => 0,
        BendType::Bend => 1,
        BendType::BendRelease => 2,
        BendType::BendReleaseBend => 3,
        BendType::Prebend => 4,
        BendType::PrebendRelease => 5,
        BendType::Dip => 6,
        BendType::Dive => 7,
        BendType::ReleaseUp => 8,
        BendType::InvertedDip => 9,
        BendType::Return => 10,
        BendType::ReleaseDown => 11,
    }
}

pub(crate) fn get_grace_effect_transition(value: i8) -> GpResult<GraceEffectTransition> {
    match value {
        0 => Ok(GraceEffectTransition::None),
        1 => Ok(GraceEffectTransition::Slide),
        2 => Ok(GraceEffectTransition::Bend),
        3 => Ok(GraceEffectTransition::Hammer),
        _ => Ok(GraceEffectTransition::None),
    }
}

pub(crate) fn from_grace_effect_transition(value: &GraceEffectTransition) -> i8 {
    match value {
        GraceEffectTransition::None => 0,
        GraceEffectTransition::Slide => 1,
        GraceEffectTransition::Bend => 2,
        GraceEffectTransition::Hammer => 3,
    }
}

pub(crate) fn from_harmonic_type(value: &HarmonicType) -> i8 {
    match value {
        HarmonicType::Natural => 1,
        HarmonicType::Artificial => 2,
        HarmonicType::Tapped => 3,
        HarmonicType::Pinch => 4,
        HarmonicType::Semi => 5,
    }
}

// Some GP5 exports write non-canonical bytes here; Guitar Pro itself falls back
// to `None`, so we accept any value instead of erroring.
pub(crate) fn get_triplet_feel(value: u8) -> TripletFeel {
    match value {
        1 => TripletFeel::Eighth,
        2 => TripletFeel::Sixteenth,
        _ => TripletFeel::None,
    }
}

pub(crate) fn from_triplet_feel(value: &TripletFeel) -> u8 {
    match value {
        TripletFeel::None => 0,
        TripletFeel::Eighth => 1,
        TripletFeel::Sixteenth => 2,
    }
}

pub(crate) fn get_line_break(value: u8) -> LineBreak {
    match value {
        1 => LineBreak::Break,
        2 => LineBreak::Protect,
        _ => LineBreak::None,
    }
}

pub(crate) fn from_line_break(value: &LineBreak) -> u8 {
    match value {
        LineBreak::None => 0,
        LineBreak::Break => 1,
        LineBreak::Protect => 2,
    }
}

pub(crate) fn get_slide_type(value: i8) -> GpResult<SlideType> {
    // Some legacy exports write junk here when no slide is active. Guitar Pro
    // itself treats unknown values as "no slide", so we do the same rather
    // than rejecting the file.
    Ok(match value {
        -2 => SlideType::IntoFromAbove,
        -1 => SlideType::IntoFromBelow,
        1 => SlideType::ShiftSlideTo,
        2 => SlideType::LegatoSlideTo,
        3 => SlideType::OutDownwards,
        4 => SlideType::OutUpWards,
        _ => SlideType::None,
    })
}

pub(crate) fn from_slide_type(value: &SlideType) -> i8 {
    match value {
        SlideType::IntoFromAbove => -2,
        SlideType::IntoFromBelow => -1,
        SlideType::None => 0,
        SlideType::ShiftSlideTo => 1,
        SlideType::LegatoSlideTo => 2,
        SlideType::OutDownwards => 3,
        SlideType::OutUpWards => 4,
    }
}

pub(crate) fn get_note_type(value: u8) -> NoteType {
    match value {
        0 => NoteType::Rest,
        1 => NoteType::Normal,
        2 => NoteType::Tie,
        3 => NoteType::Dead,
        _ => NoteType::Unknown(value),
    }
}

pub(crate) fn from_note_type(value: &NoteType) -> u8 {
    match value {
        NoteType::Rest => 0,
        NoteType::Normal => 1,
        NoteType::Tie => 2,
        NoteType::Dead => 3,
        NoteType::Unknown(value) => *value,
    }
}

pub(crate) fn get_fingering(value: i8) -> Fingering {
    match value {
        -1 => Fingering::Open,
        0 => Fingering::Thumb,
        1 => Fingering::Index,
        2 => Fingering::Middle,
        3 => Fingering::Annular,
        4 => Fingering::Little,
        _ => Fingering::Unknown(value),
    }
}

pub(crate) fn from_fingering(value: &Fingering) -> i8 {
    match value {
        Fingering::Open => -1,
        Fingering::Thumb => 0,
        Fingering::Index => 1,
        Fingering::Middle => 2,
        Fingering::Annular => 3,
        Fingering::Little => 4,
        Fingering::Unknown(value) => *value,
    }
}

pub(crate) fn get_accentuation(value: u8) -> GpResult<Accentuation> {
    match value {
        0 => Ok(Accentuation::None),
        1 => Ok(Accentuation::VerySoft),
        2 => Ok(Accentuation::Soft),
        3 => Ok(Accentuation::Medium),
        4 => Ok(Accentuation::Strong),
        5 => Ok(Accentuation::VeryStrong),
        _ => Err(GpError::InvalidValue {
            context: "accentuation",
            value: value as i64,
        }),
    }
}

pub(crate) fn from_accentuation(value: &Accentuation) -> u8 {
    match value {
        Accentuation::None => 0,
        Accentuation::VerySoft => 1,
        Accentuation::Soft => 2,
        Accentuation::Medium => 3,
        Accentuation::Strong => 4,
        Accentuation::VeryStrong => 5,
    }
}
