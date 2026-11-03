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
use egui_kittest::kittest::{NodeT as _, Queryable};
use gscore_app::{GuitarScoreApp, Keymap, Output, Settings, ThemeChoice};

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
    harness.get_by_label("Dark score").click();
    harness.run();
    assert_eq!(harness.state().view().theme, ThemeChoice::Dark);

    harness.get_by_label("Zoom in").click();
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
fn the_staff_dialogs_match_their_snapshots() {
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
    harness.get_by_label("Start repeat sign").click();
    harness.run();
    harness.snapshot("staff_repeats_dialog");
    harness.get_by_label("Cancel").click();
    harness.run();
    harness.get_by_label("Swing feel").click();
    harness.run();
    harness.snapshot("staff_feel_dialog");
    harness.get_by_label("Cancel").click();
    harness.run();
    harness.get_by_label("Fermata").click();
    harness.run();
    harness.snapshot("staff_fermata_dialog");
    harness.get_by_label("Cancel").click();
    harness.run();
    harness.get_by_label("Key signature").click();
    harness.run();
    harness.snapshot("staff_key_dialog");
    harness.get_by_label("Cancel").click();
    harness.run();
    harness.get_by_label("Tempo").click();
    harness.run();
    harness.snapshot("staff_tempo_dialog");
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

    // A mixer slider goes straight to the player: playback keeps running and keeps
    // advancing instead of being stopped and rebuilt.
    let track = gscore_model::TrackIndex::new(0);
    let volume = |harness: &Harness<'static, GuitarScoreApp>| {
        harness
            .state()
            .document()
            .unwrap()
            .song()
            .track_sound(track)
            .unwrap()
            .volume
    };
    let before = volume(&harness);
    let position = harness.state().playback().position();
    harness.get_by_label("Volume").decrement_accesskit();
    harness.run();
    assert_eq!(harness.state().notice(), None);
    assert_eq!(
        harness.state().playback().state(),
        gscore_play::PlayState::Playing,
        "a mixer change must not stop playback"
    );
    assert!(
        step_until(&mut harness, 2, |app| app.playback().position() > position),
        "playback stopped advancing after a mixer change"
    );
    assert!(volume(&harness) < before);

    // Undo of a mixer change is live too (through the menu, because the slider may hold
    // the keyboard focus).
    harness.get_by_label("Edit").click();
    harness.step();
    harness.step();
    harness.get_by_label_contains("Undo ctrl+z").click();
    harness.run();
    assert_eq!(
        harness.state().playback().state(),
        gscore_play::PlayState::Playing,
        "undoing a mixer change must not stop playback"
    );
    assert_eq!(volume(&harness), before);

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

/// The first track of the open document.
fn first_track<'a>(harness: &'a Harness<'static, GuitarScoreApp>) -> gscore_model::TrackView<'a> {
    harness
        .state()
        .document()
        .unwrap()
        .song()
        .track(gscore_model::TrackIndex::new(0))
        .unwrap()
}

#[test]
fn the_track_grid_mixes_and_undoes_tracks() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    harness.get_by_label("Track 1: Steel Guitar");
    assert!(!first_track(&harness).is_muted());

    harness.get_by_label("M").click();
    harness.run();
    assert!(first_track(&harness).is_muted());
    harness.get_by_label("M").click();
    harness.run();
    assert!(!first_track(&harness).is_muted());

    harness.get_by_label("S").click();
    harness.run();
    assert!(first_track(&harness).is_soloed());
    harness.get_by_label("S").click();
    harness.run();
    assert!(!first_track(&harness).is_soloed());

    // The volume slider is reachable by assistive technology and moves the model.
    let before = harness
        .state()
        .document()
        .unwrap()
        .song()
        .track_sound(gscore_model::TrackIndex::new(0))
        .unwrap()
        .volume;
    harness.get_by_label("Volume").decrement_accesskit();
    harness.run();
    let after = harness
        .state()
        .document()
        .unwrap()
        .song()
        .track_sound(gscore_model::TrackIndex::new(0))
        .unwrap()
        .volume;
    assert!(
        after < before,
        "volume did not go down: {before} -> {after}"
    );

    // A pan knob is exposed too (dragging it is covered by the model tests).
    harness.get_by_label("Pan");

    // The View menu hides and shows the panel again.
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Track grid").click();
    harness.run();
    assert!(harness.query_by_label("Song grid").is_none());
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Track grid").click();
    harness.run();
    harness.get_by_label("Song grid");
}

