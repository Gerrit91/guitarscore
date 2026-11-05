//! Value conversions for the GPIF import: note values, dynamics, ids, slides, harmonics,
//! directions, bends and whammy, tunings.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use super::*;

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Extract the tempo number from a Tempo automation's `<Value>`, e.g. the
/// `125` in `<Value>125 2</Value>`. The value may be plain text (`Tempo`) or a
/// structured element (`SyncPoint` wraps `<BarIndex>`, `<ModifiedTempo>`, …);
/// [`AutomationValue`] collects both shapes into a list of text items.
pub(super) fn automation_tempo(auto: &Automation) -> Option<&str> {
    auto.value
        .as_ref()
        .and_then(|v| v.items.first())
        .and_then(|s| s.split_whitespace().next())
}

/// Convert GPIF note value string to Duration.value.
/// Falls back to Quarter (4) for unknown values.
pub(super) fn note_value_to_duration(s: &str) -> u16 {
    match s {
        "Whole" => 1,
        "Half" => 2,
        "Quarter" => 4,
        "Eighth" => 8,
        "16th" => 16,
        "32nd" => 32,
        "64th" => 64,
        "128th" => 128,
        _ => 4,
    }
}

/// Convert GPIF dynamic string to MIDI velocity
pub(super) fn dynamic_to_velocity(s: &str) -> i16 {
    match s {
        "PPP" => MIN_VELOCITY,
        "PP" => MIN_VELOCITY + VELOCITY_INCREMENT,
        "P" => MIN_VELOCITY + VELOCITY_INCREMENT * 2,
        "MP" => MIN_VELOCITY + VELOCITY_INCREMENT * 3,
        "MF" => MIN_VELOCITY + VELOCITY_INCREMENT * 4,
        "F" => FORTE,
        "FF" => MIN_VELOCITY + VELOCITY_INCREMENT * 6,
        "FFF" => MIN_VELOCITY + VELOCITY_INCREMENT * 7,
        _ => FORTE,
    }
}

/// Parse space-separated integer IDs from a string.
pub(super) fn parse_ids(s: &str) -> Vec<i32> {
    s.split_whitespace()
        .filter_map(|tok| tok.parse::<i32>().ok())
        .collect()
}

/// Parse slide flags bitmask into a list of `SlideType` values.
///
/// Uses the same encoding as GP5 binary format:
/// - bit 0 (0x01): Shift slide to next note
/// - bit 1 (0x02): Legato slide to next note
/// - bit 2 (0x04): Slide out downwards
/// - bit 3 (0x08): Slide out upwards
/// - bit 4 (0x10): Slide in from below
/// - bit 5 (0x20): Slide in from above
pub(super) fn parse_slide_flags(flags: i32) -> Vec<SlideType> {
    let mut v = Vec::with_capacity(6);
    if (flags & 0x01) != 0 {
        v.push(SlideType::ShiftSlideTo);
    }
    if (flags & 0x02) != 0 {
        v.push(SlideType::LegatoSlideTo);
    }
    if (flags & 0x04) != 0 {
        v.push(SlideType::OutDownwards);
    }
    if (flags & 0x08) != 0 {
        v.push(SlideType::OutUpWards);
    }
    if (flags & 0x10) != 0 {
        v.push(SlideType::IntoFromBelow);
    }
    if (flags & 0x20) != 0 {
        v.push(SlideType::IntoFromAbove);
    }
    // GuitarScore addition: pick slides (pick scrapes).
    if (flags & 0x40) != 0 {
        v.push(SlideType::PickSlideDown);
    }
    if (flags & 0x80) != 0 {
        v.push(SlideType::PickSlideUp);
    }
    v
}

/// Parse a GPIF harmonic type string (e.g. "Natural", "Artificial", "Pinch")
/// into a `HarmonicEffect`. Falls back to `Natural` for unrecognised values.
/// "Feedback" is mapped to `Pinch` as Guitar Pro treats them equivalently.
pub(super) fn parse_harmonic_type(htype: &str) -> HarmonicEffect {
    let kind = match htype {
        "Natural" => HarmonicType::Natural,
        "Artificial" => HarmonicType::Artificial,
        "Pinch" => HarmonicType::Pinch,
        "Tap" | "Tapped" => HarmonicType::Tapped,
        "Semi" => HarmonicType::Semi,
        "Feedback" => HarmonicType::Pinch,
        _ => HarmonicType::Natural,
    };
    HarmonicEffect {
        kind,
        ..Default::default()
    }
}

