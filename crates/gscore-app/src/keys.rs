//! Turns egui's keyboard events into the keymap's [`KeyCombo`]s (AGENTS.md section 5,
//! decision 7).
//!
//! egui reports a key press twice: a `Key` event (the logical key, following the layout)
//! and, if the press typed something, a `Text` event with the character. Characters come
//! from `Text`, so `+` is `+` whether the layout needs Shift for it or not; named keys
//! (arrows, Insert, Space, ...) come from `Key`. With Ctrl (Cmd on macOS) nothing is typed,
//! so `ctrl+z` comes from the `Key` event's logical key. egui turns Ctrl+C/X/V into
//! clipboard events, which map back to `ctrl+c`/`x`/`v`.
//!
//! While an input method composes text (IME), typed text is never a command.

use egui::{Event, ImeEvent};
use gscore_edit::{Key, KeyCombo, Modifiers, NamedKey};

/// Keyboard state that lasts across frames.
#[derive(Debug, Default)]
pub(crate) struct KeyReader {
    /// An input method is composing text.
    composing: bool,
}

impl KeyReader {
    /// The key combo of `event`, if it is a key press the keymap could bind.
    pub(crate) fn combo(&mut self, event: &Event) -> Option<KeyCombo> {
        match event {
            Event::Ime(ime) => {
                // A non-empty preedit means the input method is composing; a commit or an
                // empty preedit ends it.
                match ime {
                    ImeEvent::Preedit { text, .. } => self.composing = !text.is_empty(),
                    ImeEvent::Commit(_) => self.composing = false,
                    _ => {}
                }
                None
            }
            Event::Text(text) if !self.composing => {
                let mut chars = text.chars();
                match (chars.next(), chars.next()) {
                    // A space is the named key Space (from its `Key` event).
                    (Some(c), None) if c != ' ' => {
                        Some(KeyCombo::new(Key::Char(c), Modifiers::default()))
                    }
                    _ => None,
                }
            }
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                let mods = Modifiers {
                    ctrl: modifiers.command,
                    alt: modifiers.alt,
                    shift: modifiers.shift,
                };
                if let Some(named) = named_key(*key) {
                    return Some(KeyCombo::new(Key::Named(named), mods));
                }
                // Typed characters come from `Text`; only Ctrl combinations type nothing.
                if !modifiers.command {
                    return None;
                }
                // egui names the minus key with the typographic minus sign (U+2212); the
                // keymap binds the ASCII `-` the key types (`ctrl+-` zooms out).
                if *key == egui::Key::Minus {
                    return Some(KeyCombo::new(Key::Char('-'), mods));
                }
                let mut chars = key.symbol_or_name().chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Some(KeyCombo::new(Key::Char(c), mods)),
                    _ => None,
                }
            }
            Event::Copy => Some(ctrl_char('c')),
            Event::Cut => Some(ctrl_char('x')),
            Event::Paste(_) => Some(ctrl_char('v')),
            _ => None,
        }
    }
}

fn ctrl_char(c: char) -> KeyCombo {
    KeyCombo::new(
        Key::Char(c),
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    )
}

/// The keymap name of a key that types nothing.
fn named_key(key: egui::Key) -> Option<NamedKey> {
    use egui::Key as K;
    Some(match key {
        K::ArrowLeft => NamedKey::Left,
        K::ArrowRight => NamedKey::Right,
        K::ArrowUp => NamedKey::Up,
        K::ArrowDown => NamedKey::Down,
        K::Home => NamedKey::Home,
        K::End => NamedKey::End,
        K::PageUp => NamedKey::PageUp,
        K::PageDown => NamedKey::PageDown,
        K::Insert => NamedKey::Insert,
        K::Delete => NamedKey::Delete,
        K::Backspace => NamedKey::Backspace,
        K::Enter => NamedKey::Enter,
        K::Tab => NamedKey::Tab,
        K::Escape => NamedKey::Escape,
        K::Space => NamedKey::Space,
        K::F1 => NamedKey::F(1),
        K::F2 => NamedKey::F(2),
        K::F3 => NamedKey::F(3),
        K::F4 => NamedKey::F(4),
        K::F5 => NamedKey::F(5),
        K::F6 => NamedKey::F(6),
        K::F7 => NamedKey::F(7),
        K::F8 => NamedKey::F(8),
        K::F9 => NamedKey::F(9),
        K::F10 => NamedKey::F(10),
        K::F11 => NamedKey::F(11),
        K::F12 => NamedKey::F(12),
        K::F13 => NamedKey::F(13),
        K::F14 => NamedKey::F(14),
        K::F15 => NamedKey::F(15),
        K::F16 => NamedKey::F(16),
        K::F17 => NamedKey::F(17),
        K::F18 => NamedKey::F(18),
        K::F19 => NamedKey::F(19),
        K::F20 => NamedKey::F(20),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn ctrl_minus_is_the_ascii_minus_the_keymap_binds() {
        // egui names the key with U+2212; `ctrl+-` must still match.
        let mut reader = KeyReader::default();
        assert_eq!(
            reader.combo(&key(egui::Key::Minus, egui::Modifiers::COMMAND)),
            Some(KeyCombo::parse("ctrl+-").unwrap())
        );
    }

    #[test]
    fn characters_come_from_text_and_named_keys_from_key_events() {
        let mut reader = KeyReader::default();
        // QWERTY `+`: Shift+`=`; the Key event is ignored, the text counts without Shift.
        assert_eq!(
            reader.combo(&key(egui::Key::Equals, egui::Modifiers::SHIFT)),
            None
        );
        assert_eq!(
            reader.combo(&Event::Text("+".into())),
            Some(KeyCombo::char('+'))
        );
        assert_eq!(
            reader.combo(&key(egui::Key::ArrowLeft, egui::Modifiers::NONE)),
            Some(KeyCombo::named(NamedKey::Left))
        );
        assert_eq!(reader.combo(&Event::Text(" ".into())), None);
        assert_eq!(
            reader.combo(&key(egui::Key::Z, egui::Modifiers::COMMAND)),
            Some(KeyCombo::parse("ctrl+z").unwrap())
        );
        assert_eq!(
            reader.combo(&Event::Copy),
            Some(KeyCombo::parse("ctrl+c").unwrap())
        );
    }

    #[test]
    fn text_typed_while_an_input_method_composes_is_ignored() {
        let mut reader = KeyReader::default();
        assert_eq!(
            reader.combo(&Event::Ime(ImeEvent::Preedit {
                text: "´".into(),
                active_range_chars: None,
            })),
            None
        );
        assert_eq!(reader.combo(&Event::Text("5".into())), None);
        assert_eq!(
            reader.combo(&Event::Ime(ImeEvent::Commit("é".into()))),
            None
        );
        assert_eq!(
            reader.combo(&Event::Text("5".into())),
            Some(KeyCombo::char('5'))
        );
    }
}