/// The sound of the first track.
fn first_sound(harness: &Harness<'static, GuitarScoreApp>) -> gscore_model::TrackSound {
    harness
        .state()
        .document()
        .unwrap()
        .song()
        .track_sound(gscore_model::TrackIndex::new(0))
        .unwrap()
}

#[test]
fn the_track_panel_edits_the_shown_track() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    // The panel is open by default and shows the first track.
    harness.get_by_label("SoundFont preset");

    // The SoundFont preset (the General MIDI program) changes and is undoable.
    let before = first_sound(&harness).program;
    harness.get_by_label("SoundFont preset").click();
    harness.run();
    assert!(
        harness.query_by_label("24: Nylon Str Guitar").is_some(),
        "the preset list did not open"
    );
    harness
        .get_by_label("24: Nylon Str Guitar")
        .click_accesskit();
    harness.run();
    assert_eq!(first_sound(&harness).program, 24);
    assert_ne!(before, 24);
    // Close the preset list first: keys go to the score only when no popup is open.
    harness.key_press(egui::Key::Escape);
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run();
    assert_eq!(first_sound(&harness).program, before);

    // The drum-kit flag (the label text and the checkbox share the name; click the box).
    harness
        .get_all_by_role_and_label(egui::Role::CheckBox, "Drum kit")
        .next()
        .unwrap()
        .click();
    harness.run();
    assert!(first_track(&harness).is_percussion());
    harness
        .get_all_by_role_and_label(egui::Role::CheckBox, "Drum kit")
        .next()
        .unwrap()
        .click();
    harness.run();
    assert!(!first_track(&harness).is_percussion());

    // The capo and the string count (the label text and the spin button share a name).
    harness
        .get_all_by_role_and_label(egui::Role::SpinButton, "Capo")
        .next()
        .unwrap()
        .increment_accesskit();
    harness.run();
    assert_eq!(first_track(&harness).capo().unwrap(), 1);
    harness
        .get_all_by_role_and_label(egui::Role::SpinButton, "Strings")
        .next()
        .unwrap()
        .increment_accesskit();
    harness.run();
    assert_eq!(first_track(&harness).string_count(), 7);

    // A rename in place: click the name, type, Enter. One step too.
    harness.get_by_label("Edit track name").click();
    harness.run();
    harness.get_by_label("Name field").click();
    harness.get_by_label("Name field").type_text(" Lead");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(first_track(&harness).name(), "Steel Guitar Lead");
    // Escape leaves the name as it was.
    harness.get_by_label("Edit track name").click();
    harness.run();
    harness.get_by_label("Name field").type_text(" X");
    harness.run();
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert_eq!(first_track(&harness).name(), "Steel Guitar Lead");
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

#[test]
fn the_settings_window_edits_song_information_and_undoes_it() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Settings…").click();
    harness.run();
    harness.get_by_label("Done");

    harness.get_by_label("Title field").click();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.get_by_label("Title field").type_text("New Title");
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    assert_eq!(
        harness.state().document().unwrap().song().title(),
        "New Title"
    );

    harness.get_by_label("Done").click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run();
    assert_eq!(
        harness.state().document().unwrap().song().title(),
        "Note Lengths"
    );
}

#[test]
fn recent_files_are_remembered_and_open_from_the_file_menu() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    assert!(
        harness
            .state()
            .settings()
            .recent_files
            .iter()
            .any(|path| path.ends_with("steel_guitar_dadgad.gp"))
    );
    // Open another file, then come back to the first through File > Open Recent.
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .open(testdata("basic/note_lengths.gp"), &ctx);
    settle(&mut harness);
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Note Lengths"
    );
    assert!(
        harness
            .state()
            .settings()
            .recent_files
            .first()
            .is_some_and(|path| path.ends_with("note_lengths.gp"))
    );

    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Open Recent").click();
    harness.run();
    harness.get_by_label("steel_guitar_dadgad.gp").click();
    settle(&mut harness);
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Steel Guitar DADGAD"
    );
    assert!(
        harness
            .state()
            .settings()
            .recent_files
            .first()
            .is_some_and(|path| path.ends_with("steel_guitar_dadgad.gp"))
    );
}

