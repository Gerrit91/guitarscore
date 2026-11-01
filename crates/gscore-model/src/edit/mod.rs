//! Checked editing methods on [`Song`].
//!
//! Every method validates the request and works on a copy of the wrapped song; the copy
//! replaces the original only when the edit and the invariant check both succeeded
//! (AGENTS.md rule 8, commands are transactional). A failed edit therefore leaves the song
//! exactly as it was.
//!
//! Each successful method returns a [`Change`]. Keep it and call [`Song::revert`] to undo
//! the edit. The change is opaque: its data stays behind the model boundary, and
//! `gscore-edit` only has to keep it in its undo history.

mod beats;
mod measures;
mod notes;

use gscore_gp::{
    Beat as GpBeat, BeatStatus, Duration as GpDuration, Measure as GpMeasure,
    MeasureHeader as GpMeasureHeader, Note as GpNote, Song as GpSong, Track as GpTrack,
    Voice as GpVoice,
};

use crate::duration::NoteValue;
use crate::error::ModelError;
use crate::position::{BeatPosition, Fret, MeasureIndex, StringNumber, TrackIndex};
use crate::song::Song;

/// An applied edit, kept so it can be undone with [`Song::revert`].
#[must_use = "keep the change to undo the edit"]
#[derive(Debug)]
pub struct Change {
    kind: ChangeKind,
}

/// What an edit changed, with the data needed to put it back.
#[derive(Debug)]
enum ChangeKind {
    /// A note was added or replaced; `previous` is what was on the string before.
    NoteSet {
        at: BeatPosition,
        string: StringNumber,
        previous: Option<Box<GpNote>>,
        previous_status: BeatStatus,
    },
    /// A note was removed. Clearing the last note turns the beat into a rest.
    NoteCleared {
        at: BeatPosition,
        string: StringNumber,
        note: Box<GpNote>,
        previous_status: BeatStatus,
    },
    /// A beat's length changed.
    DurationSet {
        at: BeatPosition,
        previous: GpDuration,
    },
    /// A rest beat was inserted before `at.beat`; `created_voice` if the insertion had to
    /// create the voice (a measure imported without voices).
    BeatInserted {
        at: BeatPosition,
        created_voice: bool,
    },
    /// The beat at `at.beat` was removed.
    BeatRemoved { at: BeatPosition, beat: Box<GpBeat> },
    /// A measure was inserted at `at` in every track.
    MeasureInserted { at: MeasureIndex },
    /// Measures were removed at `at`, one from every track, plus the measure header.
    MeasureRemoved {
        at: MeasureIndex,
        header: Box<GpMeasureHeader>,
        measures: Vec<GpMeasure>,
    },
}

impl Song {
    /// Sets a normal note at `fret` on `string` of the beat at `at`, replacing any note
    /// already on that string but keeping its effects. The beat becomes a normal beat.
    pub fn set_note(
        &mut self,
        at: BeatPosition,
        string: StringNumber,
        fret: Fret,
    ) -> Result<Change, ModelError> {
        notes::set(self, at, string, fret)
    }

    /// Removes the note on `string` of the beat at `at`. If it was the last note, the beat
    /// becomes a rest, which is what Guitar Pro stores for a beat without notes.
    pub fn clear_note(
        &mut self,
        at: BeatPosition,
        string: StringNumber,
    ) -> Result<Change, ModelError> {
        notes::clear(self, at, string)
    }

    /// Sets the length of the beat at `at`.
    pub fn set_duration(
        &mut self,
        at: BeatPosition,
        length: NoteValue,
    ) -> Result<Change, ModelError> {
        beats::set_duration(self, at, length)
    }

    /// Inserts a rest beat with the given length before `at.beat`. `at.beat` may be one
    /// past the voice's last beat to append.
    pub fn insert_beat(
        &mut self,
        at: BeatPosition,
        length: NoteValue,
    ) -> Result<Change, ModelError> {
        beats::insert(self, at, length)
    }

    /// Removes the beat at `at.beat`.
    pub fn delete_beat(&mut self, at: BeatPosition) -> Result<Change, ModelError> {
        beats::delete(self, at)
    }

