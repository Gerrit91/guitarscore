//! Converting GPIF notes and their effects.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use super::*;

/// A GPIF finger name as the legacy model's fingering.
fn parse_finger(name: &str) -> Option<Fingering> {
    Some(match name {
        "P" => Fingering::Thumb,
        "I" => Fingering::Index,
        "M" => Fingering::Middle,
        "A" => Fingering::Annular,
        "C" => Fingering::Little,
        "Open" => Fingering::OpenString,
        _ => return None,
    })
}

/// The GPIF name of a finger, if it has one.
pub(crate) fn finger_name(finger: &Fingering) -> Option<&'static str> {
    Some(match finger {
        Fingering::Thumb => "P",
        Fingering::Index => "I",
        Fingering::Middle => "M",
        Fingering::Annular => "A",
        Fingering::Little => "C",
        Fingering::OpenString => "Open",
        Fingering::Open | Fingering::Unknown(_) => return None,
    })
}

pub(super) fn convert_note(
    g_note: &Note,
    velocity: i16,
    is_grace_beat: bool,
    grace_on_beat: bool,
) -> SongNote {
    let mut s_note = SongNote {
        velocity,
        kind: NoteType::Normal,
        ..Default::default()
    };

    let mut bend_origin: Option<f64> = None;
    let mut bend_middle: Option<f64> = None;
    let mut bend_dest: Option<f64> = None;
    let mut bend_middle_offset: Option<f64> = None;
    let mut bend_dest_offset: Option<f64> = None;
    let mut harmonic_fret: Option<f64> = None;

    // GuitarScore addition: a GP6 drum note (kit element and variation, no fret).
    let property = |name: &str| {
        g_note
            .properties
            .properties
            .iter()
            .find(|prop| prop.name == name)
    };
    if let Some(element) = property("Element").and_then(|prop| prop.element)
        && let Some((articulation, string)) = gp6_drums::articulation(
            element,
            property("Variation")
                .and_then(|prop| prop.variation)
                .unwrap_or(0),
        )
    {
        s_note.value = articulation;
        s_note.string = string.saturating_add(1);
    }

    for prop in &g_note.properties.properties {
        match prop.name.as_str() {
            "Fret" => {
                if let Some(f) = prop.fret {
                    s_note.value = low_i16(f);
                }
            }
            "String" => {
                if let Some(s) = prop.string {
                    // GPIF stores string index 0-based; our internal legacy
                    // `Note::string` is 1-based (matches GP3/4/5 binary readers
                    // and the MSCZ / MusicXML converters). Shift on read so
                    // downstream code doesn't need to know the format quirk.
                    s_note.string = low_i8(s).saturating_add(1);
                }
            }
            "PalmMuted" if prop.enable.is_some() => {
                s_note.effect.palm_mute = true;
            }
            "BendOriginValue" => {
                bend_origin = prop.float;
            }
            "BendDestinationValue" => {
                bend_dest = prop.float;
            }
            // GuitarScore fix: the middle and the offsets carry bend-and-release shapes.
            "BendMiddleValue" => bend_middle = prop.float,
            "BendMiddleOffset1" => bend_middle_offset = prop.float,
            "BendDestinationOffset" => bend_dest_offset = prop.float,
            "Slide" => {
                if let Some(flags) = prop.flags {
                    s_note.effect.slides = parse_slide_flags(flags);
                }
            }
            "HarmonicType" => {
                if let Some(htype) = &prop.htype {
                    s_note.effect.harmonic = Some(parse_harmonic_type(htype));
                }
            }
            "HarmonicFret" => {
                // Guitar Pro writes HarmonicFret before HarmonicType, so remember it and
                // apply it after the loop.
                harmonic_fret = prop.hfret;
            }
            // GuitarScore fix: GPIF marks both ends of a hammer-on/pull-off
            // (`HopoOrigin` on the first note, `HopoDestination` on the second), while the
            // legacy model has a single `hammer` flag that means "origin" (as GP3-5 store
            // it). Ignoring the destination keeps both formats identical and lets the ASCII
            // dump print one `h` per pair.
            "HopoOrigin" if prop.enable.is_some() => {
                s_note.effect.hammer = true;
            }
            "HopoDestination" => {}
            // GuitarScore additions.
            "LeftHandTapped" => s_note.effect.left_hand_tapped = true,
            "ShowStringNumber" => s_note.effect.show_string_number = true,
            "Dead" | "Muted" if prop.enable.is_some() => {
                s_note.kind = NoteType::Dead;
            }
            _ => {}
        }
    }

    // Bend
    if let (Some(orig), Some(dest)) = (bend_origin, bend_dest) {
        s_note.effect.bend = build_note_bend(
            orig,
            bend_middle,
            dest,
            bend_middle_offset,
            bend_dest_offset,
        );
    }

    // Guitar Pro's property order puts HarmonicFret before HarmonicType.
    if let Some(hfret) = harmonic_fret
        && let Some(ref mut harmonic) = s_note.effect.harmonic
    {
        harmonic.fret = Some(round_f64_to_i8(hfret.trunc()));
    }

    // Tie
    if let Some(tie) = &g_note.tie
        && tie.destination == "true"
    {
        s_note.kind = NoteType::Tie;
    }

    // Vibrato. GPIF names the kind; keep wide so layout can draw its bigger wiggle.
    if let Some(vibrato) = &g_note.vibrato {
        s_note.effect.vibrato = true;
        s_note.effect.wide_vibrato = vibrato == "Wide";
    }

    // Let Ring
    if g_note.let_ring.is_some() {
        s_note.effect.let_ring = true;
    }

    // Ghost note
    if g_note.anti_accent.is_some() {
        s_note.effect.ghost_note = true;
    }

    // Accent bitmask
    if let Some(accent) = g_note.accent {
        if (accent & 0x01) != 0 {
            s_note.effect.staccato = true;
        }
        if (accent & 0x02) != 0 || (accent & 0x08) != 0 {
            s_note.effect.accentuated_note = true;
        }
        if (accent & 0x04) != 0 {
            s_note.effect.heavy_accentuated_note = true;
        }
    }

    // GuitarScore addition: fingering.
    if let Some(finger) = g_note.left_fingering.as_deref().and_then(parse_finger) {
        s_note.effect.left_hand_finger = finger;
    }
    if let Some(finger) = g_note.right_fingering.as_deref().and_then(parse_finger) {
        s_note.effect.right_hand_finger = finger;
    }

    // Ornament
    if let Some(orn) = &g_note.ornament {
        s_note.effect.ornament = Some(orn.clone());
    }

    // Trill
    if let Some(trill_fret) = g_note.trill {
        s_note.effect.trill = Some(TrillEffect {
            fret: low_i8(trill_fret),
            duration: Duration::default(),
        });
    }

    // Grace note
    if is_grace_beat {
        s_note.effect.grace = Some(GraceEffect {
            fret: low_i8(i32::from(s_note.value)),
            velocity: s_note.velocity,
            duration: 1,
            is_dead: s_note.kind == NoteType::Dead,
            is_on_beat: grace_on_beat,
            transition: if s_note.effect.hammer {
                GraceEffectTransition::Hammer
            } else if !s_note.effect.slides.is_empty() {
                GraceEffectTransition::Slide
            } else {
                GraceEffectTransition::None
            },
        });
    }

    s_note
}
