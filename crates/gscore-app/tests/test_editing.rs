//! Editing in the app, driven by key events as egui delivers them: the cursor, fret entry
//! (including characters that need Shift on some layouts), undo, saving, and the question
//! about unsaved changes.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gscore_app::GuitarScoreApp;
use gscore_model::StringNumber;

fn testdata(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative)
}

/// A private copy of a test file, so saving does not touch `testdata/`.
fn scratch_copy(name: &str, test: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("gscore-app-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(name);
    std::fs::copy(testdata(&format!("basic/{name}")), &file).unwrap();
    (dir, file)
}

/// Runs frames until background jobs are done.
fn settle(harness: &mut Harness<'static, GuitarScoreApp>) {
    let started = Instant::now();
    harness.run();
    while harness.state().is_busy() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run();
}

fn app_with(file: &Path) -> Harness<'static, GuitarScoreApp> {
    let file = file.to_path_buf();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        // Scrolling to the cursor animates over a few frames.
        .with_max_steps(64)
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(file)));
    settle(&mut harness);
    harness
}

/// A typed character, as the layout produced it, with the modifiers held for it.
fn type_char(harness: &mut Harness<'static, GuitarScoreApp>, text: &str, modifiers: Modifiers) {
    harness.event_modifiers(Event::Text(text.to_owned()), modifiers);
    harness.run();
}

fn press(harness: &mut Harness<'static, GuitarScoreApp>, key: Key) {
    harness.key_press(key);
    harness.run();
}

/// The fret on `string` at the cursor.
fn fret_at_cursor(app: &GuitarScoreApp, string: u8) -> Option<u8> {
    let document = app.document()?;
    let beat = document
        .song()
        .beat(document.editor().cursor().position())?;
    beat.notes()
        .find(|n| n.string().ok() == StringNumber::new(string).ok())
        .and_then(|n| n.fret().ok())
        .map(|f| f.get())
}

#[test]
fn keys_move_the_cursor_enter_frets_and_undo() {
    let (dir, file) = scratch_copy("note_lengths.gp", "keys");
    let mut harness = app_with(&file);
    // Measure 1 is one whole note, so Right goes on to measure 2.
    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::ArrowDown);
    let cursor = harness.state().document().unwrap().editor().cursor();
    assert_eq!(
        (cursor.measure.get(), cursor.beat.get(), cursor.string),
        (1, 0, 2)
    );
    harness.get_by_label("Measure 2, beat 1, string 2");

    // AZERTY: the digit 7 needs Shift; the typed character is what counts.
    type_char(&mut harness, "7", Modifiers::SHIFT);
    assert_eq!(fret_at_cursor(harness.state(), 2), Some(7));
    assert!(harness.state().document().unwrap().is_dirty());
    harness.get_by_label("note_lengths.gp (changed)");

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    assert_ne!(fret_at_cursor(harness.state(), 2), Some(7));
    assert_eq!(harness.state().notice(), None);

    // A character no key is bound to does nothing; neither does a space typed as text.
    type_char(&mut harness, "q", Modifiers::NONE);
    assert_eq!(harness.state().notice(), None);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn plus_typed_with_or_without_shift_shortens_the_note() {
    let (dir, file) = scratch_copy("note_lengths.gp", "plus");
    let mut harness = app_with(&file);
    let value = |h: &Harness<'static, GuitarScoreApp>| {
        let document = h.state().document().unwrap();
        let beat = document
            .song()
            .beat(document.editor().cursor().position())
            .unwrap();
        beat.note_value().unwrap().value()
    };
    let before = value(&harness);
    // QWERTY: Shift+`=` types `+`; QWERTZ: the `+` key without Shift.
    type_char(&mut harness, "+", Modifiers::SHIFT);
    assert_eq!(value(&harness), before * 2);
    type_char(&mut harness, "+", Modifiers::NONE);
    assert_eq!(value(&harness), before * 4);
    type_char(&mut harness, "-", Modifiers::NONE);
    assert_eq!(value(&harness), before * 2);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn ctrl_s_saves_a_gp_file_in_place_with_a_backup() {
    let (dir, file) = scratch_copy("steel_guitar_dadgad.gp", "save");
    let mut harness = app_with(&file);
    type_char(&mut harness, "9", Modifiers::NONE);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::S);
    settle(&mut harness);
    let app = harness.state();
    assert_eq!(app.notice(), None);
    assert!(!app.document().unwrap().is_dirty());
    assert!(
        app.status()
            .unwrap()
            .starts_with("Saved steel_guitar_dadgad.gp")
    );
    assert!(dir.join("steel_guitar_dadgad.gp.bak").exists());

    let saved = gscore_session::open(&file).unwrap().song;
    let beat = saved
        .beat(app.document().unwrap().editor().cursor().position())
        .unwrap();
    assert!(
        beat.notes()
            .any(|n| n.fret().ok().map(|f| f.get()) == Some(9))
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn opening_another_file_with_unsaved_changes_asks_first() {
    let (dir, file) = scratch_copy("note_lengths.gp", "unsaved");
    let mut harness = app_with(&file);
    type_char(&mut harness, "3", Modifiers::NONE);
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .open(testdata("basic/steel_guitar_dadgad.gp"), &ctx);
    harness.run();
    assert!(harness.state().has_dialog());
    harness.get_by_label("Unsaved changes");
    // Keys do not edit while the question is open.
    type_char(&mut harness, "5", Modifiers::NONE);
    assert_eq!(fret_at_cursor(harness.state(), 1), Some(3));

    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(!harness.state().has_dialog());
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Note Lengths"
    );

    harness
        .state_mut()
        .open(testdata("basic/steel_guitar_dadgad.gp"), &ctx);
    harness.run();
    harness.get_by_label("Discard changes").click();
    settle(&mut harness);
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Steel Guitar DADGAD"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_edit_menu_undoes_and_shows_the_keys() {
    let (dir, file) = scratch_copy("note_lengths.gp", "menu");
    let mut harness = app_with(&file);
    type_char(&mut harness, "4", Modifiers::NONE);
    harness.get_by_label("Edit").click();
    harness.run();
    // The entry's label includes its keys from the keymap.
    harness.get_by_label_contains("ctrl+z");
    harness.get_by_label_contains("Undo").click();
    harness.run();
    assert_ne!(fret_at_cursor(harness.state(), 1), Some(4));
    std::fs::remove_dir_all(&dir).unwrap();
}
