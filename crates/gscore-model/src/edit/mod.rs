//! Checked editing methods on [`Song`].
//!
//! Every method validates the request and is transactional (AGENTS.md rule 8): a failed
//! edit leaves the song exactly as it was. Note and beat edits work on a copy of the one
//! measure they touch ([`Song::edit_measure`]); measure insertion and removal check
//! everything before they change anything ([`Song::edit_structure`]). Neither copies the
//! whole song, so an edit takes as long in a long song as in a short one.
//!
//! Each successful method returns a [`Change`]. Keep it and call [`Song::revert`] to undo
//! the edit. The change is opaque: its data stays behind the model boundary, and
//! `gscore-edit` only has to keep it in its undo history.

mod beats;
pub(crate) mod measures;
mod notes;

use gscore_gp::{
    Beat as GpBeat, BeatStatus, Duration as GpDuration, Measure as GpMeasure,
    MeasureHeader as GpMeasureHeader, Note as GpNote, Voice as GpVoice,
};

use crate::duration::NoteValue;
use crate::error::ModelError;
use crate::position::{BeatPosition, Fret, MeasureIndex, StringNumber};
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

impl Change {
    /// About how many bytes this change keeps in memory (for an undo history capped by
    /// memory). An estimate: the sizes of the kept notes, beats and measures, not their
    /// text or allocator overhead.
    pub fn size_estimate(&self) -> usize {
        let base = size_of::<Change>();
        let extra = match &self.kind {
            ChangeKind::NoteSet { previous, .. } => {
                previous.as_ref().map_or(0, |_| size_of::<GpNote>())
            }
            ChangeKind::NoteCleared { .. } => size_of::<GpNote>(),
            ChangeKind::DurationSet { .. }
            | ChangeKind::BeatInserted { .. }
            | ChangeKind::MeasureInserted { .. } => 0,
            ChangeKind::BeatRemoved { beat, .. } => beat_size(beat),
            ChangeKind::MeasureRemoved { measures, .. } => {
                size_of::<GpMeasureHeader>().saturating_add(measures.iter().map(measure_size).sum())
            }
        };
        base.saturating_add(extra)
    }
}

/// About how many bytes a beat and its notes take.
fn beat_size(beat: &GpBeat) -> usize {
    size_of::<GpBeat>().saturating_add(beat.notes.len().saturating_mul(size_of::<GpNote>()))
}

/// About how many bytes a measure and its beats take.
fn measure_size(measure: &GpMeasure) -> usize {
    let beats: usize = measure
        .voices
        .iter()
        .flat_map(|voice| &voice.beats)
        .map(beat_size)
        .sum();
    size_of::<GpMeasure>()
        .saturating_add(measure.voices.len().saturating_mul(size_of::<GpVoice>()))
        .saturating_add(beats)
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
        match &change.kind {
            ChangeKind::NoteSet {
                at,
                string,
                previous,
                previous_status,
            } => self.edit_measure(at.track, at.measure, |measure| {
                notes::restore(measure, *at, *string, previous.as_deref())?;
                beat_in(measure, *at)?.status = previous_status.clone();
                Ok(())
            }),
            ChangeKind::NoteCleared {
                at,
                string,
                note,
                previous_status,
            } => self.edit_measure(at.track, at.measure, |measure| {
                notes::restore(measure, *at, *string, Some(note.as_ref()))?;
                beat_in(measure, *at)?.status = previous_status.clone();
                Ok(())
            }),
            ChangeKind::DurationSet { at, previous } => {
                self.edit_measure(at.track, at.measure, |measure| {
                    beat_in(measure, *at)?.duration = previous.clone();
                    Ok(())
                })
            }
            ChangeKind::BeatInserted { at, created_voice } => {
                self.edit_measure(at.track, at.measure, |measure| {
                    let voice = voice_in(measure, *at)?;
                    let index = at.beat.get();
                    if index >= voice.beats.len() {
                        return Err(ModelError::Invalid(format!("{at}: no such beat")));
                    }
                    voice.beats.remove(index);
                    if *created_voice && voice.beats.is_empty() {
                        measure.voices.clear();
                    }
                    Ok(())
                })
            }
            ChangeKind::BeatRemoved { at, beat } => {
                self.edit_measure(at.track, at.measure, |measure| {
                    let voice = voice_in(measure, *at)?;
                    let index = at.beat.get();
                    if index > voice.beats.len() {
                        return Err(ModelError::Invalid(format!("{at}: no such beat")));
                    }
                    voice.beats.insert(index, beat.as_ref().clone());
                    Ok(())
                })
            }
            ChangeKind::MeasureInserted { at } => {
                self.edit_structure(|inner| measures::remove(inner, *at))
            }
            ChangeKind::MeasureRemoved {
                at,
                header,
                measures,
            } => self
                .edit_structure(|inner| measures::restore(inner, *at, header.as_ref(), measures)),
        }
    }
}

/// The voice addressed by `at` in its measure, or an error naming what is missing.
fn voice_in(measure: &mut GpMeasure, at: BeatPosition) -> Result<&mut GpVoice, ModelError> {
    let voices = measure.voices.len();
    measure
        .voices
        .get_mut(at.voice.get())
        .ok_or_else(|| ModelError::Invalid(format!("{at}: the measure has {voices} voice(s)")))
}

/// The beat addressed by `at` in its measure, or an error naming what is missing.
fn beat_in(measure: &mut GpMeasure, at: BeatPosition) -> Result<&mut GpBeat, ModelError> {
    let voice = voice_in(measure, at)?;
    let beats = voice.beats.len();
    voice
        .beats
        .get_mut(at.beat.get())
        .ok_or_else(|| ModelError::Invalid(format!("{at}: the voice has {beats} beat(s)")))
}

#[cfg(test)]
mod tests;