#[test]
fn ctrl_equals_zooms_and_ctrl_zero_resets() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Equals);
    harness.run();
    assert!((harness.state().view().zoom() - 1.25).abs() < 1e-4);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num0);
    harness.run();
    assert!((harness.state().view().zoom() - 1.0).abs() < 1e-4);
}

#[test]
fn the_edit_palette_sets_lengths_effects_and_the_measure_header() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));

    // A note length applies to the cursor's beat.
    harness.get_by_label("1/8").click();
    harness.run();
    let value = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        document
            .song()
            .beat(document.editor().cursor().position())
            .unwrap()
            .note_value()
            .unwrap()
            .value()
    };
    assert_eq!(value(&harness), 8);

    // Enter a note on the cursor's string, then toggle an effect on it.
    harness.event_modifiers(egui::Event::Text("4".to_owned()), egui::Modifiers::NONE);
    harness.run();
    harness.get_by_label("Palm mute").click_accesskit();
    harness.run();
    let palm_muted = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        document
            .song()
            .beat(document.editor().cursor().position())
            .and_then(|beat| beat.notes().next())
            .is_some_and(|note| note.is_palm_muted())
    };
    assert!(palm_muted(&harness));
    harness.get_by_label("Palm mute").click_accesskit();
    harness.run();
    assert!(!palm_muted(&harness));

    harness.get_by_label("Rest").click();
    harness.run();
    assert_eq!(
        harness
            .state()
            .document()
            .unwrap()
            .song()
            .beat(
                harness
                    .state()
                    .document()
                    .unwrap()
                    .editor()
                    .cursor()
                    .position()
            )
            .unwrap()
            .kind(),
        gscore_model::BeatKind::Rest
    );
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run();

    // The measure tempo (measure 1 is the song's initial tempo too), in its dialog.
    harness.get_by_label("Tempo").scroll_to_me();
    harness.run();
    harness.get_by_label("Tempo").click();
    harness.run();
    harness
        .get_by_label("Tempo value")
        .set_value_accesskit(130.0);
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    assert_eq!(harness.state().document().unwrap().song().tempo(), 130);
    // Undo puts the field back to the song's tempo, not the value last typed.
    let shown_tempo = |harness: &Harness<'static, GuitarScoreApp>| {
        harness
            .get_by_label("Tempo value")
            .accesskit_node()
            .numeric_value()
    };
    let original = harness.state().document().unwrap().song().clone();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run();
    let tempo = harness.state().document().unwrap().song().tempo();
    assert_ne!(tempo, original.tempo());
    harness.get_by_label("Tempo").click();
    harness.run();
    assert_eq!(shown_tempo(&harness), Some(f64::from(tempo)));
    harness.get_by_label("Cancel").click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Y);
    harness.run();
    harness.get_by_label("Tempo").click();
    harness.run();
    assert_eq!(shown_tempo(&harness), Some(130.0));
    harness.get_by_label("Cancel").click();
    harness.run();

    // Voice 2.
    harness.get_by_label("Voice 2").scroll_to_me();
    harness.run();
    harness.get_by_label("Voice 2").click();
    harness.run();
    assert_eq!(
        harness
            .state()
            .document()
            .unwrap()
            .editor()
            .cursor()
            .voice
            .get(),
        1
    );
}

#[test]
fn a_measure_selection_is_shared_with_the_editor_for_copy_and_paste() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    // Select the first measure in the grid.
    let rect = harness.get_by_label("Song grid").rect();
    let pos = rect.left_top() + egui::vec2(24.0 * 0.0 + 12.0, 26.0 + 12.0);
    harness.hover_at(pos);
    harness.drag_at(pos);
    harness.drop_at(pos);
    harness.run();
    assert!(matches!(
        harness.state().document().unwrap().editor().selection(),
        Some(gscore_edit::Selection::Measures { .. })
    ));
}

