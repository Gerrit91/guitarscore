//! Several tracks in one score (M21): the View menu's choice of tracks, and the keyboard
//! moving the cursor from one track's staves to the next.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::Key;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gscore_app::GuitarScoreApp;

fn app_with_multi_staff() -> Harness<'static, GuitarScoreApp> {
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/basic/multi_staff.gp");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .with_max_steps(64)
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(file)));
    let started = Instant::now();
    harness.run();
    while harness.state().is_busy() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run();
    harness
}

fn view_menu(harness: &mut Harness<'static, GuitarScoreApp>, item: &str) {
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label(item).click();
    harness.run();
}

fn cursor(harness: &Harness<'static, GuitarScoreApp>) -> (usize, u8) {
    let cursor = harness.state().document().unwrap().editor().cursor();
    (cursor.track.get(), cursor.string)
}

fn press(harness: &mut Harness<'static, GuitarScoreApp>, key: Key, times: usize) {
    for _ in 0..times {
        harness.key_press(key);
        harness.run();
    }
}

#[test]
fn the_view_menu_shows_all_tracks_and_one_again() {
    let mut harness = app_with_multi_staff();
    assert_eq!(harness.state().score_tracks(), [0]);
    view_menu(&mut harness, "All tracks");
    assert_eq!(harness.state().score_tracks(), [0, 1, 2, 3, 4, 5]);
    assert_eq!(harness.state().notice(), None, "the score lays out");
    view_menu(&mut harness, "One track");
    assert_eq!(harness.state().score_tracks(), [0]);
}

#[test]
fn up_and_down_past_the_outer_strings_move_between_tracks() {
    let mut harness = app_with_multi_staff();
    // With one track shown, the cursor stays on the bottom string.
    press(&mut harness, Key::ArrowDown, 8);
    assert_eq!(cursor(&harness), (0, 6));
    view_menu(&mut harness, "All tracks");
    // Down past the clean guitar's bottom string: the rhythm guitar's top string.
    press(&mut harness, Key::ArrowDown, 1);
    assert_eq!(cursor(&harness), (1, 1));
    assert_eq!(harness.state().shown_track(), 1, "the track picker follows");
    // Up again: back onto the clean guitar's bottom string.
    press(&mut harness, Key::ArrowUp, 1);
    assert_eq!(cursor(&harness), (0, 6));
    // Above the top track there is nowhere to go.
    press(&mut harness, Key::ArrowUp, 7);
    assert_eq!(cursor(&harness), (0, 1));
    // Down through every track to the drums, the last one.
    press(&mut harness, Key::ArrowDown, 40);
    assert_eq!(cursor(&harness).0, 5);
}

#[test]
fn all_tracks_match_their_snapshot() {
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/basic/multi_staff.gp");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(file)));
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run();
    view_menu(&mut harness, "All tracks");
    // The cursor on the bass, two tracks below the top: drawn on the bass's staff.
    press(&mut harness, Key::ArrowDown, 1);
    harness.state_mut().set_shown_track(4);
    if let Some(document) = harness.state_mut().document_mut() {
        let mut cursor = document.editor().cursor();
        cursor.track = gscore_model::TrackIndex::new(4);
        cursor.string = 2;
        document.editor_mut().set_cursor(cursor);
    }
    harness.run();
    harness.snapshot("multi_staff_all_tracks");
}

#[test]
fn the_bar_menu_sets_the_multitrack_line_breaks_when_all_tracks_are_shown() {
    let mut harness = app_with_multi_staff();
    view_menu(&mut harness, "All tracks");
    // Bar 3 (index 2) has no multitrack break yet; the file's breaks are at bars 5 and 9.
    if let Some(document) = harness.state_mut().document_mut() {
        let mut cursor = document.editor().cursor();
        cursor.measure = gscore_model::MeasureIndex::new(2);
        cursor.beat = gscore_model::BeatIndex::new(0);
        document.editor_mut().set_cursor(cursor);
    }
    harness.run();
    let breaks = |harness: &Harness<'static, GuitarScoreApp>| {
        harness
            .state()
            .document()
            .unwrap()
            .song()
            .score_line_breaks()
    };
    let own = |harness: &Harness<'static, GuitarScoreApp>| {
        harness
            .state()
            .document()
            .unwrap()
            .song()
            .track(gscore_model::TrackIndex::new(0))
            .unwrap()
            .measure(gscore_model::MeasureIndex::new(2))
            .unwrap()
            .system_break()
    };
    assert!(!breaks(&harness)[2]);
    harness.get_by_label("Bar").click();
    harness.run();
    harness.get_by_label("Line break before this bar").click();
    harness.run();
    assert!(breaks(&harness)[2], "the multitrack layout breaks there");
    assert_eq!(
        own(&harness),
        gscore_model::SystemBreak::None,
        "the track's own is kept"
    );
    harness.key_press_modifiers(egui::Modifiers::COMMAND, Key::Z);
    harness.run();
    assert!(!breaks(&harness)[2], "undone");
}

#[test]
fn empty_staves_can_be_hidden_from_the_view_menu() {
    let mut harness = app_with_multi_staff();
    view_menu(&mut harness, "All tracks");
    assert!(!harness.state().view().hide_empty_staves);
    view_menu(&mut harness, "Hide empty staves");
    assert!(harness.state().view().hide_empty_staves);
    assert!(
        harness.state().settings().hide_empty_staves,
        "remembered in the settings"
    );
    assert_eq!(harness.state().notice(), None, "the score lays out");
    // The cursor's track stays reachable: moving it down to a bar where the clean guitar
    // has nothing still works.
    press(&mut harness, Key::ArrowRight, 40);
    assert_eq!(cursor(&harness).0, 0);
}

#[test]
fn the_mixer_matches_its_snapshot() {
    let mut harness = app_with_mixer_view();
    // The tracks, the master (a waveform for all tracks) and the metronome's fader.
    harness.snapshot("mixer_view");
}

#[test]
fn the_mixer_shows_what_plays_and_what_does_not() {
    let mut harness = app_with_mixer_view();
    // A muted track and, while another one is soloed, a track that a solo silences: both
    // recede, their mute buttons red and the solo button amber.
    harness.get_by_label("Mute Clean Guitar").click();
    harness.run();
    harness.get_by_label("Solo Rhythm Guitar").click();
    harness.run();
    harness.snapshot("mixer_mute_solo");
    // The same in the song grid: the muted and the unsoloed rows' squares and names dim.
    harness.get_by_label("Grid view").click();
    harness.run();
    harness.snapshot("grid_mute_solo");
}

/// The app showing [multi_staff.gp](../../../testdata/basic/multi_staff.gp) in the mixer
/// view, at the snapshot size.
fn app_with_mixer_view() -> Harness<'static, GuitarScoreApp> {
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/basic/multi_staff.gp");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(file)));
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run();
    harness.get_by_label("Mixer view").click();
    harness.run();
    harness
}

#[test]
fn the_screen_view_matches_its_snapshot() {
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/basic/multi_staff.gp");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(file)));
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run();
    view_menu(&mut harness, "Screen view (vertical)");
    // Paper from edge to edge, as wide as the room between the panels and no wider.
    harness.snapshot("screen_view");
}
