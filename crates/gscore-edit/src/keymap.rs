//! The keymap: which key runs which action (AGENTS.md section 5, decision 7).
//!
//! A binding names a **typed character** (`"+"`, `"5"`, `"z"`), not a key position, so the
//! same file works on QWERTY, QWERTZ and AZERTY layouts: whatever key produces `+` on the
//! user's layout makes the note shorter. Keys that type nothing are bound by name (`Left`,
//! `Insert`, `Space`, `F5`, ...). Modifiers combine with either: `ctrl+z`, `ctrl+Insert`.
//!
//! Shift is part of a typed character (`+` may need Shift on one layout and not on another),
//! so it is ignored for characters without Ctrl or Alt. AltGr cannot be bound.
//!
//! The file format is one binding per line, `<key> = <action id>`; `#` starts a comment
//! line. [`DEFAULT_KEYMAP`] is the built-in keymap in that format.

use std::collections::BTreeMap;
use std::fmt;

use crate::action::{Action, ActionId};

/// The built-in keymap, in the keymap file format. Keys are the owner's (AGENTS.md
/// section 7); actions without a key there stay unbound.
pub const DEFAULT_KEYMAP: &str = "\
# GuitarScore keymap: one binding per line, `<key> = <action id>`.
#
# A key is a typed character (+, 5, z) or a key name (Left, Right, Up, Down, Home, End,
# PageUp, PageDown, Insert, Delete, Backspace, Enter, Tab, Escape, Space, F1-F20),
# optionally with modifiers: ctrl+z, ctrl+Insert, alt+Left. `ctrl` is Cmd on macOS.
# Shift belongs to the character: bind `+`, not `shift+=`. AltGr cannot be bound.
# `gscore-cli keymap` lists the action ids.

Left = cursor_left
Right = cursor_right
Up = cursor_up_string
Down = cursor_down_string
shift+Left = select_left
shift+Right = select_right
shift+Up = select_beat
shift+Down = select_beat

0 = enter_fret
1 = enter_fret
2 = enter_fret
3 = enter_fret
4 = enter_fret
5 = enter_fret
6 = enter_fret
7 = enter_fret
8 = enter_fret
9 = enter_fret

+ = duration_shorter
- = duration_longer

Insert = insert_beat
Delete = delete_beat
ctrl+Insert = insert_measure
ctrl+Delete = delete_measure

ctrl+c = copy
ctrl+x = cut
ctrl+v = paste
ctrl+z = undo
ctrl+y = redo

Space = play_pause
ctrl+s = save

# Palette: note values, rest and tie, techniques.
ctrl+1 = note_whole
ctrl+2 = note_half
ctrl+3 = note_quarter
ctrl+4 = note_eighth
ctrl+5 = note_sixteenth
ctrl+6 = note_thirty_second
ctrl+7 = note_sixty_fourth
. = toggle_dot
r = make_rest
l = toggle_tie
x = toggle_dead_note
g = toggle_ghost_note
h = toggle_hammer
s = toggle_shift_slide
S = toggle_legato_slide
b = toggle_bend
v = toggle_vibrato
V = toggle_wide_vibrato
a = toggle_accent
A = toggle_heavy_accent
p = toggle_palm_mute
i = toggle_let_ring
n = toggle_natural_harmonic
d = toggle_pick_down
u = toggle_pick_up
D = toggle_brush_down
U = toggle_brush_up
t = toggle_tapping
< = toggle_fade_in
w = toggle_whammy

ctrl+= = zoom_in
ctrl++ = zoom_in
ctrl+- = zoom_out
ctrl+0 = zoom_reset
";

/// A key that types nothing, bound by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NamedKey {
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Insert.
    Insert,
    /// Delete.
    Delete,
    /// Backspace.
    Backspace,
    /// Enter (Return).
    Enter,
    /// Tab.
    Tab,
    /// Escape.
    Escape,
    /// The space bar (a key name, so typing a space in a text field is never a command).
    Space,
    /// A function key, F1 to F20.
    F(u8),
}

/// Names of the [`NamedKey`]s other than function keys, as the keymap file writes them.
const NAMES: &[(&str, NamedKey)] = &[
    ("Left", NamedKey::Left),
    ("Right", NamedKey::Right),
    ("Up", NamedKey::Up),
    ("Down", NamedKey::Down),
    ("Home", NamedKey::Home),
    ("End", NamedKey::End),
    ("PageUp", NamedKey::PageUp),
    ("PageDown", NamedKey::PageDown),
    ("Insert", NamedKey::Insert),
    ("Delete", NamedKey::Delete),
    ("Backspace", NamedKey::Backspace),
    ("Enter", NamedKey::Enter),
    ("Tab", NamedKey::Tab),
    ("Escape", NamedKey::Escape),
    ("Space", NamedKey::Space),
];

