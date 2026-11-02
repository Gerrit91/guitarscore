//! Beat edits: length, insertion and removal.

use gscore_gp::{Beat as GpBeat, BeatStatus, Measure as GpMeasure, Voice as GpVoice};

use super::{Change, ChangeKind, beat_in, voice_in};
use crate::duration::NoteValue;
use crate::error::ModelError;
use crate::position::BeatPosition;
use crate::song::Song;

/// Sets the length of an existing beat.
pub(super) fn set_duration(
    song: &mut Song,
    at: BeatPosition,
    length: NoteValue,
) -> Result<Change, ModelError> {
    song.edit_measure(at.track, at.measure, |measure| {
        let beat = beat_in(measure, at)?;
        if beat.status == BeatStatus::Empty {
            // Guitar Pro stores placeholders in unused voices and the writer drops them; a
            // length change would disappear on save.
            return Err(ModelError::Invalid(format!(
                "{at}: cannot set the length of a placeholder beat"
            )));
        }
        let previous = beat.duration.clone();
        beat.duration = length.to_gp();
        Ok(Change {
            kind: ChangeKind::DurationSet { at, previous },
        })
    })
}

/// Inserts a rest beat before `at.beat`.
pub(super) fn insert(
    song: &mut Song,
    at: BeatPosition,
    length: NoteValue,
) -> Result<Change, ModelError> {
    song.edit_measure(at.track, at.measure, |measure| {
        let (voice, created_voice) = voice_in_or_create(measure, at)?;
        let index = at.beat.get();
        if index > voice.beats.len() {
            return Err(ModelError::Invalid(format!(
                "{at}: the voice has {} beat(s)",
                voice.beats.len()
            )));
        }
        voice.beats.insert(
            index,
            GpBeat {
                duration: length.to_gp(),
                status: BeatStatus::Rest,
                ..GpBeat::default()
            },
        );
        Ok(Change {
            kind: ChangeKind::BeatInserted { at, created_voice },
        })
    })
}

/// Removes the beat at `at.beat`.
pub(super) fn delete(song: &mut Song, at: BeatPosition) -> Result<Change, ModelError> {
    song.edit_measure(at.track, at.measure, |measure| {
        let voice = voice_in(measure, at)?;
        let index = at.beat.get();
        if index >= voice.beats.len() {
            return Err(ModelError::Invalid(format!(
                "{at}: the voice has {} beat(s)",
                voice.beats.len()
            )));
        }
        let beat = voice.beats.remove(index);
        Ok(Change {
            kind: ChangeKind::BeatRemoved {
                at,
                beat: Box::new(beat),
            },
        })
    })
}

/// The voice at `at`, creating voice 1 of a measure that has no voices (GP7 empty bars
/// import that way), so a beat can be inserted into it. Also returns whether it created the
/// voice, so undo can remove it again.
fn voice_in_or_create(
    measure: &mut GpMeasure,
    at: BeatPosition,
) -> Result<(&mut GpVoice, bool), ModelError> {
    let created = measure.voices.is_empty() && at.voice.get() == 0;
    if created {
        measure.voices.push(GpVoice::default());
    }
    voice_in(measure, at).map(|voice| (voice, created))
}
