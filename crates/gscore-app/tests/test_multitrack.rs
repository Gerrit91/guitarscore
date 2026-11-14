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

/// Parks the pointer outside the window, so a snapshot does not show the fake cursor where
/// the last click left it (a menu entry moving would otherwise change the picture).
fn park_pointer(harness: &mut Harness<'static, GuitarScoreApp>) {
    harness.hover_at(egui::pos2(700.0, 884.0));
    harness.run();
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
    park_pointer(&mut harness);
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

#[test]
fn every_mixer_strip_opens_its_sound_chain_from_the_stomp_box() {
    // A track's stomp box opens the Sound chain window.
    let mut harness = app_with_mixer_view();
    harness.get_by_label("Sound chain for Clean Guitar").click();
    harness.run();
    harness.snapshot_options("mixer_stomp_track", &meter_tolerance());
}

#[test]
fn a_bus_strip_opens_its_window_from_the_stomp_box() {
    // The reverb bus's strip opens the bus window (what it runs and its knobs).
    let mut harness = app_with_mixer_view();
    harness
        .get_by_label("Sound chain for the reverb bus")
        .click();
    harness.run();
    harness.snapshot("mixer_stomp_reverb_bus");
}

#[test]
fn the_master_strip_opens_its_sound_chain() {
    let mut harness = app_with_mixer_view_at(1900.0);
    harness.get_by_label("Sound chain for the master").click();
    harness.run();
    harness.snapshot_options("mixer_stomp_master", &meter_tolerance());
}

#[test]
fn a_drum_tracks_parts_open_and_close_beside_its_strip() {
    let mut harness = app_with_mixer_view_at(1900.0);
    // The arrow opens the kit's parts as narrower strips right beside the drum track.
    harness.get_by_label("Show the drum parts").click();
    harness.run();
    harness.snapshot_options("mixer_drum_parts", &meter_tolerance());
    assert!(harness.query_by_label("Kick level").is_some());
    assert!(harness.query_by_label("Mute Kick").is_some());
    // The same arrow closes them again.
    harness.get_by_label("Hide the drum parts").click();
    harness.run();
    assert!(harness.query_by_label("Kick level").is_none());
}

/// The app showing [multi_staff.gp](../../../testdata/basic/multi_staff.gp) in the mixer
/// view, at the snapshot size.
fn app_with_mixer_view() -> Harness<'static, GuitarScoreApp> {
    app_with_mixer_view_at(1400.0)
}

/// The same, wide enough to reach the drum track and the master strip.
fn app_with_mixer_view_at(width: f32) -> Harness<'static, GuitarScoreApp> {
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/basic/multi_staff.gp");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(width, 900.0))
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
    park_pointer(&mut harness);
    harness.snapshot("screen_view");
}

#[test]
fn a_click_on_a_strip_selects_its_track() {
    // The mixer strips behave like the track rows of the song grid: clicking the
    // background selects the track (the cursor goes there and the panel shows it).
    let mut harness = app_with_multi_staff();
    harness.get_by_label("Mixer view").click();
    harness.run();
    assert_eq!(harness.state().shown_track(), 0);
    // The background beside the fader (the centre of a strip is its fader).
    let strip = harness.get_by_label("Track 3: Crunch Guitar").rect();
    let at = egui::pos2(strip.left() + 6.0, strip.center().y);
    harness.hover_at(at);
    harness.run();
    harness.drag_at(at);
    harness.run();
    harness.drop_at(at);
    harness.run();
    assert_eq!(harness.state().shown_track(), 2);
    let cursor = harness
        .state()
        .document()
        .expect("the song")
        .editor()
        .cursor();
    assert_eq!(cursor.track.get(), 2);
    // The strip now carries the selection's background (the snapshot shows it).
    harness.snapshot("mixer_selected_track");
    // A click on the first strip's background goes back.
    let strip = harness.get_by_label("Track 1: Clean Guitar").rect();
    let at = egui::pos2(strip.left() + 6.0, strip.center().y);
    harness.hover_at(at);
    harness.run();
    harness.drag_at(at);
    harness.run();
    harness.drop_at(at);
    harness.run();
    assert_eq!(harness.state().shown_track(), 0);
}

