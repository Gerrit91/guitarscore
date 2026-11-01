//! Interactive actions: what a key press (or a menu entry) asks the editor to do. Unlike a
//! [`crate::Command`], an action has no explicit position; it works at the cursor. One action
//! may run one model command, move the cursor, or ask the application for something
//! (play, save) that the editor itself cannot do.
//!
//! The ids are the ones the keymap file binds keys to (AGENTS.md section 7).

use std::fmt;

/// What an action id stands for. `EnterFret` takes its digit from the key that was typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActionId {
    /// Move to the previous beat (into the previous measure at its start).
    CursorLeft,
    /// Move to the next beat; at the end of an unfinished measure, add a rest beat; at the
    /// end of the last measure, add a measure.
    CursorRight,
    /// Move to the next higher string (towards string 1).
    CursorUpString,
    /// Move to the next lower string.
    CursorDownString,
    /// Move to the first beat of the previous measure.
    PreviousMeasure,
    /// Move to the first beat of the next measure.
    NextMeasure,
    /// Type a fret digit on the cursor's string; a second digit soon after makes two-digit
    /// frets.
    EnterFret,
    /// Remove the note on the cursor's string.
    ClearNote,
    /// Halve the beat's note value (a quarter becomes an eighth).
    DurationShorter,
    /// Double the beat's note value (a quarter becomes a half).
    DurationLonger,
    /// Insert a rest beat before the cursor.
    InsertBeat,
    /// Remove the beat at the cursor.
    DeleteBeat,
    /// Insert a measure before the cursor's measure, in every track.
    InsertMeasure,
    /// Remove the cursor's measure from every track.
    DeleteMeasure,
    /// Copy the selection.
    Copy,
    /// Cut the selection.
    Cut,
    /// Paste at the cursor.
    Paste,
    /// Undo the last edit.
    Undo,
    /// Redo the last undone edit.
    Redo,
    /// Start or pause playback.
    PlayPause,
    /// Stop playback.
    Stop,
    /// Save the file.
    Save,
}

/// An action id with its text form and a label for menus and help.
#[derive(Debug, Clone, Copy)]
pub struct ActionInfo {
    /// The action.
    pub action: ActionId,
    /// The id the keymap file uses.
    pub id: &'static str,
    /// A short, imperative label.
    pub label: &'static str,
}

/// Every action, in the order help lists them.
pub const ACTIONS: &[ActionInfo] = &[
    info(ActionId::CursorLeft, "cursor_left", "Previous beat"),
    info(ActionId::CursorRight, "cursor_right", "Next beat"),
    info(
        ActionId::CursorUpString,
        "cursor_up_string",
        "Higher string",
    ),
    info(
        ActionId::CursorDownString,
        "cursor_down_string",
        "Lower string",
    ),
    info(
        ActionId::PreviousMeasure,
        "previous_measure",
        "Previous measure",
    ),
    info(ActionId::NextMeasure, "next_measure", "Next measure"),
    info(ActionId::EnterFret, "enter_fret", "Enter fret"),
    info(ActionId::ClearNote, "clear_note", "Clear note"),
    info(
        ActionId::DurationShorter,
        "duration_shorter",
        "Shorter note",
    ),
    info(ActionId::DurationLonger, "duration_longer", "Longer note"),
    info(ActionId::InsertBeat, "insert_beat", "Insert beat"),
    info(ActionId::DeleteBeat, "delete_beat", "Delete beat"),
    info(ActionId::InsertMeasure, "insert_measure", "Insert measure"),
    info(ActionId::DeleteMeasure, "delete_measure", "Delete measure"),
    info(ActionId::Copy, "copy", "Copy"),
    info(ActionId::Cut, "cut", "Cut"),
    info(ActionId::Paste, "paste", "Paste"),
    info(ActionId::Undo, "undo", "Undo"),
    info(ActionId::Redo, "redo", "Redo"),
    info(ActionId::PlayPause, "play_pause", "Play/pause"),
    info(ActionId::Stop, "stop", "Stop"),
    info(ActionId::Save, "save", "Save"),
];

const fn info(action: ActionId, id: &'static str, label: &'static str) -> ActionInfo {
    ActionInfo { action, id, label }
}

impl ActionId {
    /// The action with keymap id `id`, if there is one.
    pub fn from_id(id: &str) -> Option<ActionId> {
        ACTIONS.iter().find(|a| a.id == id).map(|a| a.action)
    }

    /// This action's metadata.
    pub fn info(self) -> &'static ActionInfo {
        // Every variant has an entry (tested); the fallback is never used.
        static UNKNOWN: ActionInfo = info(ActionId::EnterFret, "?", "?");
        ACTIONS
            .iter()
            .find(|a| a.action == self)
            .unwrap_or(&UNKNOWN)
    }

    /// The keymap id.
    pub fn id(self) -> &'static str {
        self.info().id
    }

    /// The label.
    pub fn label(self) -> &'static str {
        self.info().label
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// An action ready to run: the id plus what the key supplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Any action without a parameter.
    Plain(ActionId),
    /// A fret digit (0-9).
    Digit(u8),
}

impl Action {
    /// The action's id.
    pub fn id(self) -> ActionId {
        match self {
            Action::Plain(id) => id,
            Action::Digit(_) => ActionId::EnterFret,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_round_trip() {
        for (i, a) in ACTIONS.iter().enumerate() {
            assert_eq!(ActionId::from_id(a.id), Some(a.action));
            assert_eq!(a.action.id(), a.id);
            assert!(
                ACTIONS
                    .iter()
                    .skip(i + 1)
                    .all(|b| b.id != a.id && b.action != a.action)
            );
        }
        assert_eq!(ActionId::from_id("no_such_action"), None);
        assert_eq!(ACTIONS.len(), 22, "one entry per ActionId variant");
    }
}
