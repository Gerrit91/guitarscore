//! The keymap: the built-in bindings, the file format and its errors, and typed characters
//! as QWERTY, QWERTZ and AZERTY layouts produce them (AGENTS.md section 5, decision 7).

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

use gscore_edit::{
    ACTIONS, Action, ActionId, DEFAULT_KEYMAP, Key, KeyCombo, Keymap, Modifiers, NamedKey,
};

fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Modifiers::default()
    }
}

fn ctrl() -> Modifiers {
    Modifiers {
        ctrl: true,
        ..Modifiers::default()
    }
}

/// What a key press on a layout delivers: the typed character (or key name) and the
/// modifiers that were held.
fn press(key: Key, modifiers: Modifiers) -> Option<Action> {
    Keymap::default().action(KeyCombo::new(key, modifiers))
}

#[test]
fn the_built_in_keymap_parses_and_has_the_owners_keys() {
    let keymap = Keymap::parse(DEFAULT_KEYMAP).expect("the built-in keymap is valid");
    assert_eq!(keymap, Keymap::default());
    let bound = |key: &str| keymap.action(KeyCombo::parse(key).unwrap());
    assert_eq!(bound("Left"), Some(Action::Plain(ActionId::CursorLeft)));
    assert_eq!(
        bound("Down"),
        Some(Action::Plain(ActionId::CursorDownString))
    );
    assert_eq!(bound("7"), Some(Action::Digit(7)));
    assert_eq!(bound("+"), Some(Action::Plain(ActionId::DurationShorter)));
    assert_eq!(bound("-"), Some(Action::Plain(ActionId::DurationLonger)));
    assert_eq!(
        bound("ctrl+Insert"),
        Some(Action::Plain(ActionId::InsertMeasure))
    );
    assert_eq!(bound("ctrl+z"), Some(Action::Plain(ActionId::Undo)));
    assert_eq!(bound("ctrl+y"), Some(Action::Plain(ActionId::Redo)));
    assert_eq!(bound("Space"), Some(Action::Plain(ActionId::PlayPause)));
    assert_eq!(
        keymap.keys_for(ActionId::Undo),
        [KeyCombo::parse("ctrl+z").unwrap()]
    );
}

#[test]
fn qwerty_qwertz_and_azerty_reach_the_same_actions() {
    // QWERTY: `+` is Shift+`=`, `-` and digits are unshifted.
    assert_eq!(
        press(Key::Char('+'), shift()),
        Some(Action::Plain(ActionId::DurationShorter))
    );
    assert_eq!(
        press(Key::Char('5'), Modifiers::default()),
        Some(Action::Digit(5))
    );
    // QWERTZ: `+` has its own key, Y and Z are swapped. Ctrl+Z is the key labelled Z,
    // which the layout reports as `z`.
    assert_eq!(
        press(Key::Char('+'), Modifiers::default()),
        Some(Action::Plain(ActionId::DurationShorter))
    );
    assert_eq!(
        press(Key::Char('z'), ctrl()),
        Some(Action::Plain(ActionId::Undo))
    );
    // AZERTY: digits need Shift (unshifted, the keys type `&é"'(` ...), `-` is on the 6
    // key, `+` is Shift+`=`.
    assert_eq!(press(Key::Char('5'), shift()), Some(Action::Digit(5)));
    assert_eq!(press(Key::Char('('), Modifiers::default()), None);
    assert_eq!(
        press(Key::Char('-'), Modifiers::default()),
        Some(Action::Plain(ActionId::DurationLonger))
    );
    // Caps lock or Shift with Ctrl: `ctrl+Z` is `ctrl+z` plus Shift, which is not undo.
    assert_eq!(
        press(Key::Char('Z'), ctrl()),
        Some(Action::Plain(ActionId::Undo))
    );
    let ctrl_shift = Modifiers {
        shift: true,
        ..ctrl()
    };
    assert_eq!(press(Key::Char('z'), ctrl_shift), None);
    // Named keys keep Shift.
    assert_eq!(press(Key::Named(NamedKey::Left), shift()), None);
}

#[test]
fn key_syntax_round_trips() {
    for text in [
        "ctrl+z",
        "Left",
        "+",
        "ctrl++",
        "alt+shift+F5",
        "=",
        "ctrl+Insert",
    ] {
        let combo = KeyCombo::parse(text).unwrap();
        assert_eq!(
            KeyCombo::parse(&combo.to_string()).unwrap(),
            combo,
            "{text}"
        );
    }
    assert_eq!(
        KeyCombo::parse("CTRL+Z").unwrap(),
        KeyCombo::parse("ctrl+z").unwrap()
    );
    assert_eq!(KeyCombo::parse("ctrl++").unwrap().key(), Key::Char('+'));
    assert_eq!(
        KeyCombo::parse("f12").unwrap().key(),
        Key::Named(NamedKey::F(12))
    );
    for bad in ["", "altgr+e", "hyper+x", "F99", "Enterr", "ctrl+"] {
        assert!(KeyCombo::parse(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn keymap_files_report_every_problem_with_its_line() {
    let text = "\
# comment
Left = cursor_left
Right = no_such_action
altgr+e = undo
Left = cursor_right
x = enter_fret
just text
= = undo
";
    let errors = Keymap::parse(text).unwrap_err();
    let lines: Vec<usize> = errors.iter().map(|e| e.line).collect();
    assert_eq!(lines, [3, 4, 5, 6, 7], "{errors:?}");
    assert!(
        errors[2].to_string().contains("bound to both"),
        "{}",
        errors[2]
    );
    // Binding the same key to the same action twice is harmless.
    assert!(Keymap::parse("Left = cursor_left\nLeft = cursor_left\n").is_ok());
    // Every action id can be bound.
    for action in ACTIONS.iter().filter(|a| a.action != ActionId::EnterFret) {
        let keymap = Keymap::parse(&format!("F1 = {}", action.id)).unwrap();
        assert_eq!(
            keymap.action(KeyCombo::named(NamedKey::F(1))),
            Some(Action::Plain(action.action))
        );
    }
}
