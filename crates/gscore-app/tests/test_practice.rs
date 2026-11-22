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
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/basic/multi_staff.gp");
    let mut harness = Harness::builder()
        .with_size(size)
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
fn the_wide_header_holds_only_the_player_and_the_rest_is_in_the_drawer() {
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
        harness.get_by_label("Count in before playing").click();
        harness.run_steps(4);
        assert_eq!(harness.state().playback().count_in(), expected);
        harness.key_press(egui::Key::Escape);
        harness.run_steps(4);
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
fn the_player_sits_in_the_middle_of_the_header() {
    for width in [1180.0, 820.0] {
        let harness = practice(egui::vec2(width, 820.0));
        let play = harness.get_by_role_and_label(Role::Button, "Play").rect();
        assert!(
            (play.center().x - width / 2.0).abs() < 1.0,
            "play at {} of {width}",
            play.center().x
        );
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