/// Highest function key that can be bound.
const MAX_FUNCTION_KEY: u8 = 20;

impl NamedKey {
    /// The key called `name` (case does not matter).
    pub fn from_name(name: &str) -> Option<NamedKey> {
        if let Some((_, key)) = NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            return Some(*key);
        }
        let number = name.strip_prefix('F').or_else(|| name.strip_prefix('f'))?;
        let number: u8 = number.parse().ok()?;
        (1..=MAX_FUNCTION_KEY)
            .contains(&number)
            .then_some(NamedKey::F(number))
    }
}

impl fmt::Display for NamedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let NamedKey::F(number) = self {
            return write!(f, "F{number}");
        }
        let name = NAMES
            .iter()
            .find(|(_, key)| key == self)
            .map_or("?", |(name, _)| name);
        f.write_str(name)
    }
}

/// What was pressed: a typed character or a named key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    /// A character, as the keyboard layout produced it.
    Char(char),
    /// A key that types nothing.
    Named(NamedKey),
}

/// Modifier keys held with a key. `ctrl` is Cmd on macOS.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Modifiers {
    /// Ctrl (Cmd on macOS).
    pub ctrl: bool,
    /// Alt (Option on macOS).
    pub alt: bool,
    /// Shift.
    pub shift: bool,
}

/// A key with its modifiers, normalized so equal presses compare equal (see
/// [`KeyCombo::new`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyCombo {
    modifiers: Modifiers,
    key: Key,
}

impl KeyCombo {
    /// A key press. For a character without Ctrl or Alt, Shift is dropped: it only chose
    /// the character. With Ctrl or Alt, letters are compared in lower case (`ctrl+Z` is
    /// `ctrl+z`) and Shift counts.
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        match key {
            Key::Char(c) if modifiers.ctrl || modifiers.alt => KeyCombo {
                // Shift only chose a symbol (`+` needs it on QWERTY); for letters it is a
                // real modifier (ctrl+Z), even though case is folded.
                modifiers: Modifiers {
                    shift: modifiers.shift && c.is_alphanumeric(),
                    ..modifiers
                },
                key: Key::Char(c.to_lowercase().next().unwrap_or(c)),
            },
            Key::Char(_) => KeyCombo {
                modifiers: Modifiers {
                    shift: false,
                    ..modifiers
                },
                key,
            },
            Key::Named(_) => KeyCombo { modifiers, key },
        }
    }

    /// A typed character without modifiers.
    pub fn char(c: char) -> Self {
        KeyCombo::new(Key::Char(c), Modifiers::default())
    }

    /// A named key without modifiers.
    pub fn named(key: NamedKey) -> Self {
        KeyCombo::new(Key::Named(key), Modifiers::default())
    }

    /// The key.
    pub fn key(&self) -> Key {
        self.key
    }

    /// Parses `ctrl+z`, `Left`, `+`, `ctrl++`, ...
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("no key given".to_owned());
        }
        // The key is after the last `+`, except that the key itself may be `+`.
        let (modifier_text, key_text) = if text == "+" {
            ("", "+")
        } else if let Some(rest) = text.strip_suffix("++") {
            (rest, "+")
        } else {
            text.rsplit_once('+').unwrap_or(("", text))
        };
        let mut modifiers = Modifiers::default();
        for name in modifier_text
            .split('+')
            .map(str::trim)
            .filter(|m| !m.is_empty())
        {
            match name.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "cmd" | "command" => modifiers.ctrl = true,
                "alt" | "option" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "altgr" => return Err("AltGr cannot be bound".to_owned()),
                _ => return Err(format!("unknown modifier `{name}`")),
            }
        }
        let key_text = key_text.trim();
        let mut chars = key_text.chars();
        let key = match (chars.next(), chars.next()) {
            (Some(c), None) => Key::Char(c),
            _ => Key::Named(
                NamedKey::from_name(key_text).ok_or_else(|| format!("unknown key `{key_text}`"))?,
            ),
        };
        Ok(KeyCombo::new(key, modifiers))
    }
}

impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.ctrl {
            f.write_str("ctrl+")?;
        }
        if self.modifiers.alt {
            f.write_str("alt+")?;
        }
        if self.modifiers.shift {
            f.write_str("shift+")?;
        }
        match self.key {
            Key::Char(c) => write!(f, "{c}"),
            Key::Named(key) => write!(f, "{key}"),
        }
    }
}

/// A problem in a keymap file, with its 1-based line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeymapError {
    /// The line it is on.
    pub line: usize,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for KeymapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// Keys bound to actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    bindings: BTreeMap<KeyCombo, ActionId>,
}

impl Default for Keymap {
    /// The built-in keymap ([`DEFAULT_KEYMAP`]).
    fn default() -> Self {
        // The built-in text is tested to parse; an empty keymap is the harmless fallback.
        Keymap::parse(DEFAULT_KEYMAP).unwrap_or_else(|_| Keymap {
            bindings: BTreeMap::new(),
        })
    }
}

impl Keymap {
    /// Reads a keymap file. Every problem is reported (unknown keys or action ids, a key
    /// bound twice); the keymap is only returned if there are none.
    pub fn parse(text: &str) -> Result<Self, Vec<KeymapError>> {
        let mut bindings = BTreeMap::new();
        let mut errors = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let number = index.saturating_add(1);
            let mut error = |message: String| {
                errors.push(KeymapError {
                    line: number,
                    message,
                })
            };
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // The last `=` separates key and action, so `=` itself can be bound.
            let Some((key, id)) = line.rsplit_once('=') else {
                error(format!("`{line}` is not `<key> = <action>`"));
                continue;
            };
            let id = id.trim();
            let Some(action) = ActionId::from_id(id) else {
                error(format!("unknown action `{id}`"));
                continue;
            };
            let combo = match KeyCombo::parse(key) {
                Ok(combo) => combo,
                Err(message) => {
                    error(message);
                    continue;
                }
            };
            if action == ActionId::EnterFret && !matches!(combo.key, Key::Char('0'..='9')) {
                error(format!("enter_fret needs a digit key, not `{combo}`"));
                continue;
            }
            match bindings.get(&combo) {
                Some(&existing) if existing != action => {
                    error(format!(
                        "`{combo}` is bound to both {existing} and {action}"
                    ));
                }
                _ => {
                    bindings.insert(combo, action);
                }
            }
        }
        if errors.is_empty() {
            Ok(Keymap { bindings })
        } else {
            Err(errors)
        }
    }

    /// The action `combo` runs, if any. A digit bound to `enter_fret` enters that digit.
    pub fn action(&self, combo: KeyCombo) -> Option<Action> {
        let id = *self.bindings.get(&combo)?;
        if id == ActionId::EnterFret {
            let Key::Char(c) = combo.key else {
                return None;
            };
            let digit = c.to_digit(10).and_then(|d| u8::try_from(d).ok())?;
            return Some(Action::Digit(digit));
        }
        Some(Action::Plain(id))
    }

    /// The keys bound to `action`, for menus and help.
    pub fn keys_for(&self, action: ActionId) -> Vec<KeyCombo> {
        self.bindings
            .iter()
            .filter(|(_, a)| **a == action)
            .map(|(combo, _)| *combo)
            .collect()
    }

    /// Every binding, as `(key, action)`, in key order.
    pub fn bindings(&self) -> impl Iterator<Item = (KeyCombo, ActionId)> + '_ {
        self.bindings
            .iter()
            .map(|(&combo, &action)| (combo, action))
    }

    /// The actions whose keys differ from `base`, each with all of its keys here (empty
    /// when it lost every key), in action order. This is what the app stores: a later
    /// built-in shortcut still reaches a user who changed other ones.
    pub fn overrides(&self, base: &Keymap) -> Vec<(ActionId, Vec<KeyCombo>)> {
        crate::ACTIONS
            .iter()
            .map(|info| info.action)
            .filter_map(|action| {
                let keys = self.keys_for(action);
                (keys != base.keys_for(action)).then_some((action, keys))
            })
            .collect()
    }

    /// Applies stored [`Keymap::overrides`]: each action gets exactly the keys given (taking
    /// them over from any other action). `enter_fret` keeps digit keys only.
    pub fn apply_overrides(
        &mut self,
        overrides: impl IntoIterator<Item = (ActionId, Vec<KeyCombo>)>,
    ) {
        for (action, keys) in overrides {
            self.unbind(action);
            for combo in keys {
                if action == ActionId::EnterFret && !matches!(combo.key, Key::Char('0'..='9')) {
                    continue;
                }
                self.bindings.insert(combo, action);
            }
        }
    }

    /// Binds `action` to `combo`, replacing all of its old keys (a custom shortcut replacing
    /// a default one). A combo that belonged to another action is taken over. Fails with a
    /// message for `enter_fret`, whose digit keys are fixed (binding one digit would drop
    /// the other nine).
    pub fn bind(&mut self, action: ActionId, combo: KeyCombo) -> Result<(), String> {
        if action == ActionId::EnterFret {
            return Err(format!(
                "enter_fret always uses the digit keys; `{combo}` cannot replace them"
            ));
        }
        self.bindings.retain(|_, bound| *bound != action);
        self.bindings.insert(combo, action);
        Ok(())
    }

    /// Removes every key bound to `action`.
    pub fn unbind(&mut self, action: ActionId) {
        self.bindings.retain(|_, bound| *bound != action);
    }

    /// Restores the built-in key(s) of `action`, dropping a custom one.
    pub fn revert_action(&mut self, action: ActionId) {
        self.unbind(action);
        let defaults = Keymap::default();
        for combo in defaults.keys_for(action) {
            self.bindings.insert(combo, action);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_custom_binding_replaces_the_action_and_takes_over_the_combo() {
        let mut keymap = Keymap::default();
        let combo = KeyCombo::parse("ctrl+k").unwrap();
        keymap.bind(ActionId::Copy, combo).unwrap();
        assert_eq!(keymap.keys_for(ActionId::Copy), [combo]);
        // No action keeps an old key.
        assert!(
            keymap
                .bindings()
                .all(|(key, action)| key != combo || action == ActionId::Copy)
        );
        // Rebinding the combo to another action takes it over.
        keymap.bind(ActionId::Cut, combo).unwrap();
        assert!(keymap.keys_for(ActionId::Copy).is_empty());
        assert_eq!(keymap.keys_for(ActionId::Cut), [combo]);
    }

    #[test]
    fn reverting_one_action_or_everything_restores_the_built_ins() {
        let mut keymap = Keymap::default();
        let default_copy = keymap.keys_for(ActionId::Copy);
        let combo = KeyCombo::parse("ctrl+k").unwrap();
        keymap.bind(ActionId::Copy, combo).unwrap();
        keymap.revert_action(ActionId::Copy);
        assert_eq!(keymap.keys_for(ActionId::Copy), default_copy);

        keymap.bind(ActionId::Save, combo).unwrap();
        keymap = Keymap::default();
        assert_eq!(keymap, Keymap::default());
    }

    #[test]
    fn enter_fret_keeps_its_digit_keys() {
        let mut keymap = Keymap::default();
        let digits = keymap.keys_for(ActionId::EnterFret);
        assert!(digits.len() >= 10, "{digits:?}");
        assert!(
            keymap
                .bind(ActionId::EnterFret, KeyCombo::parse("k").unwrap())
                .is_err()
        );
        assert!(
            keymap
                .bind(ActionId::EnterFret, KeyCombo::parse("7").unwrap())
                .is_err()
        );
        assert_eq!(keymap.keys_for(ActionId::EnterFret), digits);
    }

    #[test]
    fn overrides_hold_only_the_changed_actions_and_restore_them() {
        let defaults = Keymap::default();
        let mut keymap = Keymap::default();
        assert!(keymap.overrides(&defaults).is_empty());
        // Copy takes over Paste's key: both actions differ from the built-ins now.
        let paste = keymap.keys_for(ActionId::Paste);
        let taken = *paste.first().unwrap();
        keymap.bind(ActionId::Copy, taken).unwrap();
        let overrides = keymap.overrides(&defaults);
        let actions: Vec<ActionId> = overrides.iter().map(|(action, _)| *action).collect();
        assert!(actions.contains(&ActionId::Copy), "{actions:?}");
        assert!(actions.contains(&ActionId::Paste), "{actions:?}");
        assert!(!actions.contains(&ActionId::Undo), "{actions:?}");

        let mut restored = Keymap::default();
        restored.apply_overrides(overrides);
        assert_eq!(restored, keymap);
        assert_eq!(restored.keys_for(ActionId::Copy), [taken]);
        // An action left out of the overrides keeps its built-in keys.
        assert_eq!(
            restored.keys_for(ActionId::Undo),
            defaults.keys_for(ActionId::Undo)
        );
    }
}
