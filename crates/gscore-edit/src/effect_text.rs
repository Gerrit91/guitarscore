//! The text form of a note or beat effect for command lines and the recovery journal:
//! an effect id (`hammer`, `bend`, ...) and a value (`true`, `heavy`, `up:0:0,6:4,12:0`).

use gscore_model::{
    AccentKind, BeatEffect, BendKind, BendPoint, BendShape, Harmonic, HarmonicKind, Legato,
    NoteEffect, SlapKind, SlideKind, StrokeDirection, Vibrato, WhammyShape,
};

/// How a note effect is written: its id and its value.
pub(crate) fn note_effect_text(effect: &NoteEffect) -> (&'static str, String) {
    match effect {
        NoteEffect::Hammer(on) => ("hammer", on.to_string()),
        NoteEffect::Bend(None) => ("bend", "none".to_owned()),
        NoteEffect::Bend(Some(shape)) => ("bend", shape_text(shape)),
        NoteEffect::Slide(kind, on) => ("slide", format!("{}:{on}", slide_id(*kind))),
        NoteEffect::Harmonic(None) => ("harmonic", "none".to_owned()),
        NoteEffect::Harmonic(Some(harmonic)) => (
            "harmonic",
            match harmonic.node {
                Some(node) => format!("{}:{node}", harmonic_id(harmonic.kind)),
                None => harmonic_id(harmonic.kind).to_owned(),
            },
        ),
        NoteEffect::Ghost(on) => ("ghost", on.to_string()),
        NoteEffect::Accent(None) => ("accent", "none".to_owned()),
        NoteEffect::Accent(Some(accent)) => (
            "accent",
            match accent {
                AccentKind::Normal => "normal",
                AccentKind::Heavy => "heavy",
            }
            .to_owned(),
        ),
        NoteEffect::PalmMute(on) => ("palm_mute", on.to_string()),
        NoteEffect::LetRing(on) => ("let_ring", on.to_string()),
        NoteEffect::Staccato(on) => ("staccato", on.to_string()),
        NoteEffect::Trill(on) => ("trill", on.to_string()),
        NoteEffect::TremoloPicking(on) => ("tremolo_picking", on.to_string()),
        NoteEffect::Vibrato(None) => ("vibrato", "none".to_owned()),
        NoteEffect::Vibrato(Some(vibrato)) => ("vibrato", vibrato_id(*vibrato).to_owned()),
    }
}

/// How a beat effect is written: its id and its value.
pub(crate) fn beat_effect_text(effect: &BeatEffect) -> (&'static str, String) {
    match effect {
        BeatEffect::PickStroke(stroke) => ("pick_stroke", stroke_text(*stroke).to_owned()),
        BeatEffect::BrushStroke(stroke) => ("brush_stroke", stroke_text(*stroke).to_owned()),
        BeatEffect::Slap(None) => ("slap", "none".to_owned()),
        BeatEffect::Slap(Some(slap)) => ("slap", slap_id(*slap).to_owned()),
        BeatEffect::FadeIn(on) => ("fade_in", on.to_string()),
        BeatEffect::Whammy(None) => ("whammy", "none".to_owned()),
        BeatEffect::Whammy(Some(shape)) => ("whammy", points_text(&shape.points)),
        BeatEffect::Legato(None) => ("legato", "none".to_owned()),
        BeatEffect::Legato(Some(legato)) => (
            "legato",
            match (legato.origin, legato.destination) {
                (true, true) => "both",
                (true, false) => "origin",
                (false, true) => "destination",
                (false, false) => "none",
            }
            .to_owned(),
        ),
        BeatEffect::Vibrato(None) => ("vibrato", "none".to_owned()),
        BeatEffect::Vibrato(Some(vibrato)) => ("vibrato", vibrato_id(*vibrato).to_owned()),
    }
}

