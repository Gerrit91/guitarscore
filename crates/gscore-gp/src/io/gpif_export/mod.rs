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
mod ties;

use beats::*;
use helpers::*;
use notes::*;
use ties::TieOrigins;

/// One bar to write: its id, voices, simile mark, clef and Guitar Pro's layout properties.
struct BarXml {
    id: i32,
    voices: String,
    simile: Option<String>,
    clef: &'static str,
    properties: Vec<XProperty>,
}

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
        // GuitarScore addition: the words-and-music credit as written.
        xml.push_str(&format!(
            "<WordsAndMusic>{}</WordsAndMusic>",
            cdata(&self.words_and_music)
        ));
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
        // GuitarScore addition: the page header and footer templates (HTML in CDATA), kept
        // so a save does not lose them; Guitar Pro writes them, empty ones included.
        for (name, template) in [
            ("FirstPageHeader", &self.first_page_header),
            ("FirstPageFooter", &self.first_page_footer),
            ("PageHeader", &self.page_header),
            ("PageFooter", &self.page_footer),
        ] {
            xml.push_str(&format!("<{name}>{}</{name}>", cdata(template)));
        }
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
        // Bar-0 tempo (always); GuitarScore addition: its name (`<Text>`), when the file
        // gave one.
        let tempo_text = self
            .gpif_tempo_text
            .as_deref()
            .map(|text| format!("<Text>{}</Text>", cdata(text)))
            .unwrap_or_default();
        xml.push_str(&format!(
            "<Automation><Type>Tempo</Type><Linear>false</Linear><Bar>0</Bar><Position>0</Position><Visible>true</Visible>{tempo_text}<Value>{} 2</Value></Automation>",
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
        // GuitarScore addition: a tempo change at the bar's very end, as the file had it
        // (the next bar's tempo is what is heard; when it becomes the next bar's tempo,
        // that one is already written above).
        for (i, mh) in self.measure_headers.iter().enumerate() {
            let next_tempo = self
                .measure_headers
                .get(i.saturating_add(1))
                .map_or(0, |header| header.tempo);
            for (position, tempo) in &mh.end_tempos {
                if *position >= 1.0 && i32::from(*tempo) == next_tempo {
                    continue;
                }
                xml.push_str(&format!(
                    "<Automation><Type>Tempo</Type><Linear>false</Linear><Bar>{i}</Bar><Position>{position}</Position><Visible>true</Visible><Value>{tempo} 2</Value></Automation>"
                ));
            }
        }
        // GuitarScore addition: tempo changes inside a bar, at their position in it.
        for (bar, position, tempo) in gpif_import::tempo::mid_bar_tempos(self) {
            xml.push_str(&format!(
                "<Automation><Type>Tempo</Type><Value>{tempo} 2</Value><Bar>{bar}</Bar><Position>{position}</Position></Automation>"
            ));
        }
        xml.push_str("</Automations>");
        // GuitarScore addition: Guitar Pro's master effect chain, every effect written back
        // as it was read; the volume and pan one comes from the model so a change is saved.
        let mut master_effects = String::new();
        for effect in &self.gpif_master_effects {
            let volume_pan = if effect.id == gpif_import::MASTER_VOLUME_PAN {
                self.master_volume_pan
            } else {
                None
            };
            master_effects.push_str(&master_effect_xml(effect, volume_pan));
        }
        if let Some((volume, pan)) = self.master_volume_pan
            && !self
                .gpif_master_effects
                .iter()
                .any(|effect| effect.id == gpif_import::MASTER_VOLUME_PAN)
        {
            master_effects.push_str(&format!(
                "<Effect id=\"{}\"><Parameters>{} {}</Parameters></Effect>",
                gpif_import::MASTER_VOLUME_PAN,
                volume.clamp(0.0, 1.0),
                pan.clamp(0.0, 1.0)
            ));
        }
        if !master_effects.is_empty() {
            xml.push_str(&format!("<RSE><Master>{master_effects}</Master></RSE>"));
        }
        xml.push_str("</MasterTrack>");
        // GuitarScore addition: GP8's audio tracks, kept verbatim.
        if let Some(audio_tracks) = &self.gpif_audio_tracks {
            xml.push_str(audio_tracks);
        }

        // Tracks
        xml.push_str("<Tracks>");
        // The chord diagrams of each track, in one collection per track (GPIF references
        // them by id from the beats).
        // Each track's diagrams with the ids the file gave them, a newly used one after
        // them; and whether any is new (then the staff's own collection is written anew).
        let numbered: Vec<(Vec<(i32, ChordDiagram)>, bool)> =
            self.tracks.iter().map(numbered_diagrams).collect();
        let track_diagrams: Vec<&[(i32, ChordDiagram)]> = numbered
            .iter()
            .map(|(diagrams, _)| diagrams.as_slice())
            .collect();
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
            // An empty or unchanged layout is written back exactly as the file had it.
            let derived = systems_layout(&track.measures);
            let layout = match (&derived, &track.gpif_systems_layout) {
                (Some(derived), Some(original)) if derived == original => Some(original.clone()),
                (Some(derived), _) => Some(derived.clone()),
                // No break in the model: keep the file's own layout only when it has no
                // break either (an empty one, or one that just ends); otherwise the breaks
                // were removed and writing the old layout would put them back.
                (None, Some(original)) if !layout_implies_break(original, track.measures.len()) => {
                    Some(original.clone())
                }
                _ => None,
            };
            if let Some(layout) = layout {
                let default = track.gpif_systems_default.as_deref().unwrap_or("4");
                xml.push_str(&format!(
                    "<SystemsDefautLayout>{}</SystemsDefautLayout><SystemsLayout>{}</SystemsLayout>",
                    escape_xml(default),
                    escape_xml(&layout)
                ));
            }
            // GuitarScore addition: Guitar Pro's per-track playback settings (its icon,
            // palm-mute strength, playing style, ...), kept exactly as the file had them.
            for element in &track.gpif_playback {
                xml.push_str(element);
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
            if let Some(diagrams) = track_diagrams.get(t_idx)
                && !diagrams.is_empty()
            {
                xml.push_str(&diagram_collection_xml(diagrams));
            }
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
            // GuitarScore addition: the app's instrument symbol, when the user chose one.
            if let Some(symbol) = track.symbol.as_deref() {
                xml.push_str(&format!(
                    "<Property name=\"{}\"><Text>{}</Text></Property>",
                    gpif_import::SYMBOL_PROPERTY,
                    escape_xml(symbol)
                ));
            }
            // GuitarScore addition: the slash-notation switch (GP7's PartConfiguration
            // bit only means its alternative staff style, so keep this separately).
            if track.settings.slash {
                xml.push_str(&format!(
                    "<Property name=\"{}\"><Number>1</Number></Property>",
                    gpif_import::SLASH_PROPERTY
                ));
            }
            // GuitarScore addition: the capo fret (GP6/GP7 store it as a staff property),
            // and the tuning (only a tuning the file had, see `Track::has_tuning`).
            xml.push_str(&staff_properties(
                &track.strings,
                track.has_tuning,
                track.tuning_label.as_deref(),
                track.offset,
            ));
            xml.push_str("</Properties>");
            // GuitarScore addition: a track with several staves (a piano's treble and bass)
            // lists each with its own capo and tuning; every track is written this way, as
            // Guitar Pro 7 writes it, also a one-staff one (whose staff block holds what
            // the model does not: the fret count, a partial capo, the tuning's name,
            // instrument and flats, and the chord and diagram libraries). The track-level
            // properties above stay for Guitar Pro 6.
            // The file's chord libraries as written, while they hold every diagram the beats
            // use; with a new one, the staff's diagram collection is written from the
            // numbering above (the other libraries refer to the same ids).
            let new_diagrams = numbered.get(t_idx).is_some_and(|(_, new)| *new);
            let libraries = |kept: &StaffKept, first: bool| -> Vec<String> {
                if !new_diagrams {
                    return kept.chord_libraries.clone();
                }
                let mut libraries: Vec<String> = kept
                    .chord_libraries
                    .iter()
                    .filter(|library| !library.contains("name=\"DiagramCollection\""))
                    .cloned()
                    .collect();
                if first && let Some(diagrams) = track_diagrams.get(t_idx) {
                    libraries.insert(0, diagram_collection_xml(diagrams));
                }
                libraries
            };
            xml.push_str("<Staves><Staff><Properties>");
            xml.push_str(&staff_block_properties(
                &track.strings,
                track.has_tuning,
                track.tuning_label.as_deref(),
                track.offset,
                track.fret_count,
                &track.staff_kept,
                &libraries(&track.staff_kept, true),
            ));
            xml.push_str("</Properties></Staff>");
            for staff in &track.staves {
                xml.push_str("<Staff><Properties>");
                xml.push_str(&staff_block_properties(
                    &staff.strings,
                    staff.has_tuning,
                    staff.tuning_label.as_deref(),
                    staff.offset,
                    staff.fret_count,
                    &staff.kept,
                    &libraries(&staff.kept, false),
                ));
                xml.push_str("</Properties></Staff>");
            }
            xml.push_str("</Staves>");
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
            // Its bypassed automations are written back inside the strip.
            let mut channel_strip = format!(
                "<RSE><ChannelStrip version=\"E56\"><Parameters>{}</Parameters>",
                rse_parameters(volume, pan)
            );
            if let Some(bypassed) = &track.gpif_bypassed_automations {
                channel_strip.push_str(&format!(
                    "<ByPassedAutomations>{}</ByPassedAutomations>",
                    escape_xml(bypassed)
                ));
            }
            channel_strip.push_str("</ChannelStrip></RSE>");
            xml.push_str(&channel_strip);
            // GuitarScore addition: the track's RSE sound and audio engine choice, kept
            // exactly as the file had them (`<Sounds>` holds CDATA and an open-ended effect
            // chain, so it is not modeled). New tracks have neither.
            if let Some(sounds) = track.gpif_sounds.as_deref() {
                xml.push_str(sounds);
            }
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
            if let Some(engine) = track.gpif_audio_engine.as_deref() {
                xml.push_str(engine);
            }
            // The lyric lines as typed, on the lyric track, as Guitar Pro keeps them (the
            // syllables are written on the beats as well).
            // Another track's typed lines (backing vocals) are written on it too.
            let lines = if usize::from(self.lyrics.track_choice) == t_idx {
                &self.lyrics.lines
            } else {
                &track.gpif_lyrics
            };
            if !lines.is_empty() {
                xml.push_str("<Lyrics dispatched=\"true\">");
                for number in 0..5_u8 {
                    let (start, text) = lines
                        .iter()
                        .find(|(line, _, _)| *line == number)
                        .map_or((0, ""), |(_, start, text)| {
                            (start.saturating_sub(1), text.as_str())
                        });
                    xml.push_str(&format!(
                        "<Line><Text>{}</Text><Offset>{start}</Offset></Line>",
                        escape_xml(text)
                    ));
                }
                xml.push_str("</Lyrics>");
            }
            // GuitarScore addition: the track's sound changes during the song, as Guitar
            // Pro's `Sound` automations on their bars (bar 0's included, as Guitar Pro
            // writes it). A save used to lose every sound change after the first bar.
            let mut sound_automations = String::new();
            for (bar, measure) in track.measures.iter().enumerate() {
                for change in &measure.sound_changes {
                    sound_automations.push_str(&format!(
                        "<Automation><Type>Sound</Type><Linear>false</Linear><Bar>{bar}</Bar>\
                         <Position>{}</Position><Visible>true</Visible><Value>{}</Value></Automation>",
                        change.position,
                        cdata(&change.value)
                    ));
                }
            }
            if !sound_automations.is_empty() {
                xml.push_str("<Automations>");
                xml.push_str(&sound_automations);
                xml.push_str("</Automations>");
            }
            xml.push_str("</Track>");
        }
        xml.push_str("</Tracks>");

        // Build all bars/voices/beats/notes/rhythms with sequential IDs
        let num_measures = self.measure_headers.len();

        let mut bars: Vec<BarXml> = Vec::new();
        let mut voices_xml: Vec<String> = Vec::new();
        let mut beats_xml: Vec<String> = Vec::new();
        let mut notes_xml: Vec<String> = Vec::new();
        let mut rhythms_xml: Vec<String> = Vec::new();

        let mut next_voice_id: i32 = 0;
        let mut next_beat_id: i32 = 0;
        let mut next_note_id: i32 = 0;
        let mut next_rhythm_id: i32 = 0;

        // A master bar lists one bar per staff, each track's staves in turn: the bar of
        // staff `s` of track `t` in measure `m` is number `(first_staff[t] + s) * measures + m`.
        let first_staff: Vec<usize> = self
            .tracks
            .iter()
            .scan(0_usize, |next, track| {
                let at = *next;
                *next = next.saturating_add(track.staves.len().saturating_add(1));
                Some(at)
            })
            .collect();
        for (t_idx, track) in self.tracks.iter().enumerate() {
            let first = first_staff.get(t_idx).copied().unwrap_or(0);
            let staves = std::iter::once(&track.measures)
                .chain(track.staves.iter().map(|staff| &staff.measures));
            for (s_idx, measures) in staves.enumerate() {
                // Velocity persists across all measures/voices of a staff
                let mut prev_velocity: i16 = FORTE;
                let origins = TieOrigins::of(measures);
                // By voice and string: whether the last note there starts a hammer-on or
                // pull-off (the next note on that string is where it goes).
                let mut hopo_from: std::collections::BTreeMap<(usize, i8), bool> =
                    std::collections::BTreeMap::new();

                for m_idx in 0..num_measures {
                    let bar_id = bar_id(first.saturating_add(s_idx), num_measures, m_idx)?;
                    let measure = measures.get(m_idx).ok_or_else(|| {
                        GpError::FormatError(format!(
                            "track {} has fewer measures than the song ({num_measures})",
                            t_idx.saturating_add(1)
                        ))
                    })?;
                    let mut voice_ids: Vec<String> = Vec::new();

                    for (v_idx, voice) in measure.voices.iter().enumerate() {
                        // GuitarScore fix: a voice holding only placeholder beats (as GP3-5
                        // store unused voices) is empty; writing the placeholders made them
                        // real beats.
                        if voice.beats.iter().all(|b| b.status == BeatStatus::Empty) {
                            voice_ids.push("-1".to_string());
                            continue;
                        }

                        let voice_id = next_id(&mut next_voice_id)?;
                        let mut beat_ids: Vec<i32> = Vec::new();

                        for (b_idx, beat) in voice
                            .beats
                            .iter()
                            .enumerate()
                            .filter(|(_, b)| b.status != BeatStatus::Empty)
                        {
                            let beat_id = next_id(&mut next_beat_id)?;
                            let rhythm_id = next_id(&mut next_rhythm_id)?;
                            beat_ids.push(beat_id);
                            let chord_id = beat.chord_diagram.as_ref().and_then(|chord| {
                                track_diagrams
                                    .get(t_idx)?
                                    .iter()
                                    .find(|(_, candidate)| candidate == chord)
                                    .map(|(id, _)| *id)
                            });

                            let destinations: Vec<i8> = beat
                                .notes
                                .iter()
                                .map(|note| note.string)
                                .filter(|string| {
                                    hopo_from.get(&(v_idx, *string)).copied().unwrap_or(false)
                                })
                                .collect();
                            if beat.status == BeatStatus::Rest {
                                hopo_from.retain(|(voice, _), _| *voice != v_idx);
                            }
                            for note in &beat.notes {
                                hopo_from.insert((v_idx, note.string), note.effect.hammer);
                            }
                            let export = build_beat_xml(
                                beat,
                                beat_id,
                                rhythm_id,
                                chord_id,
                                &mut next_note_id,
                                &mut prev_velocity,
                                |string| {
                                    (
                                        origins.starts(m_idx, v_idx, b_idx, string),
                                        destinations.contains(&string),
                                    )
                                },
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

                    // GuitarScore fix: a drum staff has the neutral (percussion) clef, as
                    // Guitar Pro writes it; it was written as a treble clef.
                    let clef = match measure.clef {
                        _ if track.percussion_track => "Neutral",
                        MeasureClef::Bass => "F4",
                        MeasureClef::Alto => "C4",
                        MeasureClef::Tenor => "C3",
                        MeasureClef::Treble => "G2",
                    };
                    bars.push(BarXml {
                        id: bar_id,
                        voices: voice_ids.join(" "),
                        simile: measure.simile_mark.clone(),
                        clef,
                        properties: measure.x_properties.clone(),
                    });
                }
            }
        }
        let staff_count = self
            .tracks
            .iter()
            .map(|track| track.staves.len().saturating_add(1))
            .sum::<usize>();

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
            let bar_ids = (0..staff_count)
                .map(|staff| bar_id(staff, num_measures, m_idx).map(|id| id.to_string()))
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
                // GuitarScore fix: the section's letter, as Guitar Pro writes it (it was
                // left out).
                let letter = marker
                    .letter
                    .as_deref()
                    .map(|letter| format!("<Letter>{}</Letter>", escape_xml(letter)))
                    .unwrap_or_default();
                xml.push_str(&format!(
                    "<Section>{letter}<Text>{}</Text></Section>",
                    escape_xml(&marker.title)
                ));
            }
            if !mh.fermatas.is_empty() {
                xml.push_str("<Fermatas>");
                for (ftype, offset, length) in &mh.fermatas {
                    let length = if length.is_empty() {
                        String::new()
                    } else {
                        format!("<Length>{}</Length>", escape_xml(length))
                    };
                    xml.push_str(&format!(
                        "<Fermata><Type>{ftype}</Type><Offset>{offset}</Offset>{length}</Fermata>"
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
            // GuitarScore fix: codas, segnos and fine are targets (all were written as jumps),
            // and every sign of the bar is written.
            if !mh.directions.is_empty() {
                xml.push_str("<Directions>");
                for sign in mh.directions.iter().filter(|sign| sign.is_target()) {
                    xml.push_str(&format!("<Target>{}</Target>", direction_sign_str(sign)));
                }
                for sign in mh.directions.iter().filter(|sign| !sign.is_target()) {
                    xml.push_str(&format!("<Jump>{}</Jump>", direction_sign_str(sign)));
                }
                xml.push_str("</Directions>");
            }
            // GuitarScore addition: Guitar Pro's layout properties (the design mode's
            // song-wide bar width, beaming rules), written back as they were read.
            xml.push_str(&xproperties_xml(&mh.x_properties));
            xml.push_str("</MasterBar>");
        }
        xml.push_str("</MasterBars>");

        // Bars
        xml.push_str("<Bars>");
        // GuitarScore fix: the simile mark is stored, not parsed back out of a string (the
        // slicing could panic on a quote in the value), and it is escaped.
        for bar in &bars {
            xml.push_str(&format!(
                "<Bar id=\"{}\"><Clef>{}</Clef><Voices>{}</Voices>",
                bar.id, bar.clef, bar.voices
            ));
            if let Some(simile) = &bar.simile {
                xml.push_str(&format!("<SimileMark>{}</SimileMark>", escape_xml(simile)));
            }
            // GuitarScore addition: Guitar Pro's layout properties (the design mode's bar
            // width), written back as they were read.
            xml.push_str(&xproperties_xml(&bar.properties));
            xml.push_str("</Bar>");
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

        // GuitarScore addition: Guitar Pro's score views, kept verbatim.
        if let Some(score_views) = &self.gpif_score_views {
            xml.push_str(score_views);
        }
        xml.push_str("</GPIF>");
        Ok(xml)
    }
}

/// The chord diagrams of a track's beats, each once, in beat order.
/// A track's diagrams numbered for writing: the file's, with their ids, then every diagram
/// a beat uses that the file did not have, with the next free ids. The flag tells whether
/// there is such a new one.
fn numbered_diagrams(track: &Track) -> (Vec<(i32, ChordDiagram)>, bool) {
    let mut numbered = track.gpif_diagram_ids.clone();
    let mut next = numbered
        .iter()
        .map(|(id, _)| *id)
        .max()
        .map_or(0, |id| id.saturating_add(1));
    let mut new = false;
    for diagram in collect_diagrams(track) {
        if !numbered.iter().any(|(_, known)| *known == diagram) {
            numbered.push((next, diagram));
            next = next.saturating_add(1);
            new = true;
        }
    }
    (numbered, new)
}

/// A `DiagramCollection` property holding `diagrams` with their ids.
fn diagram_collection_xml(diagrams: &[(i32, ChordDiagram)]) -> String {
    let mut xml = String::from("<Property name=\"DiagramCollection\"><Items>");
    for (id, diagram) in diagrams {
        xml.push_str(&format!(
            "<Item id=\"{id}\" name=\"{}\"><Diagram stringCount=\"{}\" \
             fretCount=\"{}\" baseFret=\"{}\">",
            escape_xml(&diagram.name),
            diagram.string_count,
            diagram.fret_count,
            diagram.base_fret
        ));
        for &(string, fret) in &diagram.frets {
            xml.push_str(&format!("<Fret string=\"{string}\" fret=\"{fret}\"/>"));
        }
        xml.push_str("</Diagram></Item>");
    }
    xml.push_str("</Items></Property>");
    xml
}

fn collect_diagrams(track: &Track) -> Vec<ChordDiagram> {
    let mut diagrams: Vec<ChordDiagram> = Vec::new();
    for measure in track.all_measures() {
        for voice in &measure.voices {
            for beat in &voice.beats {
                let Some(chord) = &beat.chord_diagram else {
                    continue;
                };
                if !diagrams.iter().any(|known| known == chord) {
                    diagrams.push(chord.clone());
                }
            }
        }
    }
    diagrams
}

/// GuitarScore addition: one master effect as GPIF: its bypass, parameters, bypassed
/// automations and automation points. `volume_pan`, on the volume and pan effect, replaces
/// its parameters and the bar-0 volume and pan automation with the model's values.
fn master_effect_xml(effect: &MasterEffectInfo, volume_pan: Option<(f32, f32)>) -> String {
    let mut xml = format!("<Effect id=\"{}\">", escape_xml(&effect.id));
    if effect.by_pass {
        xml.push_str("<ByPass />");
    }
    match (volume_pan, &effect.parameters) {
        (Some((volume, pan)), _) => xml.push_str(&format!(
            "<Parameters>{} {}</Parameters>",
            volume.clamp(0.0, 1.0),
            pan.clamp(0.0, 1.0)
        )),
        (None, Some(parameters)) => {
            xml.push_str(&format!(
                "<Parameters>{}</Parameters>",
                escape_xml(parameters)
            ));
        }
        (None, None) => {}
    }
    if let Some(bypassed) = &effect.bypassed_automations {
        xml.push_str(&format!(
            "<ByPassedAutomations>{}</ByPassedAutomations>",
            escape_xml(bypassed)
        ));
    }
    if !effect.automations.is_empty() {
        xml.push_str("<Automations>");
        for automation in &effect.automations {
            let value = match volume_pan {
                Some((volume, pan)) if automation.bar == 0 && automation.position <= 0.0 => {
                    match automation.automation_type.as_str() {
                        "DSPParam_00" => format!("{}", volume.clamp(0.0, 1.0)),
                        "DSPParam_01" => format!("{}", pan.clamp(0.0, 1.0)),
                        _ => automation.value.clone(),
                    }
                }
                _ => automation.value.clone(),
            };
            let text = automation
                .text
                .as_deref()
                .map(|text| format!("<Text>{}</Text>", cdata(text)))
                .unwrap_or_default();
            xml.push_str(&format!(
                "<Automation><Type>{}</Type><Linear>{}</Linear><Bar>{}</Bar><Position>{}</Position><Visible>{}</Visible>{text}<Value>{}</Value></Automation>",
                escape_xml(&automation.automation_type),
                automation.linear,
                automation.bar,
                automation.position,
                automation.visible,
                escape_xml(&value)
            ));
        }
        xml.push_str("</Automations>");
    }
    xml.push_str("</Effect>");
    xml
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

/// GuitarScore addition: whether a written systems layout starts a new line before the
/// last bar (so it disagrees with a model whose measures have no break).
fn layout_implies_break(layout: &str, measures: usize) -> bool {
    layout
        .split_whitespace()
        .next()
        .and_then(|count| count.parse::<usize>().ok())
        .filter(|count| *count > 0 && *count < measures)
        .is_some()
}

/// GuitarScore addition: Guitar Pro's opaque layout properties (`<XProperties>`), written
/// back exactly as they were read (the design mode's bar scale is one of them). Empty when
/// there are none.
fn xproperties_xml(properties: &[XProperty]) -> String {
    if properties.is_empty() {
        return String::new();
    }
    let mut xml = String::from("<XProperties>");
    for property in properties {
        let value = escape_xml(&property.value);
        let element = match property.kind.as_str() {
            "Int" => "Int",
            "Float" => "Float",
            "Text" => "Text",
            _ => "Double",
        };
        xml.push_str(&format!(
            "<XProperty id=\"{}\"><{element}>{value}</{element}></XProperty>",
            escape_xml(&property.id)
        ));
    }
    xml.push_str("</XProperties>");
    xml
}

/// A staff's capo and tuning as GPIF properties (the tuning only when the file had one).
fn staff_properties(
    strings: &[(i8, i8)],
    has_tuning: bool,
    label: Option<&str>,
    capo: i32,
) -> String {
    let mut xml = format!(
        "<Property name=\"CapoFret\"><Fret>{}</Fret></Property>",
        capo.clamp(0, 127)
    );
    if has_tuning {
        // GuitarScore fix: GPIF lists pitches from the lowest string up; `strings` starts
        // at string 1, the highest.
        let pitches: Vec<String> = strings.iter().rev().map(|(_, p)| p.to_string()).collect();
        // GuitarScore addition: the tuning's name, if the file gave one.
        let label = label
            .map(|l| format!("<Label>{}</Label>", escape_xml(l)))
            .unwrap_or_default();
        xml.push_str(&format!(
            "<Property name=\"Tuning\"><Pitches>{}</Pitches>{label}</Property>",
            pitches.join(" ")
        ));
    }
    xml
}

/// GuitarScore addition: a staff block as Guitar Pro 7 writes it (`<Staff><Properties>`):
/// the modeled capo, fret count and tuning, then the parts kept from the file (a partial
/// capo, a tuning shown with flats, its instrument and label visibility, its name, and the
/// whole chord and diagram libraries).
fn staff_block_properties(
    strings: &[(i8, i8)],
    has_tuning: bool,
    label: Option<&str>,
    capo: i32,
    fret_count: u8,
    kept: &StaffKept,
    libraries: &[String],
) -> String {
    let mut xml = format!(
        "<Property name=\"CapoFret\"><Fret>{}</Fret></Property>\
         <Property name=\"FretCount\"><Number>{}</Number></Property>",
        capo.clamp(0, 127),
        fret_count
    );
    if let Some(fret) = kept.partial_capo_fret {
        xml.push_str(&format!(
            "<Property name=\"PartialCapoFret\"><Fret>{fret}</Fret></Property>"
        ));
    }
    if let Some(flags) = &kept.partial_capo_flags {
        xml.push_str(&format!(
            "<Property name=\"PartialCapoStringFlags\"><Bitset>{}</Bitset></Property>",
            escape_xml(flags)
        ));
    }
    if has_tuning {
        // GPIF lists pitches from the lowest string up; `strings` starts at string 1.
        let pitches: Vec<String> = strings
            .iter()
            .rev()
            .map(|(_, pitch)| pitch.to_string())
            .collect();
        let flat = if kept.tuning_flat_child {
            "<Flat></Flat>"
        } else {
            ""
        };
        let instrument = kept
            .tuning_instrument
            .as_deref()
            .map(|instrument| format!("<Instrument>{}</Instrument>", escape_xml(instrument)))
            .unwrap_or_default();
        // Guitar Pro writes a `<Label>`, empty when the tuning has no name.
        let label = format!("<Label>{}</Label>", escape_xml(label.unwrap_or_default()));
        let visible = kept
            .label_visible
            .as_deref()
            .map(|visible| format!("<LabelVisible>{}</LabelVisible>", escape_xml(visible)))
            .unwrap_or_default();
        xml.push_str(&format!(
            "<Property name=\"Tuning\"><Pitches>{}</Pitches>{flat}{instrument}{label}{visible}</Property>",
            pitches.join(" ")
        ));
    }
    for library in libraries {
        xml.push_str(library);
    }
    if kept.tuning_flat {
        xml.push_str("<Property name=\"TuningFlat\"><Enable /></Property>");
    }
    if let Some(name) = &kept.name {
        xml.push_str(&format!("<Name>{}</Name>", cdata(name)));
    }
    xml
}