#[test]
fn a_selection_looks_the_same_in_the_score_and_on_top_of_the_grid() {
    let mut harness = app_with_mixer_view();
    harness.get_by_label("Grid view").click();
    harness.run();
    // Beats selected in the score, over a bar line: the grid marks the bars they span in
    // that track's row, over the colored squares, in the score's selection color.
    for _ in 0..6 {
        harness.key_press_modifiers(egui::Modifiers::SHIFT, Key::ArrowRight);
        harness.run();
    }
    assert!(
        harness
            .state()
            .document()
            .unwrap()
            .editor()
            .selection()
            .is_some()
    );
    harness.snapshot("grid_score_selection");

    // Bars of the shown track selected in the grid: the same look in both places.
    press(&mut harness, Key::ArrowLeft, 1);
    harness
        .state_mut()
        .set_selection(Some(gscore_app::MeasureSelection::on(0, 1).to(2)));
    harness
        .state_mut()
        .document_mut()
        .unwrap()
        .editor_mut()
        .set_selection(Some(gscore_edit::Selection::Measures {
            anchor: gscore_model::MeasureIndex::new(1),
            focus: gscore_model::MeasureIndex::new(2),
            track: Some(gscore_model::TrackIndex::new(0)),
        }));
    harness.run();
    harness.snapshot("grid_track_selection");
}

#[test]
fn a_drum_parts_pan_knob_moves_it_in_the_kit_as_one_undo_step() {
    use gscore_app::chain_settings::DrumPart;
    let mut harness = app_with_mixer_view_at(1900.0);
    harness.get_by_label("Show the drum parts").click();
    harness.run();
    // The drum kit is the sixth track of the file.
    let pan = |harness: &Harness<'static, GuitarScoreApp>| {
        harness
            .state()
            .playback()
            .track_drum_part_mix(5, DrumPart::HiHat)
            .1
    };
    assert_eq!(pan(&harness), 64, "in the middle");
    let depth = harness.state().document().unwrap().editor().undo_depth();
    let start = harness.get_by_label("Pan Hi-hat").rect().center();
    harness.hover_at(start);
    harness.drag_at(start);
    harness.run();
    for step in 1..=4 {
        harness.hover_at(start - egui::vec2(10.0 * step as f32, 0.0));
        harness.run();
    }
    harness.drop_at(start - egui::vec2(40.0, 0.0));
    harness.run();
    assert!(pan(&harness) < 50, "turned to the left: {}", pan(&harness));
    let editor = harness.state().document().unwrap().editor();
    assert_eq!(editor.undo_depth(), depth + 1, "one drag, one undo step");
    // The other parts stay in the middle.
    assert_eq!(
        harness
            .state()
            .playback()
            .track_drum_part_mix(5, DrumPart::Snare)
            .1,
        64
    );
}

#[test]
fn a_drum_parts_room_knob_sends_it_into_the_kits_room_as_one_undo_step() {
    use gscore_app::chain_settings::DrumPart;
    let mut harness = app_with_mixer_view_at(1900.0);
    harness.get_by_label("Show the drum parts").click();
    harness.run();
    // The drum kit is the sixth track of the file.
    let room = |harness: &Harness<'static, GuitarScoreApp>| {
        harness
            .state()
            .playback()
            .track_drum_part_mix(5, DrumPart::HiHat)
            .4
    };
    assert_eq!(room(&harness), 0, "dry to begin with");
    let depth = harness.state().document().unwrap().editor().undo_depth();
    let start = harness.get_by_label("Room Hi-hat").rect().center();
    harness.hover_at(start);
    harness.drag_at(start);
    harness.run();
    for step in 1..=4 {
        harness.hover_at(start + egui::vec2(8.0 * step as f32, 0.0));
        harness.run();
    }
    harness.drop_at(start + egui::vec2(32.0, 0.0));
    harness.run();
    assert!(room(&harness) > 0, "sent into the room");
    let editor = harness.state().document().unwrap().editor();
    assert_eq!(editor.undo_depth(), depth + 1, "one drag, one undo step");
    // The kit's other parts stay dry.
    assert_eq!(
        harness
            .state()
            .playback()
            .track_drum_part_mix(5, DrumPart::Snare)
            .4,
        0
    );
}

