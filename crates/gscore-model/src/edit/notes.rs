//! Note edits on one string of one beat.

use gscore_gp::{Beat as GpBeat, BeatStatus, Measure as GpMeasure, Note as GpNote, NoteType};

use super::{Change, ChangeKind, beat_in};
use crate::error::ModelError;
use crate::position::{BeatPosition, Fret, StringNumber};
use crate::song::Song;

/// Adds or replaces a normal note on `string`.
pub(super) fn set(
    song: &mut Song,
    at: BeatPosition,
    string: StringNumber,
    fret: Fret,
) -> Result<Change, ModelError> {
    let tracks = song.track_count();
    let string_count = song
        .track(at.track)
        .ok_or_else(|| ModelError::Invalid(format!("{at}: the song has {tracks} track(s)")))?
        .string_count();
    if usize::from(string.get()) > string_count {
        return Err(ModelError::Invalid(format!(
            "{at}: {string}, but the track has {string_count} string(s)"
        )));
    }
    let gp_string = i8::try_from(string.get())
        .map_err(|_| ModelError::Invalid(format!("{at}: {string} is out of range")))?;
    song.edit_measure(at.track, at.measure, |measure| {
        let beat = beat_in(measure, at)?;
        let previous_status = beat.status.clone();
        let previous = replace_or_add(beat, gp_string, fret).map(Box::new);
        beat.status = BeatStatus::Normal;
        Ok(Change {
            kind: ChangeKind::NoteSet {
                at,
                string,
                previous,
                previous_status,
            },
        })
    })
}

/// Removes the note on `string`.
pub(super) fn clear(
    song: &mut Song,
    at: BeatPosition,
    string: StringNumber,
) -> Result<Change, ModelError> {
    let gp_string = i8::try_from(string.get())
        .map_err(|_| ModelError::Invalid(format!("{at}: {string} is out of range")))?;
    song.edit_measure(at.track, at.measure, |measure| {
        let beat = beat_in(measure, at)?;
        let index = find(beat, gp_string)
            .ok_or_else(|| ModelError::Invalid(format!("{at}: no note on {string}")))?;
        let previous_status = beat.status.clone();
        let note = beat.notes.remove(index);
        // Guitar Pro has no "normal beat without notes": it writes such a beat as a rest.
        if beat.notes.is_empty() && beat.status == BeatStatus::Normal {
            beat.status = BeatStatus::Rest;
        }
        Ok(Change {
            kind: ChangeKind::NoteCleared {
                at,
                string,
                note: Box::new(note),
                previous_status,
            },
        })
    })
}

/// Reverts a note edit: replaces the note on `string`, adds `note` if there was none, or
/// removes the note when `note` is `None`.
pub(super) fn restore(
    measure: &mut GpMeasure,
    at: BeatPosition,
    string: StringNumber,
    note: Option<&GpNote>,
) -> Result<(), ModelError> {
    let gp_string = i8::try_from(string.get())
        .map_err(|_| ModelError::Invalid(format!("{at}: {string} is out of range")))?;
    let beat = beat_in(measure, at)?;
    match note {
        Some(note) => {
            if let Some(index) = find(beat, gp_string)
                && let Some(slot) = beat.notes.get_mut(index)
            {
                *slot = note.clone();
                return Ok(());
            }
            beat.notes.push(note.clone());
            Ok(())
        }
        None => {
            let index = find(beat, gp_string)
                .ok_or_else(|| ModelError::Invalid(format!("{at}: no note on {string}")))?;
            beat.notes.remove(index);
            Ok(())
        }
    }
}

/// Replaces the note on `gp_string`, or adds a normal note; returns the previous note.
fn replace_or_add(beat: &mut GpBeat, gp_string: i8, fret: Fret) -> Option<GpNote> {
    let value = i16::from(fret.get());
    match find(beat, gp_string).and_then(|index| beat.notes.get_mut(index)) {
        Some(note) => {
            let previous = note.clone();
            note.value = value;
            note.kind = NoteType::Normal;
            Some(previous)
        }
        None => {
            beat.notes.push(GpNote {
                string: gp_string,
                value,
                kind: NoteType::Normal,
                ..GpNote::default()
            });
            None
        }
    }
}

/// Index of the note on `gp_string`.
fn find(beat: &GpBeat, gp_string: i8) -> Option<usize> {
    beat.notes.iter().position(|note| note.string == gp_string)
}
