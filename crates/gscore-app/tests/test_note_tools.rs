//! The note tools (fretboard, keyboard, drum map) driven headlessly: they open from the
//! toolbar and the Tools menu, show the cursor's beat and enter notes on a click.
//!
//! The snapshot needs a GPU or a software Vulkan driver; regenerate it with
//! `UPDATE_SNAPSHOTS=1 cargo test -p gscore-app --test test_note_tools`.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gscore_app::GuitarScoreApp;

/// The app with a test file opened, in a window with room for the three tools.
fn app_with(file: &str) -> Harness<'static, GuitarScoreApp> {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(file);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_max_steps(64)
        .wgpu()
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(file)));
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run();
    harness
}

/// The notes of the cursor's beat: each note's string and fret.
fn beat_notes(harness: &Harness<'static, GuitarScoreApp>) -> Vec<(u8, u8)> {
    let document = harness.state().document().unwrap();
    let cursor = document.editor().cursor();
    let beat = document
        .song()
        .measure_at(cursor.position())
        .and_then(|measure| measure.voice(cursor.voice))
        .and_then(|voice| voice.beat(cursor.beat));
    let mut notes: Vec<(u8, u8)> = beat
        .map(|beat| {
            beat.notes()
                .map(|note| (note.string().unwrap().get(), note.fret().unwrap().get()))
                .collect()
        })
        .unwrap_or_default();
    notes.sort_unstable();
    notes
}

fn undo(harness: &mut Harness<'static, GuitarScoreApp>) {
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run();
}

#[test]
fn the_fretboard_shows_the_beat_and_a_click_enters_a_note() {
    let mut harness = app_with("basic/steel_guitar_dadgad.gp");
    assert_eq!(beat_notes(&harness), [(6, 0)]);
    harness.get_by_label("Fretboard").click();
    harness.run();
    // The beat's note is on the board: the open sixth string, a D.
    let board = harness.get_by_label("Fretboard: D on string 6 fret 0");
    // A click in the middle of the board puts a note there (the fourth string of six).
    board.click();
    harness.run();
    let notes = beat_notes(&harness);
    assert_eq!(notes.len(), 2, "{notes:?}");
    let (string, fret) = notes[0];
    assert_eq!(string, 4);
    assert!(fret > 0, "the middle of the neck is not the open string");
    assert!(
        harness
            .query_all_by_label_contains(&format!("on string 4 fret {fret}"))
            .next()
            .is_some(),
        "the new note is marked"
    );
    // The same click takes the note out again, and each click is one undo step.
    harness.get_by_label_contains("Fretboard: ").click();
    harness.run();
    assert_eq!(beat_notes(&harness), [(6, 0)]);
    undo(&mut harness);
    assert_eq!(beat_notes(&harness).len(), 2);
    undo(&mut harness);
    assert_eq!(beat_notes(&harness), [(6, 0)]);
    // The toolbar button closes the window again.
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Fretboard")
        .click();
    harness.run();
    assert!(harness.query_by_label_contains("Fretboard: ").is_none());
}

