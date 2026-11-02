//! The keymap file: missing, valid, broken and unreadable files all leave a usable keymap.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;

use gscore_edit::{Action, ActionId, KeyCombo, Keymap, NamedKey};
use gscore_session::load_keymap;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gscore-config-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn keymap_files_are_used_when_valid_and_replaced_by_the_default_otherwise() {
    let dir = temp_dir("keymap");
    let path = dir.join("keymap.txt");

    let missing = load_keymap(&path);
    assert_eq!(missing.keymap, Keymap::default());
    assert_eq!((missing.path, missing.problem), (None, None));

    std::fs::write(&path, "F2 = undo\n").unwrap();
    let valid = load_keymap(&path);
    assert_eq!(valid.problem, None);
    assert_eq!(
        valid.keymap.action(KeyCombo::named(NamedKey::F(2))),
        Some(Action::Plain(ActionId::Undo))
    );

    std::fs::write(&path, "F2 = undo\nF3 = frobnicate\n").unwrap();
    let broken = load_keymap(&path);
    assert_eq!(broken.keymap, Keymap::default());
    let problem = broken.problem.unwrap();
    assert!(
        problem.contains("line 2") && problem.contains("frobnicate"),
        "{problem}"
    );

    std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
    assert!(load_keymap(&path).problem.unwrap().contains("cannot read"));

    std::fs::remove_dir_all(&dir).unwrap();
}