#[test]
fn the_view_menu_and_the_row_menu_show_the_track_panel() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    harness.get_by_label("SoundFont preset");

    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Track panel").click();
    harness.run();
    assert!(harness.query_by_label("SoundFont preset").is_none());

    // The row's "Edit track…" opens it again.
    right_click_track(&mut harness, "Track 1: Steel Guitar");
    harness.get_by_label("Edit track…").click();
    harness.run();
    harness.get_by_label("SoundFont preset");
}

#[test]
fn clicking_the_song_grid_moves_the_cursor_and_selects_measures() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    let rect = harness.get_by_label("Song grid").rect();
    // The third measure, first track: 24 px square cells under a 26 px ruler.
    let pos = rect.left_top() + egui::vec2(24.0 * 2.0 + 12.0, 26.0 + 12.0);
    harness.hover_at(pos);
    harness.drag_at(pos);
    harness.drop_at(pos);
    harness.run();
    let cursor = harness.state().document().unwrap().editor().cursor();
    assert_eq!(cursor.measure, gscore_model::MeasureIndex::new(2));
    assert_eq!(cursor.track, gscore_model::TrackIndex::new(0));
    assert_eq!(
        harness
            .state()
            .view()
            .selection
            .map(gscore_app::MeasureSelection::start),
        Some(2)
    );
}

/// Right-clicks at `pos` (kittest's helpers only click the primary button).
fn right_click_at(harness: &mut Harness<'static, GuitarScoreApp>, pos: egui::Pos2) {
    harness.hover_at(pos);
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    harness.run();
}

/// Clicks at `pos` with the primary button.
fn left_click_at(harness: &mut Harness<'static, GuitarScoreApp>, pos: egui::Pos2) {
    harness.hover_at(pos);
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    harness.run();
}

/// Right-clicks the name part of a track row (the controls are on the right).
fn right_click_track(harness: &mut Harness<'static, GuitarScoreApp>, label: &str) {
    let rect = harness.get_by_label(label).rect();
    right_click_at(harness, rect.left_center() + egui::vec2(60.0, 0.0));
}

#[test]
fn a_track_row_offers_add_delete_duplicate_and_rename_with_undo() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    right_click_track(&mut harness, "Track 1: Steel Guitar");
    harness.get_by_label("Duplicate track").click();
    harness.run();
    assert_eq!(harness.state().document().unwrap().song().track_count(), 2);
    harness.get_by_label("Track 2: Steel Guitar 2");

    right_click_track(&mut harness, "Track 1: Steel Guitar");
    harness.get_by_label("Rename…").click();
    harness.run();
    harness.get_by_label("Track name").click();
    harness.get_by_label("Track name").type_text(" 2");
    harness.run();
    harness.get_by_label("Rename").click();
    harness.run();
    assert_eq!(first_track(&harness).name(), "Steel Guitar 2");

    right_click_track(&mut harness, "Track 1: Steel Guitar 2");
    harness.get_by_label("Delete track").click();
    harness.run();
    assert_eq!(harness.state().document().unwrap().song().track_count(), 1);
    assert!(harness.state().notice().is_none());
}

/// The app built with `settings`, waiting until opening finishes.
fn app_with_settings(settings: Settings) -> Harness<'static, GuitarScoreApp> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_eframe(|cc| {
            GuitarScoreApp::with_settings(
                &cc.egui_ctx,
                None,
                Keymap::default(),
                None,
                settings,
                None,
            )
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
    harness
}

#[test]
fn the_last_file_reopens_at_startup() {
    let settings = Settings {
        reopen_last_file: true,
        last_file: Some(testdata("basic/note_lengths.gp")),
        ..Settings::default()
    };
    let harness = app_with_settings(settings);
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Note Lengths"
    );
}

#[test]
fn a_missing_last_file_starts_without_a_song() {
    let settings = Settings {
        reopen_last_file: true,
        last_file: Some(testdata("basic/no_such_file.gp")),
        ..Settings::default()
    };
    let harness = app_with_settings(settings);
    assert!(harness.state().document().is_none());
    harness.get_by_label("No file open");
}

#[test]
fn the_last_file_stays_closed_when_reopening_is_off() {
    let settings = Settings {
        reopen_last_file: false,
        last_file: Some(testdata("basic/note_lengths.gp")),
        ..Settings::default()
    };
    let harness = app_with_settings(settings);
    assert!(harness.state().document().is_none());
}

