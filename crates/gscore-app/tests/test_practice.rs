//! The practice view's drawer, driven through its accessibility tree: what it holds, and
//! how its tiles that open windows behave.

#![allow(unused_crate_dependencies)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gscore_app::{GuitarScoreApp, PracticeChoice};

/// The practice view at `size` with a song open and the drawer pulled up.
fn practice(size: egui::Vec2) -> Harness<'static, GuitarScoreApp> {
    practice_stepping(size, 0.25)
}

/// [`practice`], each frame `step` seconds after the last.
fn practice_stepping(size: egui::Vec2, step: f32) -> Harness<'static, GuitarScoreApp> {
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/basic/multi_staff.gp");
    let mut harness = Harness::builder()
        .with_size(size)
        .with_step_dt(step)
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(file)));
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "opening timed out"
        );
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.state_mut().set_practice_view(PracticeChoice::On);
    harness.run_steps(4);
    harness.get_by_label("More controls").click();
    harness.run_steps(4);
    harness
}

#[test]
fn the_player_and_everything_else_are_in_the_drawer() {
    for size in [egui::vec2(1180.0, 820.0), egui::vec2(390.0, 760.0)] {
        let harness = practice(size);
        for label in ["Previous bar", "Play", "Next bar"] {
            assert!(
                harness.query_by_label(label).is_some(),
                "{label} at {size:?}"
            );
        }
        for label in [
            "Loop",
            "Speed trainer",
            "Count-in",
            "Metronome",
            "Faster",
            "Print (PDF)",
            "Full interface",
        ] {
            assert!(
                harness.query_by_label(label).is_some(),
                "{label} at {size:?}"
            );
        }
        assert!(harness.query_by_label("Fewer controls").is_some());
        assert!(
            harness.query_by_label("Menu").is_none(),
            "no menu in the corner at {size:?}"
        );
    }
}

#[test]
fn the_full_interface_tile_leaves_the_practice_view() {
    let mut harness = practice(egui::vec2(1180.0, 820.0));
    harness.get_by_label("Full interface").click();
    harness.run_steps(4);
    assert_eq!(harness.state().settings().practice_view, "off");
}

#[test]
fn the_count_in_is_switched_on_and_off_in_the_drawer() {
    let mut harness = practice(egui::vec2(1180.0, 820.0));
    let before = harness.state().playback().count_in();
    for expected in [!before, before] {
        harness.get_by_label("Count-in").click();
        harness.run_steps(4);
        // Its settings are a page of the drawer, in place of the tiles.
        assert!(
            harness.query_by_label("Loop").is_none(),
            "the tiles make way"
        );
        harness.get_by_label("Count in before playing").click();
        harness.run_steps(4);
        assert_eq!(harness.state().playback().count_in(), expected);
        harness.get_by_label("Back").click();
        harness.run_steps(4);
        assert!(
            harness.query_by_label("Loop").is_some(),
            "back to the tiles"
        );
    }
}

#[test]
fn a_window_tile_puts_the_drawer_away_and_a_second_tap_shuts_its_window() {
    let mut harness = practice(egui::vec2(1180.0, 820.0));
    harness.get_by_role_and_label(Role::Button, "Tuner").click();
    harness.run_steps(4);
    assert!(
        harness.query_by_label("More controls").is_some(),
        "the drawer is put away"
    );
    assert!(harness.query_by_label("Fewer controls").is_none());
    assert!(
        harness.query_by_label("440.00 Hz").is_some(),
        "the tuner is open"
    );
    harness.get_by_label("More controls").click();
    harness.run_steps(4);
    harness.get_by_role_and_label(Role::Button, "Tuner").click();
    harness.run_steps(4);
    assert!(
        harness.query_by_label("440.00 Hz").is_none(),
        "the tuner is shut again"
    );
    assert!(harness.query_by_label("More controls").is_some());
}

