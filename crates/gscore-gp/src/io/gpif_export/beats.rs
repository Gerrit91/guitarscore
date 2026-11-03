//! Writing GPIF beats.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use super::*;

// ---------------------------------------------------------------------------
// Beat export
// ---------------------------------------------------------------------------

/// One beat as GPIF XML: the beat, its rhythm and its notes.
pub(super) struct BeatExport {
    pub(super) beat_xml: String,
    pub(super) rhythm_xml: String,
    pub(super) note_xmls: Vec<String>,
}

pub(super) fn build_beat_xml(
    beat: &Beat,
    beat_id: i32,
    rhythm_id: i32,
    next_note_id: &mut i32,
    prev_velocity: &mut i16,
) -> GpResult<BeatExport> {
    let mut note_xmls: Vec<String> = Vec::new();
    let mut note_ids: Vec<i32> = Vec::new();

    let is_grace_beat = beat.grace;

    // Build note XMLs
    if beat.status == BeatStatus::Normal {
        for note in &beat.notes {
            let note_id = next_id(next_note_id)?;
            note_ids.push(note_id);
            note_xmls.push(build_note_xml(note, note_id));
        }
    }

    // Determine beat velocity (from first note)
    let beat_velocity = beat
        .notes
        .first()
        .map(|n| n.velocity)
        .unwrap_or(*prev_velocity);

    let mut bxml = format!("<Beat id=\"{beat_id}\">");

    // Grace notes marker
    if is_grace_beat {
        let on_beat = beat
            .notes
            .first()
            .and_then(|n| n.effect.grace.as_ref())
            .map(|g| g.is_on_beat)
            .unwrap_or(false);
        if on_beat {
            bxml.push_str("<GraceNotes>OnBeat</GraceNotes>");
        } else {
            bxml.push_str("<GraceNotes>BeforeBeat</GraceNotes>");
        }
    }

    // Notes reference
    if !note_ids.is_empty() {
        let ids_str: Vec<String> = note_ids.iter().map(|id| id.to_string()).collect();
        bxml.push_str(&format!("<Notes>{}</Notes>", ids_str.join(" ")));
    }

    // Dynamic (only when velocity changes)
    if beat_velocity != *prev_velocity {
        bxml.push_str(&format!(
            "<Dynamic>{}</Dynamic>",
            velocity_to_dynamic(beat_velocity)
        ));
        *prev_velocity = beat_velocity;
    }

    // Free text
    if !beat.text.is_empty() {
        bxml.push_str(&format!("<FreeText>{}</FreeText>", escape_xml(&beat.text)));
    }

    // Fade in
    if beat.effect.fade_in {
        bxml.push_str("<Fadding>FadeIn</Fadding>");
    }

    // Beat-level legato (GP6/GP7 `<Legato>`).
    let legato = beat.effect.legato;
    if legato.origin || legato.destination {
        bxml.push_str(&format!(
            "<Legato origin=\"{}\" destination=\"{}\" />",
            legato.origin, legato.destination
        ));
    }

    // Tremolo bar. GuitarScore fix: written as GP7's <Whammy> with origin, middle and
    // destination (the importer reads it back); the legacy <Tremolo> only held the
    // destination, so a dip (down and back up) was lost.
    if let Some(trem) = &beat.effect.tremolo_bar
        && let (Some(first), Some(last)) = (trem.points.first(), trem.points.last())
    {
        let middle = trem.points.get(trem.points.len() / 2).unwrap_or(first);
        let value = |p: &BendPoint| f64::from(p.value) * f64::from(GP_BEND_SEMITONE);
        let offset =
            |p: &BendPoint| f64::from(p.position) * 100.0 / f64::from(BEND_EFFECT_MAX_POSITION);
        bxml.push_str(&format!(
            "<Whammy originValue=\"{:.6}\" middleValue=\"{:.6}\" destinationValue=\"{:.6}\" \
             originOffset=\"{:.6}\" middleOffset1=\"{:.6}\" middleOffset2=\"{:.6}\" \
             destinationOffset=\"{:.6}\"/>",
            value(first),
            value(middle),
            value(last),
            offset(first),
            offset(middle),
            offset(middle),
            offset(last),
        ));
    }

    // Beat properties (Brush, Rasgueado, PickStroke, VibratoWTremBar)
    let has_brush = !matches!(beat.effect.stroke.direction, BeatStrokeDirection::None);
    let has_pick = !matches!(beat.effect.pick_stroke, BeatStrokeDirection::None);
    let has_rasg = beat.effect.has_rasgueado;
    let has_vibrato = beat.effect.vibrato;

    if has_brush || has_pick || has_rasg || has_vibrato {
        bxml.push_str("<Properties>");
        if has_brush {
            let dir = if matches!(beat.effect.stroke.direction, BeatStrokeDirection::Down) {
                "Down"
            } else {
                "Up"
            };
            bxml.push_str(&format!(
                "<Property name=\"Brush\"><Direction>{dir}</Direction></Property>"
            ));
        }
        if has_pick {
            let dir = if matches!(beat.effect.pick_stroke, BeatStrokeDirection::Down) {
                "Down"
            } else {
                "Up"
            };
            bxml.push_str(&format!(
                "<Property name=\"PickStroke\"><Direction>{dir}</Direction></Property>"
            ));
        }
        if has_rasg {
            bxml.push_str("<Property name=\"Rasgueado\"><Enable /></Property>");
        }
        if has_vibrato {
            let strength = if beat.effect.wide_vibrato {
                "Wide"
            } else {
                "Slight"
            };
            bxml.push_str(&format!(
                "<Property name=\"VibratoWTremBar\"><Strength>{strength}</Strength></Property>"
            ));
        }
        bxml.push_str("</Properties>");
    }

    // Rhythm reference
    bxml.push_str(&format!("<Rhythm ref=\"{rhythm_id}\"/>"));

    bxml.push_str("</Beat>");

    // Rhythm XML
    let dur = &beat.duration;
    let mut rxml = format!(
        "<Rhythm id=\"{rhythm_id}\"><NoteValue>{}</NoteValue>",
        note_value_str(dur.value)
    );
    if dur.dotted {
        rxml.push_str("<AugmentationDot count=\"1\"/>");
    } else if dur.double_dotted {
        rxml.push_str("<AugmentationDot count=\"2\"/>");
    }
    if dur.tuplet_enters != 1 || dur.tuplet_times != 1 {
        rxml.push_str(&format!(
            "<PrimaryTuplet num=\"{}\" den=\"{}\"/>",
            dur.tuplet_enters, dur.tuplet_times
        ));
    }
    rxml.push_str("</Rhythm>");

    Ok(BeatExport {
        beat_xml: bxml,
        rhythm_xml: rxml,
        note_xmls,
    })
}
