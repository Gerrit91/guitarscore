// GuitarScore: the GP7 writer is held to the workspace's no-panic rules (unlike most of this
// crate, see lib.rs): it writes the user's files.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unreachable,
    clippy::indexing_slicing
)]
#![warn(clippy::arithmetic_side_effects)]
// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::Beat;
use crate::io::Note;
use crate::io::Song;
use crate::io::*;
use crate::io::{FORTE, GP_BEND_SEMITONE, MIN_VELOCITY, VELOCITY_INCREMENT};

pub trait SongGpifExportOps {
    /// Serialize this `Song` to a GPIF XML string (GP6 flavour).
    fn write_gpif_xml(&self) -> GpResult<String>;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// GuitarScore addition: returns the counter's current id and advances it.
fn next_id(counter: &mut i32) -> GpResult<i32> {
    let id = *counter;
    *counter = counter
        .checked_add(1)
        .ok_or_else(|| GpError::FormatError("too many elements to number".to_owned()))?;
    Ok(id)
}

/// GuitarScore addition: the id of a track's bar for a measure; bars are numbered track by
/// track (`track * measure_count + measure`).
fn bar_id(track: usize, measure_count: usize, measure: usize) -> GpResult<i32> {
    track
        .checked_mul(measure_count)
        .and_then(|first| first.checked_add(measure))
        .and_then(|id| i32::try_from(id).ok())
        .ok_or_else(|| GpError::FormatError("too many bars to number".to_owned()))
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn velocity_to_dynamic(velocity: i16) -> &'static str {
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

fn note_value_str(value: u16) -> &'static str {
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

fn slide_types_to_flags(slides: &[SlideType]) -> i32 {
    let mut flags = 0i32;
    for s in slides {
        flags |= match s {
            SlideType::ShiftSlideTo => 0x01,
            SlideType::LegatoSlideTo => 0x02,
            SlideType::OutDownwards => 0x04,
            SlideType::OutUpWards => 0x08,
            SlideType::IntoFromBelow => 0x10,
            SlideType::IntoFromAbove => 0x20,
            SlideType::None => 0,
        };
    }
    flags
}

fn harmonic_type_str(kind: &HarmonicType) -> &'static str {
    match kind {
        HarmonicType::Natural => "Natural",
        HarmonicType::Artificial => "Artificial",
        HarmonicType::Pinch => "Pinch",
        HarmonicType::Tapped => "Tap",
        HarmonicType::Semi => "Semi",
    }
}

fn direction_sign_str(d: &DirectionSign) -> &'static str {
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

// ---------------------------------------------------------------------------
// Note export
// ---------------------------------------------------------------------------

fn build_note_xml(note: &Note, note_id: i32) -> String {
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
        s.push_str("<Vibrato>Slight</Vibrato>");
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

// ---------------------------------------------------------------------------
// Beat export
// ---------------------------------------------------------------------------

struct BeatExport {
    beat_xml: String,
    rhythm_xml: String,
    note_xmls: Vec<String>,
}

fn build_beat_xml(
    beat: &Beat,
    beat_id: i32,
    rhythm_id: i32,
    next_note_id: &mut i32,
    prev_velocity: &mut i16,
) -> GpResult<BeatExport> {
    let mut note_xmls: Vec<String> = Vec::new();
    let mut note_ids: Vec<i32> = Vec::new();

    let is_grace_beat =
        !beat.notes.is_empty() && beat.notes.iter().all(|n| n.effect.grace.is_some());

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

    // Beat properties (Brush, Rasgueado, PickStroke)
    let has_brush = !matches!(beat.effect.stroke.direction, BeatStrokeDirection::None);
    let has_pick = !matches!(beat.effect.pick_stroke, BeatStrokeDirection::None);
    let has_rasg = beat.effect.has_rasgueado;

    if has_brush || has_pick || has_rasg {
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

// ---------------------------------------------------------------------------
// Main implementation
// ---------------------------------------------------------------------------

impl SongGpifExportOps for Song {
    fn write_gpif_xml(&self) -> GpResult<String> {
        // GuitarScore fix: GPIF numbers strings from the lowest; convert a copy.
        let mut flipped = self.clone();
        crate::io::gpif_import::flip_note_strings(&mut flipped.tracks);
        flipped.write_gpif_xml_in_gpif_string_order()
    }
}

/// The original `write_gpif_xml`; expects note strings already in GPIF order.
trait WriteGpifInGpifStringOrder {
    fn write_gpif_xml_in_gpif_string_order(&self) -> GpResult<String>;
}

impl WriteGpifInGpifStringOrder for Song {
    fn write_gpif_xml_in_gpif_string_order(&self) -> GpResult<String> {
        let mut xml = String::new();
        xml.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?><GPIF>");

        // Score
        xml.push_str("<Score>");
        xml.push_str(&format!("<Title>{}</Title>", escape_xml(&self.name)));
        xml.push_str(&format!(
            "<SubTitle>{}</SubTitle>",
            escape_xml(&self.subtitle)
        ));
        xml.push_str(&format!("<Artist>{}</Artist>", escape_xml(&self.artist)));
        xml.push_str(&format!("<Album>{}</Album>", escape_xml(&self.album)));
        xml.push_str(&format!("<Words>{}</Words>", escape_xml(&self.words)));
        xml.push_str(&format!("<Music>{}</Music>", escape_xml(&self.author)));
        xml.push_str(&format!(
            "<Copyright>{}</Copyright>",
            escape_xml(&self.copyright)
        ));
        xml.push_str(&format!("<Tabber>{}</Tabber>", escape_xml(&self.writer)));
        xml.push_str(&format!(
            "<Instructions>{}</Instructions>",
            escape_xml(&self.instructions)
        ));
        let notices = self.notice.join("\n");
        xml.push_str(&format!("<Notices>{}</Notices>", escape_xml(&notices)));
        xml.push_str("</Score>");

        // MasterTrack
        let num_tracks = self.tracks.len();
        let track_ids: Vec<String> = (0..num_tracks).map(|i| i.to_string()).collect();
        xml.push_str("<MasterTrack>");
        xml.push_str(&format!("<Tracks>{}</Tracks>", track_ids.join(" ")));
        xml.push_str("<Automations>");
        // Bar-0 tempo (always)
        xml.push_str(&format!(
            "<Automation><Type>Tempo</Type><Value>{} 2</Value><Bar>0</Bar><Position>0</Position></Automation>",
            self.tempo
        ));
        // Per-measure tempo changes
        for (i, mh) in self.measure_headers.iter().enumerate() {
            if i == 0 {
                continue;
            }
            if mh.tempo > 0 {
                xml.push_str(&format!(
                    "<Automation><Type>Tempo</Type><Value>{} 2</Value><Bar>{i}</Bar><Position>0</Position></Automation>",
                    mh.tempo
                ));
            }
        }
        xml.push_str("</Automations>");
        xml.push_str("</MasterTrack>");

        // Tracks
        xml.push_str("<Tracks>");
        for (t_idx, track) in self.tracks.iter().enumerate() {
            xml.push_str(&format!("<Track id=\"{t_idx}\">"));
            xml.push_str(&format!("<Name>{}</Name>", escape_xml(&track.name)));
            xml.push_str(&format!(
                "<ShortName>{}</ShortName>",
                escape_xml(&track.short_name)
            ));
            let r = (track.color >> 16) & 0xFF;
            let g = (track.color >> 8) & 0xFF;
            let b = track.color & 0xFF;
            xml.push_str(&format!("<Color>{r} {g} {b}</Color>"));
            // GuitarScore addition: the instrument, as read by the importer. Drum tracks
            // from GP3-5 (no instrument type) are written as a drum kit.
            let instrument_type = track
                .instrument_type
                .as_deref()
                .or(track.percussion_track.then_some("drumKit"));
            if let Some(kind) = instrument_type {
                let name = track
                    .instrument_name
                    .as_deref()
                    .unwrap_or(if kind == "drumKit" { "Drums" } else { kind });
                xml.push_str(&format!(
                    "<InstrumentSet><Name>{}</Name><Type>{}</Type></InstrumentSet>",
                    escape_xml(name),
                    escape_xml(kind)
                ));
            }
            xml.push_str("<Properties>");
            // GuitarScore: only write a tuning the file had (see `Track::has_tuning`).
            if track.has_tuning {
                // GuitarScore fix: GPIF lists pitches from the lowest string up; `strings`
                // starts at string 1, the highest.
                let pitches: Vec<String> = track
                    .strings
                    .iter()
                    .rev()
                    .map(|(_, p)| p.to_string())
                    .collect();
                // GuitarScore addition: the tuning's name, if the file gave one.
                let label = track
                    .tuning_label
                    .as_deref()
                    .map(|l| format!("<Label>{}</Label>", escape_xml(l)))
                    .unwrap_or_default();
                xml.push_str(&format!(
                    "<Property name=\"Tuning\"><Pitches>{}</Pitches>{label}</Property>",
                    pitches.join(" ")
                ));
            }
            xml.push_str("</Properties>");
            xml.push_str("<GeneralMidi>");
            if let Some(prog) = track.midi_program_gpif {
                xml.push_str(&format!("<Program>{prog}</Program>"));
            }
            xml.push_str(&format!("<Port>{}</Port>", track.port));
            xml.push_str(&format!(
                "<PrimaryChannel>{}</PrimaryChannel>",
                track.channel_index
            ));
            xml.push_str(&format!(
                "<SecondaryChannel>{}</SecondaryChannel>",
                track.channel_index.saturating_add(1).min(15)
            ));
            xml.push_str("</GeneralMidi>");
            if track.transpose_chromatic != 0 || track.transpose_octave != 0 {
                xml.push_str("<Transpose>");
                xml.push_str(&format!(
                    "<Chromatic>{}</Chromatic>",
                    track.transpose_chromatic
                ));
                xml.push_str(&format!("<Octave>{}</Octave>", track.transpose_octave));
                xml.push_str("</Transpose>");
            }
            xml.push_str("</Track>");
        }
        xml.push_str("</Tracks>");

        // Build all bars/voices/beats/notes/rhythms with sequential IDs
        let num_measures = self.measure_headers.len();

        // (bar_id, voice_ids_string)
        // (bar id, voice ids, simile mark)
        let mut bars: Vec<(i32, String, Option<String>)> = Vec::new();
        let mut voices_xml: Vec<String> = Vec::new();
        let mut beats_xml: Vec<String> = Vec::new();
        let mut notes_xml: Vec<String> = Vec::new();
        let mut rhythms_xml: Vec<String> = Vec::new();

        let mut next_voice_id: i32 = 0;
        let mut next_beat_id: i32 = 0;
        let mut next_note_id: i32 = 0;
        let mut next_rhythm_id: i32 = 0;

        // bar_id = t_idx * num_measures + m_idx
        for (t_idx, track) in self.tracks.iter().enumerate() {
            // Velocity persists across all measures/voices for a given track
            let mut prev_velocity: i16 = FORTE;

            for m_idx in 0..num_measures {
                let bar_id = bar_id(t_idx, num_measures, m_idx)?;
                let measure = track.measures.get(m_idx).ok_or_else(|| {
                    GpError::FormatError(format!(
                        "track {} has fewer measures than the song ({num_measures})",
                        t_idx.saturating_add(1)
                    ))
                })?;
                let mut voice_ids: Vec<String> = Vec::new();

                for voice in &measure.voices {
                    // GuitarScore fix: a voice holding only placeholder beats (as GP3-5 store
                    // unused voices) is empty; writing the placeholders made them real beats.
                    if voice.beats.iter().all(|b| b.status == BeatStatus::Empty) {
                        voice_ids.push("-1".to_string());
                        continue;
                    }

                    let voice_id = next_id(&mut next_voice_id)?;
                    let mut beat_ids: Vec<i32> = Vec::new();

                    for beat in voice.beats.iter().filter(|b| b.status != BeatStatus::Empty) {
                        let beat_id = next_id(&mut next_beat_id)?;
                        let rhythm_id = next_id(&mut next_rhythm_id)?;
                        beat_ids.push(beat_id);

                        let export = build_beat_xml(
                            beat,
                            beat_id,
                            rhythm_id,
                            &mut next_note_id,
                            &mut prev_velocity,
                        )?;
                        beats_xml.push(export.beat_xml);
                        rhythms_xml.push(export.rhythm_xml);
                        notes_xml.extend(export.note_xmls);
                    }

                    let beat_ids_str: Vec<String> =
                        beat_ids.iter().map(|id| id.to_string()).collect();
                    voices_xml.push(format!(
                        "<Voice id=\"{voice_id}\"><Beats>{}</Beats></Voice>",
                        beat_ids_str.join(" ")
                    ));
                    voice_ids.push(voice_id.to_string());
                }

                bars.push((bar_id, voice_ids.join(" "), measure.simile_mark.clone()));
            }
        }

        // MasterBars
        xml.push_str("<MasterBars>");
        for (m_idx, mh) in self.measure_headers.iter().enumerate() {
            xml.push_str("<MasterBar>");
            xml.push_str(&format!(
                "<Time>{}/{}</Time>",
                mh.time_signature.numerator, mh.time_signature.denominator.value
            ));
            let mode = if mh.key_signature.is_minor {
                "Minor"
            } else {
                "Major"
            };
            xml.push_str(&format!(
                "<Key><AccidentalCount>{}</AccidentalCount><Mode>{mode}</Mode></Key>",
                mh.key_signature.key
            ));
            let bar_ids = (0..self.tracks.len())
                .map(|t| bar_id(t, num_measures, m_idx).map(|id| id.to_string()))
                .collect::<GpResult<Vec<String>>>()?;
            xml.push_str(&format!("<Bars>{}</Bars>", bar_ids.join(" ")));

            if mh.repeat_open || mh.repeat_close > 0 {
                let start = if mh.repeat_open { "true" } else { "false" };
                let end = if mh.repeat_close > 0 { "true" } else { "false" };
                // The model counts repeats (jumps back); GPIF counts plays.
                let plays = i16::from(mh.repeat_close.max(1)).saturating_add(1);
                xml.push_str(&format!(
                    "<Repeat start=\"{start}\" end=\"{end}\" count=\"{plays}\"/>"
                ));
            }
            if mh.repeat_alternative > 0 {
                let mut endings: Vec<String> = Vec::new();
                for i in 0..8u8 {
                    if (mh.repeat_alternative >> i) & 1 != 0 {
                        endings.push(i.saturating_add(1).to_string());
                    }
                }
                xml.push_str(&format!(
                    "<AlternateEndings>{}</AlternateEndings>",
                    endings.join(" ")
                ));
            }
            if mh.double_bar {
                xml.push_str("<DoubleBar/>");
            }
            if let Some(marker) = &mh.marker {
                xml.push_str(&format!(
                    "<Section><Text>{}</Text></Section>",
                    escape_xml(&marker.title)
                ));
            }
            if !mh.fermatas.is_empty() {
                xml.push_str("<Fermatas>");
                for (ftype, offset) in &mh.fermatas {
                    xml.push_str(&format!(
                        "<Fermata><Type>{ftype}</Type><Offset>{offset}</Offset></Fermata>"
                    ));
                }
                xml.push_str("</Fermatas>");
            }
            if mh.free_time {
                xml.push_str("<FreeTime/>");
            }
            if let Some(dir) = &mh.direction {
                xml.push_str(&format!(
                    "<Directions><Jump>{}</Jump></Directions>",
                    direction_sign_str(dir)
                ));
            }
            xml.push_str("</MasterBar>");
        }
        xml.push_str("</MasterBars>");

        // Bars
        xml.push_str("<Bars>");
        // GuitarScore fix: the simile mark is stored, not parsed back out of a string (the
        // slicing could panic on a quote in the value), and it is escaped.
        for (bar_id, voices, simile) in &bars {
            match simile {
                Some(simile) => xml.push_str(&format!(
                    "<Bar id=\"{bar_id}\"><Voices>{voices}</Voices><SimileMark>{}</SimileMark></Bar>",
                    escape_xml(simile)
                )),
                None => xml.push_str(&format!("<Bar id=\"{bar_id}\"><Voices>{voices}</Voices></Bar>")),
            }
        }
        xml.push_str("</Bars>");

        // Voices
        xml.push_str("<Voices>");
        for v in &voices_xml {
            xml.push_str(v);
        }
        xml.push_str("</Voices>");

        // Beats
        xml.push_str("<Beats>");
        for b in &beats_xml {
            xml.push_str(b);
        }
        xml.push_str("</Beats>");

        // Notes
        xml.push_str("<Notes>");
        for n in &notes_xml {
            xml.push_str(n);
        }
        xml.push_str("</Notes>");

        // Rhythms
        xml.push_str("<Rhythms>");
        for r in &rhythms_xml {
            xml.push_str(r);
        }
        xml.push_str("</Rhythms>");

        xml.push_str("</GPIF>");
        Ok(xml)
    }
}

#[cfg(test)]
mod repeat_tests {
    use super::SongGpifExportOps;
    use crate::io::Song;

    /// A plain repeat (one jump back) is `count="2"` in GPIF, which counts plays; reading
    /// it back gives one repeat again. Both directions used to pass the number through,
    /// so GP7 repeats played once too often and GP5 <-> GP7 conversion shifted the count.
    #[test]
    fn repeat_counts_convert_between_repeats_and_plays() {
        let mut song = Song::default();
        song.tracks.push(Default::default());
        for index in 0..2 {
            let mut header = crate::model::MeasureHeader::default();
            header.number = index + 1;
            song.measure_headers.push(header);
            song.tracks[0].measures.push(crate::model::Measure {
                header_index: usize::from(index),
                ..Default::default()
            });
        }
        song.measure_headers[0].repeat_open = true;
        song.measure_headers[1].repeat_close = 1;

        let xml = song.write_gpif_xml().expect("written");
        assert!(xml.contains("end=\"true\" count=\"2\""), "{xml}");

        let bytes = song.write_gp().expect("written");
        let mut reread = Song::default();
        reread.read_gp(&bytes).expect("read back");
        assert!(reread.measure_headers[0].repeat_open);
        assert_eq!(reread.measure_headers[1].repeat_close, 1);
    }
}