#[test]
fn the_speed_tile_changes_the_playback_speed() {
    let mut harness = practice(egui::vec2(1180.0, 820.0));
    let before = harness.state().playback().speed();
    harness.get_by_label("Faster").click();
    harness.run_steps(4);
    assert!(harness.state().playback().speed() > before);
}

#[test]
fn the_player_sits_in_the_drawer_under_its_tiles_with_the_drawers_button() {
    for size in [
        egui::vec2(1180.0, 820.0),
        egui::vec2(820.0, 1180.0),
        egui::vec2(390.0, 760.0),
        egui::vec2(360.0, 740.0),
        egui::vec2(844.0, 390.0),
    ] {
        let mut harness = practice(size);
        for open in [true, false] {
            let handle = harness
                .get_by_label(if open {
                    "Fewer controls"
                } else {
                    "More controls"
                })
                .rect();
            let play = harness.get_by_role_and_label(Role::Button, "Play").rect();
            // The drawer's button is in the player's row, at its left end.
            assert!(
                (play.center().y - handle.center().y).abs() < 1.0,
                "the drawer's button beside play at {size:?}: {play:?} {handle:?}"
            );
            let previous = harness.get_by_label("Previous bar").rect();
            assert!(
                handle.right() < previous.left(),
                "nothing overlaps at {size:?}"
            );
            if open {
                let tile = harness.get_by_label("Print (PDF)").rect();
                assert!(play.top() > tile.bottom(), "play under the tiles");
                harness.get_by_label("Fewer controls").click();
                harness.run_steps(4);
            }
        }
    }
}

#[test]
fn on_a_phone_the_sound_chain_covers_all_but_the_drawer() {
    for size in [egui::vec2(390.0, 844.0), egui::vec2(844.0, 390.0)] {
        let mut harness = practice(size);
        harness
            .query_all_by_label_contains("Sound chain of")
            .next()
            .expect("the drawer's Sound tile")
            .click();
        harness.run_steps(6);
        // The sheet starts at the top left corner, under the desktop's title bar.
        let source = harness
            .query_all_by_label_contains("Block 0:")
            .next()
            .expect("the board's source")
            .rect();
        assert!(source.left() < 100.0, "{size:?}: {source:?}");
        assert!(source.top() < 200.0, "{size:?}: {source:?}");
        // The drawer and its player are still there, and open again over it.
        assert!(harness.query_by_label("Play").is_some());
        harness.get_by_label("More controls").click();
        harness.run_steps(4);
        assert!(harness.query_by_label("Fewer controls").is_some());
        assert!(harness.query_by_label("Loop").is_some());
    }
}

#[test]
fn the_floating_drawer_is_as_wide_shut_as_open() {
    let mut harness = practice(egui::vec2(1180.0, 820.0));
    let open = harness.get_by_label("Fewer controls").rect();
    harness.get_by_label("Fewer controls").click();
    harness.run_steps(4);
    let shut = harness.get_by_label("More controls").rect();
    assert!((open.width() - shut.width()).abs() < 1.0);
    assert!((open.right() - shut.right()).abs() < 1.0);
}

#[test]
fn the_zoom_keys_size_the_practice_views_score_and_leave_the_full_interfaces() {
    let mut harness = practice(egui::vec2(1180.0, 820.0));
    // The drawer's popups and windows do not take the keys: put it away first.
    harness.get_by_label("Fewer controls").click();
    harness.run_steps(4);
    let full = harness.state().view().zoom();
    let start = harness.state().settings().practice_zoom;
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Equals);
    harness.run_steps(4);
    assert!(harness.state().settings().practice_zoom > start);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Minus);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Minus);
    harness.run_steps(4);
    assert!(harness.state().settings().practice_zoom < start);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num0);
    harness.run_steps(4);
    assert!((harness.state().settings().practice_zoom - 1.0).abs() < 1e-4);
    assert!(
        (harness.state().view().zoom() - full).abs() < 1e-4,
        "the full interface keeps its zoom"
    );
}

