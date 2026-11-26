//! Crash recovery in the app: edits are journaled, a crashed document is offered at the
//! next start (Restore, Discard, Later), a normal exit leaves nothing behind, and a command
//! that panics is contained (AGENTS.md "Never lose work", "Panic-injection test").

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Event, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gscore_app::GuitarScoreApp;
use gscore_edit::Keymap;
use gscore_model::{StringNumber, differences};
use gscore_session::find_sessions;

/// A private copy of a test file and a recovery directory next to it.
fn scratch(test: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("gscore-app-rec-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("note_lengths.gp");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/basic/note_lengths.gp");
    std::fs::copy(source, &file).unwrap();
    (file, dir.join("recovery"))
}

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

fn app(file: Option<&Path>, recovery: &Path) -> Harness<'static, GuitarScoreApp> {
    let file = file.map(Path::to_path_buf);
    let recovery = recovery.to_path_buf();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .with_max_steps(64)
        .build_eframe(move |cc| {
            GuitarScoreApp::with_options(&cc.egui_ctx, file, Keymap::default(), Some(recovery))
        });
    settle(&mut harness);
    harness
}

fn type_digit(harness: &mut Harness<'static, GuitarScoreApp>, digit: &str) {
    harness.event_modifiers(Event::Text(digit.to_owned()), Modifiers::NONE);
    harness.run();
}

fn fret_at_cursor(app: &GuitarScoreApp) -> Option<u8> {
    let document = app.document()?;
    let beat = document
        .song()
        .beat(document.editor().cursor().position())?;
    beat.notes()
        .find(|n| n.string().ok() == StringNumber::new(1).ok())
        .and_then(|n| n.fret().ok())
        .map(|fret| fret.get())
}

/// The folders in the recovery directory.
fn folders(recovery: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(recovery)
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default()
}