#[test]
fn the_shown_track_changes_from_the_grid_during_playback() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = temp_dir("grid-track-switch");
    let path = dir.join("sine.sf2");
    std::fs::write(&path, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    right_click_track(&mut harness, "Track 1: Steel Guitar");
    harness.get_by_label("Duplicate track").click();
    harness.run();
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

    // Clicking the second track's row shows it; playback goes on and keeps advancing.
    let position = harness.state().playback().position();
    let row = harness.get_by_label("Track 2: Steel Guitar 2").rect();
    left_click_at(&mut harness, row.left_center() + egui::vec2(60.0, 0.0));
    assert_eq!(harness.state().view().track, 1);
    assert_eq!(
        harness.state().playback().state(),
        gscore_play::PlayState::Playing,
        "switching tracks must not stop playback"
    );
    assert!(
        step_until(&mut harness, 2, |app| app.playback().position() > position),
        "playback stopped advancing after a track switch"
    );
    harness.state_mut().playback_mut().stop().unwrap();
}

#[test]
fn a_count_in_plays_one_bar_then_the_song_and_the_metronome_stays_live() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = temp_dir("count-in");
    let path = dir.join("sine.sf2");
    std::fs::write(&path, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.state_mut().playback_mut().use_soundfont(path);
    harness.state_mut().set_count_in(true);
    cursor_to_measure(&mut harness, 2);

    harness.get_by_label("Play").click();
    let started = step_until(&mut harness, 10, |app| {
        app.playback().state() == gscore_play::PlayState::Playing || app.notice().is_some()
    });
    assert!(started, "neither played nor failed");
    if let Some(notice) = harness.state().notice() {
        eprintln!("skipped: no usable audio device ({notice})");
        return;
    }
    // The count-in is running, but the readout already shows where the song will start
    // (measure 3: two 4/4 bars at 120 BPM, four seconds in) and not the clicks' position.
    assert!(harness.state().playback().count_in_pending());
    assert_eq!(harness.state().playback().current_measure(), Some(3));
    let bar = u64::from(gscore_play::PPQ).saturating_mul(4);
    assert_eq!(harness.state().playback().position(), bar.saturating_mul(2));

    // Turning the metronome on while playing swaps the sequence without a stop.
    harness.state_mut().set_metronome(true);
    assert_eq!(
        harness.state().playback().state(),
        gscore_play::PlayState::Playing,
        "the metronome must not stop playback"
    );

    // After the count-in bar the song starts at measure 3, with the metronome on.
    assert!(
        step_until(&mut harness, 10, |app| {
            app.playback().state() == gscore_play::PlayState::Playing
                && !app.playback().count_in_pending()
                && app.playback().current_measure().is_some()
        }),
        "the count-in did not lead into the song"
    );
    assert_eq!(harness.state().playback().current_measure(), Some(3));
    harness.state_mut().playback_mut().stop().unwrap();
}

#[test]
fn the_player_moves_by_bar_and_to_the_score_ends() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    let measure = |harness: &Harness<'static, GuitarScoreApp>| {
        harness
            .state()
            .document()
            .unwrap()
            .editor()
            .cursor()
            .measure
            .get()
    };
    assert_eq!(measure(&harness), 0);

    harness.get_by_label("Next bar").click();
    harness.run();
    assert_eq!(measure(&harness), 1);
    harness.get_by_label("Next bar").click();
    harness.run();
    assert_eq!(measure(&harness), 2);
    harness.get_by_label("Previous bar").click();
    harness.run();
    assert_eq!(measure(&harness), 1);
    harness.get_by_label("Go to end").click();
    harness.run();
    assert_eq!(measure(&harness), 8);
    harness.get_by_label("Go to start").click();
    harness.run();
    assert_eq!(measure(&harness), 0);
    // Already at the start: going back stays there.
    harness.get_by_label("Previous bar").click();
    harness.run();
    assert_eq!(measure(&harness), 0);
}

