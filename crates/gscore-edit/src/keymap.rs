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
//!
//! Step entry (M23) has a layer of its own: the actions it alone uses
//! ([`ActionId::step_entry_only`]) are bound in it, and while the mode is on its keys come
//! before the others. So `a` enters the note A in step entry and an accent otherwise.

use std::collections::BTreeMap;
use std::fmt;

use crate::action::{Action, ActionId, Letter};

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
shift+Up = select_line_up
shift+Down = select_line_down
Home = cursor_measure_start
End = cursor_measure_end
shift+Home = select_home
shift+End = select_end
ctrl+Home = cursor_home
ctrl+End = cursor_end
ctrl+Left = previous_measure
ctrl+Right = next_measure
ctrl+shift+Left = select_home
ctrl+shift+Right = select_end

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
Delete = delete
ctrl+Insert = insert_measure
ctrl+Delete = delete_measure

ctrl+a = select_all
ctrl+c = copy
ctrl+shift+c = copy_all_tracks
ctrl+x = cut
ctrl+v = paste
ctrl+shift+v = paste_around
ctrl+shift+Insert = paste_around
ctrl+z = undo
ctrl+y = redo

Space = play_pause
ctrl+s = save
ctrl+shift+s = save_as
ctrl+n = new_song
ctrl+o = open_file
ctrl+, = open_settings
ctrl+w = close_tab
ctrl+q = quit
H = show_history
N = toggle_step_entry

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
G = grace_note
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
t = edit_beat_text
L = edit_lyrics
T = toggle_tapping
< = toggle_fade_in
w = toggle_whammy

ctrl+= = zoom_in
ctrl++ = zoom_in
ctrl+- = zoom_out
ctrl+0 = zoom_reset
PageUp = scroll_page_up
PageDown = scroll_page_down
F11 = toggle_fullscreen
F3 = toggle_mixer

# Step entry: these keys only work while it is on (N switches it on and off).
# Letters enter notes by name, capitals add them to the chord entered last, alt+digit
# types a fret that keeps the beat open for the next note of a chord.
Escape = leave_step_entry
c = step_note
d = step_note
e = step_note
f = step_note
g = step_note
a = step_note
b = step_note
C = step_chord_note
D = step_chord_note
E = step_chord_note
F = step_chord_note
G = step_chord_note
A = step_chord_note
B = step_chord_note
alt+0 = enter_chord_fret
alt+1 = enter_chord_fret
alt+2 = enter_chord_fret
alt+3 = enter_chord_fret
alt+4 = enter_chord_fret
alt+5 = enter_chord_fret
alt+6 = enter_chord_fret
alt+7 = enter_chord_fret
alt+8 = enter_chord_fret
alt+9 = enter_chord_fret
r = step_rest
alt+Up = step_semitone_up
alt+Down = step_semitone_down
ctrl+Up = step_octave_up
ctrl+Down = step_octave_down
ctrl+shift+Up = step_string_up
ctrl+shift+Down = step_string_down
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
    /// The step-entry layer: [`ActionId::step_entry_only`] actions.
    step: BTreeMap<KeyCombo, ActionId>,
}

impl Default for Keymap {
    /// The built-in keymap ([`DEFAULT_KEYMAP`]).
    fn default() -> Self {
        // The built-in text is tested to parse; an empty keymap is the harmless fallback.
        Keymap::parse(DEFAULT_KEYMAP).unwrap_or_else(|_| Keymap {
            bindings: BTreeMap::new(),
            step: BTreeMap::new(),
        })
    }
}

