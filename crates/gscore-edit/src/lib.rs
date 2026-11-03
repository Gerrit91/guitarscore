//! Editing: the command registry with undo/redo.
//!
//! Every edit is a [`Command`]: an id, a label, parameters, an `enabled` check, `apply`
//! (which returns an opaque [`gscore_model::Change`]) and the change kept for undo. The
//! [`Editor`] owns a [`Song`] and the undo/redo history; it never mutates the model outside
//! a command. Commands are transactional because the model applies each change to a copy
//! first (see [`gscore_model::Song`]).
//!
//! Commands are driven from text, which is what `gscore-cli commands`, `gscore-cli edit
//! -c ...` and script files use: one command per line, `id key=value key=value ...`.
//! Positions are 1-based everywhere. `parse_line` turns a line into a [`Command`]; a blank
//! line or a `#` comment line becomes `None`.
//!
//! Interactive editing (M8) works at a [`Cursor`]: the [`TabEditor`] runs [`Action`]s from
//! the [`Keymap`] (typed characters and key names to action ids) as commands.

#![warn(clippy::arithmetic_side_effects)]

mod action;
mod command;
mod cursor;
mod editor;
mod effect_text;
mod info;
mod keymap;
mod parse;
mod selection;
mod tab_editor;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;

pub use action::{ACTIONS, Action, ActionId, ActionInfo};
pub use command::{Command, Recorded};
pub use cursor::Cursor;
pub use editor::{DEFAULT_HISTORY_BYTES, DEFAULT_HISTORY_LIMIT, EditError, Editor, HistoryEntry};
pub use info::{COMMANDS, CommandInfo, ParamInfo, find_command};
pub use keymap::{DEFAULT_KEYMAP, Key, KeyCombo, Keymap, KeymapError, Modifiers, NamedKey};
pub use parse::{ParseError, parse_line};
pub use selection::{BeatSpot, Selection};
pub use tab_editor::{FRET_DIGIT_WINDOW_MS, Outcome, Request, TabEditor};

// The integration tests save edited songs again with `gscore-io`; this marks the
// test-only dependency for the library test target's unused-dependency check.
#[cfg(test)]
use gscore_io as _;