#[test]
fn looping_the_selected_measure_restarts_at_its_beginning() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = temp_dir("loop");
    let path = dir.join("sine.sf2");
    std::fs::write(&path, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.state_mut().playback_mut().use_soundfont(path);
    let document = harness.state_mut().document_mut().unwrap();
    document
        .editor_mut()
        .set_selection(Some(gscore_edit::Selection::Measures {
            anchor: gscore_model::MeasureIndex::new(0),
            focus: gscore_model::MeasureIndex::new(0),
        }));
    harness.state_mut().view_mut().selection = Some(gscore_app::MeasureSelection::new(0));
    harness.state_mut().set_loop_selection(true);

    harness.get_by_label("Play").click();
    let started = step_until(&mut harness, 10, |app| {
        app.playback().state() == gscore_play::PlayState::Playing || app.notice().is_some()
    });
    assert!(started, "neither played nor failed");
    if let Some(notice) = harness.state().notice() {
        eprintln!("skipped: no usable audio device ({notice})");
        return;
    }
    // Play through most of the first (2 s) bar, then see the loop take it back to the top.
    let bar = u64::from(gscore_play::PPQ).saturating_mul(4);
    assert!(
        step_until(&mut harness, 5, |app| app.playback().position()
            >= bar.saturating_mul(3) / 4),
        "the first bar did not play"
    );
    assert!(
        step_until(&mut harness, 5, |app| app.playback().position() < bar / 4),
        "playback did not loop back to the start of the selection"
    );
    harness.state_mut().playback_mut().stop().unwrap();
}

#[test]
fn the_new_song_button_opens_a_one_measure_score() {
    let mut harness = app_with(None);
    assert!(harness.state().document().is_none());
    harness.get_by_label("New song").click();
    harness.run();
    let document = harness.state().document().expect("a new document");
    assert_eq!(document.song().measure_count(), 1);
    assert_eq!(document.song().track_count(), 1);
    assert_eq!(document.display_title(), "Untitled.gp");
    harness.get_by_label("1 measure, 1 track");
}

#[test]
fn the_audio_tab_is_last_and_named_audio_io() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness
        .state_mut()
        .playback_mut()
        .use_soundfont(PathBuf::from("/fonts/Test.sf2"));
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Settings…").click();
    harness.run();
    harness.get_by_label("Audio I/O").click();
    harness.run();
    harness.get_by_label("SoundFont file");
    harness.get_by_label("MIDI");
}

#[test]
fn shortcuts_can_be_changed_and_reverted_from_the_editor() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Settings…").click();
    harness.run();
    harness.get_by_label("Editor").click();
    harness.run();
    harness.get_by_label("Edit shortcuts…").click();
    harness.run();
    harness.get_by_label("Filter shortcuts");

    harness.get_by_label("Filter shortcuts").click();
    harness.get_by_label("Filter shortcuts").type_text("Copy");
    harness.run();
    harness.get_by_label("Change").click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::K);
    harness.run();
    assert_eq!(
        harness
            .state()
            .keymap()
            .keys_for(gscore_edit::ActionId::Copy),
        [gscore_edit::KeyCombo::parse("ctrl+k").unwrap()]
    );

    harness.get_by_label("Revert shortcut").click();
    harness.run();
    assert_eq!(
        harness
            .state()
            .keymap()
            .keys_for(gscore_edit::ActionId::Copy),
        Keymap::default().keys_for(gscore_edit::ActionId::Copy)
    );

    // A key another action uses is reported and can replace it.
    harness.get_by_label("Change").click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::X);
    harness.run();
    harness.get_by_label_contains("is already used by");
    harness.get_by_label("Replace it").click();
    harness.run();
    assert_eq!(
        harness
            .state()
            .keymap()
            .keys_for(gscore_edit::ActionId::Copy),
        [gscore_edit::KeyCombo::parse("ctrl+x").unwrap()]
    );
    assert!(
        harness
            .state()
            .keymap()
            .keys_for(gscore_edit::ActionId::Cut)
            .is_empty()
    );
}

