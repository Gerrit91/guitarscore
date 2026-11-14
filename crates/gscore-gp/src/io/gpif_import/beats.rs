//! Converting GPIF beats, and the string order shared by import and export.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use std::collections::HashMap;

use super::*;
use crate::model::beat_spacing;

/// GuitarScore addition: converts note string numbers between GPIF order (1 = lowest) and
/// legacy order (1 = highest). Its own inverse, so import and export use the same function.
/// Notes outside the track's strings (e.g. mapped piano notes) are left unchanged.
pub(crate) fn flip_note_strings(tracks: &mut [SongTrack]) {
    for staff in tracks.iter_mut().flat_map(SongTrack::staves_mut) {
        let count = i8::try_from(staff.strings.len()).unwrap_or(i8::MAX);
        for measure in staff.measures.iter_mut() {
            for voice in &mut measure.voices {
                for beat in &mut voice.beats {
                    for note in &mut beat.notes {
                        if (1..=count).contains(&note.string) {
                            note.string = count.saturating_sub(note.string).saturating_add(1);
                        }
                    }
                }
            }
        }
    }
}

/// The note value (8, 16 or 32) of a GPIF tremolo picking speed.
pub(crate) fn tremolo_speed(text: &str) -> Option<u16> {
    match text {
        "1/2" => Some(8),
        "1/4" => Some(16),
        "1/8" => Some(32),
        _ => None,
    }
}

/// A stroke speed from a GuitarScore property, clamped to the note values a stroke may
/// use; `default` when the value is missing or hostile.
fn stroke_speed(value: Option<i32>, default: u16) -> u16 {
    value
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| matches!(value, 1 | 2 | 4 | 8 | 16 | 32 | 64 | 128))
        .unwrap_or(default)
}

/// The GPIF tremolo picking speed of a note value (8, 16 or 32; others round to 16).
pub(crate) fn tremolo_text(value: u16) -> &'static str {
    match value {
        0..=8 => "1/2",
        9..=16 => "1/4",
        _ => "1/8",
    }
}

/// GuitarScore fix: GPIF stores a trill's other note as a MIDI pitch, the legacy model as
/// a fret on the note's string. Converts every trill of `tracks` (strings in legacy order,
/// 1 = highest): to frets on import, back to pitches (`to_pitch`) on export. The capo is
/// part of the pitch.
pub(crate) fn convert_trills(tracks: &mut [SongTrack], to_pitch: bool) {
    for staff in tracks.iter_mut().flat_map(SongTrack::staves_mut) {
        let capo = staff.offset;
        let tuning: Vec<(i8, i8)> = staff.strings.to_vec();
        for measure in staff.measures.iter_mut() {
            for voice in &mut measure.voices {
                for beat in &mut voice.beats {
                    for note in &mut beat.notes {
                        let Some(trill) = note.effect.trill.as_mut() else {
                            continue;
                        };
                        let Some(&(_, open)) =
                            tuning.iter().find(|(number, _)| *number == note.string)
                        else {
                            continue;
                        };
                        let base = i32::from(open).saturating_add(capo);
                        let value = if to_pitch {
                            i32::from(trill.fret).saturating_add(base)
                        } else {
                            i32::from(trill.fret).saturating_sub(base)
                        };
                        trill.fret = low_i8(value.clamp(0, 127));
                    }
                }
            }
        }
    }
}