/// Parse direction string to DirectionSign enum.
pub(super) fn parse_direction_sign(s: &str) -> Option<DirectionSign> {
    match s {
        "Coda" => Some(DirectionSign::Coda),
        "DoubleCoda" => Some(DirectionSign::DoubleCoda),
        "Segno" => Some(DirectionSign::Segno),
        "SegnoSegno" => Some(DirectionSign::SegnoSegno),
        "Fine" => Some(DirectionSign::Fine),
        "DaCapo" => Some(DirectionSign::DaCapo),
        "DaCapoAlCoda" => Some(DirectionSign::DaCapoAlCoda),
        "DaCapoAlDoubleCoda" => Some(DirectionSign::DaCapoAlDoubleCoda),
        "DaCapoAlFine" => Some(DirectionSign::DaCapoAlFine),
        "DaSegno" => Some(DirectionSign::DaSegno),
        "DaSegnoAlCoda" => Some(DirectionSign::DaSegnoAlCoda),
        "DaSegnoAlDoubleCoda" => Some(DirectionSign::DaSegnoAlDoubleCoda),
        "DaSegnoAlFine" => Some(DirectionSign::DaSegnoAlFine),
        "DaSegnoSegno" => Some(DirectionSign::DaSegnoSegno),
        "DaSegnoSegnoAlCoda" => Some(DirectionSign::DaSegnoSegnoAlCoda),
        "DaSegnoSegnoAlDoubleCoda" => Some(DirectionSign::DaSegnoSegnoAlDoubleCoda),
        "DaSegnoSegnoAlFine" => Some(DirectionSign::DaSegnoSegnoAlFine),
        "DaCoda" => Some(DirectionSign::DaCoda),
        "DaDoubleCoda" => Some(DirectionSign::DaDoubleCoda),
        _ => None,
    }
}

/// Build bend effect from GPIF origin/destination values (float, in 1/100 semitone).
pub(super) fn build_bend_effect(origin: f64, destination: f64) -> BendEffect {
    let mut bend = BendEffect::default();
    let semitone = f64::from(GP_BEND_SEMITONE);
    let origin_val = round_f64_to_i8(origin / semitone);
    let dest_val = round_f64_to_i8(destination / semitone);

    if origin == 0.0 && destination > 0.0 {
        bend.kind = BendType::Bend;
    } else if origin > 0.0 && destination == 0.0 {
        bend.kind = BendType::ReleaseUp;
    } else if origin > 0.0 && destination > 0.0 {
        if destination > origin {
            bend.kind = BendType::Bend;
        } else if destination < origin {
            bend.kind = BendType::ReleaseUp;
        } else {
            bend.kind = BendType::Bend;
        }
    }

    bend.value = round_f64_to_i16(destination.max(origin) / semitone * 2.0);
    bend.points.push(BendPoint {
        position: 0,
        value: origin_val,
        vibrato: false,
    });
    bend.points.push(BendPoint {
        position: 6,
        value: i8::try_from(
            (i16::from(origin_val).saturating_add(i16::from(dest_val))).wrapping_div(2),
        )
        .unwrap_or_default(),
        vibrato: false,
    });
    bend.points.push(BendPoint {
        position: 12,
        value: dest_val,
        vibrato: false,
    });
    bend
}

/// GuitarScore addition: the three points of a GPIF bend or whammy shape (values in
/// hundredths of a semitone, offsets in percent of the note), in quarter tones and
/// positions 0-12. The middle is kept as stored, so a bend-and-release or a whammy dip
/// (origin = destination) survives; it used to be averaged away. `None` for a shape that
/// does not move from 0.
pub(super) fn shape_points(
    origin: f64,
    middle: Option<f64>,
    destination: f64,
    middle_offset: Option<f64>,
    destination_offset: Option<f64>,
) -> Option<Vec<BendPoint>> {
    let middle = middle.unwrap_or((origin + destination) / 2.0);
    if origin == 0.0 && middle == 0.0 && destination == 0.0 {
        return None;
    }
    let value = |hundredths: f64| round_f64_to_i8(hundredths / f64::from(GP_BEND_SEMITONE));
    let position = |percent: Option<f64>, default: f64| {
        let position = percent.unwrap_or(default).clamp(0.0, 100.0)
            * f64::from(BEND_EFFECT_MAX_POSITION)
            / 100.0;
        u8::try_from(round_f64_to_i32(position)).unwrap_or(BEND_EFFECT_MAX_POSITION)
    };
    let point = |position, value| BendPoint {
        position,
        value,
        vibrato: false,
    };
    Some(vec![
        point(0, value(origin)),
        point(position(middle_offset, 50.0), value(middle)),
        point(position(destination_offset, 100.0), value(destination)),
    ])
}

/// GuitarScore addition: a whammy-bar effect from its GPIF shape (see [`shape_points`]).
pub(crate) fn build_whammy_effect(
    origin: f64,
    middle: Option<f64>,
    destination: f64,
    middle_offset: Option<f64>,
    destination_offset: Option<f64>,
) -> Option<BendEffect> {
    let points = shape_points(
        origin,
        middle,
        destination,
        middle_offset,
        destination_offset,
    )?;
    let kind = if destination == origin {
        BendType::Dip
    } else if destination < origin {
        BendType::Dive
    } else {
        BendType::ReleaseUp
    };
    Some(BendEffect {
        kind,
        points,
        ..BendEffect::default()
    })
}