#[test]
fn the_history_window_lists_changes_and_undoes_them() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.get_by_label("1/8").click();
    harness.run();
    harness.get_by_label("Rest").click();
    harness.run();
    let depth = |harness: &Harness<'static, GuitarScoreApp>| {
        harness.state().document().unwrap().editor().undo_depth()
    };
    let before = depth(&harness);
    assert!(before >= 2);

    harness.get_by_label("Edit").click();
    harness.run();
    harness.get_by_label("History…").click();
    harness.run();
    // The newest change is where the song is; the one before it is a step back.
    harness.get_by_label_contains("Current point, change 1:");
    harness.get_by_label_contains("Go to change 2:").click();
    harness.run();
    assert_eq!(depth(&harness), before.saturating_sub(1));
    // The undone change is still listed first and brings the song forward again.
    harness.get_by_label_contains("Current point, change 2:");
    harness.get_by_label_contains("Go to change 1:").click();
    harness.run();
    assert_eq!(depth(&harness), before);

    // The last row goes back to the song as it was opened.
    let start = before.saturating_add(1);
    harness
        .get_by_label_contains(&format!("Go to change {start}: Start"))
        .click();
    harness.run();
    assert_eq!(depth(&harness), 0);
    assert!(harness.state().document().unwrap().editor().can_redo());
}

#[test]
fn a_section_title_is_applied_in_place_with_enter() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.get_by_label("Section").scroll_to_me();
    harness.run();
    harness.get_by_label("Section").click();
    harness.run();
    harness.get_by_label("Section title").click();
    harness.run();
    harness.get_by_label("Section title").type_text("Verse");
    harness.key_press(egui::Key::Enter);
    harness.run();
    let section = harness
        .state()
        .document()
        .unwrap()
        .song()
        .section(gscore_model::MeasureIndex::new(0))
        .expect("a section");
    assert_eq!(section.title, "Verse");
}

#[test]
fn a_new_track_asks_for_name_color_and_symbol() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    harness.get_by_label("New track").click();
    harness.run();
    let count = |harness: &Harness<'static, GuitarScoreApp>| {
        harness.state().document().unwrap().song().track_count()
    };
    assert_eq!(
        count(&harness),
        1,
        "the track is not created before the popup"
    );
    harness.get_by_label("Track name").click();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.get_by_label("Track name").type_text("Bass 2");
    harness
        .get_all_by_role_and_label(egui::Role::Button, "Bass")
        .last()
        .expect("the Bass symbol")
        .click();
    harness
        .get_all_by_role_and_label(egui::Role::Button, "Color Blue")
        .last()
        .expect("the blue color")
        .click();
    harness.get_by_label("Create track").click();
    harness.run();

    assert_eq!(count(&harness), 2);
    let song = harness.state().document().unwrap().song();
    let track = song
        .track(gscore_model::TrackIndex::new(1))
        .expect("the new track");
    assert_eq!(track.name(), "Bass 2");
    assert_eq!(track.color().rgb(), 0x1E_88E5);
    assert_eq!(
        song.track_sound(gscore_model::TrackIndex::new(1))
            .expect("a sound")
            .program,
        33
    );
    assert_eq!(harness.state().view().track, 1);
}

#[test]
fn the_grid_switches_to_a_mixer_view() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    harness.get_by_label("Mixer view").click();
    harness.run();
    assert!(harness.state().view().grid_mixer);
    harness.get_by_label("Grid view");
    harness.get_by_label("Volume Steel Guitar");
    harness.get_by_label("Master volume");
    harness.get_by_label("Pan Steel Guitar");
    harness.get_by_label("Pan Master");
    harness.get_by_label("Mute Steel Guitar");
    harness.get_by_label("Solo Steel Guitar");

    // Mute and solo are live from the mixer too.
    let muted = |harness: &Harness<'static, GuitarScoreApp>| {
        harness
            .state()
            .document()
            .unwrap()
            .song()
            .track(gscore_model::TrackIndex::new(0))
            .unwrap()
            .is_muted()
    };
    harness.get_by_label("Mute Steel Guitar").click();
    harness.run();
    assert!(muted(&harness));
    harness.get_by_label("Mute Steel Guitar").click();
    harness.run();
    assert!(!muted(&harness));

    // All-tracks mute/solo live in the master strip.
    harness.get_by_label("Mute all").click();
    harness.run();
    assert!(muted(&harness));
    harness.get_by_label("Mute all").click();
    harness.run();
    assert!(!muted(&harness));
    harness.get_by_label("Solo all");

    // Clicking the level lets one type a value in decibels.
    harness
        .get_by_label("Level Steel Guitar in decibels")
        .click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness
        .get_by_label("Level Steel Guitar in decibels")
        .type_text("-12");
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(
        harness
            .state()
            .document()
            .unwrap()
            .song()
            .track_sound(gscore_model::TrackIndex::new(0))
            .unwrap()
            .volume,
        64,
        "-12 dB is (64/127)² of the amplitude"
    );

    harness.get_by_label("Grid view").click();
    harness.run();
    assert!(!harness.state().view().grid_mixer);
    harness.get_by_label("Song grid");
    harness.get_by_label("Output activity Steel Guitar");
    harness.get_by_label("Output activity master");
}