/// Reads a note effect from its id and value.
pub(crate) fn parse_note_effect(id: &str, value: &str) -> Result<NoteEffect, String> {
    Ok(match id {
        "hammer" => NoteEffect::Hammer(boolean(value)?),
        "bend" => NoteEffect::Bend(parse_shape(value, true)?),
        "slide" => {
            let (kind, on) = split_bool(value, "slide:true")?;
            NoteEffect::Slide(parse_slide(kind)?, on)
        }
        "harmonic" => NoteEffect::Harmonic(parse_harmonic(value)?),
        "ghost" => NoteEffect::Ghost(boolean(value)?),
        "accent" => NoteEffect::Accent(match value {
            "none" => None,
            "normal" => Some(AccentKind::Normal),
            "heavy" => Some(AccentKind::Heavy),
            _ => return Err("accent: normal, heavy or none".to_owned()),
        }),
        "palm_mute" => NoteEffect::PalmMute(boolean(value)?),
        "let_ring" => NoteEffect::LetRing(boolean(value)?),
        "staccato" => NoteEffect::Staccato(boolean(value)?),
        "trill" => NoteEffect::Trill(boolean(value)?),
        "tremolo_picking" => NoteEffect::TremoloPicking(boolean(value)?),
        "vibrato" => NoteEffect::Vibrato(match value {
            "none" => None,
            "slight" => Some(Vibrato::Slight),
            "wide" => Some(Vibrato::Wide),
            _ => return Err("vibrato: slight, wide or none".to_owned()),
        }),
        _ => return Err(format!("unknown note effect \"{id}\"")),
    })
}

/// Reads a beat effect from its id and value.
pub(crate) fn parse_beat_effect(id: &str, value: &str) -> Result<BeatEffect, String> {
    Ok(match id {
        "pick_stroke" => BeatEffect::PickStroke(parse_stroke(value)?),
        "brush_stroke" => BeatEffect::BrushStroke(parse_stroke(value)?),
        "slap" => BeatEffect::Slap(match value {
            "none" => None,
            "tapping" => Some(SlapKind::Tapping),
            "slapping" => Some(SlapKind::Slapping),
            "popping" => Some(SlapKind::Popping),
            _ => return Err("slap: tapping, slapping, popping or none".to_owned()),
        }),
        "fade_in" => BeatEffect::FadeIn(boolean(value)?),
        "whammy" => BeatEffect::Whammy(match value {
            "none" => None,
            _ => Some(WhammyShape::new(parse_points(value)?).map_err(|e| e.to_string())?),
        }),
        "legato" => BeatEffect::Legato(match value {
            "none" => None,
            "origin" => Some(Legato {
                origin: true,
                destination: false,
            }),
            "destination" => Some(Legato {
                origin: false,
                destination: true,
            }),
            "both" => Some(Legato {
                origin: true,
                destination: true,
            }),
            _ => return Err("legato: origin, destination, both or none".to_owned()),
        }),
        "vibrato" => BeatEffect::Vibrato(match value {
            "none" => None,
            "slight" => Some(Vibrato::Slight),
            "wide" => Some(Vibrato::Wide),
            _ => return Err("vibrato: slight, wide or none".to_owned()),
        }),
        _ => return Err(format!("unknown beat effect \"{id}\"")),
    })
}

fn bend_kind_id(kind: BendKind) -> &'static str {
    match kind {
        BendKind::Up => "up",
        BendKind::Release => "release",
    }
}

fn shape_text(shape: &BendShape) -> String {
    format!(
        "{}:{}",
        bend_kind_id(shape.kind),
        points_text(&shape.points)
    )
}

fn points_text(points: &[BendPoint]) -> String {
    points
        .iter()
        .map(|point| format!("{}:{}", point.position, point.value))
        .collect::<Vec<_>>()
        .join(",")
}