/// Waits until `done` or a few seconds have passed.
fn eventually(done: impl Fn() -> bool) -> bool {
    let started = Instant::now();
    while !done() {
        if started.elapsed() > Duration::from_secs(10) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

/// Opens `file`, types a fret and "crashes" (the app goes away without its normal exit).
/// Returns the song as it was edited.
fn edit_and_crash(file: &Path, recovery: &Path) -> gscore_model::Song {
    let mut harness = app(Some(file), recovery);
    type_digit(&mut harness, "7");
    assert_eq!(fret_at_cursor(harness.state()), Some(7));
    // While the app runs, its journal is locked: nothing to offer.
    assert!(find_sessions(recovery).is_empty());
    let edited = harness.state().document().unwrap().song().clone();
    drop(harness);
    edited
}

#[test]
fn a_sound_change_is_restored_from_the_journal() {
    use gscore_model::TrackIndex;

    let (file, recovery) = scratch("sound");
    let mut harness = app(Some(&file), &recovery);
    // The first track's send into the compressor bus: a drag of the mixer's Comp knob.
    harness.get_by_label("Mixer view").click();
    harness.run();
    let knob = harness.get_by_label("Comp Jazz Guitar").rect().center();
    harness.hover_at(knob);
    harness.run();
    harness.drag_at(knob);
    harness.run();
    for step in 1..=10 {
        harness.hover_at(knob + egui::vec2(6.0 * step as f32, 0.0));
        harness.run();
    }
    harness.drop_at(knob + egui::vec2(60.0, 0.0));
    harness.run();
    let state = |song: &gscore_model::Song| {
        song.track(TrackIndex::new(0))
            .and_then(|view| view.sound_state())
            .map(str::to_owned)
    };
    let edited = harness.state().document().unwrap().song().clone();
    let payload = state(&edited).expect("the send is saved with the song");
    drop(harness);
    assert_eq!(find_sessions(&recovery).len(), 1);

    // Restored from the journal, the song has the same per-track state.
    let mut harness = app(None, &recovery);
    harness.get_by_label("Restore").click();
    settle(&mut harness);
    assert_eq!(
        state(harness.state().document().unwrap().song()),
        Some(payload)
    );
    // And undo takes the change back too.
    harness
        .state_mut()
        .document_mut()
        .unwrap()
        .editor_mut()
        .undo()
        .unwrap();
    assert_eq!(state(harness.state().document().unwrap().song()), None);
}

#[test]
fn a_crashed_document_is_restored_with_its_edits_and_undo() {
    let (file, recovery) = scratch("restore");
    let edited = edit_and_crash(&file, &recovery);
    assert_eq!(find_sessions(&recovery).len(), 1);

    let mut harness = app(None, &recovery);
    harness.get_by_label("Recover unsaved changes?");
    harness.get_by_label("Restore").click();
    settle(&mut harness);
    let app = harness.state();
    let document = app.document().expect("the recovered document");
    assert_eq!(document.path(), file.as_path());
    assert!(differences(&edited, document.song()).is_empty());
    assert!(document.is_dirty(), "recovered edits are not saved yet");
    assert!(
        app.status().unwrap().contains("Recovered 1 edit"),
        "{:?}",
        app.status()
    );

    // Undo goes back to the file as it was.
    let original = gscore_session::open(&file).unwrap().song;
    harness
        .state_mut()
        .document_mut()
        .unwrap()
        .editor_mut()
        .undo()
        .unwrap();
    assert!(differences(&original, harness.state().document().unwrap().song()).is_empty());

    // The old folder went; the restored document has its own (locked) journal.
    assert!(eventually(|| folders(&recovery).len() == 1));
    assert!(find_sessions(&recovery).is_empty());
}

#[test]
fn later_keeps_a_crashed_document_and_discard_removes_it() {
    let (file, recovery) = scratch("later");
    let _ = edit_and_crash(&file, &recovery);

    let mut harness = app(None, &recovery);
    harness.get_by_label("Later").click();
    harness.run();
    assert!(harness.state().document().is_none());
    drop(harness);
    assert_eq!(find_sessions(&recovery).len(), 1, "offered again next time");

    let mut harness = app(None, &recovery);
    harness.get_by_label("Discard").click();
    harness.run();
    assert!(eventually(|| folders(&recovery).is_empty()));
    assert!(harness.state().document().is_none());
}

#[test]
fn a_normal_exit_leaves_nothing_to_recover() {
    let (file, recovery) = scratch("exit");
    let mut harness = app(Some(&file), &recovery);
    type_digit(&mut harness, "5");
    assert_eq!(folders(&recovery).len(), 1);
    eframe::App::on_exit(harness.state_mut());
    assert!(folders(&recovery).is_empty());
}

#[test]
fn a_panicking_command_changes_nothing_and_editing_goes_on() {
    let (file, recovery) = scratch("panic");
    let mut harness = app(Some(&file), &recovery);
    let before = harness.state().document().unwrap().song().clone();
    gscore_edit::testing::panic_next_command();
    type_digit(&mut harness, "5");
    let app = harness.state();
    assert!(
        app.notice().unwrap().contains("failed unexpectedly"),
        "{:?}",
        app.notice()
    );
    assert!(differences(&before, app.document().unwrap().song()).is_empty());
    // An emergency snapshot was written to the recovery folder.
    let folder = folders(&recovery).pop().unwrap();
    assert!(eventually(|| folder.join("snapshot.gp").exists()));

    // The next edit works as usual.
    type_digit(&mut harness, "3");
    assert_eq!(fret_at_cursor(harness.state()), Some(3));
}

#[test]
fn a_crashed_document_comes_back_where_the_cursor_was() {
    let (file, recovery) = scratch("place");
    let mut harness = app(Some(&file), &recovery);
    // An edit, then the cursor moves on: two measures right and a string down.
    type_digit(&mut harness, "7");
    for key in [
        egui::Key::ArrowRight,
        egui::Key::ArrowRight,
        egui::Key::ArrowDown,
    ] {
        harness.key_press(key);
        harness.run();
    }
    let left = harness.state().document().unwrap().editor().cursor();
    assert!(left.measure.get() > 0, "{left:?}");
    drop(harness); // a crash: the journal's writer still writes the last place

    let mut harness = app(None, &recovery);
    harness.get_by_label("Restore").click();
    settle(&mut harness);
    let cursor = harness.state().document().unwrap().editor().cursor();
    assert_eq!(
        (cursor.measure, cursor.beat, cursor.string),
        (left.measure, left.beat, left.string),
        "the recovered document opens where the cursor was"
    );
}

#[test]
fn a_panicking_layout_is_contained_and_the_next_one_works() {
    use gscore_layout::testing::{Stage, panic_next};

    let (file, recovery) = scratch("panic-layout");
    let mut harness = app(Some(&file), &recovery);
    // The edit changes a measure, so the page is laid out again: that layout panics.
    panic_next(Stage::Layout);
    type_digit(&mut harness, "5");
    harness.get_by_label("drawing this track failed unexpectedly; a crash report was written");
    let app = harness.state();
    assert_eq!(fret_at_cursor(app), Some(5), "the edit itself stands");
    assert!(app.document().unwrap().is_dirty());
    // The next edit lays the page out again, from a fresh cache, and editing goes on.
    type_digit(&mut harness, "3");
    assert_eq!(fret_at_cursor(harness.state()), Some(3));
    assert!(
        harness
            .query_by_label("drawing this track failed unexpectedly; a crash report was written")
            .is_none(),
        "the page is back"
    );
    // The journal kept recording throughout: a crash now would recover both edits.
    drop(harness);
    let recovered = gscore_session::recover(&find_sessions(&recovery)[0]).unwrap();
    assert_eq!(recovered.replayed, 2);
}

#[test]
fn a_panicking_drawing_is_contained_and_the_next_frame_draws() {
    use gscore_layout::testing::{Stage, panic_next};

    let (file, recovery) = scratch("panic-draw");
    let mut harness = app(Some(&file), &recovery);
    let before = harness.state().document().unwrap().song().clone();
    panic_next(Stage::Draw);
    harness.run();
    let app = harness.state();
    assert_eq!(
        app.notice(),
        Some("Drawing the page failed unexpectedly; a crash report was written")
    );
    assert!(differences(&before, app.document().unwrap().song()).is_empty());
    // The next frames draw the page again, and editing goes on.
    type_digit(&mut harness, "4");
    assert_eq!(fret_at_cursor(harness.state()), Some(4));
    harness.get_by_label("Measure 1, beat 1, string 1");
}
