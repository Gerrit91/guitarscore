//! Converting the GPIF XML of GP6 and GP7+ files into the song data: the song, master
//! bars and tracks here; beats in [`beats`], notes in [`notes`], value conversions in
//! [`helpers`].

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use std::collections::HashMap;

use crate::io::gpif::*;
use crate::io::primitive::{
    clamp_i16, low_i8, low_i16, low_u8, low_u16, round_f64_to_i8, round_f64_to_i16,
    round_f64_to_i32, truncate_f64_to_i32,
};
use crate::model::{
    beat::{Beat as SongBeat, LegatoEffect, Voice as SongVoice},
    effects::*,
    enums::*,
    headers::{Marker, MeasureHeader},
    key_signature::*,
    measure::Measure,
    midi::MidiChannel,
    note::Note as SongNote,
    song::*,
    track::Track as SongTrack,
};

pub(crate) trait SongGpifOps {
    fn read_gpif(&mut self, gpif: &Gpif);
}

mod beats;
#[cfg(test)]
mod display_tests;
#[cfg(test)]
mod effect_tests;
mod helpers;
mod notes;
#[cfg(test)]
mod page_tests;
#[cfg(test)]
mod section_tests;
#[cfg(test)]
mod tuning_tests;
#[cfg(test)]
mod vibrato_tests;

use beats::convert_beat;
pub(crate) use beats::flip_note_strings;
use helpers::*;
pub(crate) use helpers::{CHORUS_PROPERTY, REVERB_PROPERTY, build_note_bend, build_whammy_effect};
use notes::convert_note;

// ---------------------------------------------------------------------------
// Main conversion
// ---------------------------------------------------------------------------

