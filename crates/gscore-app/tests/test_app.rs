//! The app driven headlessly through its accessibility tree (as a user would click), plus
//! offscreen snapshots compared with `tests/snapshots/`.
//!
//! The snapshot test renders with wgpu and needs a GPU or a software Vulkan driver. If the
//! picture changes on purpose, regenerate with
//! `UPDATE_SNAPSHOTS=1 cargo test -p gscore-app --test test_app` and have the owner review
//! the new images before committing.

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
use gscore_app::{GuitarScoreApp, Output, ThemeChoice};

fn testdata(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative)
}

/// The app with `file` opened (or failed to open), ready for interaction.
fn app_with(file: Option<PathBuf>) -> Harness<'static, GuitarScoreApp> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, file));
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "opening timed out"
        );
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run();
    harness
}

#[test]
fn a_file_given_at_start_opens_in_the_background() {
    let harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    let app = harness.state();
    assert_eq!(app.notice(), None);
    let document = app.document().expect("a document");
    assert_eq!(document.display_title(), "Steel Guitar DADGAD");
    harness.get_by_label("1 measure, 1 track");
}

#[test]
fn without_a_file_the_app_explains_how_to_open_one() {
    let harness = app_with(None);
    assert!(harness.state().document().is_none());
    harness.get_by_label("No file open");
}

#[test]
fn files_that_cannot_be_opened_give_a_notice_not_a_crash() {
    for (file, expected) in [
        (testdata("basic/no_such_file.gp"), "cannot read"),
        (testdata("basic/README.md"), "cannot import"),
    ] {
        let harness = app_with(Some(file));
        let notice = harness.state().notice().expect("a notice");
        assert!(notice.contains(expected), "{notice}");
        assert!(harness.state().document().is_none());
        harness.get_by_label("Dismiss");
    }
}

#[test]
fn the_view_menu_switches_the_theme_and_the_toolbar_zooms() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Dark theme").click();
    harness.run();
    assert_eq!(harness.state().view().theme, ThemeChoice::Dark);

    harness.get_by_label("+").click();
    harness.run();
    assert!((harness.state().view().zoom() - 1.25).abs() < 1e-4);
    harness.get_by_label("125 %");
}

#[test]
fn the_page_matches_its_snapshot_in_both_themes() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .wgpu()
        .build_eframe(|cc| {
            GuitarScoreApp::new(&cc.egui_ctx, Some(testdata("basic/steel_guitar_dadgad.gp")))
        });
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "opening timed out"
        );
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run();
    harness.snapshot("steel_guitar_dadgad_light");
    harness.state_mut().view_mut().theme = ThemeChoice::Dark;
    harness.run();
    harness.snapshot("steel_guitar_dadgad_dark");
}

