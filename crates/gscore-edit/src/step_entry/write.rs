//! Writing a step: the commands that make the cursor's beat hold the entered notes at the
//! entry length, together with the cursor's advance, as one undo step.

use gscore_model::{BeatKind, Fret, NoteKind, StringNumber};

use super::{Open, Step};
use crate::command::Command;
use crate::cursor::Cursor;
use crate::editor::EditError;
use crate::fingering::Place;
use crate::tab_editor::{Advance, Outcome, TabEditor};

impl TabEditor {
    /// Writes a new step at the cursor.
    pub(super) fn write_new(
        &mut self,
        notes: Vec<Place>,
        follow: bool,
        open: Option<Open>,
    ) -> Result<Outcome, EditError> {
        let at = self.cursor;
        let next = self.write_step(at, &notes)?;
        self.finish(at, notes, next, follow, open);
        Ok(Outcome::Edited)
    }

    /// Writes `step` again with other notes (or another open state), as one undo step.
    pub(super) fn rewrite(
        &mut self,
        step: &Step,
        notes: Vec<Place>,
        open: Option<Open>,
    ) -> Result<Outcome, EditError> {
        let cursor = self.cursor;
        self.undo()?;
        let next = match self.write_step(step.at, &notes) {
            Ok(next) => next,
            Err(error) => {
                // Put the step back as it was.
                self.redo()?;
                self.cursor = cursor;
                if let Some(old) = self.step.as_mut() {
                    old.revision = self.revision;
                }
                return Err(error);
            }
        };
        let string = cursor.string;
        self.finish(step.at, notes, next, step.follow, open);
        if open.is_some() {
            // Still waiting on the beat, on the string the cursor is on.
            self.cursor.string = string;
        }
        Ok(Outcome::Edited)
    }

    /// Remembers a written step and puts the cursor on its beat (while it is open) or
    /// after it.
    pub(super) fn finish(
        &mut self,
        at: Cursor,
        notes: Vec<Place>,
        next: Cursor,
        follow: bool,
        open: Option<Open>,
    ) {
        let mut next = next;
        if follow && let Some(last) = notes.last() {
            next.string = last.string.get();
        }
        self.cursor = if open.is_some() { at } else { next };
        self.cursor.clamp(self.editor.song());
        self.step = Some(Step {
            at,
            notes,
            next,
            revision: self.revision,
            follow,
            open,
        });
    }

    /// Writes the beat at `at` with `notes` (a rest when there are none) at the entry
    /// length, together with what the cursor needs to move on (a beat or a measure added),
    /// as one undo step. Returns where the cursor goes on to.
    pub(super) fn write_step(&mut self, at: Cursor, notes: &[Place]) -> Result<Cursor, EditError> {
        self.cursor = at;
        let write = self.write_commands(at, notes)?;
        self.execute(
            Command::Batch {
                commands: write.clone(),
            },
            at,
            false,
        )?;
        match self.advance()? {
            Advance::Move(next) => Ok(next),
            Advance::Edit(command, next) => {
                // Write the beat and the advance together, as one step.
                self.undo()?;
                let mut commands = write;
                commands.push(command);
                if let Err(error) = self.execute(Command::Batch { commands }, at, false) {
                    self.redo()?;
                    return Err(error);
                }
                Ok(next)
            }
        }
    }

    /// The commands that make the beat at `at` hold `notes` (a rest without notes) at the
    /// entry length.
    pub(super) fn write_commands(
        &self,
        at: Cursor,
        notes: &[Place],
    ) -> Result<Vec<Command>, EditError> {
        let song = self.editor.song();
        let position = at.position();
        let duration = self.entry_value;
        let mut commands = Vec::new();
        match song.beat(position) {
            None if at.beat.get() == 0 => {
                // A measure without beats (an empty GP7 bar): the beat comes first.
                commands.push(Command::InsertBeat {
                    at: position,
                    duration,
                });
            }
            None => {
                return Err(EditError::Unsupported(
                    "there is no beat at the cursor to enter a note on".to_owned(),
                ));
            }
            Some(beat) => {
                if beat.kind() != BeatKind::Empty && beat.note_value().ok() != Some(duration) {
                    commands.push(Command::SetDuration {
                        at: position,
                        duration,
                    });
                }
                if notes.is_empty() {
                    if beat.kind() == BeatKind::Normal {
                        commands.push(Command::SetBeatRest { at: position });
                    }
                } else {
                    // The beat's other notes go: step entry writes the beat anew.
                    for note in beat.notes().filter(|note| note.kind() != NoteKind::Rest) {
                        if let Ok(string) = note.string()
                            && !notes.iter().any(|place| place.string == string)
                        {
                            commands.push(Command::ClearNote {
                                at: position,
                                string,
                            });
                        }
                    }
                }
            }
        }
        commands.extend(notes.iter().map(|place| Command::SetFret {
            at: position,
            string: place.string,
            fret: place.fret,
        }));
        Ok(commands)
    }
}

/// `notes` with `fret` on `string`: replacing that string's note, or added.
pub(super) fn with_place(notes: &[Place], string: StringNumber, fret: Fret) -> Vec<Place> {
    let mut notes: Vec<Place> = notes
        .iter()
        .copied()
        .filter(|p| p.string != string)
        .collect();
    notes.push(Place { string, fret });
    notes
}