#[test]
fn the_master_rows_mute_and_solo_line_up_with_the_tracks() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    harness.run();
    let track_mute = harness.get_by_label("M").rect();
    let track_solo = harness.get_by_label("S").rect();
    let all_mute = harness.get_by_label("Mute all").rect();
    let all_solo = harness.get_by_label("Solo all").rect();
    assert!(
        (track_mute.center().x - all_mute.center().x).abs() < 0.5,
        "{track_mute:?} {all_mute:?}"
    );
    assert!(
        (track_solo.center().x - all_solo.center().x).abs() < 0.5,
        "{track_solo:?} {all_solo:?}"
    );
    assert!(
        all_mute.top() > track_mute.bottom(),
        "the master row is below"
    );
}

#[test]
fn the_player_never_covers_the_toolbar_groups() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    harness.run();
    let panels = harness.get_by_label("Show track panel").rect();
    let start = harness.get_by_label("Go to start").rect();
    let history = harness.get_by_label("History").rect();
    let settings = harness.get_by_label("Settings").rect();
    assert!(
        start.left() >= panels.right(),
        "{start:?} overlaps {panels:?}"
    );
    assert!(history.left() > start.right());
    assert!(settings.left() > history.right());
}

#[test]
fn while_stopped_the_clock_shows_where_play_starts() {
    // Nine 4/4 bars at 120 BPM: two seconds each.
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.run();
    harness.get_by_label_contains("Time 00:00 of 00:18");
    let document = harness.state_mut().document_mut().unwrap();
    let cursor = document.editor().cursor();
    document.editor_mut().set_cursor(gscore_edit::Cursor {
        measure: gscore_model::MeasureIndex::new(2),
        beat: gscore_model::BeatIndex::new(0),
        ..cursor
    });
    harness.run();
    // The third bar starts four seconds in, and the measure readout agrees.
    harness.get_by_label_contains("Time 00:04 of 00:18");
    harness.get_by_label("Measure 3");
}

#[test]
fn the_mixer_sets_reverb_and_chorus_with_one_undo_per_drag() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    harness.get_by_label("Mixer view").click();
    harness.run();
    let sends = |harness: &Harness<'static, GuitarScoreApp>| {
        let sound = first_sound(harness);
        (sound.reverb, sound.chorus)
    };
    assert_eq!(sends(&harness), (0, 0));
    // The master has a pan but no sends.
    harness.get_by_label("Pan Master");
    assert!(harness.query_by_label("Reverb Master").is_none());

    // Dragging right turns the send up (half a step per point), as one undo step.
    let depth = harness.state().document().unwrap().editor().undo_depth();
    let start = harness.get_by_label("Reverb Steel Guitar").rect().center();
    harness.hover_at(start);
    harness.drag_at(start);
    harness.run();
    for step in 1..=4 {
        harness.hover_at(start + egui::vec2(10.0 * step as f32, 0.0));
        harness.run();
    }
    harness.drop_at(start + egui::vec2(40.0, 0.0));
    harness.run();
    let (reverb, chorus) = sends(&harness);
    assert!(reverb >= 15, "reverb {reverb}");
    assert_eq!(chorus, 0);
    let editor = harness.state().document().unwrap().editor();
    assert_eq!(editor.undo_depth(), depth + 1, "one drag, one undo step");

    harness.get_by_label("Chorus Steel Guitar");

    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run();
    assert_eq!(sends(&harness), (0, 0));
}