fn parse_shape(value: &str, with_kind: bool) -> Result<Option<BendShape>, String> {
    if value == "none" {
        return Ok(None);
    }
    let (kind, points) = if with_kind {
        let (kind, points) = value
            .split_once(':')
            .ok_or_else(|| "bend: kind:points, such as up:0:0,12:4".to_owned())?;
        let kind = match kind {
            "up" => BendKind::Up,
            "release" => BendKind::Release,
            _ => return Err("bend kind: up or release".to_owned()),
        };
        (kind, points)
    } else {
        (BendKind::Up, value)
    };
    let points = parse_points(points)?;
    BendShape::new(kind, points)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn parse_points(value: &str) -> Result<Vec<BendPoint>, String> {
    let mut points = Vec::new();
    for pair in value.split(',') {
        let (position, point_value) = pair
            .split_once(':')
            .ok_or_else(|| "a point is position:value, such as 6:4".to_owned())?;
        let position: u8 = position
            .parse()
            .map_err(|_| "a point position from 0 to 12".to_owned())?;
        let point_value: i8 = point_value
            .parse()
            .map_err(|_| "a point value in quarter tones, -48 to 48".to_owned())?;
        points.push(BendPoint {
            position,
            value: point_value,
        });
    }
    if points.is_empty() {
        return Err("a shape needs at least one point".to_owned());
    }
    Ok(points)
}

fn slide_id(kind: SlideKind) -> &'static str {
    match kind {
        SlideKind::IntoFromAbove => "into_from_above",
        SlideKind::IntoFromBelow => "into_from_below",
        SlideKind::ShiftSlideTo => "shift_slide_to",
        SlideKind::LegatoSlideTo => "legato_slide_to",
        SlideKind::OutDownwards => "out_downwards",
        SlideKind::OutUpwards => "out_upwards",
    }
}

fn parse_slide(id: &str) -> Result<SlideKind, String> {
    Ok(match id {
        "into_from_above" => SlideKind::IntoFromAbove,
        "into_from_below" => SlideKind::IntoFromBelow,
        "shift_slide_to" => SlideKind::ShiftSlideTo,
        "legato_slide_to" => SlideKind::LegatoSlideTo,
        "out_downwards" => SlideKind::OutDownwards,
        "out_upwards" => SlideKind::OutUpwards,
        _ => return Err(format!("unknown slide \"{id}\"")),
    })
}

fn harmonic_id(kind: HarmonicKind) -> &'static str {
    match kind {
        HarmonicKind::Natural => "natural",
        HarmonicKind::Artificial => "artificial",
        HarmonicKind::Tapped => "tapped",
        HarmonicKind::Pinch => "pinch",
        HarmonicKind::Semi => "semi",
    }
}

fn parse_harmonic(value: &str) -> Result<Option<Harmonic>, String> {
    if value == "none" {
        return Ok(None);
    }
    let (kind, node) = match value.split_once(':') {
        Some((kind, node)) => (kind, Some(node)),
        None => (value, None),
    };
    let kind = match kind {
        "natural" => HarmonicKind::Natural,
        "artificial" => HarmonicKind::Artificial,
        "tapped" => HarmonicKind::Tapped,
        "pinch" => HarmonicKind::Pinch,
        "semi" => HarmonicKind::Semi,
        _ => return Err(format!("unknown harmonic kind \"{kind}\"")),
    };
    let node = match node {
        Some(node) => Some(
            node.parse()
                .map_err(|_| "a harmonic node fret from 0 to 127".to_owned())?,
        ),
        None => None,
    };
    Ok(Some(Harmonic { kind, node }))
}

fn stroke_text(stroke: Option<StrokeDirection>) -> &'static str {
    match stroke {
        None => "none",
        Some(StrokeDirection::Up) => "up",
        Some(StrokeDirection::Down) => "down",
    }
}

fn parse_stroke(value: &str) -> Result<Option<StrokeDirection>, String> {
    Ok(match value {
        "none" => None,
        "up" => Some(StrokeDirection::Up),
        "down" => Some(StrokeDirection::Down),
        _ => return Err("stroke: up, down or none".to_owned()),
    })
}

fn slap_id(kind: SlapKind) -> &'static str {
    match kind {
        SlapKind::Tapping => "tapping",
        SlapKind::Slapping => "slapping",
        SlapKind::Popping => "popping",
    }
}

fn vibrato_id(kind: Vibrato) -> &'static str {
    match kind {
        Vibrato::Slight => "slight",
        Vibrato::Wide => "wide",
    }
}

fn boolean(value: &str) -> Result<bool, String> {
    match value {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        _ => Err("true or false".to_owned()),
    }
}

fn split_bool<'a>(value: &'a str, expected: &str) -> Result<(&'a str, bool), String> {
    let (name, on) = value
        .rsplit_once(':')
        .ok_or_else(|| format!("expected {expected}"))?;
    Ok((name, boolean(on)?))
}
