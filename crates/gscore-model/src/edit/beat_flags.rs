//! Whole-beat and whole-note switches the edit palette uses: making a beat a rest and
//! tying a note to the one before it.

use gscore_gp::{BeatStatus, NoteType};

use super::{Change, ChangeKind};
use crate::error::ModelError;
use crate::position::{BeatPosition, StringNumber};
use crate::song::Song;

impl Song {
    /// Makes the beat a rest: its notes go and its status becomes a rest. The length and
    /// the beat's own effects stay. Undo puts the notes back.
    pub fn set_beat_rest(&mut self, at: BeatPosition) -> Result<Change, ModelError> {
        let previous_status = self
            .beat(at)
            .ok_or_else(|| ModelError::Invalid(format!("{at}: no such beat")))?
            .inner
            .status
            .clone();
        self.edit_measure(at.track, at.measure, |measure| {
            let beat = beat_mut(measure, at)?;
            let notes = std::mem::take(&mut beat.notes);
            beat.status = BeatStatus::Rest;
            Ok(Change {
                kind: ChangeKind::BeatRest {
                    at,
                    previous_status,
                    notes,
                },
            })
        })
    }

    /// Ties the note on `string` to the previous one (or removes the tie). A tie needs the
    /// note to exist; whether it makes musical sense is the user's call.
    pub fn set_note_tie(
        &mut self,
        at: BeatPosition,
        string: StringNumber,
        tie: bool,
    ) -> Result<Change, ModelError> {
        self.edit_measure(at.track, at.measure, |measure| {
            let beat = beat_mut(measure, at)?;
            let note = beat
                .notes
                .iter_mut()
                .find(|note| u8::try_from(note.string).ok() == Some(string.get()))
                .ok_or_else(|| ModelError::Invalid(format!("{at} has no {string}")))?;
            let previous = note.kind.clone();
            note.kind = if tie { NoteType::Tie } else { NoteType::Normal };
            Ok(Change {
                kind: ChangeKind::NoteTie {
                    at,
                    string,
                    previous,
                },
            })
        })
    }
}

/// The beat at `at` in a measure being edited.
fn beat_mut(
    measure: &mut gscore_gp::Measure,
    at: BeatPosition,
) -> Result<&mut gscore_gp::Beat, ModelError> {
    let voices = measure.voices.len();
    let voice = measure
        .voices
        .get_mut(at.voice.get())
        .ok_or_else(|| ModelError::Invalid(format!("{at}: the measure has {voices} voice(s)")))?;
    let beats = voice.beats.len();
    voice
        .beats
        .get_mut(at.beat.get())
        .ok_or_else(|| ModelError::Invalid(format!("{at}: the voice has {beats} beat(s)")))
}
