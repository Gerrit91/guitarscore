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
//! The keymap (typed characters to command ids) and the cursor belong to milestone M7.

#![warn(clippy::arithmetic_side_effects)]

mod command;
mod editor;
mod info;
mod parse;

pub use command::Command;
pub use editor::{DEFAULT_HISTORY_LIMIT, EditError, Editor};
pub use info::{COMMANDS, CommandInfo, ParamInfo, find_command};
pub use parse::{ParseError, parse_line};

// The integration tests save edited songs again with `gscore-io`; this marks the
// test-only dependency for the library test target's unused-dependency check.
#[cfg(test)]
use gscore_io as _;