    /// Inserts an empty measure (filled with rests) before `at` in every track, plus its
    /// measure header. `at` may be one past the last measure to append. The new measure
    /// inherits the time and key signature in effect there.
    pub fn insert_measure(&mut self, at: MeasureIndex) -> Result<Change, ModelError> {
        measures::insert(self, at)
    }

    /// Removes the measure at `at` from every track, plus its measure header. The last
    /// remaining measure cannot be removed: a song without measures is not useful, and
    /// Guitar Pro stores no such score.
    pub fn delete_measure(&mut self, at: MeasureIndex) -> Result<Change, ModelError> {
        measures::delete(self, at)
    }

    /// Undoes a change returned by one of the editing methods. The editor must revert
    /// changes in the reverse order they were applied; anything else is reported as an
    /// error and leaves the song untouched.
    pub fn revert(&mut self, change: &Change) -> Result<(), ModelError> {
        self.edit_inner(|inner| revert(inner, &change.kind))
    }
}

/// Applies the inverse of `kind`.
fn revert(inner: &mut GpSong, kind: &ChangeKind) -> Result<(), ModelError> {
    match kind {
        ChangeKind::NoteSet {
            at,
            string,
            previous,
            previous_status,
        } => {
            notes::restore(inner, *at, *string, previous.as_deref())?;
            beat_mut(inner, *at)?.status = previous_status.clone();
            Ok(())
        }
        ChangeKind::NoteCleared {
            at,
            string,
            note,
            previous_status,
        } => {
            notes::restore(inner, *at, *string, Some(note.as_ref()))?;
            beat_mut(inner, *at)?.status = previous_status.clone();
            Ok(())
        }
        ChangeKind::DurationSet { at, previous } => {
            beat_mut(inner, *at)?.duration = previous.clone();
            Ok(())
        }
        ChangeKind::BeatInserted { at, created_voice } => {
            let voice = voice_mut(inner, *at)?;
            let index = at.beat.get();
            if index >= voice.beats.len() {
                return Err(ModelError::Invalid(format!("{at}: no such beat")));
            }
            voice.beats.remove(index);
            if *created_voice && voice.beats.is_empty() {
                let measure = track_mut(inner, at.track)?
                    .measures
                    .get_mut(at.measure.get())
                    .ok_or_else(|| ModelError::Invalid(format!("{at}: no such measure")))?;
                measure.voices.clear();
            }
            Ok(())
        }
        ChangeKind::BeatRemoved { at, beat } => {
            let voice = voice_mut(inner, *at)?;
            let index = at.beat.get();
            if index > voice.beats.len() {
                return Err(ModelError::Invalid(format!("{at}: no such beat")));
            }
            voice.beats.insert(index, beat.as_ref().clone());
            Ok(())
        }
        ChangeKind::MeasureInserted { at } => measures::remove(inner, *at),
        ChangeKind::MeasureRemoved {
            at,
            header,
            measures,
        } => measures::restore(inner, *at, header.as_ref(), measures),
    }
}

/// The track at `index`, or an error naming what is missing.
fn track_mut(song: &mut GpSong, index: TrackIndex) -> Result<&mut GpTrack, ModelError> {
    let count = song.tracks.len();
    song.tracks
        .get_mut(index.get())
        .ok_or_else(|| ModelError::Invalid(format!("{index}: the song has {count} track(s)")))
}

/// The voice addressed by `at`, or an error naming what is missing.
fn voice_mut(song: &mut GpSong, at: BeatPosition) -> Result<&mut GpVoice, ModelError> {
    let track = track_mut(song, at.track)?;
    let measure = track
        .measures
        .get_mut(at.measure.get())
        .ok_or_else(|| ModelError::Invalid(format!("{at}: no such measure")))?;
    let voices = measure.voices.len();
    measure
        .voices
        .get_mut(at.voice.get())
        .ok_or_else(|| ModelError::Invalid(format!("{at}: the measure has {voices} voice(s)")))
}

/// The beat addressed by `at`, or an error naming what is missing.
fn beat_mut(song: &mut GpSong, at: BeatPosition) -> Result<&mut GpBeat, ModelError> {
    let voice = voice_mut(song, at)?;
    let beats = voice.beats.len();
    voice
        .beats
        .get_mut(at.beat.get())
        .ok_or_else(|| ModelError::Invalid(format!("{at}: the voice has {beats} beat(s)")))
}

#[cfg(test)]
mod tests;
