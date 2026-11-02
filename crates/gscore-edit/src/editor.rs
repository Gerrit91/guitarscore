//! The [`Editor`]: one song, a command history, undo and redo.

use std::collections::VecDeque;
use std::fmt;

use gscore_model::{Change, ModelError, Song};

use crate::command::Command;

/// How many edits the default editor can undo at most.
pub const DEFAULT_HISTORY_LIMIT: usize = 10_000;

/// How much memory the default editor's undo history may take (an estimate, see
/// `Change::size_estimate`). Most edits keep a few hundred bytes; deleting a measure of a
/// many-track song keeps every track's measure, so this bound matters for those.
pub const DEFAULT_HISTORY_BYTES: usize = 64 * 1024 * 1024;

/// A document being edited, with its undo/redo history.
#[derive(Debug)]
pub struct Editor {
    song: Song,
    /// Oldest first; a deque so trimming at the limit is cheap.
    undo: VecDeque<Applied>,
    redo: Vec<Command>,
    history_limit: usize,
    /// The most the undo history may take, and what it takes now (estimated bytes).
    history_bytes_limit: usize,
    history_bytes: usize,
    /// Set by [`Editor::break_merge`]: the next command starts a new undo entry even if it
    /// would otherwise continue the previous one.
    merge_barrier: bool,
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
    /// The action is not implemented yet; nothing changed.
    Unsupported(String),
    /// Copy, cut or paste needs a selection, and there is none.
    NothingSelected,
    /// There is nothing on the clipboard to paste.
    NothingToPaste,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::Model(error) => write!(f, "{error}"),
            EditError::NothingToUndo => f.write_str("there is nothing to undo"),
            EditError::NothingToRedo => f.write_str("there is nothing to redo"),
            EditError::Unsupported(what) => f.write_str(what),
            EditError::NothingSelected => f.write_str("select some beats or measures first"),
            EditError::NothingToPaste => f.write_str("there is nothing on the clipboard"),
        }
    }
}

impl std::error::Error for EditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EditError::Model(error) => Some(error),
            EditError::NothingToUndo
            | EditError::NothingToRedo
            | EditError::Unsupported(_)
            | EditError::NothingSelected
            | EditError::NothingToPaste => None,
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
            history_bytes_limit: DEFAULT_HISTORY_BYTES,
            history_bytes: 0,
            merge_barrier: false,
        }
    }

    /// Caps the undo history at about `bytes` of memory as well: the oldest entries go
    /// first. The newest entry is always kept, however large.
    pub fn with_history_bytes(mut self, bytes: usize) -> Self {
        self.history_bytes_limit = bytes;
        self.trim_history();
        self
    }

    /// About how many bytes the undo history takes.
    pub fn history_bytes(&self) -> usize {
        self.history_bytes
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

    /// Marks the end of a slider drag: the next command starts a new undo entry, even if it
    /// changes the same volume or pan again.
    pub fn break_merge(&mut self) {
        self.merge_barrier = true;
    }

    /// Applies `command`, or handles `undo`/`redo`. A failed command changes nothing and is
    /// not added to the history.
    pub fn execute(&mut self, command: Command) -> Result<(), EditError> {
        self.execute_with_merge(command, false).map(|_| ())
    }

    /// Like [`Editor::execute`]; with `merge`, a volume or pan command continues the one
    /// before it as one undo step (a slider drag sends many changes). Callers pass `merge`
    /// only while the same drag is in progress, so separate drags stay separate steps.
    /// Returns whether the command was merged into the step before.
    pub fn execute_with_merge(&mut self, command: Command, merge: bool) -> Result<bool, EditError> {
        match command {
            Command::Undo => self.undo().map(|_| false),
            Command::Redo => self.redo().map(|_| false),
            command => {
                #[cfg(any(test, feature = "test-support"))]
                crate::testing::maybe_panic();
                let change = command.apply(&mut self.song)?;
                self.redo.clear();
                // The barrier ends a drag even when this command does not ask to merge.
                let barrier = std::mem::take(&mut self.merge_barrier);
                let merge = merge && !barrier;
                if merge
                    && let Some(last) = self.undo.back_mut()
                    && last.command.continues(&command)
                {
                    // One slider drag: keep the change from before it, so undo goes back to
                    // the value the drag started from, and keep only the latest command.
                    last.command = command;
                    return Ok(true);
                }
                self.push_undo(Applied { command, change });
                Ok(false)
            }
        }
    }

    /// Undoes the last applied edit and returns its command. The edit can be redone
    /// afterwards.
    pub fn undo(&mut self) -> Result<Option<Command>, EditError> {
        let applied = self.undo.pop_back().ok_or(EditError::NothingToUndo)?;
        if let Err(error) = applied.command.undo(&mut self.song, &applied.change) {
            // Keep the entry: the song was not changed, so the history is still consistent.
            self.undo.push_back(applied);
            return Err(EditError::Model(error));
        }
        self.history_bytes = self
            .history_bytes
            .saturating_sub(applied.change.size_estimate());
        let command = applied.command;
        self.redo.push(command.clone());
        Ok(Some(command))
    }

    /// Re-applies the last undone edit and returns its command. A new edit clears the redo
    /// stack.
    pub fn redo(&mut self) -> Result<Option<Command>, EditError> {
        let command = self.redo.pop().ok_or(EditError::NothingToRedo)?;
        match command.apply(&mut self.song) {
            Ok(change) => {
                self.push_undo(Applied {
                    command: command.clone(),
                    change,
                });
                Ok(Some(command))
            }
            Err(error) => {
                // Put it back: the song is unchanged, so it can still be redone.
                self.redo.push(command);
                Err(EditError::Model(error))
            }
        }
    }

    fn push_undo(&mut self, applied: Applied) {
        self.history_bytes = self
            .history_bytes
            .saturating_add(applied.change.size_estimate());
        self.undo.push_back(applied);
        self.trim_history();
    }

    /// Drops the oldest undo entries beyond the count limit or the memory limit; the
    /// newest entry stays even if it alone is over the memory limit.
    fn trim_history(&mut self) {
        while self.undo.len() > self.history_limit
            || (self.history_bytes > self.history_bytes_limit && self.undo.len() > 1)
        {
            match self.undo.pop_front() {
                Some(dropped) => {
                    self.history_bytes = self
                        .history_bytes
                        .saturating_sub(dropped.change.size_estimate());
                }
                None => break,
            }
        }
    }
}