/// The drum kit's own Room send in the sixth track's sound payload (`0` when it has none).
fn song_room_send(harness: &Harness<'static, GuitarScoreApp>) -> u8 {
    let document = harness.state().document().unwrap();
    let Some(json) = document
        .song()
        .track(gscore_model::TrackIndex::new(5))
        .and_then(|track| track.sound_state())
    else {
        return 0;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return 0;
    };
    value
        .get("drum_room_send")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .unwrap_or(0)
}

/// Whether the drum kit's track (the sixth) is soloed.
fn drum_track_soloed(harness: &Harness<'static, GuitarScoreApp>) -> bool {
    harness
        .state()
        .document()
        .unwrap()
        .song()
        .track(gscore_model::TrackIndex::new(5))
        .unwrap()
        .is_soloed()
}

#[test]
fn a_drum_parts_solo_also_solos_its_track() {
    let mut harness = app_with_mixer_view_at(1900.0);
    harness.get_by_label("Show the drum parts").click();
    harness.run();
    assert!(!drum_track_soloed(&harness));
    // Soloing a part is hearing that kit: the rest of the song falls silent with it.
    harness.get_by_label("Solo Kick").click();
    harness.run();
    assert!(drum_track_soloed(&harness), "the kit's track is soloed");
    // Another part's solo keeps it; the last one un-soloed brings the song back.
    harness.get_by_label("Solo Snare").click();
    harness.run();
    assert!(drum_track_soloed(&harness));
    harness.get_by_label("Solo Kick").click();
    harness.run();
    assert!(
        drum_track_soloed(&harness),
        "the snare is still soloed, so the kit is"
    );
    harness.get_by_label("Solo Snare").click();
    harness.run();
    assert!(!drum_track_soloed(&harness), "the song plays again");
}

#[test]
fn a_drum_kits_room_knob_sends_the_whole_kit_as_one_undo_step() {
    let mut harness = app_with_mixer_view_at(1900.0);
    assert_eq!(song_room_send(&harness), 0, "dry to begin with");
    let depth = harness.state().document().unwrap().editor().undo_depth();
    let start = harness.get_by_label("Room Drumkit").rect().center();
    harness.hover_at(start);
    harness.drag_at(start);
    harness.run();
    for step in 1..=4 {
        harness.hover_at(start + egui::vec2(8.0 * step as f32, 0.0));
        harness.run();
    }
    harness.drop_at(start + egui::vec2(32.0, 0.0));
    harness.run();
    assert!(song_room_send(&harness) > 0, "the whole kit is sent");
    let editor = harness.state().document().unwrap().editor();
    assert_eq!(editor.undo_depth(), depth + 1, "one drag, one undo step");
}

#[test]
fn the_grid_header_deletes_the_shown_track_and_zooms_the_song() {
    let mut harness = app_with_multi_staff();
    // The grid starts at 100 %; a step smaller is a step the whole song can use.
    assert!((harness.state().view().grid_zoom - 1.0).abs() < 1e-6);
    harness.get_by_label("Zoom the grid out").click();
    harness.run();
    let zoom = harness.state().view().grid_zoom;
    assert!(zoom < 1.0, "{zoom}");
    harness.snapshot("grid_zoomed_out");
    // The value button is the way back to 100 %.
    let label = format!("Grid zoom: {:.0} %", zoom * 100.0);
    harness.get_by_label(&label).click();
    harness.run();
    assert!((harness.state().view().grid_zoom - 1.0).abs() < 1e-6);
    // The trash can in the header deletes the shown track.
    let before = harness.state().document().unwrap().song().track_count();
    harness.get_by_label("Delete the shown track").click();
    harness.run();
    assert_eq!(
        harness.state().document().unwrap().song().track_count(),
        before.saturating_sub(1)
    );
}

/// Snapshot options for a wide mixer with many thin meters and needles, whose edge pixels
/// the GitHub runner's software GPU anti-aliases a little differently (11 and 13 pixels
/// past 0.6, none past 1.5): a colour tolerance of 2 (YIQ distance) per pixel, as for the
/// EQ curve in `test_app.rs`. A moved strip or border differs by far more.
fn meter_tolerance() -> egui_kittest::SnapshotOptions {
    egui_kittest::SnapshotOptions::new().threshold(2.0)
}