pub(super) fn convert_beat(
    g_beat: &Beat,
    rhythms_map: &HashMap<i32, &Rhythm>,
    notes_map: &HashMap<i32, &Note>,
    current_velocity: &mut i16,
) -> SongBeat {
    let mut s_beat = SongBeat::default();

    // Duration from Rhythm
    if let Some(rhythm_ref) = &g_beat.rhythm
        && let Some(rhythm) = rhythms_map.get(&rhythm_ref.r#ref)
    {
        s_beat.duration.value = note_value_to_duration(&rhythm.note_value);
        if let Some(dot) = &rhythm.augmentation_dot {
            match dot.count {
                1 => s_beat.duration.dotted = true,
                2 => s_beat.duration.double_dotted = true,
                _ => {}
            }
        }
        if let Some(tuplet) = &rhythm.primary_tuplet {
            s_beat.duration.tuplet_enters = low_u8(tuplet.num);
            s_beat.duration.tuplet_times = low_u8(tuplet.den);
        }
    }

    // Dynamic
    if let Some(dyn_str) = &g_beat.dynamic {
        *current_velocity = dynamic_to_velocity(dyn_str);
    }

    // Grace notes
    let is_grace_beat = g_beat.grace_notes.is_some();
    let grace_on_beat = g_beat.grace_notes.as_deref() == Some("OnBeat");
    s_beat.grace = is_grace_beat;

    // Text
    if let Some(text) = &g_beat.free_text {
        s_beat.text = text.clone();
    }
    // Lyric syllables for each line (GP6/GP7 store them on the beat).
    // Trailing empty lines are dropped (Guitar Pro writes all five).
    if let Some(lyrics) = &g_beat.lyrics {
        let used = lyrics
            .lines
            .iter()
            .rposition(|line| !line.is_empty())
            .map_or(0, |last| last.saturating_add(1));
        s_beat.lyrics = lyrics.lines.iter().take(used).cloned().collect();
    }

    // Octave sign
    s_beat.octave = match g_beat.ottavia.as_deref() {
        Some("8va") => Octave::Ottava,
        Some("8vb") => Octave::OttavaBassa,
        Some("15ma") => Octave::Quindicesima,
        Some("15mb") => Octave::QuindicesimaBassa,
        _ => Octave::None,
    };

    // Fade in, and (GuitarScore addition) fade out and volume swell.
    match g_beat.fadding.as_deref() {
        Some("FadeIn") => s_beat.effect.fade_in = true,
        Some("FadeOut") => s_beat.effect.fade_out = true,
        Some("VolumeSwell") => s_beat.effect.volume_swell = true,
        _ => {}
    }

    // GuitarScore addition: a rolled chord, and a dead slapped beat.
    s_beat.effect.arpeggio = match g_beat.arpeggio.as_deref() {
        Some("Down") => BeatStrokeDirection::Down,
        Some("Up") => BeatStrokeDirection::Up,
        _ => BeatStrokeDirection::None,
    };
    s_beat.effect.dead_slapped = g_beat.dead_slapped.is_some();

    // Beat-level legato (a slur over a run of notes, not a slide or a hammer-on).
    if let Some(legato) = &g_beat.legato {
        s_beat.effect.legato = LegatoEffect {
            origin: legato.origin == "true",
            destination: legato.destination == "true",
        };
    }

    // Wah effect
    if let Some(wah_str) = &g_beat.wah
        && wah_str == "Open"
    {
        s_beat.effect.slap_effect = SlapEffect::None; // placeholder, wah is stored at mix table level in GP5
    }

    // Whammy bar: GP7 stores the shape in <Whammy>, GP6 (GPX) in the WhammyBar* beat
    // properties below. <Tremolo> is a separate legacy field.
    if let Some(whammy) = &g_beat.whammy {
        s_beat.effect.tremolo_bar = build_whammy_effect(
            whammy.origin_value,
            whammy.middle_value,
            whammy.destination_value,
            whammy.middle_offset,
            whammy.destination_offset,
        );
    } else if let Some(tremolo_str) = &g_beat.tremolo
        && tremolo_speed(tremolo_str).is_none()
        && let Ok(val) = tremolo_str.parse::<f64>()
        && val != 0.0
    {
        s_beat.effect.tremolo_bar = Some(build_bend_effect(0.0, val));
    }
    // GuitarScore fix: GP6/GP7 write tremolo picking as the beat's `<Tremolo>` speed
    // ("1/2", "1/4", "1/8": eighths, sixteenths, 32nds); it is a note effect here.
    let tremolo_picking = g_beat.tremolo.as_deref().and_then(tremolo_speed);

    // Beat properties
    let mut whammy_origin: Option<f64> = None;
    let mut whammy_middle: Option<f64> = None;
    let mut whammy_destination: Option<f64> = None;
    if let Some(beat_props) = &g_beat.properties {
        for bp in &beat_props.properties {
            match bp.name.as_str() {
                "Brush" => {
                    if let Some(dir) = &bp.direction {
                        s_beat.effect.stroke.direction = match dir.as_str() {
                            "Down" => BeatStrokeDirection::Down,
                            "Up" => BeatStrokeDirection::Up,
                            _ => BeatStrokeDirection::None,
                        };
                        s_beat.effect.stroke.value = DURATION_THIRTY_SECOND as u16;
                    }
                }
                // GuitarScore additions: the brush's speed and timing, next to the
                // standard Brush direction above.
                "GuitarScoreBrush" => {
                    s_beat.effect.stroke.value =
                        stroke_speed(bp.duration, DURATION_THIRTY_SECOND as u16);
                    if let Some(text) = &bp.text {
                        s_beat.effect.stroke.on_beat = text != "before";
                    }
                }
                "GuitarScoreArpeggio" => {
                    s_beat.effect.arpeggio_speed =
                        stroke_speed(bp.duration, DURATION_EIGHTH as u16);
                    if let Some(text) = &bp.text {
                        s_beat.effect.arpeggio_on_beat = text != "before";
                    }
                }
                "Rasgueado" => {
                    s_beat.effect.has_rasgueado = true;
                }
                "PickStroke" => {
                    if let Some(dir) = &bp.direction {
                        s_beat.effect.pick_stroke = match dir.as_str() {
                            "Down" => BeatStrokeDirection::Down,
                            "Up" => BeatStrokeDirection::Up,
                            _ => BeatStrokeDirection::None,
                        };
                    }
                }
                // Beat-level vibrato (Guitar Pro writes it as VibratoWTremBar; the note-level
                // `<Vibrato>` tag is a different field).
                "VibratoWTremBar" => {
                    s_beat.effect.vibrato = true;
                    s_beat.effect.wide_vibrato = bp.strength.as_deref() == Some("Wide");
                }
                // GP6 (GPX) stores the whammy bar as three beat properties, in hundredths
                // of a semitone; GP7 uses <Whammy> instead. Every beat carries them (0 for
                // "no whammy"), so only a non-zero shape becomes an effect.
                "WhammyBarOriginValue" => whammy_origin = bp.float,
                "WhammyBarMiddleValue" => whammy_middle = bp.float,
                "WhammyBarDestinationValue" => whammy_destination = bp.float,
                // GuitarScore addition: the design mode's offset of this beat's column, in
                // staff spaces (the format has no field this code knows for it).
                "GuitarScoreSpacing" => {
                    s_beat.spacing = bp.float.and_then(beat_spacing);
                }
                _ => {}
            }
        }
    }
    // GuitarScore addition: Guitar Pro's own beat layout properties are kept, so saving
    // does not lose what Guitar Pro 8 wrote there.
    s_beat.x_properties = xproperties(g_beat.x_properties.as_ref());
    if s_beat.effect.tremolo_bar.is_none()
        && let (Some(origin), Some(destination)) = (whammy_origin, whammy_destination)
    {
        s_beat.effect.tremolo_bar =
            build_whammy_effect(origin, whammy_middle, destination, None, None);
    }

    // Notes
    match &g_beat.notes {
        Some(notes_str) => {
            let note_ids = parse_ids(notes_str);
            s_beat.status = if note_ids.is_empty() {
                BeatStatus::Rest
            } else {
                BeatStatus::Normal
            };

            for &nid in &note_ids {
                if let Some(g_note) = notes_map.get(&nid) {
                    let mut s_note =
                        convert_note(g_note, *current_velocity, is_grace_beat, grace_on_beat);
                    if let Some(value) = tremolo_picking {
                        s_note.effect.tremolo_picking = Some(TremoloPickingEffect {
                            duration: Duration {
                                value,
                                ..Duration::default()
                            },
                        });
                    }
                    s_beat.notes.push(s_note);
                }
            }
        }
        None => {
            s_beat.status = BeatStatus::Rest;
        }
    }

    s_beat
}
