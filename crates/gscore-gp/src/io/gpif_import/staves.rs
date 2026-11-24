//! GuitarScore addition: a track's staves. A GPIF master bar lists one bar per staff, not
//! per track, so a track with two staves (a piano's or synth's treble and bass) takes two
//! places; every staff's bars become measures, the first staff's on the track itself and
//! the others' in [`crate::model::Track::staves`], each with its own tuning and capo.

use std::collections::HashMap;

use super::{convert_beat, extract_capo, extract_tuning, extract_tuning_label, parse_ids};
use crate::io::gpif::xproperties;
use crate::io::gpif::{Bar, Beat, Note, Property, Rhythm, Track, Voice};
use crate::model::{
    beat::Voice as SongVoice,
    chord::ChordDiagram,
    enums::MeasureClef,
    headers::MeasureHeader,
    measure::Measure,
    track::{Staff, StaffKept},
};

use crate::io::FORTE;

/// The parts of a staff's `<Properties>` GuitarScore does not model, kept as written (see
/// [`StaffKept`]).
pub(super) fn kept_staff(
    properties: &[Property],
    name: Option<&str>,
    libraries: Vec<String>,
) -> StaffKept {
    let find = |property_name: &str| {
        properties
            .iter()
            .find(|property| property.name == property_name)
    };
    let tuning = find("Tuning");
    StaffKept {
        partial_capo_fret: find("PartialCapoFret").and_then(|property| property.fret),
        partial_capo_flags: find("PartialCapoStringFlags")
            .and_then(|property| property.bitset.clone()),
        tuning_flat: find("TuningFlat")
            .and_then(|property| property.enable.as_ref())
            .is_some(),
        tuning_flat_child: tuning.and_then(|property| property.flat.as_ref()).is_some(),
        tuning_instrument: tuning.and_then(|property| property.instrument.clone()),
        label_visible: tuning.and_then(|property| property.label_visible.clone()),
        name: name.map(str::to_owned),
        chord_libraries: libraries,
    }
}

/// The number of frets in a staff's properties (`FretCount`), or `None`.
pub(super) fn extract_fret_count(properties: &[Property]) -> Option<u8> {
    properties
        .iter()
        .find(|property| property.name == "FretCount")
        .and_then(|property| property.number)
        .and_then(|count| u8::try_from(count.clamp(1, 48)).ok())
}

/// The file's bars, voices, beats, notes and rhythms by id.
pub(super) struct Lookup<'a> {
    pub(super) bars: HashMap<i32, &'a Bar>,
    pub(super) voices: HashMap<i32, &'a Voice>,
    pub(super) beats: HashMap<i32, &'a Beat>,
    pub(super) notes: HashMap<i32, &'a Note>,
    pub(super) rhythms: HashMap<i32, &'a Rhythm>,
}

/// How many staves `track` has (at least one).
pub(super) fn staff_count(track: &Track) -> usize {
    track
        .staves
        .as_ref()
        .map_or(1, |staves| staves.staves.len().max(1))
}

/// Where each track's first staff sits in a master bar's list of bars.
pub(super) fn first_staves(tracks: &[Track]) -> Vec<usize> {
    tracks
        .iter()
        .scan(0_usize, |next, track| {
            let at = *next;
            *next = next.saturating_add(staff_count(track));
            Some(at)
        })
        .collect()
}

/// The measures of one staff: its bar in each master bar (`bar_ids`, -1 for none), read
/// against the song's measure headers.
pub(super) fn measures(
    lookup: &Lookup<'_>,
    headers: &[MeasureHeader],
    track_index: usize,
    bar_ids: &[i32],
    diagrams: &HashMap<i32, ChordDiagram>,
) -> Vec<Measure> {
    // The dynamic persists across the staff's beats.
    let mut velocity: i16 = FORTE;
    let mut measures = Vec::with_capacity(headers.len());
    for (index, header) in headers.iter().enumerate() {
        let mut measure = Measure {
            number: index.saturating_add(1),
            track_index,
            time_signature: header.time_signature.clone(),
            key_signature: header.key_signature.clone(),
            ..Default::default()
        };
        let bar_id = bar_ids.get(index).copied().unwrap_or(-1);
        if let Some(bar) = lookup.bars.get(&bar_id) {
            measure.simile_mark = bar.simile_mark.clone();
            // GuitarScore addition: Guitar Pro's layout properties for this bar (the
            // design mode's width scale), kept verbatim.
            measure.x_properties = xproperties(bar.x_properties.as_ref());
            measure.has_clef = bar.clef.is_some();
            measure.clef = match bar.clef.as_deref() {
                Some("F4") => MeasureClef::Bass,
                Some("C4") => MeasureClef::Alto,
                Some("C3") => MeasureClef::Tenor,
                _ => MeasureClef::Treble,
            };
            measure.voices.clear();
            for &voice_id in &parse_ids(&bar.voices) {
                if voice_id < 0 {
                    continue;
                }
                let mut voice = SongVoice::default();
                if let Some(g_voice) = lookup.voices.get(&voice_id) {
                    for beat_id in parse_ids(&g_voice.beats) {
                        let Some(g_beat) = lookup.beats.get(&beat_id) else {
                            continue;
                        };
                        let mut beat =
                            convert_beat(g_beat, &lookup.rhythms, &lookup.notes, &mut velocity);
                        if let Some(id) = g_beat
                            .chord
                            .as_deref()
                            .and_then(|id| id.trim().parse::<i32>().ok())
                        {
                            beat.chord_diagram = diagrams.get(&id).cloned();
                        }
                        voice.beats.push(beat);
                    }
                }
                measure.voices.push(voice);
            }
        }
        measures.push(measure);
    }
    measures
}

/// The staves of `g_track` after its first, their measures read from `bar_ids` (one list
/// per staff, the first staff's included).
pub(super) fn further_staves(
    lookup: &Lookup<'_>,
    headers: &[MeasureHeader],
    track_index: usize,
    g_track: &Track,
    bar_ids: &[Vec<i32>],
    diagrams: &HashMap<i32, ChordDiagram>,
    libraries: &[Vec<String>],
) -> Vec<Staff> {
    let Some(staves) = &g_track.staves else {
        return Vec::new();
    };
    staves
        .staves
        .iter()
        .enumerate()
        .zip(bar_ids)
        .skip(1)
        .map(|((index, g_staff), ids)| {
            let mut staff = Staff {
                measures: measures(lookup, headers, track_index, ids, diagrams),
                ..Staff::default()
            };
            let kept = libraries.get(index).cloned().unwrap_or_default();
            if let Some(props) = &g_staff.properties {
                let strings = extract_tuning(&props.properties);
                if !strings.is_empty() {
                    staff.strings = strings;
                    staff.has_tuning = true;
                    staff.tuning_label = extract_tuning_label(&props.properties);
                }
                if let Some(capo) = extract_capo(&props.properties) {
                    staff.offset = capo;
                }
                if let Some(count) = extract_fret_count(&props.properties) {
                    staff.fret_count = count;
                }
                staff.kept = kept_staff(&props.properties, props.name.as_deref(), kept);
            } else {
                staff.kept = kept_staff(&[], None, kept);
            }
            staff
        })
        .collect()
}