#[test]
fn the_staves_tile_switches_the_shown_tracks_staves_as_an_edit() {
    let mut harness = practice(egui::vec2(1180.0, 820.0));
    let staves = |harness: &Harness<'static, GuitarScoreApp>| {
        let state = harness.state();
        let document = state.document().expect("a song");
        let track = gscore_model::TrackIndex::new(document.editor().cursor().track.get());
        document
            .song()
            .track(track)
            .expect("the shown track")
            .staves()
    };
    let before = staves(&harness);
    harness.get_by_label("Staves").click();
    harness.run_steps(4);
    harness.get_by_label("Slash notation").click();
    harness.run_steps(4);
    assert_eq!(staves(&harness).slash(), !before.slash());
    // Undone like any edit.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run_steps(4);
    assert_eq!(staves(&harness), before);
}

/// A finger on the score at `from`, held `hold` seconds, then moved to `to` in steps and
/// lifted: the touch events with the pointer events a browser sends with them.
fn finger(
    harness: &mut Harness<'static, GuitarScoreApp>,
    from: egui::Pos2,
    to: egui::Pos2,
    hold: f64,
) {
    let touch = |phase, pos| egui::Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: None,
    };
    harness.event(touch(egui::TouchPhase::Start, from));
    harness.event(egui::Event::PointerMoved(from));
    harness.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();
    // Held: time passes with the finger still (the harness steps at 60 frames a second).
    for _ in 0..(hold * 60.0).ceil() as usize {
        harness.step();
    }
    for step in 1..=10 {
        let pos = from + (to - from) * (step as f32 / 10.0);
        harness.event(touch(egui::TouchPhase::Move, pos));
        harness.event(egui::Event::PointerMoved(pos));
        harness.step();
    }
    harness.event(touch(egui::TouchPhase::End, to));
    harness.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.event(egui::Event::PointerGone);
    harness.run_steps(4);
}

#[test]
fn a_swipe_scrolls_the_score_and_a_long_press_then_a_drag_loops_bars() {
    let mut harness = practice_stepping(egui::vec2(390.0, 844.0), 1.0 / 60.0);
    harness.get_by_label("Fewer controls").click();
    harness.run_steps(4);
    let page = harness.get_by_label("Score page").rect();
    // A quick swipe up moves the score up and selects nothing.
    let from = egui::pos2(200.0, 600.0);
    finger(&mut harness, from, from - egui::vec2(0.0, 300.0), 0.0);
    let moved = harness.get_by_label("Score page").rect();
    assert!(moved.top() < page.top() - 100.0, "{page:?} → {moved:?}");
    assert_eq!(harness.state().selection(), None);
    assert!(!harness.state().playback().loop_selection());
    // Back to the top, then a finger held on the first bar and dragged on selects bars.
    let from = egui::pos2(200.0, 200.0);
    finger(&mut harness, from, from + egui::vec2(0.0, 400.0), 0.0);
    let top = harness.get_by_label("Score page").rect();
    let bar = top.left_top() + egui::vec2(150.0, 250.0);
    finger(&mut harness, bar, bar + egui::vec2(0.0, 200.0), 1.0);
    let selection = harness
        .state()
        .selection()
        .expect("the bars dragged across");
    assert!(selection.end() > selection.start(), "{selection:?}");
    assert!(harness.state().playback().loop_selection());
}

#[test]
fn the_desktops_title_bar_leads_back_to_the_full_interface() {
    let mut harness = practice(egui::vec2(1180.0, 820.0));
    let back = harness.get_by_label("Back to the full interface").rect();
    let close = harness.get_by_label("Close window").rect();
    assert!(back.left() < 100.0, "on the bar's left: {back:?}");
    assert!(
        (back.center().y - close.center().y).abs() < 1.0,
        "in the title bar"
    );
    harness.get_by_label("Back to the full interface").click();
    harness.run_steps(4);
    assert_eq!(harness.state().settings().practice_view, "off");
}
