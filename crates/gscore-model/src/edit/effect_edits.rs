//! Note and beat effect edits: each sets or clears one effect and leaves the others alone.

use super::{Change, ChangeKind, beat_in};
use crate::effects::{BeatEffect, NoteEffect};
use crate::error::ModelError;
use crate::position::{BeatPosition, StringNumber};
use crate::song::Song;

impl Song {
    /// Sets or clears one effect of the note on `string` of the beat at `at`. Every other
    /// effect of the note stays as it was, and so do the beat's effects. The note must
    /// exist; the change fails (and changes nothing) otherwise.
    pub fn set_note_effect(
        &mut self,
        at: BeatPosition,
        string: StringNumber,
        effect: NoteEffect,
    ) -> Result<Change, ModelError> {
        effect.validate()?;
        let tracks = self.track_count();
        let string_count = self
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
        self.edit_measure(at.track, at.measure, |measure| {
            let beat = beat_in(measure, at)?;
            let note = beat
                .notes
                .iter_mut()
                .find(|note| note.string == gp_string)
                .ok_or_else(|| ModelError::Invalid(format!("{at}: no note on {string}")))?;
            let previous = Box::new(note.effect.clone());
            effect.apply(note)?;
            Ok(Change {
                kind: ChangeKind::NoteEffect {
                    at,
                    string,
                    previous,
                },
            })
        })
    }

    /// Sets or clears one effect of the beat at `at`, leaving its other effects alone.
    /// The beat must exist; the change fails (and changes nothing) otherwise.
    pub fn set_beat_effect(
        &mut self,
        at: BeatPosition,
        effect: BeatEffect,
    ) -> Result<Change, ModelError> {
        effect.validate()?;
        self.edit_measure(at.track, at.measure, |measure| {
            let beat = beat_in(measure, at)?;
            let previous = Box::new(beat.effect.clone());
            effect.apply(beat)?;
            Ok(Change {
                kind: ChangeKind::BeatEffect { at, previous },
            })
        })
    }
}
