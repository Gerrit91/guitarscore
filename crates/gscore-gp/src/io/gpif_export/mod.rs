//! Writing the song as GPIF XML (GP6 and GP7+): the song, master bars and tracks here;
//! beats in [`beats`], notes in [`notes`], value conversions in [`helpers`].

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::Beat;
use crate::io::Note;
use crate::io::Song;
use crate::io::*;
use crate::io::{FORTE, GP_BEND_SEMITONE, MIN_VELOCITY, VELOCITY_INCREMENT};

mod beats;
mod helpers;
mod notes;
#[cfg(test)]
mod repeat_tests;

use beats::*;
use helpers::*;
use notes::*;

pub(crate) trait SongGpifExportOps {
    /// Serialize this `Song` to a GPIF XML string (GP6 flavour).
    fn write_gpif_xml(&self) -> GpResult<String>;
}

// ---------------------------------------------------------------------------
// Main implementation
// ---------------------------------------------------------------------------

impl SongGpifExportOps for Song {
    fn write_gpif_xml(&self) -> GpResult<String> {
        // GuitarScore fix: GPIF numbers strings from the lowest; convert a copy.
        let mut flipped = self.clone();
        gpif_import::convert_trills(&mut flipped.tracks, true);
        gpif_import::flip_note_strings(&mut flipped.tracks);
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
        // The multitrack view's bars per line, as the file had them.
        if let Some(default) = &self.score_systems_default {
            xml.push_str(&format!(
                "<ScoreSystemsDefaultLayout>{}</ScoreSystemsDefaultLayout>",
                escape_xml(default)
            ));
        }
        if let Some(layout) = &self.score_systems_layout {
            xml.push_str(&format!(
                "<ScoreSystemsLayout>{}</ScoreSystemsLayout>",
                escape_xml(layout)
            ));
        }
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
        // GuitarScore addition: the master volume and pan.
        if let Some((volume, pan)) = self.master_volume_pan {
            xml.push_str(&format!(
                "<RSE><Master><Effect id=\"{}\"><Parameters>{} {}</Parameters></Effect></Master></RSE>",
                gpif_import::MASTER_VOLUME_PAN,
                volume.clamp(0.0, 1.0),
                pan.clamp(0.0, 1.0)
            ));
        }
        xml.push_str("</MasterTrack>");

        // Tracks
        xml.push_str("<Tracks>");
        for (t_idx, track) in self.tracks.iter().enumerate() {
            // GuitarScore addition: page breaks, which Guitar Pro does not have.
            let page_breaks: Vec<String> = track
                .measures
                .iter()
                .enumerate()
                .filter(|(index, measure)| *index > 0 && measure.line_break == LineBreak::Page)
                .map(|(index, _)| index.to_string())
                .collect();
            xml.push_str(&format!("<Track id=\"{t_idx}\""));
            if !page_breaks.is_empty() {
                xml.push_str(&format!(" gsPageBreaks=\"{}\"", page_breaks.join(" ")));
            }
            // GuitarScore addition: the most bars per line.
            if let Some(bars) = track.bars_per_system {
                xml.push_str(&format!(" gsBarsPerSystem=\"{bars}\""));
            }
            xml.push('>');
            xml.push_str(&format!("<Name>{}</Name>", escape_xml(&track.name)));
            xml.push_str(&format!(
                "<ShortName>{}</ShortName>",
                escape_xml(&track.short_name)
            ));
            let r = (track.color >> 16) & 0xFF;
            let g = (track.color >> 8) & 0xFF;
            let b = track.color & 0xFF;
            xml.push_str(&format!("<Color>{r} {g} {b}</Color>"));
            // GuitarScore addition: line (and page) breaks as Guitar Pro's systems layout,
            // the bars of each line; without breaks Guitar Pro lays the track out itself.
            if let Some(systems) = systems_layout(&track.measures) {
                xml.push_str(&format!(
                    "<SystemsDefautLayout>4</SystemsDefautLayout><SystemsLayout>{systems}</SystemsLayout>"
                ));
            }
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
            // GuitarScore addition: the channel's reverb and chorus sends. GP7 has no
            // per-track send (its reverb is an RSE effect), so they are written as named
            // properties, and only when set, so untouched songs are written as before.
            let sends = self
                .channels
                .get(track.channel_index)
                .map_or((0, 0), |channel| (channel.reverb, channel.chorus));
            for (name, value) in [
                (gpif_import::REVERB_PROPERTY, sends.0),
                (gpif_import::CHORUS_PROPERTY, sends.1),
            ] {
                if value > 0 {
                    xml.push_str(&format!(
                        "<Property name=\"{name}\"><Number>{}</Number></Property>",
                        value.clamp(0, 127)
                    ));
                }
            }
            // GuitarScore addition: the capo fret (GP6/GP7 store it as a staff property).
            xml.push_str(&format!(
                "<Property name=\"CapoFret\"><Fret>{}</Fret></Property>",
                track.offset.clamp(0, 127)
            ));
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
            // The track's sound: GP7 reads the program from `GeneralMidi` here and the
            // mixer from the RSE channel strip; GP3-5 keep both in the song's channel
            // table, so fall back to the channel (a save from those formats would
            // otherwise silently reset the program, volume and pan).
            let channel = self.channels.get(track.channel_index);
            let program = track
                .midi_program_gpif
                .or_else(|| channel.map(|channel| channel.instrument))
                .map_or(25, |program| program.clamp(0, 127));
            let volume = midi_value(channel.map(|channel| channel.volume), 104);
            let pan = midi_value(channel.map(|channel| channel.balance), 64);
            xml.push_str("<GeneralMidi>");
            xml.push_str(&format!("<Program>{program}</Program>"));
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
            // GuitarScore addition: the track's mixer and playback state in the places
            // Guitar Pro 7 reads them (volume and pan are RSE channel-strip parameters).
            xml.push_str(&format!(
                "<RSE><ChannelStrip version=\"E56\"><Parameters>{}</Parameters></ChannelStrip></RSE>",
                rse_parameters(volume, pan)
            ));
            xml.push_str(&format!(
                "<MidiConnection><Port>{}</Port><PrimaryChannel>{}</PrimaryChannel><SecondaryChannel>{}</SecondaryChannel><ForeOneChannelPerString>false</ForeOneChannelPerString></MidiConnection>",
                track.port,
                track.channel_index,
                track.channel_index.saturating_add(1).min(15)
            ));
            let state = if track.solo {
                "Solo"
            } else if track.mute {
                "Mute"
            } else {
                "Default"
            };
            xml.push_str(&format!("<PlaybackState>{state}</PlaybackState>"));
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
            // GuitarScore fix: the measure's feel was not written.
            if let Some(feel) = mh.triplet_feel.gpif_name() {
                xml.push_str(&format!("<TripletFeel>{feel}</TripletFeel>"));
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

/// A MIDI value from the channel table, or `default` when the track has no channel.
fn midi_value(value: Option<i8>, default: u8) -> u8 {
    value
        .and_then(|value| u8::try_from(value).ok())
        .map_or(default, |value| value.min(127))
}

/// The 16 RSE channel-strip parameters, with index 11 (pan) and 12 (volume) from the
/// track's mixer; the rest use Guitar Pro's defaults.
fn rse_parameters(volume: u8, pan: u8) -> String {
    let mut parts = Vec::with_capacity(16);
    for index in 0..16 {
        let value = match index {
            9 => "0".to_owned(),
            11 => format!("{:.3}", f64::from(pan.min(127)) / 127.0),
            12 => format!("{:.3}", f64::from(volume.min(127)) / 127.0),
            _ => "0.5".to_owned(),
        };
        parts.push(value);
    }
    parts.join(" ")
}

/// GuitarScore addition: the bars of each line between line (or page) breaks, `"4 4 3"`,
/// or `None` when the track has no breaks.
fn systems_layout(measures: &[Measure]) -> Option<String> {
    let starts: Vec<usize> = measures
        .iter()
        .enumerate()
        .filter(|(index, measure)| {
            *index > 0 && matches!(measure.line_break, LineBreak::Break | LineBreak::Page)
        })
        .map(|(index, _)| index)
        .collect();
    if starts.is_empty() {
        return None;
    }
    let mut counts = Vec::with_capacity(starts.len().saturating_add(1));
    let mut from = 0_usize;
    for start in starts.into_iter().chain(std::iter::once(measures.len())) {
        counts.push(start.saturating_sub(from).to_string());
        from = start;
    }
    Some(counts.join(" "))
}