/// Whether `combo` can be one of `action`'s keys, for the actions whose key is their
/// parameter (a digit or a note's letter). The error says what it needs.
fn key_fits(action: ActionId, combo: KeyCombo) -> Result<(), String> {
    let fits = match (action, combo.key) {
        (ActionId::EnterFret | ActionId::EnterChordFret, Key::Char(c)) => c.is_ascii_digit(),
        (ActionId::StepNote, Key::Char(c)) => {
            Letter::from_char(c).is_some() && c.is_ascii_lowercase()
        }
        (ActionId::StepChordNote, Key::Char(c)) => {
            Letter::from_char(c).is_some() && c.is_ascii_uppercase()
        }
        (action, _) if action.has_fixed_keys() => false,
        _ => true,
    };
    if fits {
        return Ok(());
    }
    let needs = match action {
        ActionId::StepNote => "a letter from a to g",
        ActionId::StepChordNote => "a capital letter from A to G",
        _ => "a digit key",
    };
    Err(format!("{action} needs {needs}, not `{combo}`"))
}

impl Keymap {
    /// Reads a keymap file. Every problem is reported (unknown keys or action ids, a key
    /// bound twice); the keymap is only returned if there are none.
    pub fn parse(text: &str) -> Result<Self, Vec<KeymapError>> {
        let mut bindings = BTreeMap::new();
        let mut step = BTreeMap::new();
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
            if let Err(message) = key_fits(action, combo) {
                error(message);
                continue;
            }
            let layer = if action.step_entry_only() {
                &mut step
            } else {
                &mut bindings
            };
            match layer.get(&combo) {
                Some(&existing) if existing != action => {
                    error(format!(
                        "`{combo}` is bound to both {existing} and {action}"
                    ));
                }
                _ => {
                    layer.insert(combo, action);
                }
            }
        }
        if errors.is_empty() {
            Ok(Keymap { bindings, step })
        } else {
            Err(errors)
        }
    }

    /// The action `combo` runs outside step entry, if any. A digit bound to `enter_fret`
    /// enters that digit.
    pub fn action(&self, combo: KeyCombo) -> Option<Action> {
        self.action_in(combo, false)
    }

    /// The action `combo` runs, with step entry on or off: in step entry its own keys come
    /// first.
    pub fn action_in(&self, combo: KeyCombo, step_entry: bool) -> Option<Action> {
        let id = step_entry
            .then(|| self.step.get(&combo))
            .flatten()
            .or_else(|| self.bindings.get(&combo))?;
        let c = match combo.key {
            Key::Char(c) => Some(c),
            Key::Named(_) => None,
        };
        let digit = || {
            c.and_then(|c| c.to_digit(10))
                .and_then(|d| u8::try_from(d).ok())
        };
        let letter = || c.and_then(Letter::from_char);
        Some(match id {
            ActionId::EnterFret => Action::Digit(digit()?),
            ActionId::EnterChordFret => Action::ChordDigit(digit()?),
            ActionId::StepNote => Action::Note {
                letter: letter()?,
                chord: false,
            },
            ActionId::StepChordNote => Action::Note {
                letter: letter()?,
                chord: true,
            },
            id => Action::Plain(*id),
        })
    }

    fn layer(&self, action: ActionId) -> &BTreeMap<KeyCombo, ActionId> {
        if action.step_entry_only() {
            &self.step
        } else {
            &self.bindings
        }
    }

    fn layer_mut(&mut self, action: ActionId) -> &mut BTreeMap<KeyCombo, ActionId> {
        if action.step_entry_only() {
            &mut self.step
        } else {
            &mut self.bindings
        }
    }

    /// The other action that `combo` runs where `action` would (in the same layer), if
    /// any: what binding `combo` to `action` takes the key from.
    pub fn conflict(&self, action: ActionId, combo: KeyCombo) -> Option<ActionId> {
        self.layer(action)
            .get(&combo)
            .copied()
            .filter(|other| *other != action)
    }

    /// The keys bound to `action`, for menus and help.
    pub fn keys_for(&self, action: ActionId) -> Vec<KeyCombo> {
        self.layer(action)
            .iter()
            .filter(|(_, a)| **a == action)
            .map(|(combo, _)| *combo)
            .collect()
    }

    /// Every binding, as `(key, action)`, in key order: those outside step entry, then
    /// the step-entry layer's.
    pub fn bindings(&self) -> impl Iterator<Item = (KeyCombo, ActionId)> + '_ {
        self.bindings
            .iter()
            .chain(&self.step)
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
                if key_fits(action, combo).is_err() {
                    continue;
                }
                self.layer_mut(action).insert(combo, action);
            }
        }
    }

    /// Binds `action` to `combo`, replacing all of its old keys (a custom shortcut replacing
    /// a default one). A combo that belonged to another action is taken over. Fails with a
    /// message for `enter_fret`, whose digit keys are fixed (binding one digit would drop
    /// the other nine).
    pub fn bind(&mut self, action: ActionId, combo: KeyCombo) -> Result<(), String> {
        if action.has_fixed_keys() {
            return Err(format!(
                "{action} always uses its own keys; `{combo}` cannot replace them"
            ));
        }
        let layer = self.layer_mut(action);
        layer.retain(|_, bound| *bound != action);
        layer.insert(combo, action);
        Ok(())
    }

    /// Removes every key bound to `action`.
    pub fn unbind(&mut self, action: ActionId) {
        self.layer_mut(action).retain(|_, bound| *bound != action);
    }

    /// Restores the built-in key(s) of `action`, dropping a custom one.
    pub fn revert_action(&mut self, action: ActionId) {
        self.unbind(action);
        let defaults = Keymap::default();
        for combo in defaults.keys_for(action) {
            self.layer_mut(action).insert(combo, action);
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
    fn step_entry_keys_only_work_in_step_entry() {
        let keymap = Keymap::default();
        let a = KeyCombo::char('a');
        assert_eq!(
            keymap.action(a),
            Some(Action::Plain(ActionId::ToggleAccent))
        );
        assert_eq!(
            keymap.action_in(a, true),
            Some(Action::Note {
                letter: Letter::A,
                chord: false
            })
        );
        assert_eq!(
            keymap.action_in(KeyCombo::char('G'), true),
            Some(Action::Note {
                letter: Letter::G,
                chord: true
            })
        );
        let alt_three = KeyCombo::parse("alt+3").unwrap();
        assert_eq!(keymap.action(alt_three), None);
        assert_eq!(
            keymap.action_in(alt_three, true),
            Some(Action::ChordDigit(3))
        );
        // Keys the layer does not bind still do what they do outside it.
        assert_eq!(
            keymap.action_in(KeyCombo::char('5'), true),
            Some(Action::Digit(5))
        );
        // N switches step entry on, and off again from inside it.
        for step_entry in [false, true] {
            assert_eq!(
                keymap.action_in(KeyCombo::char('N'), step_entry),
                Some(Action::Plain(ActionId::ToggleStepEntry))
            );
        }
        // A key can be in both layers: no conflict.
        assert_eq!(keymap.conflict(ActionId::StepNote, a), None);
        assert_eq!(
            keymap.conflict(ActionId::StepRest, a),
            Some(ActionId::StepNote)
        );
        assert_eq!(
            keymap.conflict(ActionId::Copy, a),
            Some(ActionId::ToggleAccent)
        );
    }

    #[test]
    fn note_keys_must_be_their_letters() {
        assert!(Keymap::parse("h = step_note").is_err());
        assert!(Keymap::parse("A = step_note").is_err());
        assert!(Keymap::parse("a = step_chord_note").is_err());
        assert!(Keymap::parse("alt+x = enter_chord_fret").is_err());
        let mut keymap = Keymap::default();
        assert!(
            keymap
                .bind(ActionId::StepNote, KeyCombo::char('h'))
                .is_err()
        );
        // Rebinding a step-entry action stays in its layer.
        let combo = KeyCombo::parse("ctrl+j").unwrap();
        keymap.bind(ActionId::StepRest, combo).unwrap();
        assert_eq!(keymap.action(combo), None);
        assert_eq!(
            keymap.action_in(combo, true),
            Some(Action::Plain(ActionId::StepRest))
        );
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
