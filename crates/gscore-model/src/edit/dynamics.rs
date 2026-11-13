//! Setting a beat's dynamic: the velocity of every note in it.

use super::{Change, ChangeKind, beat_in};
use crate::dynamics::Dynamic;
use crate::error::ModelError;
use crate::position::BeatPosition;
use crate::song::Song;

impl Song {
    /// Sets the dynamic of the beat at `at`: every note in it is played that loud. The beat
    /// must have notes (a rest has no dynamic); the change fails (and changes nothing)
    /// otherwise. Later beats keep their own dynamics.
    pub fn set_dynamic(
        &mut self,
        at: BeatPosition,
        dynamic: Dynamic,
    ) -> Result<Change, ModelError> {
        self.edit_measure_at(at, |measure| {
            let beat = beat_in(measure, at)?;
            if beat.notes.is_empty() {
                return Err(ModelError::Invalid(format!(
                    "{at} has no notes to give a dynamic"
                )));
            }
            let velocity = dynamic.raw_velocity();
            let previous = beat
                .notes
                .iter_mut()
                .map(|note| std::mem::replace(&mut note.velocity, velocity))
                .collect();
            Ok(Change {
                kind: ChangeKind::BeatDynamic { at, previous },
            })
        })
    }
}

/// Puts back the notes' velocities, in note order. Fails (changing nothing) if the beat
/// no longer has as many notes.
pub(super) fn restore(
    measure: &mut gscore_gp::Measure,
    at: BeatPosition,
    previous: &[i16],
) -> Result<(), ModelError> {
    let beat = beat_in(measure, at)?;
    if beat.notes.len() != previous.len() {
        return Err(ModelError::Invalid(format!(
            "{at}: the undo data does not match the beat's notes"
        )));
    }
    for (note, velocity) in beat.notes.iter_mut().zip(previous) {
        note.velocity = *velocity;
    }
    Ok(())
}
