//! Value conversions for the GPIF export: ids, XML escaping, dynamics, note values,
//! slides, harmonics, directions.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use super::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// GuitarScore addition: returns the counter's current id and advances it.
pub(super) fn next_id(counter: &mut i32) -> GpResult<i32> {
    let id = *counter;
    *counter = counter
        .checked_add(1)
        .ok_or_else(|| GpError::FormatError("too many elements to number".to_owned()))?;
    Ok(id)
}

/// GuitarScore addition: the id of a track's bar for a measure; bars are numbered track by
/// track (`track * measure_count + measure`).
pub(super) fn bar_id(track: usize, measure_count: usize, measure: usize) -> GpResult<i32> {
    track
        .checked_mul(measure_count)
        .and_then(|first| first.checked_add(measure))
        .and_then(|id| i32::try_from(id).ok())
        .ok_or_else(|| GpError::FormatError("too many bars to number".to_owned()))
}

pub(super) fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(super) fn velocity_to_dynamic(velocity: i16) -> &'static str {
    let levels: [(i16, &str); 8] = [
        (MIN_VELOCITY, "PPP"),
        (MIN_VELOCITY + VELOCITY_INCREMENT, "PP"),
        (MIN_VELOCITY + VELOCITY_INCREMENT * 2, "P"),
        (MIN_VELOCITY + VELOCITY_INCREMENT * 3, "MP"),
        (MIN_VELOCITY + VELOCITY_INCREMENT * 4, "MF"),
        (FORTE, "F"),
        (MIN_VELOCITY + VELOCITY_INCREMENT * 6, "FF"),
        (MIN_VELOCITY + VELOCITY_INCREMENT * 7, "FFF"),
    ];
    let mut best = "F";
    let mut best_diff = u16::MAX;
    for (v, name) in &levels {
        let diff = velocity.abs_diff(*v);
        if diff < best_diff {
            best_diff = diff;
            best = name;
        }
    }
    best
}

pub(super) fn note_value_str(value: u16) -> &'static str {
    match value {
        1 => "Whole",
        2 => "Half",
        4 => "Quarter",
        8 => "Eighth",
        16 => "16th",
        32 => "32nd",
        64 => "64th",
        128 => "128th",
        _ => "Quarter",
    }
}

pub(super) fn slide_types_to_flags(slides: &[SlideType]) -> i32 {
    let mut flags = 0i32;
    for s in slides {
        flags |= match s {
            SlideType::ShiftSlideTo => 0x01,
            SlideType::LegatoSlideTo => 0x02,
            SlideType::OutDownwards => 0x04,
            SlideType::OutUpWards => 0x08,
            SlideType::IntoFromBelow => 0x10,
            SlideType::IntoFromAbove => 0x20,
            SlideType::PickSlideDown => 0x40,
            SlideType::PickSlideUp => 0x80,
            SlideType::None => 0,
        };
    }
    flags
}

pub(super) fn harmonic_type_str(kind: &HarmonicType) -> &'static str {
    match kind {
        HarmonicType::Natural => "Natural",
        HarmonicType::Artificial => "Artificial",
        HarmonicType::Pinch => "Pinch",
        HarmonicType::Tapped => "Tap",
        HarmonicType::Semi => "Semi",
    }
}

pub(super) fn direction_sign_str(d: &DirectionSign) -> &'static str {
    match d {
        DirectionSign::Coda => "Coda",
        DirectionSign::DoubleCoda => "DoubleCoda",
        DirectionSign::Segno => "Segno",
        DirectionSign::SegnoSegno => "SegnoSegno",
        DirectionSign::Fine => "Fine",
        DirectionSign::DaCapo => "DaCapo",
        DirectionSign::DaCapoAlCoda => "DaCapoAlCoda",
        DirectionSign::DaCapoAlDoubleCoda => "DaCapoAlDoubleCoda",
        DirectionSign::DaCapoAlFine => "DaCapoAlFine",
        DirectionSign::DaSegno => "DaSegno",
        DirectionSign::DaSegnoAlCoda => "DaSegnoAlCoda",
        DirectionSign::DaSegnoAlDoubleCoda => "DaSegnoAlDoubleCoda",
        DirectionSign::DaSegnoAlFine => "DaSegnoAlFine",
        DirectionSign::DaSegnoSegno => "DaSegnoSegno",
        DirectionSign::DaSegnoSegnoAlCoda => "DaSegnoSegnoAlCoda",
        DirectionSign::DaSegnoSegnoAlDoubleCoda => "DaSegnoSegnoAlDoubleCoda",
        DirectionSign::DaSegnoSegnoAlFine => "DaSegnoSegnoAlFine",
        DirectionSign::DaCoda => "DaCoda",
        DirectionSign::DaDoubleCoda => "DaDoubleCoda",
    }
}