/// A note entered with a tool is heard like a typed fret; taking it out is silent.
#[test]
fn a_note_entered_on_the_fretboard_is_heard() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = std::env::temp_dir().join(format!("gscore-note-tools-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let font = dir.join("sine.sf2");
    std::fs::write(&font, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = app_with("basic/steel_guitar_dadgad.gp");
    harness.state_mut().playback_mut().use_soundfont(font);
    harness.get_by_label("Fretboard").click();
    harness.run();
    // The first note starts the synthesizer; notes are heard once the SoundFont is loaded.
    harness.get_by_label_contains("Fretboard: ").click();
    harness.run();
    let started = Instant::now();
    while harness.state().playback().is_loading() && harness.state().playback().error().is_none() {
        assert!(started.elapsed() < Duration::from_secs(10), "never loaded");
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    if let Some(error) = harness.state().playback().error() {
        eprintln!("skipped: no usable audio device ({error})");
        std::fs::remove_dir_all(dir).ok();
        return;
    }
    // Let the first note end, then take it out (silent) and put it in again (heard).
    let started = Instant::now();
    while harness.state().playback().auditioning().is_some() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the note kept sounding"
        );
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    harness.get_by_label_contains("Fretboard: ").click();
    harness.run();
    assert_eq!(beat_notes(&harness), [(6, 0)]);
    assert_eq!(harness.state().playback().auditioning(), None);
    harness.get_by_label_contains("Fretboard: ").click();
    harness.run();
    assert_eq!(beat_notes(&harness).len(), 2);
    assert!(
        harness.state().playback().auditioning().is_some(),
        "the entered note is heard"
    );
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_keyboard_shows_the_beat_and_a_click_enters_a_note() {
    let mut harness = app_with("basic/steel_guitar_dadgad.gp");
    harness.get_by_label("Keyboard").click();
    harness.run();
    let keys = harness.get_by_label("Keyboard: D");
    // A key in the middle of the keyboard goes onto a free string.
    keys.click();
    harness.run();
    let notes = beat_notes(&harness);
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(notes.contains(&(6, 0)));
    // The same key again takes it out.
    harness.get_by_label_contains("Keyboard: ").click();
    harness.run();
    assert_eq!(beat_notes(&harness), [(6, 0)]);
    // Escape closes the window.
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(harness.query_by_label_contains("Keyboard: ").is_none());
}

#[test]
fn the_drum_map_lists_the_kit_and_enters_drums_on_a_drum_track() {
    let mut harness = app_with("basic/multi_staff.gp");
    // From the Tools menu.
    harness.get_by_label("Tools").click();
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, "Drum map")
        .click();
    harness.run();
    harness.run();
    // On a guitar track the map is only shown.
    harness.get_by_label("Bass drum 1");
    harness.get_by_label("Closed hi-hat");
    assert!(harness.query_all_by_label("Kick").count() >= 2);
    let before = beat_notes(&harness);
    // On the drum track (the sixth) a click on a drum puts it into the beat.
    harness.state_mut().set_shown_track(5);
    harness.run();
    harness.run();
    let on_drums = beat_notes(&harness);
    assert_ne!(before, on_drums, "the cursor is on the drum track");
    let cowbell = 56_u8;
    assert!(!on_drums.iter().any(|(_, key)| *key == cowbell));
    harness.get_by_label("Cowbell").scroll_to_me();
    harness.run();
    harness.get_by_label("Cowbell").click();
    harness.run();
    assert!(beat_notes(&harness).iter().any(|(_, key)| *key == cowbell));
    harness.get_by_label("Cowbell").click();
    harness.run();
    assert_eq!(beat_notes(&harness), on_drums);
}

#[test]
fn the_note_tools_match_their_snapshot() {
    let mut harness = app_with("basic/steel_guitar_dadgad.gp");
    for tool in ["Fretboard", "Keyboard", "Drum map"] {
        harness.get_by_label(tool).click();
        harness.run();
    }
    harness.snapshot("note_tools");
}

/// A drum track's Appearance section has the drum tab's switch: it turns the track's
/// tablature on (the MIDI drum notes below the percussion staff), as one undo step.
#[test]
fn the_track_panel_switches_a_drum_tracks_tablature() {
    let mut harness = app_with("basic/multi_staff.gp");
    harness.state_mut().set_shown_track(5);
    harness.run();
    harness.run();
    let tablature = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        let track = document
            .song()
            .track(gscore_model::TrackIndex::new(5))
            .unwrap();
        assert!(track.is_percussion());
        track.staves().tablature()
    };
    assert!(!tablature(&harness), "the file's kit has no tab");
    harness.get_by_label("Drum tablature").click();
    harness.run();
    assert!(tablature(&harness));
    assert_eq!(harness.state().notice(), None, "the score lays out");
    harness.snapshot("drum_tablature");
    harness.get_by_label("Drum tablature").click();
    harness.run();
    assert!(!tablature(&harness));
    undo(&mut harness);
    assert!(tablature(&harness));
}

/// A note entered with a tool in a bar without a beat (an empty Guitar Pro 7 bar) goes in
/// like a typed one: the note's beat is made first, as one undo step with the note.
#[test]
fn the_tools_enter_a_note_in_a_bar_without_beats() {
    use gscore_edit::Command;
    use gscore_model::TrackIndex;

    let mut harness = app_with("basic/multi_staff.gp");
    // The clean guitar rests from bar 5 on: bar 7 has no beat stored.
    let to_bar_7 = |harness: &mut Harness<'static, GuitarScoreApp>| {
        let document = harness.state_mut().document_mut().unwrap();
        let mut cursor = document.editor().cursor();
        cursor.measure = gscore_model::MeasureIndex::new(6);
        cursor.beat = gscore_model::BeatIndex::new(0);
        document.editor_mut().set_cursor(cursor);
    };
    let beatless = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        document
            .song()
            .beat(document.editor().cursor().position())
            .is_none()
    };
    to_bar_7(&mut harness);
    harness.run();
    assert!(beatless(&harness), "the bar has no beat");
    // On the fretboard.
    harness.get_by_label("Fretboard").click();
    harness.run();
    harness.get_by_label_contains("Fretboard: ").click();
    harness.run();
    assert_eq!(harness.state().notice(), None);
    assert_eq!(beat_notes(&harness).len(), 1);
    undo(&mut harness);
    assert!(
        beatless(&harness),
        "the beat and its note are one undo step"
    );

    // On the drum map, with the track made a kit.
    {
        let document = harness.state_mut().document_mut().unwrap();
        document
            .editor_mut()
            .execute_command(Command::SetTrackDrums {
                track: TrackIndex::new(0),
                drums: true,
            })
            .unwrap();
    }
    to_bar_7(&mut harness);
    harness.run();
    assert!(beatless(&harness));
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Drum map")
        .click();
    harness.run();
    harness.get_by_label("Bass drum 1").click();
    harness.run();
    assert_eq!(harness.state().notice(), None);
    let notes = beat_notes(&harness);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(notes[0].1, 36);
}