impl SongGpifOps for Song {
    fn read_gpif(&mut self, gpif: &Gpif) {
        // 1. Metadata
        self.name = gpif.score.title.clone();
        self.subtitle = gpif.score.sub_title.clone();
        self.artist = gpif.score.artist.clone();
        self.album = gpif.score.album.clone();
        // GuitarScore fix: use the same fields as the GP3-5 reader (`writer` is the tab
        // author, `instructions` the instructions); `writer` used to receive the composer.
        self.words = gpif.score.words.clone();
        self.author = gpif.score.music.clone();
        if !gpif.score.words_and_music.is_empty() {
            if self.words.is_empty() {
                self.words = gpif.score.words_and_music.clone();
            }
            if self.author.is_empty() {
                self.author = gpif.score.words_and_music.clone();
            }
        }
        self.writer = gpif.score.tabber.clone();
        self.copyright = gpif.score.copyright.clone();
        self.instructions = gpif.score.instructions.clone();
        // Notices
        if !gpif.score.notices.is_empty() {
            self.notice = gpif.score.notices.lines().map(|l| l.to_string()).collect();
        }

        // 2. Tempo from MasterTrack automations
        if let Some(automations) = &gpif.master_track.automations {
            for auto in &automations.automations {
                if auto.automation_type == "Tempo"
                    && auto.bar == 0
                    && let Some(tempo_str) = automation_tempo(auto)
                {
                    // An unreadable tempo counts as 120 BPM.
                    self.tempo = tempo_str
                        .parse::<f64>()
                        .map_or(120, |v| clamp_i16(truncate_f64_to_i32(v)));
                }
            }
        }

        // 2a. GuitarScore addition: the master volume and pan (`I01_VolumeAndPan`).
        self.master_volume_pan = gpif
            .master_track
            .rse
            .as_ref()
            .and_then(|rse| rse.master.as_ref())
            .and_then(|master| {
                master
                    .effects
                    .iter()
                    .find(|effect| effect.id == MASTER_VOLUME_PAN)
            })
            .and_then(|effect| effect.parameters.as_deref())
            .and_then(master_volume_pan);

        // 2b. SyncPoint anchors (GP8 audio sync): musical position → audio
        // frame offset into the embedded backing track.
        self.sync_points.clear();
        if let Some(automations) = &gpif.master_track.automations {
            for auto in &automations.automations {
                if auto.automation_type != "SyncPoint" {
                    continue;
                }
                let Some(value) = &auto.value else { continue };
                let Some(bar_index) = value.bar_index else {
                    continue;
                };
                self.sync_points.push(SyncPoint {
                    bar_index,
                    bar_occurrence: value.bar_occurrence.unwrap_or(0),
                    modified_tempo: value.modified_tempo,
                    original_tempo: value.original_tempo,
                    frame_offset: value.frame_offset,
                });
            }
        }

        // 3. Build lookup maps
        let bars_map: HashMap<i32, &Bar> = gpif.bars.bars.iter().map(|b| (b.id, b)).collect();
        let voices_map: HashMap<i32, &Voice> =
            gpif.voices.voices.iter().map(|v| (v.id, v)).collect();
        let beats_map: HashMap<i32, &Beat> = gpif.beats.beats.iter().map(|b| (b.id, b)).collect();
        let notes_map: HashMap<i32, &Note> = gpif.notes.notes.iter().map(|n| (n.id, n)).collect();
        let rhythms_map: HashMap<i32, &Rhythm> =
            gpif.rhythms.rhythms.iter().map(|r| (r.id, r)).collect();

        // 4. Measure Headers (MasterBars) — also collects per-track bar IDs
        self.measure_headers.clear();
        let num_tracks = gpif.tracks.tracks.len();
        let mut track_bar_ids: Vec<Vec<i32>> = vec![Vec::new(); num_tracks];

        for (mh_idx, mb) in gpif.master_bars.master_bars.iter().enumerate() {
            let mut mh = MeasureHeader {
                number: u16::try_from(mh_idx.saturating_add(1)).unwrap_or(u16::MAX),
                ..Default::default()
            };

            // GuitarScore fix: the measure's feel (`<TripletFeel>`) was dropped.
            mh.triplet_feel = mb
                .triplet_feel
                .as_deref()
                .map_or(TripletFeel::None, TripletFeel::from_gpif_name);
            if mh_idx == 0 {
                self.triplet_feel = mh.triplet_feel.clone();
            }

            // Time signature
            if let Some((numerator, denominator)) = mb.time.split_once('/')
                && !denominator.contains('/')
            {
                // Values that do not fit are truncated, as before.
                mh.time_signature.numerator = low_i8(numerator.parse().unwrap_or(4));
                mh.time_signature.denominator.value = low_u16(denominator.parse().unwrap_or(4));
            }

            // Key signature
            if let Some(key) = &mb.key {
                mh.key_signature.key = low_i8(key.accidental_count);
                mh.key_signature.is_minor = key.mode == "Minor";
            }

            // Tempo at this bar
            if let Some(automations) = &gpif.master_track.automations {
                for auto in &automations.automations {
                    if auto.automation_type == "Tempo"
                        && usize::try_from(auto.bar).ok() == Some(mh_idx)
                        && let Some(tempo_str) = automation_tempo(auto)
                    {
                        mh.tempo = truncate_f64_to_i32(tempo_str.parse::<f64>().unwrap_or(0.0));
                    }
                }
            }

            // Repeat
            if let Some(repeat) = &mb.repeat {
                mh.repeat_open = repeat.start == "true";
                if repeat.end == "true" {
                    // GPIF counts plays (2 for a plain repeat); the model counts repeats
                    // (jumps back), as GP3-5 do after reading.
                    mh.repeat_close = repeat.count.saturating_sub(1).clamp(1, 127) as i8;
                }
            }

            // Alternate endings (volta)
            if let Some(alt_str) = &mb.alternate_endings {
                let mut bitmask: u8 = 0;
                for tok in alt_str.split_whitespace() {
                    if let Ok(n) = tok.parse::<u8>()
                        && n > 0
                        && n <= 8
                    {
                        bitmask |= 1_u8 << n.saturating_sub(1);
                    }
                }
                mh.repeat_alternative = bitmask;
            }

            // Double bar
            mh.double_bar = mb.double_bar.is_some();

            // Marker (Section)
            if let Some(section) = &mb.section {
                // GuitarScore fix: Guitar Pro writes the text inside a CDATA block on its
                // own lines (`<Text>\n<![CDATA[Chorus]]>\n</Text>`), so the reader used to
                // keep the newlines around the title. A section title is one line: trim the
                // formatting whitespace and fall back to the letter when the text is empty.
                let text = section
                    .text
                    .as_deref()
                    .map(str::trim)
                    .filter(|text| !text.is_empty());
                let letter = section
                    .letter
                    .as_deref()
                    .map(str::trim)
                    .filter(|letter| !letter.is_empty());
                let title = text.or(letter).unwrap_or("Section");
                // GP6/7 GPIF XML does not include the marker color.
                mh.marker = Some(Marker {
                    title: title.to_string(),
                    color: Marker::DEFAULT_COLOR,
                });
            }

            // Fermatas
            if let Some(fermatas_w) = &mb.fermatas {
                for f in &fermatas_w.fermatas {
                    let ftype = f.fermata_type.as_deref().unwrap_or("Medium").to_string();
                    let offset = f.offset.as_deref().unwrap_or("").to_string();
                    mh.fermatas.push((ftype, offset));
                }
            }

            // Free time
            mh.free_time = mb.free_time.is_some();

            // Directions
            if let Some(dirs) = &mb.directions {
                if let Some(target) = &dirs.target {
                    mh.direction = parse_direction_sign(target);
                } else if let Some(jump) = &dirs.jump {
                    mh.direction = parse_direction_sign(jump);
                }
            }

            // Per-track bar IDs
            let bar_ids = parse_ids(&mb.bars);
            for (ids, &bar_id) in track_bar_ids.iter_mut().zip(&bar_ids) {
                ids.push(bar_id);
            }

            self.measure_headers.push(mh);
        }

        let num_measures = self.measure_headers.len();

        // 5. Tracks
        self.tracks.clear();

        for (t_idx, g_track) in gpif.tracks.tracks.iter().enumerate() {
            let mut track = SongTrack {
                name: g_track.name.clone(),
                short_name: g_track.short_name.clone(),
                number: i32::try_from(t_idx.saturating_add(1)).unwrap_or(i32::MAX),
                ..Default::default()
            };

            // Color
            if let Some(color_str) = &g_track.color {
                let rgb: Vec<i32> = color_str
                    .split_whitespace()
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if let [r, g, b] = rgb[..] {
                    track.color = r
                        .wrapping_mul(65536)
                        .wrapping_add(g.wrapping_mul(256))
                        .wrapping_add(b);
                }
            }

            // Tuning: GP6 track-level properties, GP7 staves
            // GuitarScore fix: `..Default::default()` above fills `strings` with E standard,
            // so the emptiness checks below never fired and the file's tuning was ignored.
            track.strings = Vec::new();
            let mut capo = None;
            let mut sends = (None, None);
            if let Some(props) = &g_track.properties {
                sends = extract_sends(&props.properties);
                track.strings = extract_tuning(&props.properties);
                track.tuning_label = extract_tuning_label(&props.properties);
                capo = extract_capo(&props.properties);
            }
            if (track.strings.is_empty() || capo.is_none())
                && let Some(staves) = &g_track.staves
            {
                for staff in &staves.staves {
                    if let Some(props) = &staff.properties {
                        if track.strings.is_empty() {
                            track.strings = extract_tuning(&props.properties);
                            track.tuning_label = extract_tuning_label(&props.properties);
                        }
                        if capo.is_none() {
                            capo = extract_capo(&props.properties);
                        }
                        if !track.strings.is_empty() && capo.is_some() {
                            break;
                        }
                    }
                }
            }
            if let Some(capo) = capo {
                track.offset = capo;
            }
            if track.strings.is_empty() {
                // GuitarScore: keep placeholder strings (the code base indexes them), but
                // record that the file has no tuning so it is neither shown nor written.
                track.strings = vec![(1, 64), (2, 59), (3, 55), (4, 50), (5, 45), (6, 40)];
                track.has_tuning = false;
            }

            track.fret_count = 24;

            // MIDI
            if let Some(gm) = &g_track.general_midi {
                if let Some(ch) = gm.primary_channel {
                    // A negative channel wraps to a huge index, which writing refuses.
                    track.channel_index = usize::try_from(ch).unwrap_or(usize::MAX);
                    track.percussion_track = ch == 9;
                }
                track.midi_program_gpif = gm.program;
                if let Some(port) = gm.port {
                    track.port = low_u8(port);
                }
            }
            // GP7: the MIDI port and channels live in `<MidiConnection>`; GP6 uses
            // `<GeneralMidi>`.
            if g_track.general_midi.is_none()
                && let Some(connection) = &g_track.midi_connection
            {
                if let Some(ch) = connection.primary_channel {
                    // A negative channel wraps to a huge index, which writing refuses.
                    track.channel_index = usize::try_from(ch).unwrap_or(usize::MAX);
                    track.percussion_track = ch == 9;
                }
                if let Some(port) = connection.port {
                    track.port = low_u8(port);
                }
            }

            // GP7: whether the track is soloed or muted.
            if let Some(state) = g_track.playback_state.as_deref() {
                let state = state.trim();
                track.solo = state == "Solo";
                track.mute = state == "Mute";
            }

            // GuitarScore: GP7 names the program in the track's first sound instead.
            if track.midi_program_gpif.is_none() {
                track.midi_program_gpif = g_track
                    .sounds
                    .as_ref()
                    .and_then(|s| s.sounds.first())
                    .and_then(|sound| sound.midi.as_ref())
                    .and_then(|midi| midi.program);
            }

            // GuitarScore: instrument type and name (GP7). Drum kits are percussion even
            // when their MIDI channel is not 10 (GP7 stores channels elsewhere).
            if let Some(set) = &g_track.instrument_set {
                track.instrument_type = set.kind.clone();
                track.instrument_name = set.name.clone();
                if set.kind.as_deref() == Some("drumKit") {
                    track.percussion_track = true;
                }
            }

            // Transpose
            if let Some(tr) = &g_track.transpose {
                track.transpose_chromatic = tr.chromatic.unwrap_or(0);
                track.transpose_octave = tr.octave.unwrap_or(0);
            }

            // GP7: the mixer volume and pan are parameters of the RSE channel strip.
            let (volume, pan) = g_track
                .rse
                .as_ref()
                .and_then(|rse| rse.channel_strip.as_ref())
                .and_then(|strip| strip.parameters.as_deref())
                .map(mixer_from_parameters)
                .unwrap_or((None, None));
            apply_track_mix(&mut self.channels, track.channel_index, volume, pan);
            apply_track_sends(&mut self.channels, track.channel_index, sends);

            // Current dynamic (persists across beats)
            let mut current_velocity: i16 = FORTE;

            // Measures
            for m_idx in 0..num_measures {
                let mut measure = Measure {
                    number: m_idx.saturating_add(1),
                    track_index: t_idx,
                    ..Default::default()
                };

                if let Some(header) = self.measure_headers.get(m_idx) {
                    measure.time_signature = header.time_signature.clone();
                    measure.key_signature = header.key_signature.clone();
                }

                let bar_id = track_bar_ids
                    .get(t_idx)
                    .and_then(|ids| ids.get(m_idx))
                    .copied()
                    .unwrap_or(-1);

                if let Some(bar) = bars_map.get(&bar_id) {
                    measure.simile_mark = bar.simile_mark.clone();
                    let voice_ids = parse_ids(&bar.voices);
                    measure.voices.clear();

                    for &vid in &voice_ids {
                        if vid < 0 {
                            continue;
                        }
                        let mut s_voice = SongVoice::default();

                        if let Some(g_voice) = voices_map.get(&vid) {
                            let beat_ids = parse_ids(&g_voice.beats);

                            for &bid in &beat_ids {
                                if let Some(g_beat) = beats_map.get(&bid) {
                                    let s_beat = convert_beat(
                                        g_beat,
                                        &rhythms_map,
                                        &notes_map,
                                        &mut current_velocity,
                                    );
                                    s_voice.beats.push(s_beat);
                                }
                            }
                        }
                        measure.voices.push(s_voice);
                    }
                }
                track.measures.push(measure);
            }
            track.bars_per_system = g_track.bars_per_system.filter(|bars| *bars > 0);
            apply_breaks(
                &mut track.measures,
                g_track.systems_layout.as_deref(),
                g_track.page_breaks.as_deref(),
            );
            self.tracks.push(track);
        }
        // GuitarScore fix: GPIF numbers strings from the lowest; the legacy model's string 1
        // is the highest (as its tuning list is, see `extract_tuning`).
        flip_note_strings(&mut self.tracks);
        // GuitarScore: a grace beat becomes a grace note attached to its main note where
        // nothing is lost, as the editor works with attached grace notes. Writing turns
        // them back into grace beats (`Song::with_grace_beats`).
        self.fold_grace_beats();
    }
}

