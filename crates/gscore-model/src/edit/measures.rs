//! Measure edits: inserting and removing a measure in every track.

use gscore_gp::{
    Beat as GpBeat, BeatStatus, KeySignature, Measure as GpMeasure,
    MeasureHeader as GpMeasureHeader, Song as GpSong, TimeSignature as GpTimeSignature,
    TripletFeel, Voice as GpVoice,
};

use super::{Change, ChangeKind};
use crate::duration::{Duration, NoteValue, TimeSignature};
use crate::error::ModelError;
use crate::position::MeasureIndex;
use crate::song::Song;

/// Most rest beats a new measure may be filled with. Time signatures allow up to 255
/// whole notes; anything that needs more rests than this is refused instead of allocating
/// without bound.
const MAX_FILL_BEATS: usize = 256;

/// Most voices copied from a neighbouring measure into a new one.
const MAX_VOICES: usize = 8;

/// Inserts a rest-filled measure before `at` in every track.
pub(super) fn insert(song: &mut Song, at: MeasureIndex) -> Result<Change, ModelError> {
    song.edit_inner(|inner| {
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
        for track in &inner.tracks {
            if at.get() > track.measures.len() {
                return Err(ModelError::Invalid(format!(
                    "{at}: a track has only {} measure(s)",
                    track.measures.len()
                )));
            }
        }

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
        inner.measure_headers.insert(at.get(), header);
        for (track_index, track) in inner.tracks.iter_mut().enumerate() {
            // Voice 1 is filled with rests; further voices stay empty (one placeholder beat,
            // as Guitar Pro stores unused voices), so no rests appear in voice 2.
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
            let measure = GpMeasure {
                number: 1, // renumbered below
                track_index,
                header_index: at.get(),
                time_signature: time_signature.clone(),
                key_signature: key_signature.clone(),
                voices,
                ..GpMeasure::default()
            };
            track.measures.insert(at.get(), measure);
        }
        renumber(inner)?;
        Ok(Change {
            kind: ChangeKind::MeasureInserted { at },
        })
    })
}

/// Removes the measure at `at` from every track.
pub(super) fn delete(song: &mut Song, at: MeasureIndex) -> Result<Change, ModelError> {
    song.edit_inner(|inner| {
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
        for track in &inner.tracks {
            if at.get() >= track.measures.len() {
                return Err(ModelError::Invalid(format!(
                    "{at}: a track has only {} measure(s)",
                    track.measures.len()
                )));
            }
        }
        let header = inner.measure_headers.remove(at.get());
        let measures = inner
            .tracks
            .iter_mut()
            .map(|track| track.measures.remove(at.get()))
            .collect();
        renumber(inner)?;
        Ok(Change {
            kind: ChangeKind::MeasureRemoved {
                at,
                header: Box::new(header),
                measures,
            },
        })
    })
}

/// Reverts an inserted measure by removing it again.
pub(super) fn remove(song: &mut GpSong, at: MeasureIndex) -> Result<(), ModelError> {
    if at.get() >= song.measure_headers.len() {
        return Err(ModelError::Invalid(format!(
            "{at}: the song has {} measure(s)",
            song.measure_headers.len()
        )));
    }
    for track in &song.tracks {
        if at.get() >= track.measures.len() {
            return Err(ModelError::Invalid(format!(
                "{at}: a track has only {} measure(s)",
                track.measures.len()
            )));
        }
    }
    song.measure_headers.remove(at.get());
    for track in &mut song.tracks {
        track.measures.remove(at.get());
    }
    renumber(song)
}

/// Reverts a removed measure by putting its header and every track's measure back.
pub(super) fn restore(
    song: &mut GpSong,
    at: MeasureIndex,
    header: &GpMeasureHeader,
    measures: &[GpMeasure],
) -> Result<(), ModelError> {
    if at.get() > song.measure_headers.len() {
        return Err(ModelError::Invalid(format!(
            "{at}: the song has {} measure(s)",
            song.measure_headers.len()
        )));
    }
    if measures.len() != song.tracks.len() {
        return Err(ModelError::Invalid(
            "the undo data does not match the song's tracks".to_owned(),
        ));
    }
    for track in &song.tracks {
        if at.get() > track.measures.len() {
            return Err(ModelError::Invalid(format!(
                "{at}: a track has only {} measure(s)",
                track.measures.len()
            )));
        }
    }
    song.measure_headers.insert(at.get(), header.clone());
    for (track, measure) in song.tracks.iter_mut().zip(measures) {
        track.measures.insert(at.get(), measure.clone());
    }
    renumber(song)
}

/// Numbers measures and their track links 1, 2, 3, ... again after an insertion/removal.
fn renumber(song: &mut GpSong) -> Result<(), ModelError> {
    for (index, header) in song.measure_headers.iter_mut().enumerate() {
        let number = index
            .checked_add(1)
            .and_then(|n| u16::try_from(n).ok())
            .ok_or_else(|| ModelError::Invalid("too many measures".to_owned()))?;
        header.number = number;
    }
    for (track_index, track) in song.tracks.iter_mut().enumerate() {
        for (measure_index, measure) in track.measures.iter_mut().enumerate() {
            let number = measure_index
                .checked_add(1)
                .ok_or_else(|| ModelError::Invalid("too many measures".to_owned()))?;
            measure.number = number;
            measure.header_index = measure_index;
            measure.track_index = track_index;
        }
    }
    Ok(())
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
