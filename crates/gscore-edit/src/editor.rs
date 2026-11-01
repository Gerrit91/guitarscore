//! The [`Editor`]: one song, a command history, undo and redo.

use std::collections::VecDeque;
use std::fmt;

use gscore_model::{Change, ModelError, Song};

use crate::command::Command;

/// How many edits the default editor can undo. The changes are small (one note, beat or
/// measure), unlike whole-song snapshots, so this is a generous memory-safe bound.
pub const DEFAULT_HISTORY_LIMIT: usize = 10_000;

/// A document being edited, with its undo/redo history.
#[derive(Debug)]
pub struct Editor {
    song: Song,
    /// Oldest first; a deque so trimming at the limit is cheap.
    undo: VecDeque<Applied>,
    redo: Vec<Command>,
    history_limit: usize,
}

/// One applied command and the change that undoes it.
#[derive(Debug)]
struct Applied {
    command: Command,
    change: Change,
}

/// Why an edit did not happen.
#[derive(Debug)]
pub enum EditError {
    /// The model rejected the edit; the song is unchanged.
    Model(ModelError),
    /// There is nothing left to undo.
    NothingToUndo,
    /// There is nothing left to redo.
    NothingToRedo,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::Model(error) => write!(f, "{error}"),
            EditError::NothingToUndo => f.write_str("there is nothing to undo"),
            EditError::NothingToRedo => f.write_str("there is nothing to redo"),
        }
    }
}

impl std::error::Error for EditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EditError::Model(error) => Some(error),
            EditError::NothingToUndo | EditError::NothingToRedo => None,
        }
    }
}

impl From<ModelError> for EditError {
    fn from(error: ModelError) -> Self {
        EditError::Model(error)
    }
}

impl Editor {
    /// A new editor for `song`, with the default history limit.
    pub fn new(song: Song) -> Self {
        Editor::with_history_limit(song, DEFAULT_HISTORY_LIMIT)
    }

    /// A new editor that keeps at most `history_limit` undo steps (0 disables undo).
    pub fn with_history_limit(song: Song, history_limit: usize) -> Self {
        Editor {
            song,
            undo: VecDeque::new(),
            redo: Vec::new(),
            history_limit,
        }
    }

    /// The song as edited so far.
    pub fn song(&self) -> &Song {
        &self.song
    }

    /// The finished song, consuming the editor.
    pub fn into_song(self) -> Song {
        self.song
    }

    /// Whether [`Editor::undo`] has something to do.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether [`Editor::redo`] has something to do.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Number of edits that can still be undone.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Applies `command`, or handles `undo`/`redo`. A failed command changes nothing and is
    /// not added to the history.
    pub fn execute(&mut self, command: Command) -> Result<(), EditError> {
        match command {
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            command => {
                let change = command.apply(&mut self.song)?;
                self.redo.clear();
                self.undo.push_back(Applied { command, change });
                self.trim_history();
                Ok(())
            }
        }
    }

    /// Undoes the last applied edit. The edit can be redone afterwards.
    pub fn undo(&mut self) -> Result<(), EditError> {
        let applied = self.undo.pop_back().ok_or(EditError::NothingToUndo)?;
        if let Err(error) = applied.command.undo(&mut self.song, &applied.change) {
            // Keep the entry: the song was not changed, so the history is still consistent.
            self.undo.push_back(applied);
            return Err(EditError::Model(error));
        }
        self.redo.push(applied.command);
        Ok(())
    }

    /// Re-applies the last undone edit. A new edit clears the redo stack.
    pub fn redo(&mut self) -> Result<(), EditError> {
        let command = self.redo.pop().ok_or(EditError::NothingToRedo)?;
        match command.apply(&mut self.song) {
            Ok(change) => {
                self.undo.push_back(Applied { command, change });
                self.trim_history();
                Ok(())
            }
            Err(error) => {
                // Put it back: the song is unchanged, so it can still be redone.
                self.redo.push(command);
                Err(EditError::Model(error))
            }
        }
    }

    /// Drops the oldest undo entries beyond the limit.
    fn trim_history(&mut self) {
        while self.undo.len() > self.history_limit {
            self.undo.pop_front();
        }
    }
}
