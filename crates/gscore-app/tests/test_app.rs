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
use gscore_app::{GuitarScoreApp, ThemeChoice};

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
