//! Measure edits: inserting and removing a measure in every staff of every track.

use gscore_gp::{
    Beat as GpBeat, BeatStatus, KeySignature, Measure as GpMeasure,
    MeasureHeader as GpMeasureHeader, Song as GpSong, TimeSignature as GpTimeSignature,
    TripletFeel, Voice as GpVoice,
};

use super::{Change, ChangeKind};
use crate::duration::{Duration, NoteValue, TimeSignature};
use crate::error::ModelError;
use crate::position::MeasureIndex;
use crate::song::{Moved, Song};
use crate::validate;

/// Most rest beats a new measure may be filled with. Time signatures allow up to 255
/// whole notes; anything that needs more rests than this is refused instead of allocating
/// without bound.
const MAX_FILL_BEATS: usize = 256;

/// Most voices copied from a neighbouring measure into a new one.
const MAX_VOICES: usize = 8;

/// Inserts a rest-filled measure before `at` in every track.
pub(super) fn insert(song: &mut Song, at: MeasureIndex) -> Result<Change, ModelError> {
    song.edit_structure(|inner| {
        if inner.tracks.is_empty() {
            return Err(ModelError::Invalid(
                "cannot insert a measure: the song has no tracks".to_owned(),
            ));
        }
        let count = inner.measure_headers.len();
        if at.get() > count {
            return Err(ModelError::Invalid(format!(
                "{at}: the song has {count} measure(s)"
            )));
        }
        if count >= usize::from(u16::MAX) {
            return Err(ModelError::Invalid("too many measures".to_owned()));
        }
        check_lengths(inner, at, true)?;

        let source = header_source(inner, at);
        let time_signature =
            source.map_or_else(GpTimeSignature::default, |h| h.time_signature.clone());
        let key_signature = source.map_or_else(KeySignature::default, |h| h.key_signature.clone());
        let triplet_feel = source.map_or(TripletFeel::None, |h| h.triplet_feel.clone());
        let rests = rests_for(&time_signature)?;
        let voice_count = voice_count(inner, at);

        let header = GpMeasureHeader {
            number: 1, // renumbered below
            time_signature: time_signature.clone(),
            key_signature: key_signature.clone(),
            triplet_feel,
            ..GpMeasureHeader::default()
        };
        // Every new measure is built and checked before anything changes: one per staff,
        // in the clef its staff has there.
        let mut new_measures: Vec<Vec<GpMeasure>> = Vec::with_capacity(inner.tracks.len());
        for (track_index, track) in inner.tracks.iter().enumerate() {
            let mut staves = Vec::new();
            for measures in track.measure_lists() {
                // Voice 1 is filled with rests; further voices stay empty (one placeholder
                // beat, as Guitar Pro stores unused voices), so no rests appear in voice 2.
                let voices = (0..voice_count)
                    .map(|index| GpVoice {
                        beats: if index == 0 {
                            rests.clone()
                        } else {
                            vec![placeholder_beat()]
                        },
                        ..GpVoice::default()
                    })
                    .collect();
                let clef = neighbour(measures, at).map(|measure| measure.clef.clone());
                let defaults = GpMeasure::default();
                let measure = GpMeasure {
                    number: 1, // renumbered below
                    track_index,
                    header_index: at.get(),
                    time_signature: time_signature.clone(),
                    key_signature: key_signature.clone(),
                    voices,
                    clef: clef.unwrap_or(defaults.clef.clone()),
                    ..defaults
                };
                validate::check_measure_data(at, &measure)?;
                staves.push(measure);
            }
            new_measures.push(staves);
        }
        inner.measure_headers.insert(at.get(), header);
        for (track, staves) in inner.tracks.iter_mut().zip(new_measures) {
            for (measures, measure) in track.measure_lists_mut().zip(staves) {
                measures.insert(at.get(), measure);
            }
        }
        renumber(inner);
        let change = Change {
            kind: ChangeKind::MeasureInserted { at },
        };
        Ok((change, Moved::Inserted(at)))
    })
}

/// Removes the measure at `at` from every track.
pub(super) fn delete(song: &mut Song, at: MeasureIndex) -> Result<Change, ModelError> {
    song.edit_structure(|inner| {
        let count = inner.measure_headers.len();
        if count <= 1 {
            return Err(ModelError::Invalid(
                "cannot delete the only measure".to_owned(),
            ));
        }
        if at.get() >= count {
            return Err(ModelError::Invalid(format!(
                "{at}: the song has {count} measure(s)"
            )));
        }
        check_lengths(inner, at, false)?;
        let header = inner.measure_headers.remove(at.get());
        let measures = inner
            .tracks
            .iter_mut()
            .flat_map(gscore_gp::Track::measure_lists_mut)
            .map(|measures| measures.remove(at.get()))
            .collect();
        renumber(inner);
        let change = Change {
            kind: ChangeKind::MeasureRemoved {
                at,
                header: Box::new(header),
                measures,
            },
        };
        Ok((change, Moved::Removed(at)))
    })
}

/// Reverts an inserted measure by removing it again.
pub(super) fn remove(song: &mut GpSong, at: MeasureIndex) -> Result<((), Moved), ModelError> {
    if at.get() >= song.measure_headers.len() {
        return Err(ModelError::Invalid(format!(
            "{at}: the song has {} measure(s)",
            song.measure_headers.len()
        )));
    }
    check_lengths(song, at, false)?;
    song.measure_headers.remove(at.get());
    for measures in song
        .tracks
        .iter_mut()
        .flat_map(gscore_gp::Track::measure_lists_mut)
    {
        measures.remove(at.get());
    }
    renumber(song);
    Ok(((), Moved::Removed(at)))
}

