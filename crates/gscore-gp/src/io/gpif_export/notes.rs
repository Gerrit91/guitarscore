//! Writing GPIF notes.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use super::*;

// ---------------------------------------------------------------------------
// Note export
// ---------------------------------------------------------------------------

pub(super) fn build_note_xml(note: &Note, note_id: i32) -> String {
    let mut s = format!("<Note id=\"{note_id}\"><Properties>");

    s.push_str(&format!(
        "<Property name=\"Fret\"><Fret>{}</Fret></Property>",
        note.value
    ));
    // GPIF stores string index 0-based (top/high-pitched string = 0). Our
    // internal legacy `Note::string` is 1-based to match the model used by
    // GP3/4/5 binary readers and the MSCZ / MusicXML converters — subtract 1
    // on write so AlphaTab and other real GP7 tools see the standard values.
    // `saturating_sub` guards rest/tie sentinel `string == 0` from underflow.
    s.push_str(&format!(
        "<Property name=\"String\"><String>{}</String></Property>",
        note.string.saturating_sub(1)
    ));

    if note.effect.palm_mute {
        s.push_str("<Property name=\"PalmMuted\"><Enable /></Property>");
    }

    // GuitarScore fix: write the shape the importer reads: origin (first point), middle
    // (the point furthest from the origin) and destination (last point), with offsets.
    // Writing points[2] as the destination skipped bends with fewer points and turned a
    // bend-and-release into no bend at all.
    if let Some(bend) = &note.effect.bend
        && let (Some(first), Some(last)) = (bend.points.first(), bend.points.last())
    {
        let middle = bend
            .points
            .iter()
            .max_by_key(|p| i16::from(p.value).abs_diff(i16::from(first.value)))
            .unwrap_or(first);
        let value = |p: &BendPoint| f64::from(p.value) * f64::from(GP_BEND_SEMITONE);
        let offset =
            |p: &BendPoint| f64::from(p.position) * 100.0 / f64::from(BEND_EFFECT_MAX_POSITION);
        for (name, number) in [
            ("BendOriginValue", value(first)),
            ("BendMiddleValue", value(middle)),
            ("BendDestinationValue", value(last)),
            ("BendOriginOffset", offset(first)),
            ("BendMiddleOffset1", offset(middle)),
            ("BendMiddleOffset2", offset(middle)),
            ("BendDestinationOffset", offset(last)),
        ] {
            s.push_str(&format!(
                "<Property name=\"{name}\"><Float>{number:.6}</Float></Property>"
            ));
        }
    }

    if !note.effect.slides.is_empty() {
        let flags = slide_types_to_flags(&note.effect.slides);
        s.push_str(&format!(
            "<Property name=\"Slide\"><Flags>{flags}</Flags></Property>"
        ));
    }

    if let Some(harm) = &note.effect.harmonic {
        s.push_str(&format!(
            "<Property name=\"HarmonicType\"><HType>{}</HType></Property>",
            harmonic_type_str(&harm.kind)
        ));
        if let Some(fret) = harm.fret {
            s.push_str(&format!(
                "<Property name=\"HarmonicFret\"><HFret>{fret}</HFret></Property>"
            ));
        }
    }

    if note.effect.hammer {
        s.push_str("<Property name=\"HopoOrigin\"><Enable /></Property>");
    }

    if note.kind == NoteType::Dead {
        s.push_str("<Property name=\"Dead\"><Enable /></Property>");
    }

    s.push_str("</Properties>");

    if note.kind == NoteType::Tie {
        s.push_str("<Tie destination=\"true\" />");
    }
    if note.effect.vibrato {
        if note.effect.wide_vibrato {
            s.push_str("<Vibrato>Wide</Vibrato>");
        } else {
            s.push_str("<Vibrato>Slight</Vibrato>");
        }
    }
    if note.effect.let_ring {
        s.push_str("<LetRing />");
    }
    if note.effect.ghost_note {
        s.push_str("<AntiAccent>Normal</AntiAccent>");
    }

    let mut accent = 0i32;
    if note.effect.staccato {
        accent |= 0x01;
    }
    if note.effect.accentuated_note {
        accent |= 0x02;
    }
    if note.effect.heavy_accentuated_note {
        accent |= 0x04;
    }
    if accent != 0 {
        s.push_str(&format!("<Accent>{accent}</Accent>"));
    }

    if let Some(trill) = &note.effect.trill {
        s.push_str(&format!("<Trill>{}</Trill>", trill.fret));
    }
    if let Some(orn) = &note.effect.ornament {
        s.push_str(&format!("<Ornament>{}</Ornament>", escape_xml(orn)));
    }

    s.push_str("</Note>");
    s
}
