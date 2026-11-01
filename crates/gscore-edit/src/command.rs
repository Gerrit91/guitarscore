//! The command enum: what an edit is, whether it can run, and how it applies.

use gscore_model::{
    BeatKind, BeatPosition, BeatView, Change, Fret, MeasureIndex, ModelError, NoteValue, Song,
    StringNumber,
};

use crate::info::{
    CLEAR_NOTE, CommandInfo, DELETE_BEAT, DELETE_MEASURE, INSERT_BEAT, INSERT_MEASURE, REDO,
    SET_DURATION, SET_FRET, UNDO,
};

/// One edit, with every parameter already checked for syntax and range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Put a note on a string of one beat.
    SetFret {
        /// The beat.
        at: BeatPosition,
        /// The string.
        string: StringNumber,
        /// The fret.
        fret: Fret,
    },
    /// Remove the note on one string of a beat.
    ClearNote {
        /// The beat.
        at: BeatPosition,
        /// The string.
        string: StringNumber,
    },
    /// Set a beat's note value, dots and tuplet.
    SetDuration {
        /// The beat.
        at: BeatPosition,
        /// The new length.
        duration: NoteValue,
    },
    /// Insert a rest beat before `at.beat`.
    InsertBeat {
        /// Where to insert.
        at: BeatPosition,
        /// The rest's length.
        duration: NoteValue,
    },
    /// Remove the beat at `at.beat`.
    DeleteBeat {
        /// The beat.
        at: BeatPosition,
    },
    /// Insert a rest-filled measure in every track before `measure`.
    InsertMeasure {
        /// Where to insert.
        measure: MeasureIndex,
    },
    /// Remove the measure in every track at `measure`.
    DeleteMeasure {
        /// The measure.
        measure: MeasureIndex,
    },
    /// Undo the last edit (handled by [`crate::Editor`]).
    Undo,
    /// Redo the last undone edit (handled by [`crate::Editor`]).
    Redo,
}

impl Command {
    /// This command's metadata.
    pub fn info(&self) -> &'static CommandInfo {
        match self {
            Command::SetFret { .. } => &SET_FRET,
            Command::ClearNote { .. } => &CLEAR_NOTE,
            Command::SetDuration { .. } => &SET_DURATION,
            Command::InsertBeat { .. } => &INSERT_BEAT,
            Command::DeleteBeat { .. } => &DELETE_BEAT,
            Command::InsertMeasure { .. } => &INSERT_MEASURE,
            Command::DeleteMeasure { .. } => &DELETE_MEASURE,
            Command::Undo => &UNDO,
            Command::Redo => &REDO,
        }
    }

    /// The command id (the canonical one, not an alias).
    pub fn id(&self) -> &'static str {
        self.info().id
    }

    /// The short, imperative label.
    pub fn label(&self) -> &'static str {
        self.info().label
    }

    /// True for `undo` and `redo`, which the editor handles instead of the model.
    pub fn is_history(&self) -> bool {
        matches!(self, Command::Undo | Command::Redo)
    }

    /// Whether the command can run on `song` right now. A command that is not enabled fails
    /// when applied; this is the cheap check for menus and key bindings.
    pub fn enabled(&self, song: &Song) -> bool {
        match self {
            Command::SetFret { at, string, .. } => note_target(song, *at, *string).is_some(),
            Command::ClearNote { at, string } => note_target(song, *at, *string)
                .is_some_and(|beat| beat.notes().any(|note| note.string().ok() == Some(*string))),
            Command::SetDuration { at, .. } => song
                .beat(*at)
                .is_some_and(|beat| beat.kind() != BeatKind::Empty),
            Command::InsertBeat { at, .. } => {
                // `apply` creates voice 1 of a measure that has no voices (GP7 empty
                // bars import that way), so the first beat of such a measure is editable.
                let Some(measure) = song.track(at.track).and_then(|t| t.measure(at.measure)) else {
                    return false;
                };
                if measure.voices().len() == 0 {
                    at.voice.get() == 0 && at.beat.get() == 0
                } else {
                    measure
                        .voice(at.voice)
                        .is_some_and(|voice| at.beat.get() <= voice.beats().len())
                }
            }
            Command::DeleteBeat { at } => song.beat(*at).is_some(),
            Command::InsertMeasure { measure } => {
                song.track_count() > 0 && measure.get() <= song.measure_count()
            }
            Command::DeleteMeasure { measure } => {
                song.track_count() > 0
                    && song.measure_count() > 1
                    && measure.get() < song.measure_count()
            }
            // The editor decides; the history stack is not part of the song.
            Command::Undo | Command::Redo => true,
        }
    }

    /// Undoes a change previously returned by [`Command::apply`]. History commands
    /// (`undo`/`redo`) have no model change; the editor handles them.
    pub fn undo(&self, song: &mut Song, change: &Change) -> Result<(), ModelError> {
        song.revert(change)
    }

    /// Applies the command, returning the change to keep for undo. The model applies it to a
    /// copy first, so an error leaves the song unchanged.
    pub fn apply(&self, song: &mut Song) -> Result<Change, ModelError> {
        match self {
            Command::SetFret { at, string, fret } => song.set_note(*at, *string, *fret),
            Command::ClearNote { at, string } => song.clear_note(*at, *string),
            Command::SetDuration { at, duration } => song.set_duration(*at, *duration),
            Command::InsertBeat { at, duration } => song.insert_beat(*at, *duration),
            Command::DeleteBeat { at } => song.delete_beat(*at),
            Command::InsertMeasure { measure } => song.insert_measure(*measure),
            Command::DeleteMeasure { measure } => song.delete_measure(*measure),
            Command::Undo | Command::Redo => Err(ModelError::Invalid(
                "undo and redo are handled by the editor".to_owned(),
            )),
        }
    }
}

/// The beat at `at`, if it exists and the track has `string`. Both must hold before a note
/// can be set or cleared.
fn note_target(song: &Song, at: BeatPosition, string: StringNumber) -> Option<BeatView<'_>> {
    let track = song.track(at.track)?;
    if usize::from(string.get()) > track.string_count() {
        return None;
    }
    song.beat(at)
}