/// Reverts a removed measure by putting its header and every staff's measure back.
pub(super) fn restore(
    song: &mut GpSong,
    at: MeasureIndex,
    header: &GpMeasureHeader,
    measures: &[GpMeasure],
) -> Result<((), Moved), ModelError> {
    if at.get() > song.measure_headers.len() {
        return Err(ModelError::Invalid(format!(
            "{at}: the song has {} measure(s)",
            song.measure_headers.len()
        )));
    }
    let staves: usize = song
        .tracks
        .iter()
        .map(|track| track.staves.len().saturating_add(1))
        .sum();
    if measures.len() != staves {
        return Err(ModelError::Invalid(
            "the undo data does not match the song's staves".to_owned(),
        ));
    }
    check_lengths(song, at, true)?;
    if song.measure_headers.len() >= usize::from(u16::MAX) {
        return Err(ModelError::Invalid("too many measures".to_owned()));
    }
    for measure in measures {
        validate::check_measure_data(at, measure)?;
    }
    song.measure_headers.insert(at.get(), header.clone());
    for (list, measure) in song
        .tracks
        .iter_mut()
        .flat_map(gscore_gp::Track::measure_lists_mut)
        .zip(measures)
    {
        list.insert(at.get(), measure.clone());
    }
    renumber(song);
    Ok(((), Moved::Inserted(at)))
}

/// Numbers measures and their track links 1, 2, 3, ... again after an insertion/removal.
/// Callers check beforehand that the count fits a measure number (at most `u16::MAX`).
fn renumber(song: &mut GpSong) {
    for (index, header) in song.measure_headers.iter_mut().enumerate() {
        header.number = u16::try_from(index.saturating_add(1)).unwrap_or(u16::MAX);
    }
    for (track_index, track) in song.tracks.iter_mut().enumerate() {
        for measures in track.measure_lists_mut() {
            for (measure_index, measure) in measures.iter_mut().enumerate() {
                measure.number = measure_index.saturating_add(1);
                measure.header_index = measure_index;
                measure.track_index = track_index;
            }
        }
    }
}

/// Fails unless every staff of every track has a measure at `at` (or, `inserting`, at
/// least `at` measures): a song whose staves are out of step is not edited further.
fn check_lengths(song: &GpSong, at: MeasureIndex, inserting: bool) -> Result<(), ModelError> {
    for measures in song.tracks.iter().flat_map(gscore_gp::Track::measure_lists) {
        let fits = if inserting {
            at.get() <= measures.len()
        } else {
            at.get() < measures.len()
        };
        if !fits {
            return Err(ModelError::Invalid(format!(
                "{at}: a staff has only {} measure(s)",
                measures.len()
            )));
        }
    }
    Ok(())
}

/// The measure of a staff whose clef a measure inserted at `at` continues: the one before
/// it, or the one it pushes aside at the beginning.
fn neighbour(measures: &[GpMeasure], at: MeasureIndex) -> Option<&GpMeasure> {
    at.get()
        .checked_sub(1)
        .and_then(|index| measures.get(index))
        .or_else(|| measures.get(at.get()))
}

/// The header whose time/key signature is in effect at `at`: the previous measure, or the
/// one being pushed aside when inserting at the beginning.
fn header_source(song: &GpSong, at: MeasureIndex) -> Option<&GpMeasureHeader> {
    at.get()
        .checked_sub(1)
        .and_then(|index| song.measure_headers.get(index))
        .or_else(|| song.measure_headers.get(at.get()))
}

/// How many voices the new measure gets: the neighbour's, bounded to a sane range.
fn voice_count(song: &GpSong, at: MeasureIndex) -> usize {
    let neighbour = song.tracks.first().and_then(|track| {
        track
            .measures
            .get(at.get())
            .or_else(|| at.get().checked_sub(1).and_then(|i| track.measures.get(i)))
    });
    neighbour.map_or(1, |measure| measure.voices.len().clamp(1, MAX_VOICES))
}

/// Rest lengths that exactly fill `length`, largest first, with dotted values preferred so
/// a measure needs as few beats as possible.
pub(crate) fn fill_rests(length: Duration) -> Result<Vec<NoteValue>, ModelError> {
    let mut remaining = length;
    let mut beats = Vec::new();
    while remaining > Duration::ZERO {
        if beats.len() >= MAX_FILL_BEATS {
            return Err(ModelError::Invalid(format!(
                "the measure needs more than {MAX_FILL_BEATS} rests to fill it"
            )));
        }
        let mut chosen = None;
        for value in [1_u16, 2, 4, 8, 16, 32, 64, 128] {
            for dots in [2_u8, 1, 0] {
                let candidate = NoteValue::new(value, dots, None)?;
                let candidate_length = candidate.duration()?;
                if candidate_length <= remaining {
                    chosen = Some((candidate, candidate_length));
                    break;
                }
            }
            if chosen.is_some() {
                break;
            }
        }
        let Some((candidate, candidate_length)) = chosen else {
            return Err(ModelError::Invalid(
                "the measure length cannot be filled with note values".to_owned(),
            ));
        };
        remaining = remaining.checked_sub(candidate_length)?;
        beats.push(candidate);
    }
    Ok(beats)
}

/// A placeholder beat for an unused voice (no length, written as an empty voice).
fn placeholder_beat() -> GpBeat {
    GpBeat {
        status: BeatStatus::Empty,
        ..GpBeat::default()
    }
}

/// Rest beats that fill a measure with `signature`.
pub(super) fn rests_for(signature: &GpTimeSignature) -> Result<Vec<GpBeat>, ModelError> {
    let length = TimeSignature::from_gp(signature)?.measure_length();
    Ok(fill_rests(length)?
        .into_iter()
        .map(|length| GpBeat {
            duration: length.to_gp(),
            status: BeatStatus::Rest,
            ..GpBeat::default()
        })
        .collect())
}