/// GuitarScore addition: a note bend from its GPIF shape (see [`shape_points`]). The kind
/// follows the shape: up (destination higher), release (lower), bend-and-release (back to
/// the origin through a higher middle), prebend (held above 0).
pub(crate) fn build_note_bend(
    origin: f64,
    middle: Option<f64>,
    destination: f64,
    middle_offset: Option<f64>,
    destination_offset: Option<f64>,
) -> Option<BendEffect> {
    let points = shape_points(
        origin,
        middle,
        destination,
        middle_offset,
        destination_offset,
    )?;
    let middle = middle.unwrap_or((origin + destination) / 2.0);
    let kind = if destination > origin {
        BendType::Bend
    } else if destination < origin {
        BendType::ReleaseUp
    } else if middle > origin {
        BendType::BendRelease
    } else if middle < origin {
        BendType::PrebendRelease
    } else {
        BendType::Prebend
    };
    let highest = origin.max(middle).max(destination);
    Some(BendEffect {
        kind,
        value: round_f64_to_i16(highest / f64::from(GP_BEND_SEMITONE) * 2.0),
        points,
        ..BendEffect::default()
    })
}

/// GuitarScore addition: the capo fret from a property list (`CapoFret`), if present.
/// Hostile values are clamped to the MIDI range instead of becoming an unplayable capo.
pub(super) fn extract_capo(properties: &[Property]) -> Option<i32> {
    properties
        .iter()
        .find(|p| p.name == "CapoFret")
        .and_then(|p| p.fret)
        .map(|capo| capo.clamp(0, 127))
}

/// GuitarScore addition: the channel's reverb and chorus sends (`GuitarScoreReverb`,
/// `GuitarScoreChorus`, MIDI values 0-127), if the file stores them. GP7 has no per-track
/// send, so only GuitarScore writes them; hostile values are clamped.
pub(super) fn extract_sends(properties: &[Property]) -> (Option<i8>, Option<i8>) {
    let send = |name: &str| {
        properties
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| p.number)
            .and_then(|value| i8::try_from(value.clamp(0, 127)).ok())
    };
    (send(REVERB_PROPERTY), send(CHORUS_PROPERTY))
}

/// GuitarScore addition: the track's symbol id, if the file stores one.
pub(crate) fn extract_symbol(properties: &[Property]) -> Option<String> {
    properties
        .iter()
        .find(|p| p.name == SYMBOL_PROPERTY)
        .and_then(|p| p.text.as_deref())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// The track property holding the app's instrument symbol (a GuitarScore addition).
pub(crate) const SYMBOL_PROPERTY: &str = "GuitarScoreSymbol";

/// GuitarScore addition: whether the track shows slash notation, from a GuitarScore
/// property (GP7 keeps the same switch in `PartConfiguration` bit 2, which wins when the
/// file has one).
pub(crate) fn extract_slash(properties: &[Property]) -> Option<bool> {
    properties
        .iter()
        .find(|p| p.name == SLASH_PROPERTY)
        .and_then(|p| p.number)
        .map(|value| value != 0)
}

/// The track property holding the slash-notation switch (a GuitarScore addition).
pub(crate) const SLASH_PROPERTY: &str = "GuitarScoreSlash";

/// The track property holding the reverb send (a GuitarScore addition).
pub(crate) const REVERB_PROPERTY: &str = "GuitarScoreReverb";
/// The track property holding the chorus send (a GuitarScore addition).
pub(crate) const CHORUS_PROPERTY: &str = "GuitarScoreChorus";

/// GuitarScore addition: the user-given tuning name from a property list, if non-empty.
pub(super) fn extract_tuning_label(properties: &[Property]) -> Option<String> {
    properties
        .iter()
        .find(|p| p.name == "Tuning")
        .and_then(|p| p.label.as_deref())
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_owned)
}

/// Extract tuning pitches from a property list.
///
/// GuitarScore fix: GPIF lists pitches from the lowest string up, while the legacy model
/// numbers strings from the highest (string 1) down, so the list is reversed.
pub(super) fn extract_tuning(properties: &[Property]) -> Vec<(i8, i8)> {
    for prop in properties {
        if prop.name == "Tuning"
            && let Some(pitches_str) = &prop.pitches
        {
            let pitches: Vec<i8> = pitches_str
                .split_whitespace()
                .filter_map(|s| s.parse::<i8>().ok())
                .collect();
            return pitches
                .iter()
                .rev()
                .enumerate()
                .map(|(i, &pitch)| (i8::try_from(i.saturating_add(1)).unwrap_or(i8::MAX), pitch))
                .collect();
        }
    }
    Vec::new()
}