/// The id of the master effect that holds the master volume and pan.
pub(crate) const MASTER_VOLUME_PAN: &str = "I01_VolumeAndPan";

/// The master volume and pan from `I01_VolumeAndPan`'s parameters (`volume pan`, each 0.0
/// to 1.0), or `None` if they are not two finite numbers.
fn master_volume_pan(text: &str) -> Option<(f32, f32)> {
    let mut values = text
        .split_whitespace()
        .map(|part| part.parse::<f32>().ok().filter(|v| v.is_finite()));
    let volume = values.next()??;
    let pan = values.next()??;
    Some((volume.clamp(0.0, 1.0), pan.clamp(0.0, 1.0)))
}

/// The mixer values in an RSE `<ChannelStrip><Parameters>` list: `(volume, pan)` as MIDI
/// values. Index 11 is the balance and index 12 the volume, each 0.0 to 1.0 (checked
/// against Guitar Pro 7 files and alphaTab's GPIF importer).
fn mixer_from_parameters(text: &str) -> (Option<i8>, Option<i8>) {
    let mut volume = None;
    let mut pan = None;
    for (index, part) in text.split_whitespace().enumerate() {
        if index != 11 && index != 12 {
            continue;
        }
        let Some(value) = part.parse::<f64>().ok().filter(|v| v.is_finite()) else {
            continue;
        };
        let midi = round_f64_to_i8(value.clamp(0.0, 1.0) * 127.0).clamp(0, 127);
        if index == 11 {
            pan = Some(midi);
        } else {
            volume = Some(midi);
        }
    }
    (volume, pan)
}