#[test]
fn play_and_stop_drive_the_midi_output() {
    // Plays to ALSA's silent "Midi Through" port; skipped where there is none.
    const THROUGH: &str = "Midi Through";
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    if harness.state_mut().playback_mut().refresh_ports().is_err()
        || !harness
            .state()
            .playback()
            .ports()
            .iter()
            .any(|p| p.contains(THROUGH))
    {
        eprintln!("skipped: no \"{THROUGH}\" MIDI port");
        return;
    }
    let port = harness
        .state()
        .playback()
        .ports()
        .iter()
        .find(|p| p.contains(THROUGH))
        .cloned();
    harness.state_mut().playback_mut().choose_port(port);

    harness.get_by_label("Play").click();
    harness.step();
    let started = Instant::now();
    while harness.state().playback().state() != gscore_play::PlayState::Playing {
        assert!(started.elapsed() < Duration::from_secs(5), "did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(harness.state().notice(), None);
    harness.step();
    harness.get_by_label("Pause");
    harness.get_by_label("Measure 1");

    harness.get_by_label("Stop").click();
    harness.step();
    let stopping = Instant::now();
    while harness.state().playback().state() != gscore_play::PlayState::Stopped {
        assert!(stopping.elapsed() < Duration::from_secs(5), "did not stop");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gscore-app-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Steps the app until `done` or `seconds` have passed; false on timeout.
fn step_until(
    harness: &mut Harness<'static, GuitarScoreApp>,
    seconds: u64,
    done: impl Fn(&GuitarScoreApp) -> bool,
) -> bool {
    let started = Instant::now();
    while !done(harness.state()) {
        if started.elapsed() > Duration::from_secs(seconds) {
            return false;
        }
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

#[test]
fn outputs_are_chosen_and_labelled() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    let playback = harness.state_mut().playback_mut();
    assert_eq!(*playback.output(), Output::Midi(None));
    assert_eq!(playback.output_label(), "MIDI (automatic)");
    assert_eq!(playback.soundfont(), None);

    let soundfont = PathBuf::from("/fonts/Piano.sf2");
    playback.use_soundfont(soundfont.clone());
    assert_eq!(*playback.output(), Output::Synth(soundfont.clone()));
    assert_eq!(playback.output_label(), "Synthesizer (Piano.sf2)");
    assert_eq!(playback.port(), None);

    // Switching to MIDI keeps the SoundFont on offer.
    playback.choose_port(Some("FLUID Synth".to_owned()));
    assert_eq!(playback.output_label(), "MIDI: FLUID Synth");
    assert_eq!(playback.port(), Some("FLUID Synth"));
    assert_eq!(playback.soundfont(), Some(soundfont.as_path()));
    harness.run();
    harness.get_by_label("Output");
}

#[test]
fn a_broken_soundfont_is_reported_and_editing_goes_on() {
    let dir = temp_dir("broken-sf2");
    let path = dir.join("broken.sf2");
    std::fs::write(&path, "not a soundfont").unwrap();
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.state_mut().playback_mut().use_soundfont(path);
    harness.get_by_label("Play").click();
    assert!(
        step_until(&mut harness, 10, |app| app.notice().is_some()),
        "the problem is shown"
    );
    let notice = harness.state().notice().unwrap().to_owned();
    assert!(notice.contains("broken.sf2"), "{notice}");
    assert_eq!(
        harness.state().playback().state(),
        gscore_play::PlayState::Stopped
    );
}

#[test]
fn play_and_stop_drive_the_built_in_synthesizer() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = temp_dir("synth");
    let path = dir.join("sine.sf2");
    std::fs::write(&path, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.state_mut().playback_mut().use_soundfont(path);

    harness.get_by_label("Play").click();
    let started = step_until(&mut harness, 10, |app| {
        app.playback().state() == gscore_play::PlayState::Playing || app.notice().is_some()
    });
    assert!(started, "neither played nor failed");
    if let Some(notice) = harness.state().notice() {
        eprintln!("skipped: no usable audio device ({notice})");
        return;
    }
    harness.step();
    harness.get_by_label("Pause");
    harness.get_by_label("Measure 1");
    assert_eq!(
        harness.state().playback().output_label(),
        "Synthesizer (sine.sf2)"
    );

    harness.get_by_label("Stop").click();
    assert!(
        step_until(&mut harness, 5, |app| app.playback().state()
            == gscore_play::PlayState::Stopped),
        "did not stop"
    );
}

/// Moves the cursor of the open document to `measure` (0-based), beat 1.
fn cursor_to_measure(harness: &mut Harness<'static, GuitarScoreApp>, measure: usize) {
    let document = harness.state_mut().document_mut().unwrap();
    let mut cursor = document.editor().cursor();
    cursor.measure = gscore_model::MeasureIndex::new(measure);
    cursor.beat = gscore_model::BeatIndex::new(0);
    document.editor_mut().set_cursor(cursor);
}

#[test]
fn playback_starts_at_the_cursor_and_resumes_where_it_paused() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = temp_dir("from-cursor");
    let path = dir.join("sine.sf2");
    std::fs::write(&path, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.state_mut().playback_mut().use_soundfont(path);
    let playing = |app: &GuitarScoreApp| {
        app.playback().state() == gscore_play::PlayState::Playing || app.notice().is_some()
    };

    // Play from measure 3.
    cursor_to_measure(&mut harness, 2);
    harness.state_mut().play_pause();
    assert!(step_until(&mut harness, 10, playing), "did not start");
    if let Some(notice) = harness.state().notice() {
        eprintln!("skipped: no usable audio device ({notice})");
        return;
    }
    assert_eq!(harness.state().playback().current_measure(), Some(3));

    // Pause and play again without moving the cursor: it resumes where it paused (not
    // at the start of measure 3).
    std::thread::sleep(Duration::from_millis(200));
    harness.state_mut().play_pause();
    assert!(step_until(&mut harness, 5, |app| app.playback().state()
        == gscore_play::PlayState::Paused));
    let paused = harness.state().playback().position();
    harness.state_mut().play_pause();
    assert!(step_until(&mut harness, 5, playing));
    assert!(harness.state().playback().position() >= paused);
    assert_eq!(harness.state().playback().current_measure(), Some(3));

    // Pause, move the cursor to measure 1: playing starts there.
    harness.state_mut().play_pause();
    assert!(step_until(&mut harness, 5, |app| app.playback().state()
        == gscore_play::PlayState::Paused));
    cursor_to_measure(&mut harness, 0);
    harness.state_mut().play_pause();
    assert!(step_until(&mut harness, 5, |app| app
        .playback()
        .current_measure()
        == Some(1)
        && app.playback().state() == gscore_play::PlayState::Playing));

    // Stop, cursor to measure 2: Play starts at measure 2, not at the beginning.
    harness.state_mut().playback_mut().stop().unwrap();
    assert!(step_until(&mut harness, 5, |app| app.playback().state()
        == gscore_play::PlayState::Stopped));
    cursor_to_measure(&mut harness, 1);
    harness.state_mut().play_pause();
    assert!(step_until(&mut harness, 5, |app| app
        .playback()
        .current_measure()
        == Some(2)
        && app.playback().state() == gscore_play::PlayState::Playing));
    harness.state_mut().playback_mut().stop().unwrap();
}