/// Puts the GuitarScore reverb and chorus sends into the track's channel (which
/// [`apply_track_mix`] has created).
fn apply_track_sends(
    channels: &mut [MidiChannel],
    index: usize,
    (reverb, chorus): (Option<i8>, Option<i8>),
) {
    if let Some(channel) = channels.get_mut(index) {
        if let Some(reverb) = reverb {
            channel.reverb = reverb;
        }
        if let Some(chorus) = chorus {
            channel.chorus = chorus;
        }
    }
}

/// Most MIDI channels a GPIF track may name; a corrupt index is ignored instead of
/// allocating a huge channel table.
const MAX_MIX_CHANNELS: usize = 64;

/// Puts the track's GP7 mixer values into the channel it plays on, creating the channel
/// entry when the file has none (GP7 files do not store a channel table).
fn apply_track_mix(
    channels: &mut Vec<MidiChannel>,
    index: usize,
    volume: Option<i8>,
    pan: Option<i8>,
) {
    if index >= MAX_MIX_CHANNELS {
        return;
    }
    while channels.len() <= index {
        channels.push(MidiChannel::default());
    }
    if let Some(channel) = channels.get_mut(index) {
        if let Some(volume) = volume {
            channel.volume = volume;
        }
        if let Some(pan) = pan {
            channel.balance = pan;
        }
    }
}

/// GuitarScore addition: Guitar Pro's systems layout (bars per line, `"4 4 3"`) as a line
/// break at the first bar of every line after the first, so the page is broken where
/// Guitar Pro breaks it; and the GuitarScore page breaks. Odd counts (0, unreadable) end
/// the list; when it runs out before the last bar, the rest is laid out freely.
fn apply_breaks(measures: &mut [Measure], systems: Option<&str>, pages: Option<&str>) {
    let mut at = 0_usize;
    for count in systems.unwrap_or_default().split_whitespace() {
        let Some(count) = count.parse::<usize>().ok().filter(|count| *count > 0) else {
            break;
        };
        at = at.saturating_add(count);
        match measures.get_mut(at) {
            Some(measure) => measure.line_break = LineBreak::Break,
            None => break,
        }
    }
    for index in pages.unwrap_or_default().split_whitespace() {
        if let Some(measure) = index
            .parse::<usize>()
            .ok()
            .filter(|index| *index > 0)
            .and_then(|index| measures.get_mut(index))
        {
            measure.line_break = LineBreak::Page;
        }
    }
}
