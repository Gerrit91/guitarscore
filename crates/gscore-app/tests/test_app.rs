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

/// A window wide enough for the whole toolbar readout (a narrow one drops the position,
/// measure and tempo before it drops its own buttons).
fn wide_app_with(file: Option<PathBuf>) -> Harness<'static, GuitarScoreApp> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 800.0))
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
fn a_new_empty_workspace_comes_from_the_tab_strip() {
    // Short frames: a double click must land within egui's 0.3 s window, and the default
    // frame is a quarter of a second long.
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .with_step_dt(1.0 / 120.0)
        .build_eframe(|cc| {
            GuitarScoreApp::new(&cc.egui_ctx, Some(testdata("basic/steel_guitar_dadgad.gp")))
        });
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run();
    assert!(harness.state().document().is_some());
    // The "+" after the tabs opens the empty workspace, without closing the tab.
    harness.get_by_label("New workspace").click();
    harness.run();
    assert!(harness.state().document().is_none(), "nothing active");
    harness.get_by_label_contains("Open a song");
    // The tab is still there, and clicking it brings the song back.
    let tab = harness
        .query_all_by_label_contains("Tab 1:")
        .next()
        .expect("the tab");
    tab.click();
    harness.run();
    assert!(harness.state().document().is_some(), "the tab came back");
    // The rest of the strip is there for the double click (egui's own detection, as on the
    // mixer's knobs); a physical double click cannot be replayed reliably here.
    harness.get_by_label_contains("Empty tab strip");
}

#[test]
fn the_grid_header_stays_while_the_tracks_scroll() {
    // A short panel, so the six tracks of the song do not fit.
    let settings = Settings {
        grid_height_chosen: Some(150.0),
        ..Settings::default()
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_eframe(|cc| {
            GuitarScoreApp::with_settings(
                &cc.egui_ctx,
                Some(testdata("basic/multi_staff.gp")),
                Keymap::default(),
                None,
                settings.clone(),
                None,
            )
        });
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run();
    let header = harness.get_by_label("Mixer view").rect();
    let track = harness.get_by_label("Track 1: Clean Guitar").rect();
    // Wheel down over the matrix: the rows move, the header stays.
    let grid = harness.get_by_label("Song grid").rect();
    harness.hover_at(grid.center());
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -220.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();
    let scrolled = harness.get_by_label("Track 1: Clean Guitar").rect();
    assert!(scrolled.top() < track.top(), "{scrolled:?} {track:?}");
    let still = harness.get_by_label("Mixer view").rect();
    assert!(
        (still.top() - header.top()).abs() < 0.5,
        "the header stayed: {still:?} {header:?}"
    );
}

#[test]
fn a_stored_panel_size_cannot_cover_the_window() {
    // A width saved while a bug grew the panel (or a hand-edited one) must not hide the
    // window: the panels keep room for the score.
    for width in [5000.0_f32, f32::INFINITY, f32::NAN] {
        let settings = Settings {
            track_panel_width: Some(width),
            edit_panel_width: Some(width),
            ..Settings::default()
        };
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1000.0, 700.0))
            .build_eframe(|cc| {
                GuitarScoreApp::with_settings(
                    &cc.egui_ctx,
                    Some(testdata("basic/note_lengths.gp")),
                    Keymap::default(),
                    None,
                    settings.clone(),
                    None,
                )
            });
        let started = Instant::now();
        while harness.state().is_busy() {
            assert!(started.elapsed() < Duration::from_secs(30), "timed out");
            harness.step();
            std::thread::sleep(Duration::from_millis(5));
        }
        harness.run();
        // The track panel's first control is in the right half, and the palette's one in
        // the left half: the score has the middle.
        let track_panel = harness.get_by_label("Instrument").rect().left();
        assert!(
            track_panel > 500.0,
            "a stored width of {width} pushed the track panel to {track_panel}"
        );
        let palette = harness.get_by_label("Tie beat").rect().right();
        assert!(
            palette < 500.0,
            "a stored width of {width} pushed the edit palette to {palette}"
        );
    }
}

#[test]
fn the_track_panel_does_not_grow_by_itself() {
    let mut harness = app_with(Some(testdata("basic/multi_staff.gp")));
    let panel = |harness: &Harness<'static, GuitarScoreApp>| {
        let rect = harness.get_by_label("Instrument").rect();
        (rect.left(), rect.top())
    };
    let before = panel(&harness);
    // A mouse move repaints the window; the panel must keep its size (a width that followed
    // the panel made it grow a little every frame).
    for step in 0..20 {
        harness.hover_at(egui::pos2(300.0 + 10.0 * step as f32, 300.0));
        harness.run();
    }
    let after = panel(&harness);
    assert!(
        (after.0 - before.0).abs() < 0.5 && (after.1 - before.1).abs() < 0.5,
        "the panel grew: {before:?} -> {after:?}"
    );
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
fn a_notice_also_shows_in_the_status_bar_beside_the_spinner() {
    let mut harness = app_with(None);
    harness.state_mut().set_notice("Something went wrong");
    harness.run();
    // The notice bar at the top and the status bar at the bottom both carry it.
    assert!(
        harness
            .query_all_by_label_contains("Something went wrong")
            .count()
            >= 2
    );
    harness.get_by_label("Dismiss");
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
    // Back to 100 %, so the settings the other tests share are left as they were.
    harness.get_by_label("125 %").click();
    harness.run();
    assert!((harness.state().view().zoom() - 1.0).abs() < 1e-4);
    harness.state_mut().view_mut().theme = ThemeChoice::Light;
    harness.run();
}

#[test]
fn escape_closes_the_shared_windows() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    // About
    harness.get_by_label("Help").click();
    harness.run();
    harness.get_by_label("About GuitarScore").click();
    harness.run();
    harness.get_by_label("GuitarScore icon");
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(
        harness.query_by_label("GuitarScore icon").is_none(),
        "Escape closes About"
    );

    // Settings
    harness.get_by_label("Settings").click();
    harness.run();
    harness.get_by_label("Page layout");
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(
        harness.query_by_label("Page layout").is_none(),
        "Escape closes Settings"
    );

    // History
    harness.get_by_label("History").click();
    harness.run();
    harness.get_by_label_contains("Newest first");
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(
        harness.query_by_label_contains("Newest first").is_none(),
        "Escape closes History"
    );

    // New song (the song behind it stays)
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("New ctrl+n").click();
    harness.run();
    harness.get_by_label("New song title");
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(
        harness.query_by_label("New song title").is_none(),
        "Escape closes New song"
    );
    assert!(harness.state().document().is_some());
}

#[test]
fn help_about_shows_the_version() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.get_by_label("Help").click();
    harness.run();
    harness.get_by_label("About GuitarScore").click();
    harness.run();
    harness.get_by_label(&format!("Version {}", gscore_version::VERSION));
    harness.get_by_label("GuitarScore icon");
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
fn the_design_mode_overlay_matches_its_snapshot() {
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
    // The design mode's guides, handles and buttons (a view mode, not part of the song).
    harness.state_mut().view_mut().design_mode = true;
    harness.run();
    harness.snapshot("design_mode");
    harness.state_mut().view_mut().theme = ThemeChoice::Dark;
    harness.run();
    harness.snapshot("design_mode_dark");
}

#[test]
fn the_design_mode_buttons_take_and_give_back_a_bar() {
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
    harness.state_mut().view_mut().design_mode = true;
    harness.run();
    // The measures that start a line of their own.
    let breaks = |harness: &Harness<'_, GuitarScoreApp>| -> Vec<usize> {
        harness
            .state()
            .document()
            .expect("a document")
            .song()
            .track(gscore_model::TrackIndex::new(0))
            .expect("a track")
            .measures()
            .enumerate()
            .filter(|(_, measure)| measure.system_break() != gscore_model::SystemBreak::None)
            .map(|(index, _)| index)
            .collect()
    };
    let before = breaks(&harness);
    // One bar fewer on the first line: it ends one bar earlier.
    harness
        .get_all_by_label("Widen the line's bars")
        .next()
        .expect("a widen button")
        .click();
    harness.run();
    let wider = breaks(&harness);
    assert_ne!(wider, before, "the line ends earlier: {before:?} {wider:?}");
    // And back: the next line gives its first bar again.
    harness
        .get_all_by_label("Narrow the line's bars")
        .next()
        .expect("a narrow button")
        .click();
    harness.run();
    assert_eq!(breaks(&harness), before);
}

#[test]
fn the_design_mode_reset_puts_a_lines_layout_back() {
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
    harness.state_mut().view_mut().design_mode = true;
    harness.run();
    let scale = |harness: &Harness<'_, GuitarScoreApp>| -> f32 {
        harness
            .state()
            .document()
            .expect("a document")
            .song()
            .measure_scale(
                gscore_model::TrackIndex::new(0),
                gscore_model::StaffIndex::new(0),
                gscore_model::MeasureIndex::new(0),
            )
    };
    assert!((scale(&harness) - 1.0).abs() < 1e-6);
    // A barline drag sets a width; one undo puts it back (the Edit menu, as a user would).
    let from = harness.get_by_label("Barline of measure 1").rect().center();
    harness.drag_at(from);
    harness.step();
    let to = from + egui::vec2(50.0, 0.0);
    harness.hover_at(to);
    harness.step();
    harness.hover_at(to);
    harness.step();
    harness.drop_at(to);
    harness.run();
    assert!(scale(&harness) > 1.0, "{}", scale(&harness));
    harness.get_by_label("Edit").click();
    harness.run();
    harness.get_by_label_contains("Undo ctrl+z").click();
    harness.run();
    assert!((scale(&harness) - 1.0).abs() < 1e-6);
    // The reset button clears a line's widths and offsets in one step.
    let from = harness.get_by_label("Barline of measure 1").rect().center();
    harness.drag_at(from);
    harness.step();
    harness.hover_at(from + egui::vec2(50.0, 0.0));
    harness.step();
    harness.hover_at(from + egui::vec2(50.0, 0.0));
    harness.step();
    harness.drop_at(from + egui::vec2(50.0, 0.0));
    harness.run();
    assert!(scale(&harness) > 1.0, "{}", scale(&harness));
    harness
        .get_all_by_label("Reset the line's layout")
        .next()
        .expect("a reset button")
        .click();
    harness.run();
    assert!((scale(&harness) - 1.0).abs() < 1e-6, "{}", scale(&harness));
}

#[test]
fn a_barline_dragged_in_the_design_mode_widens_the_bar() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    harness.state_mut().view_mut().design_mode = true;
    harness.run();
    let from = harness.get_by_label("Barline of measure 1").rect().center();
    harness.drag_at(from);
    harness.step();
    let to = from + egui::vec2(60.0, 0.0);
    harness.hover_at(to);
    harness.step();
    harness.hover_at(to);
    harness.step();
    harness.drop_at(to);
    harness.run();
    let scale = harness
        .state()
        .document()
        .expect("a document")
        .song()
        .measure_scale(
            gscore_model::TrackIndex::new(0),
            gscore_model::StaffIndex::new(0),
            gscore_model::MeasureIndex::new(0),
        );
    assert!(scale > 1.05, "the drag widened the bar: {scale}");
}

#[test]
fn the_first_start_asks_about_updates_and_remembers_a_no() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .wgpu()
        .build_eframe(|cc| {
            let mut app = GuitarScoreApp::new(&cc.egui_ctx, None);
            app.enable_automatic_update_check();
            app
        });
    harness.run();
    harness.snapshot("update_question");
    // "Don't check": nothing is sent, and the answer is kept.
    harness.get_by_label("Don't check").click();
    harness.run();
    assert_eq!(harness.state().settings().check_for_updates, Some(false));
    assert!(harness.query_by_label("Check daily").is_none());
}

#[test]
fn the_track_dialogs_match_their_snapshots() {
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
    harness.get_by_label("Tuning: DADGAD…").click();
    harness.run();
    harness.snapshot("tuning_dialog");
    harness.get_by_label("Cancel").click();
    harness.run();
    harness.get_by_label("Rename track").click();
    harness.run();
    harness.snapshot("rename_track_dialog");
    harness.get_by_label("Cancel").click();
    harness.run();
    harness.get_by_label("Tuner").click();
    harness.run();
    harness.snapshot("tuner_dialog");
    harness.get_by_label("Close").click();
    harness.run();
    harness.get_by_label("Instrument symbol").click();
    harness.run();
    harness.snapshot("symbol_picker");
    harness.key_press(egui::Key::Escape);
    harness.run();
    harness.get_by_label("Track color").click();
    harness.run();
    harness.snapshot("color_picker");
    harness.key_press(egui::Key::Escape);
    harness.run();
    // The Sound chain with an empty chain (the MIDI synth plays it), then the strings'
    // own dialog.
    harness.get_by_label("MIDI synth").click();
    harness.run();
    harness.get_by_label("Sound chain").click();
    harness.run();
    harness.run();
    // Forget a tone the song carries, so the chain starts from the default for the editors.
    harness.get_by_label("Built-in").click();
    harness.run();
    harness.get_by_label("Block 2: +").click();
    harness.run();
    harness.get_by_label("Reverb").click();
    harness.run();
    harness.get_by_label("Block 2: Reverb");
    harness.snapshot("sound_chain_window");
    // An amp in the third place, switched to its lead voice.
    harness.get_by_label("Block 3: +").click();
    harness.run();
    harness.get_by_label("Amp").click();
    harness.run();
    harness.get_by_label("Lead").click();
    harness.run();
    harness.get_by_label("Block 3: Amp");
    harness.snapshot("sound_chain_amp");
    // A drive in front of it, and the reverb's knobs.
    harness.get_by_label("Block 1: +").click();
    harness.run();
    harness.get_by_label("Drive").click();
    harness.run();
    harness.get_by_label("Block 1: Drive");
    harness.snapshot("sound_chain_drive");
    // A compressor after the reverb, at its "All" ratio.
    harness.get_by_label("Block 4: +").click();
    harness.run();
    harness.get_by_label("Compressor").click();
    harness.run();
    harness.get_by_label("Block 4: Comp");
    harness.snapshot("sound_chain_compressor");
    // The 4-band EQ in the last place: its curve and columns have their own snapshot (the
    // window is taller than this small one).
    harness.get_by_label("Block 5: +").click();
    harness.run();
    harness.get_by_label("EQ").click();
    harness.run();
    harness.get_by_label("Block 5: EQ");
    harness.get_by_label("Block 2: Reverb").click();
    harness.run();
    // Park the mouse: the snapshot should not depend on whether the last click still
    // hovers the box.
    harness.hover_at(egui::pos2(600.0, 690.0));
    harness.run();
    harness.snapshot("sound_chain_reverb");
    // The strings' own window (their box opens it once PM synthesis plays the track).
    harness.get_by_label("Block 0: MIDI").click();
    harness.run();
    harness
        .query_all_by_label("PM Synthesis")
        .find(|node| node.rect().center().x < 700.0)
        .expect("the Sound chain window's source switch")
        .click();
    harness.run();
    harness.get_by_label("Block 0: Strings").click();
    harness.run();
    // The steel guitar played by modelled strings, to show its knobs beside the chain.
    harness
        .query_all_by_label_contains("Steel guitar")
        .last()
        .unwrap()
        .click();
    harness.run();
    // A track played by a hardware synthesizer: its port and channel, next to the two
    // internal sources.
    harness
        .query_all_by_label("MIDI out")
        .find(|node| node.rect().center().x < 700.0)
        .expect("the Sound chain window's source switch")
        .click();
    harness.run();
    harness.get_by_label("Block 0: MIDI out");
    harness.snapshot("sound_chain_midi_out");
}

#[test]
fn the_eq_curve_and_its_knob_columns_match_their_snapshot() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 1000.0))
        .build_eframe(|cc| {
            GuitarScoreApp::new(&cc.egui_ctx, Some(testdata("basic/steel_guitar_dadgad.gp")))
        });
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run();
    harness.get_by_label("MIDI synth").click();
    harness.run();
    harness.get_by_label("Sound chain").click();
    harness.run();
    harness.run();
    harness.get_by_label("Built-in").click();
    harness.run();
    harness.get_by_label("Block 2: +").click();
    harness.run();
    harness.get_by_label("EQ").click();
    harness.run();
    harness.get_by_label("Block 2: EQ");
    // A bell lifted and another cut, so the curve shows its bands. The knobs type numbers
    // as the user does: the curve follows them and stays in step with the handles.
    let type_value = |harness: &mut Harness<'static, GuitarScoreApp>, label: &str, value: &str| {
        harness.get_by_label(label).click();
        harness.run();
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        harness.get_by_label(label).type_text(value);
        harness.run();
        harness.key_press(egui::Key::Enter);
        harness.run();
    };
    type_value(&mut harness, "2 Gain value", "8.0");
    type_value(&mut harness, "2 Q value", "2.0");
    type_value(&mut harness, "3 Gain value", "-6.0");
    // Park the mouse off the knobs so no hover highlight depends on the last field.
    harness.hover_at(egui::pos2(600.0, 990.0));
    harness.run();
    // A curve (or needle) drawn anti-aliased differs a little between GPUs: on the CI
    // runner a few pixels are off by up to 1.5 (YIQ distance), none above 2.
    harness.snapshot_options("sound_chain_eq", &curve_tolerance());
}

#[test]
fn section_titles_sit_in_the_master_row() {
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
    harness.get_by_label("Section").click();
    harness.run();
    harness.get_by_label("Section title").click();
    harness.get_by_label("Section title").type_text("Chorus");
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    harness.snapshot("grid_section_title");
}

#[test]
fn the_brush_dialog_sets_speed_and_timing() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    harness.get_by_label("Brush (strum) down").scroll_to_me();
    harness.run();
    harness.get_by_label("Brush (strum) down").click();
    harness.run();
    harness.snapshot("brush_dialog");
    harness.get_by_label("Spread over a 1/16 note").click();
    harness.run();
    harness.get_by_label("Ends on the beat").click();
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    let document = harness.state().document().unwrap();
    let at = document.editor().cursor().position();
    let beat = document.song().beat(at).expect("the cursor is on a beat");
    assert_eq!(
        beat.stroke(gscore_model::StrokeKind::Brush),
        Some(gscore_model::Stroke {
            direction: gscore_model::StrokeDirection::Down,
            speed: 16,
            on_beat: false,
        })
    );
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
    // The value has a small split pair beside it, as the transposition does.
    harness.get_by_label("One beat per minute slower");
    harness.get_by_label("One beat per minute faster");
    harness.snapshot("staff_tempo_dialog");
}

#[test]
fn a_track_can_be_played_by_a_hardware_synthesizer() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.get_by_label("Sound chain").click();
    harness.run();
    harness.run();
    harness
        .query_all_by_label("MIDI out")
        .find(|node| node.rect().center().x < 700.0)
        .expect("the Sound chain window's source switch")
        .click();
    harness.run();
    {
        let playback = harness.state().playback();
        assert!(playback.track_is_midi_out(0));
        let routes = playback.midi_routes();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].0, 0, "the track's own route");
        assert_eq!(routes[0].1.port, None, "automatic until a port is chosen");
        assert_eq!(routes[0].1.channel, None, "automatic channel");
    }
    harness.get_by_label("Block 0: MIDI out");
    // Back to the internal synthesizer: the route is kept but no longer used.
    harness
        .query_all_by_label("MIDI synth")
        .find(|node| node.rect().center().x < 700.0)
        .expect("the Sound chain window's source switch")
        .click();
    harness.run();
    assert!(!harness.state().playback().track_is_midi_out(0));
}

#[test]
fn play_and_stop_drive_the_midi_output() {
    // Plays to ALSA's silent "Midi Through" port; skipped where there is none.
    const THROUGH: &str = "Midi Through";
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
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
fn opening_a_song_starts_loading_the_synthesizer() {
    // A SoundFont is read on the player's own thread; no audio device is needed to start
    // it, so this also runs where playback cannot actually be heard.
    let dir = temp_dir("preload");
    let path = dir.join("sine.sf2");
    std::fs::write(&path, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));

    // Choosing the synthesizer alone does not start it; a song opening does.
    harness.state_mut().playback_mut().use_soundfont(path);
    assert!(!harness.state().playback().is_connected());

    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .open(testdata("basic/note_lengths.gp"), &ctx);
    // Step (not `run`, which stops at a step limit while the loading spinner repaints)
    // until the file is open and the synthesizer is connected.
    assert!(
        step_until(&mut harness, 30, |app| {
            !app.is_busy()
                && app
                    .document()
                    .is_some_and(|document| document.display_title() == "Note Lengths")
        }),
        "the second file did not open"
    );
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Note Lengths"
    );
    assert!(
        harness.state().playback().is_connected(),
        "the synthesizer should load as soon as the song opens"
    );
}

#[test]
fn a_broken_soundfont_is_reported_and_editing_goes_on() {
    let dir = temp_dir("broken-sf2");
    let path = dir.join("broken.sf2");
    std::fs::write(&path, "not a soundfont").unwrap();
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    // The SoundFont must actually play the track (PM synthesis is the default for strings).
    harness.get_by_label("MIDI synth").click();
    harness.run();
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
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
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
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
    harness.state_mut().playback_mut().use_soundfont(path);
    // With "return on stop" (the default) Space stops; pausing is what is tested here.
    harness.state_mut().set_return_on_stop(false);
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

#[test]
fn stop_goes_back_to_where_playback_started_when_asked() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = temp_dir("return-on-stop");
    let path = dir.join("sine.sf2");
    std::fs::write(&path, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
    harness.state_mut().playback_mut().use_soundfont(path);
    harness.state_mut().set_return_on_stop(true);
    let measure = |app: &GuitarScoreApp| app.document().unwrap().editor().cursor().measure.get();

    cursor_to_measure(&mut harness, 1);
    // As the user does it: Space, then the toolbar's Stop.
    harness.key_press(egui::Key::Space);
    harness.run();
    let started = step_until(&mut harness, 10, |app| {
        app.playback().state() == gscore_play::PlayState::Playing || app.notice().is_some()
    });
    assert!(started, "did not start");
    if let Some(notice) = harness.state().notice() {
        eprintln!("skipped: no usable audio device ({notice})");
        return;
    }
    // A bar at 120 bpm lasts two seconds: let the cursor follow into the next one.
    let moved = (0..60).any(|_| {
        std::thread::sleep(Duration::from_millis(50));
        harness.step();
        measure(harness.state()) >= 2
    });
    assert!(moved, "the cursor follows playback");
    harness.get_by_label("Stop").click();
    harness.step();
    // It stays back there, also while the audio thread catches up with the stop.
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(20));
        harness.step();
        assert_eq!(measure(harness.state()), 1);
    }
    assert_eq!(
        harness.state().playback().state(),
        gscore_play::PlayState::Stopped
    );

    // Space while playing stops and goes back too.
    harness.key_press(egui::Key::Space);
    harness.run();
    assert!(step_until(&mut harness, 10, |app| app.playback().state()
        == gscore_play::PlayState::Playing));
    let moved = (0..60).any(|_| {
        std::thread::sleep(Duration::from_millis(50));
        harness.step();
        measure(harness.state()) >= 2
    });
    assert!(moved, "the cursor follows playback again");
    harness.key_press(egui::Key::Space);
    harness.run();
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(20));
        harness.step();
        assert_eq!(measure(harness.state()), 1);
    }
    assert_ne!(
        harness.state().playback().state(),
        gscore_play::PlayState::Playing
    );
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

/// One drum part's own tone in the drum track's sound payload (the sixth track).
#[derive(Debug)]
struct SongDrumPart {
    chain: gscore_app::chain_settings::ChainSetting,
    level: u8,
}

fn song_drum_part(harness: &Harness<'static, GuitarScoreApp>, id: &str) -> Option<SongDrumPart> {
    let document = harness.state().document()?;
    let json = document
        .song()
        .track(gscore_model::TrackIndex::new(5))?
        .sound_state()?;
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let part = value
        .get("drum_parts")?
        .as_array()?
        .iter()
        .find(|part| part.get("part").and_then(serde_json::Value::as_str) == Some(id))?;
    Some(SongDrumPart {
        chain: serde_json::from_value(part.get("chain")?.clone()).ok()?,
        level: part
            .get("level")
            .and_then(serde_json::Value::as_u64)
            .and_then(|level| u8::try_from(level).ok())
            .unwrap_or(127),
    })
}

#[test]
fn the_track_panel_edits_the_shown_track() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    // The panel is open by default and shows the first track (on PM synthesis).
    harness.get_by_label("Instrument");

    // The General MIDI program changes and is undoable: the MIDI synth's table lists it.
    let before = first_sound(&harness).program;
    harness.get_by_label("MIDI synth").click();
    harness.run();
    assert!(
        harness.query_by_label("24: Nylon Str Guitar").is_some(),
        "the program table did not show"
    );
    harness.get_by_label("24: Nylon Str Guitar").click();
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

    // The instrument symbol is a grid of clickable symbols; one click chooses it.
    assert_eq!(first_track(&harness).symbol(), None);
    harness.get_by_label("Instrument symbol").click();
    harness.run();
    harness.get_by_label("Banjo").click();
    harness.run();
    assert_eq!(
        first_track(&harness).symbol(),
        Some(gscore_model::InstrumentSymbol::Banjo)
    );
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run();
    assert_eq!(first_track(&harness).symbol(), None);

    // The capo and the string count live in the tuning dialog.
    harness.get_by_label("Tuning: DADGAD…").click();
    harness.run();
    harness
        .get_all_by_role_and_label(egui::Role::SpinButton, "Capo")
        .next()
        .unwrap()
        .increment_accesskit();
    harness.run();
    harness
        .get_all_by_role_and_label(egui::Role::SpinButton, "Strings")
        .next()
        .unwrap()
        .increment_accesskit();
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    assert_eq!(first_track(&harness).capo().unwrap(), 1);
    assert_eq!(first_track(&harness).string_count(), 7);

    // A rename in place: click the name, type, Enter. One step too.
    let panel_right = |harness: &Harness<'_, GuitarScoreApp>| {
        harness.get_by_label_contains("Tuning:").rect().right()
    };
    let before = panel_right(&harness);
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

    // The rename button is another way in, with the same window.
    harness.get_by_label("Rename track").click();
    harness.run();
    harness.get_by_label("Track name").click();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.run();
    harness.get_by_label("Track name").type_text("Renamed");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(first_track(&harness).name(), "Renamed");

    // Neither editing a name nor a name too long for the header widens the panel (the
    // full-width tuning button keeps its right edge): the name is shortened instead.
    harness.get_by_label("Edit track name").click();
    harness.run();
    assert!(
        (panel_right(&harness) - before).abs() < 0.5,
        "while editing"
    );
    harness
        .get_by_label("Name field")
        .type_text(" with a name much longer than the header");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(first_track(&harness).name().ends_with("than the header"));
    assert!(
        (panel_right(&harness) - before).abs() < 0.5,
        "the panel grew from {before} to {}",
        panel_right(&harness)
    );
}

/// Runs frames until background jobs are done and the UI stops repainting. It steps first,
/// so a click that starts a background job is processed; while a job runs, the UI may
/// repaint for more frames than `Harness::run` allows (the "Opening…" bar, the player
/// measured again as its contents change), so those frames are stepped, not run. The
/// final `try_run` shows what the job produced.
fn settle(harness: &mut Harness<'static, GuitarScoreApp>) {
    let started = Instant::now();
    harness.step();
    while harness.state().is_busy() || harness.try_run().is_err() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_settings_window_edits_song_information_and_undoes_it() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Settings… ctrl+,").click();
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
fn the_empty_workspace_suggests_recent_files() {
    let settings = Settings {
        reopen_last_file: false,
        recent_files: vec![
            testdata("basic/note_lengths.gp"),
            testdata("basic/steel_guitar_dadgad.gp"),
        ],
        ..Settings::default()
    };
    let mut harness = app_with_settings(settings);
    assert!(harness.state().document().is_none());
    // The empty workspace lists the recent files; a click opens one.
    harness.get_by_label("Recent files");
    harness.get_by_label("note_lengths.gp").click();
    settle(&mut harness);
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Note Lengths"
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
fn ctrl_zooms_the_score_and_ctrl_shift_the_interface() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    let interface = |harness: &Harness<'static, GuitarScoreApp>| harness.ctx.zoom_factor();
    // Ctrl+- shrinks the score only (egui's own interface zoom is off).
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Minus);
    harness.run();
    assert!((harness.state().view().zoom() - 0.8).abs() < 1e-4);
    assert!(
        (interface(&harness) - 1.0).abs() < 1e-4,
        "the interface stays"
    );
    // Ctrl+Shift+- and Ctrl+Shift++ size the interface, not the score.
    let both = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    harness.key_press_modifiers(both, egui::Key::Minus);
    harness.run();
    assert!(interface(&harness) < 0.95, "{}", interface(&harness));
    assert!(
        (harness.state().view().zoom() - 0.8).abs() < 1e-4,
        "the score stays"
    );
    harness.key_press_modifiers(both, egui::Key::Plus);
    harness.key_press_modifiers(both, egui::Key::Plus);
    harness.run();
    assert!(interface(&harness) > 1.05, "{}", interface(&harness));
    assert!(
        (harness.state().settings().interface_zoom - interface(&harness)).abs() < 1e-4,
        "remembered in the settings"
    );
    // Ctrl+0 resets the score, Ctrl+Shift+0 the interface.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num0);
    harness.run();
    assert!((harness.state().view().zoom() - 1.0).abs() < 1e-4);
    assert!(interface(&harness) > 1.05);
    harness.key_press_modifiers(both, egui::Key::Num0);
    harness.run();
    assert!((interface(&harness) - 1.0).abs() < 1e-4);
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
    // Select the first measure in the grid (a click only places the cursor; shift-click
    // selects from the cursor's bar).
    let rect = harness.get_by_label("Song grid").rect();
    // The matrix now starts under the pinned ruler: its rect is the first track row.
    let pos = rect.left_top() + egui::vec2(24.0 * 0.0 + 12.0, 12.0);
    click_with(&mut harness, pos, egui::Modifiers::SHIFT);
    assert!(matches!(
        harness.state().document().unwrap().editor().selection(),
        Some(gscore_edit::Selection::Measures { .. })
    ));
}

#[test]
fn the_view_menu_and_the_row_menu_show_the_track_panel() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    harness.get_by_label("Instrument");

    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Track panel").click();
    harness.run();
    assert!(harness.query_by_label("Instrument").is_none());

    // The row's "Edit track…" opens it again.
    right_click_track(&mut harness, "Track 1: Steel Guitar");
    harness.get_by_label("Edit track…").click();
    harness.run();
    harness.get_by_label("Instrument");
}

#[test]
fn clicking_the_song_grid_puts_the_cursor_on_the_bar_without_selecting_it() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    let rect = harness.get_by_label("Song grid").rect();
    // The third measure, first track: 24 px square cells (the ruler is a separate strip).
    let cell = |measure: usize| rect.left_top() + egui::vec2(24.0 * measure as f32 + 12.0, 12.0);
    // A selection first: the click drops it (and the loop it turned on).
    click_with(&mut harness, cell(1), egui::Modifiers::SHIFT);
    assert!(harness.state().selection().is_some());
    assert!(harness.state().playback().loop_selection());
    left_click_at(&mut harness, cell(2));
    let editor = harness.state().document().unwrap().editor();
    let cursor = editor.cursor();
    assert_eq!(cursor.measure, gscore_model::MeasureIndex::new(2));
    assert_eq!(cursor.beat, gscore_model::BeatIndex::new(0));
    assert_eq!(cursor.track, gscore_model::TrackIndex::new(0));
    assert!(editor.selection().is_none());
    assert!(harness.state().selection().is_none());
    assert!(!harness.state().playback().loop_selection());
    // Shift-click selects from the cursor's bar.
    click_with(&mut harness, cell(4), egui::Modifiers::SHIFT);
    let selected = harness.state().selection().unwrap();
    assert_eq!((selected.start(), selected.end()), (2, 4));
}

#[test]
fn dragging_in_the_song_grid_selects_one_track_or_all() {
    let mut harness = app_with(Some(testdata("basic/multi_staff.gp")));
    let rect = harness.get_by_label("Song grid").rect();
    let cell = |measure: usize, track: usize| {
        rect.left_top() + egui::vec2(24.0 * measure as f32 + 12.0, 24.0 * track as f32 + 12.0)
    };
    let selection = |harness: &Harness<'static, GuitarScoreApp>| harness.state().selection();

    // Within the first track's row: bars of that track only.
    harness.hover_at(cell(1, 0));
    harness.drag_at(cell(1, 0));
    harness.run();
    // The first movement starts the drag; keep it inside the starting bar.
    harness.hover_at(cell(1, 0) + egui::vec2(4.0, 0.0));
    harness.run();
    harness.hover_at(cell(3, 0));
    harness.run();
    harness.drop_at(cell(3, 0));
    harness.run();
    let selected = selection(&harness).unwrap();
    assert_eq!(
        (selected.start(), selected.end(), selected.track),
        (1, 3, Some(0))
    );
    assert!(matches!(
        harness.state().document().unwrap().editor().selection(),
        Some(gscore_edit::Selection::Measures {
            track: Some(track),
            ..
        }) if track == gscore_model::TrackIndex::new(0)
    ));

    // Leaving the row on to another track makes it every track; coming back makes it one.
    harness.hover_at(cell(1, 0));
    harness.drag_at(cell(1, 0));
    harness.run();
    harness.hover_at(cell(1, 0) + egui::vec2(4.0, 0.0));
    harness.run();
    harness.hover_at(cell(1, 1));
    harness.run();
    assert_eq!(selection(&harness).unwrap().track, None);
    harness.hover_at(cell(1, 0));
    harness.run();
    assert_eq!(selection(&harness).unwrap().track, Some(0));
    harness.drop_at(cell(1, 0));
    harness.run();

    // A click on the ruler places the cursor; shift-click selects whole bars across every
    // track, from the cursor's bar.
    left_click_at(&mut harness, cell(2, 0) - egui::vec2(0.0, 25.0));
    assert!(selection(&harness).is_none());
    click_with(
        &mut harness,
        cell(3, 0) - egui::vec2(0.0, 25.0),
        egui::Modifiers::SHIFT,
    );
    let selected = selection(&harness).unwrap();
    assert_eq!(
        (selected.start(), selected.end(), selected.track),
        (2, 3, None)
    );
}

#[test]
fn copy_and_delete_act_on_a_one_track_grid_selection() {
    let mut harness = app_with(Some(testdata("basic/multi_staff.gp")));
    let notes = |harness: &Harness<'static, GuitarScoreApp>, track: usize, measure: usize| {
        let document = harness.state().document().unwrap();
        let at = gscore_model::BeatPosition {
            staff: gscore_model::StaffIndex::new(0),
            track: gscore_model::TrackIndex::new(track),
            measure: gscore_model::MeasureIndex::new(measure),
            voice: gscore_model::VoiceIndex::new(0),
            beat: gscore_model::BeatIndex::new(0),
        };
        document.song().beat(at).map_or_else(Vec::new, |beat| {
            beat.notes()
                .map(|note| (note.string().unwrap().get(), note.fret().unwrap().get()))
                .collect()
        })
    };
    let rect = harness.get_by_label("Song grid").rect();
    // One bar of the first track: a click puts the cursor there, shift-click selects it.
    let select = |harness: &mut Harness<'static, GuitarScoreApp>, measure: usize| {
        let cell = rect.left_top() + egui::vec2(24.0 * measure as f32 + 12.0, 12.0);
        left_click_at(harness, cell);
        click_with(harness, cell, egui::Modifiers::SHIFT);
    };

    let source = notes(&harness, 0, 0);
    assert!(!source.is_empty(), "the first bar has notes");
    let other = notes(&harness, 1, 1);
    select(&mut harness, 0);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::C);
    harness.run();
    select(&mut harness, 1);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::V);
    harness.run();
    assert_eq!(notes(&harness, 0, 1), source, "the copied track's bar");
    assert_eq!(
        notes(&harness, 1, 1),
        other,
        "the other track did not change"
    );

    // Delete clears the selected track's notes only.
    select(&mut harness, 1);
    harness.key_press(egui::Key::Delete);
    harness.run();
    assert!(notes(&harness, 0, 1).is_empty());
    assert_eq!(notes(&harness, 1, 1), other);
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
    click_with(harness, pos, egui::Modifiers::NONE);
}

/// Left-clicks at `pos` with `modifiers` held.
fn click_with(
    harness: &mut Harness<'static, GuitarScoreApp>,
    pos: egui::Pos2,
    modifiers: egui::Modifiers,
) {
    harness
        .input_mut()
        .events
        .push(egui::Event::ModifiersChanged(modifiers));
    harness.hover_at(pos);
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        });
    }
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
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
    // Settings written before tabs existed only know one file; it still reopens.
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
fn every_open_tab_is_restored_at_startup() {
    let settings = Settings {
        reopen_last_file: true,
        open_files: vec![
            testdata("basic/note_lengths.gp"),
            testdata("basic/steel_guitar_dadgad.gp"),
        ],
        active_file: Some(testdata("basic/note_lengths.gp")),
        ..Settings::default()
    };
    let harness = app_with_settings(settings);
    assert_eq!(harness.state().tab_count(), 2, "both tabs come back");
    assert_eq!(
        harness.state().tab_document(0).unwrap().display_title(),
        "Note Lengths"
    );
    assert_eq!(
        harness.state().tab_document(1).unwrap().display_title(),
        "Steel Guitar DADGAD"
    );
    // The tab that was active last time is active again.
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Note Lengths"
    );
}

#[test]
fn a_missing_tab_is_skipped_at_startup() {
    let settings = Settings {
        reopen_last_file: true,
        open_files: vec![
            testdata("basic/no_such_file.gp"),
            testdata("basic/note_lengths.gp"),
        ],
        active_file: Some(testdata("basic/note_lengths.gp")),
        ..Settings::default()
    };
    let harness = app_with_settings(settings);
    assert_eq!(harness.state().tab_count(), 1, "only the file that exists");
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Note Lengths"
    );
    assert_eq!(harness.state().notice(), None);
}

#[test]
fn closing_and_reopening_remembers_every_open_tab() {
    // Build the settings one app would write after opening two files, then start afresh
    // with them: both tabs must come back.
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .open(testdata("basic/steel_guitar_dadgad.gp"), &ctx);
    settle(&mut harness);
    assert_eq!(harness.state().tab_count(), 2);
    let saved = harness.state().settings();
    assert_eq!(saved.open_files.len(), 2);
    assert_eq!(
        saved.active_file.as_deref(),
        Some(testdata("basic/steel_guitar_dadgad.gp").as_path())
    );

    let harness = app_with_settings(saved);
    assert_eq!(harness.state().tab_count(), 2);
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Steel Guitar DADGAD"
    );
}

#[test]
fn the_same_file_is_never_open_in_two_tabs() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    let ctx = harness.ctx.clone();
    assert_eq!(harness.state().tab_count(), 1);

    // Opening the file that is already open switches to its tab instead of adding another.
    harness
        .state_mut()
        .open(testdata("basic/note_lengths.gp"), &ctx);
    settle(&mut harness);
    assert_eq!(harness.state().tab_count(), 1);
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Note Lengths"
    );

    // Another file still opens in a new tab; reopening the first switches back to it.
    harness
        .state_mut()
        .open(testdata("basic/steel_guitar_dadgad.gp"), &ctx);
    settle(&mut harness);
    assert_eq!(harness.state().tab_count(), 2);
    harness
        .state_mut()
        .open(testdata("basic/note_lengths.gp"), &ctx);
    settle(&mut harness);
    assert_eq!(harness.state().tab_count(), 2);
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
    assert_eq!(harness.state().shown_track(), 1);
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
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
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

    // The count-in clicks without the metronome, and the mixer's metronome meter shows them.
    assert!(!harness.state().playback().metronome());
    assert!(
        step_until(&mut harness, 2, |app| app.playback().metronome_level()
            > 0.0),
        "the count-in's clicks show on the metronome's meter"
    );

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
fn an_interrupted_count_in_starts_again_where_the_cursor_is_now() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = temp_dir("count-in-again");
    let path = dir.join("sine.sf2");
    std::fs::write(&path, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
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
    // Half way into the count-in: pause, move the cursor, play again.
    assert!(step_until(&mut harness, 5, |app| {
        app.playback().metronome_level() > 0.0
    }));
    std::thread::sleep(Duration::from_millis(1100));
    harness.step();
    assert!(harness.state().playback().count_in_pending());
    harness.get_by_label("Pause").click();
    harness.run();
    cursor_to_measure(&mut harness, 0);
    harness.get_by_label("Play").click();
    harness.run();
    // A whole new count-in for the new place, not the rest of the old one.
    assert!(harness.state().playback().count_in_pending());
    assert_eq!(harness.state().playback().current_measure(), Some(1));
    assert!(
        step_until(&mut harness, 10, |app| {
            app.playback().state() == gscore_play::PlayState::Playing
                && !app.playback().count_in_pending()
        }),
        "the count-in did not lead into the song"
    );
    assert_eq!(harness.state().playback().current_measure(), Some(1));

    // A click on the score while the song plays counts in again, then plays from there.
    let song = harness.state().document().unwrap().song().clone();
    let at = gscore_model::BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: gscore_model::TrackIndex::new(0),
        measure: gscore_model::MeasureIndex::new(3),
        voice: gscore_model::VoiceIndex::new(0),
        beat: gscore_model::BeatIndex::new(0),
    };
    harness
        .state_mut()
        .playback_mut()
        .jump_to(&song, at)
        .unwrap();
    harness.step();
    assert!(harness.state().playback().count_in_pending());
    assert_eq!(harness.state().playback().current_measure(), Some(4));
    assert!(
        step_until(&mut harness, 10, |app| !app.playback().count_in_pending()),
        "the second count-in did not lead into the song"
    );
    assert_eq!(harness.state().playback().current_measure(), Some(4));
    harness.state_mut().playback_mut().stop().unwrap();
}

#[test]
fn previous_and_next_bar_during_a_count_in_count_in_again() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = temp_dir("count-in-navigate");
    let path = dir.join("sine.sf2");
    std::fs::write(&path, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
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
    assert!(harness.state().playback().count_in_pending());
    assert_eq!(harness.state().playback().current_measure(), Some(3));

    // Next bar while the clicks run: a new count-in for the new bar, not a stop.
    harness.get_by_label("Next bar").click();
    harness.run();
    assert!(
        harness.state().playback().count_in_pending(),
        "playback stopped instead of counting in again"
    );
    assert_eq!(harness.state().playback().current_measure(), Some(4));
    assert_eq!(
        harness.state().playback().state(),
        gscore_play::PlayState::Playing
    );

    // Previous bar: again a count-in, now for the bar before.
    harness.get_by_label("Previous bar").click();
    harness.run();
    assert!(harness.state().playback().count_in_pending());
    assert_eq!(harness.state().playback().current_measure(), Some(3));

    // The clicks lead into the song at the bar chosen last.
    assert!(
        step_until(&mut harness, 10, |app| {
            app.playback().state() == gscore_play::PlayState::Playing
                && !app.playback().count_in_pending()
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
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
    harness.state_mut().playback_mut().use_soundfont(path);
    let document = harness.state_mut().document_mut().unwrap();
    document
        .editor_mut()
        .set_selection(Some(gscore_edit::Selection::Measures {
            anchor: gscore_model::MeasureIndex::new(0),
            focus: gscore_model::MeasureIndex::new(0),
            track: None,
        }));
    harness
        .state_mut()
        .set_selection(Some(gscore_app::MeasureSelection::new(0)));
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
fn the_new_song_dialog_creates_a_one_measure_score_in_a_new_tab() {
    let mut harness = app_with(None);
    assert!(harness.state().document().is_none());
    harness.get_by_label("New song").click();
    harness.run();
    // The dialog appears first; no document exists yet.
    assert!(harness.state().document().is_none());
    harness.get_by_label("New song title");
    harness.get_by_label("Create").click();
    harness.run();
    let document = harness.state().document().expect("a new document");
    assert_eq!(document.song().measure_count(), 1);
    assert_eq!(document.song().track_count(), 1);
    assert_eq!(document.display_title(), "New Song");
    assert_eq!(document.song().tempo(), 120);
    harness.get_by_label("1 measure, 1 track");
}

#[test]
fn closing_a_changed_tab_asks_before_closing_it() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    // Make the single tab dirty.
    harness.event_modifiers(egui::Event::Text("3".to_owned()), egui::Modifiers::NONE);
    harness.run();
    assert!(harness.state().document().unwrap().is_dirty());

    // The tab's close icon asks before closing; Cancel keeps the tab.
    harness.get_by_label("Close tab 1").click();
    harness.run();
    assert!(harness.state().has_dialog());
    assert_eq!(harness.state().tab_count(), 1);
    harness.get_by_label("Cancel").click();
    harness.run();
    assert_eq!(harness.state().tab_count(), 1);

    // Discard changes closes it, leaving an empty workspace.
    harness.get_by_label("Close tab 1").click();
    harness.run();
    harness.get_by_label("Discard changes").click();
    harness.run();
    assert!(harness.state().document().is_none());
    assert_eq!(harness.state().tab_count(), 0);
    harness.get_by_label("No file open");
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
    harness.get_by_label("Settings… ctrl+,").click();
    harness.run();
    harness.get_by_label("Audio I/O").click();
    harness.run();
    harness.get_by_label("SoundFont file");
    harness.get_by_label("MIDI");
}

#[test]
fn the_metronome_tab_sets_the_click_and_the_count_in() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Settings… ctrl+,").click();
    harness.run();
    // The tab (the toolbar's metronome switch has the same name).
    harness
        .get_all_by_label("Metronome")
        .last()
        .unwrap()
        .click();
    harness.run();
    harness.get_by_label("Cowbell").click();
    harness.run();
    assert_eq!(
        harness.state().playback().click_sound(),
        gscore_play::ClickSound::Cowbell
    );
    harness
        .get_by_label("Count in before playback starts")
        .click();
    harness.run();
    assert!(harness.state().playback().count_in());
    harness.get_by_label("Count-in length").click();
    harness.run();
    harness.get_by_label("2 bars").click();
    harness.run();
    assert_eq!(harness.state().playback().count_in_bars(), 2);
    harness
        .get_by_label("Count in again each time a loop starts over")
        .click();
    harness.run();
    assert!(harness.state().playback().count_in_loops());
    // All of it is kept in the settings.
    let settings = harness.state().settings();
    assert_eq!(settings.click_sound, "cowbell");
    assert_eq!(settings.count_in_bars, 2);
    assert!(settings.count_in_loops);
}

#[test]
fn a_loop_is_counted_in_again_each_time_it_starts_over() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = temp_dir("count-in-loop");
    let path = dir.join("sine.sf2");
    std::fs::write(&path, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
    harness.state_mut().playback_mut().use_soundfont(path);
    harness.state_mut().set_count_in(true);
    harness.state_mut().playback_mut().set_count_in_loops(true);
    // Loop the second bar.
    let document = harness.state_mut().document_mut().unwrap();
    document
        .editor_mut()
        .set_selection(Some(gscore_edit::Selection::Measures {
            anchor: gscore_model::MeasureIndex::new(1),
            focus: gscore_model::MeasureIndex::new(1),
            track: None,
        }));
    harness
        .state_mut()
        .set_selection(Some(gscore_app::MeasureSelection::new(1)));
    harness.state_mut().set_loop_selection(true);
    cursor_to_measure(&mut harness, 1);
    harness.get_by_label("Play").click();
    let started = step_until(&mut harness, 10, |app| {
        app.playback().state() == gscore_play::PlayState::Playing || app.notice().is_some()
    });
    assert!(started, "neither played nor failed");
    if let Some(notice) = harness.state().notice() {
        eprintln!("skipped: no usable audio device ({notice})");
        return;
    }
    assert!(step_until(&mut harness, 5, |app| !app
        .playback()
        .count_in_pending()));
    assert_eq!(harness.state().playback().current_measure(), Some(2));
    // At the bar's end, a new count-in, then the bar again.
    assert!(
        step_until(&mut harness, 5, |app| app.playback().count_in_pending()),
        "the loop's next pass was not counted in"
    );
    assert_eq!(harness.state().playback().current_measure(), Some(2));
    harness.state_mut().playback_mut().stop().unwrap();
}

#[test]
fn a_looped_note_selection_starts_at_its_first_bar() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = temp_dir("loop-notes");
    let path = dir.join("sine.sf2");
    std::fs::write(&path, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
    harness.state_mut().playback_mut().use_soundfont(path);
    harness.state_mut().set_count_in(true);
    // The third and fourth beat of the fourth bar (quarter notes), looped.
    let spot = |beat: usize| gscore_edit::BeatSpot {
        measure: gscore_model::MeasureIndex::new(3),
        beat: gscore_model::BeatIndex::new(beat),
    };
    let document = harness.state_mut().document_mut().unwrap();
    document
        .editor_mut()
        .set_selection(Some(gscore_edit::Selection::Beats {
            staff: gscore_model::StaffIndex::new(0),
            track: gscore_model::TrackIndex::new(0),
            voice: gscore_model::VoiceIndex::new(0),
            anchor: spot(2),
            focus: spot(3),
        }));
    harness.run();
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
    // While counting in, the readout shows the bar's start, not the selected note.
    let bar = u64::from(gscore_play::PPQ).saturating_mul(4);
    assert!(harness.state().playback().count_in_pending());
    assert_eq!(harness.state().playback().position(), bar.saturating_mul(3));
    assert_eq!(harness.state().playback().current_measure(), Some(4));
    harness.state_mut().playback_mut().stop().unwrap();
}

#[test]
fn shortcuts_can_be_changed_and_reverted_from_the_editor() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Settings… ctrl+,").click();
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
    harness.get_by_label("History… H").click();
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
    assert_eq!(harness.state().shown_track(), 1);
}

#[test]
fn the_loop_button_works_without_a_selection() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    assert!(!harness.state().playback().loop_selection());
    // Nothing selected: Loop still turns on, and then the whole song repeats.
    harness.get_by_label("Loop selection").click();
    harness.run();
    assert!(
        harness.state().playback().loop_selection(),
        "looping the whole song"
    );
    harness.get_by_label("Loop selection").click();
    harness.run();
    assert!(!harness.state().playback().loop_selection());
    // With just one beat selected it works too.
    harness
        .state_mut()
        .document_mut()
        .unwrap()
        .editor_mut()
        .set_selection(Some(gscore_edit::Selection::Beats {
            staff: gscore_model::StaffIndex::new(0),
            track: gscore_model::TrackIndex::new(0),
            voice: gscore_model::VoiceIndex::new(0),
            anchor: gscore_edit::BeatSpot {
                measure: gscore_model::MeasureIndex::new(0),
                beat: gscore_model::BeatIndex::new(0),
            },
            focus: gscore_edit::BeatSpot {
                measure: gscore_model::MeasureIndex::new(0),
                beat: gscore_model::BeatIndex::new(0),
            },
        }));
    harness.run();
    harness.get_by_label("Loop selection").click();
    harness.run();
    assert!(harness.state().playback().loop_selection());
}

#[test]
fn the_grid_switches_to_a_mixer_view() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    assert!(
        harness.query_by_label("Metronome volume").is_none(),
        "the grid view has no metronome fader"
    );
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
    // The metronome has its volume fader in the mixer, and only there.
    let fader = harness.get_by_label("Metronome volume");
    let start = fader.rect().center();
    harness.hover_at(start);
    harness.drag_at(start);
    harness.run();
    for step in 1..=4 {
        harness.hover_at(start + egui::vec2(0.0, 8.0 * step as f32));
        harness.run();
    }
    harness.drop_at(start + egui::vec2(0.0, 32.0));
    harness.run();
    assert!(
        harness.state().playback().metronome_volume() < 0.5,
        "the fader turns the click volume down"
    );

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
fn the_toolbar_transposes_by_semitones_and_the_fork_bypasses_it() {
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
    let state = |harness: &Harness<'static, GuitarScoreApp>| {
        let song = harness.state().document().unwrap().song();
        (song.transposition(), song.transpose_active())
    };
    assert_eq!(state(&harness), (Some(0), false), "nothing transposed yet");
    harness.get_by_label("Transpose up a semitone").click();
    harness.run();
    assert_eq!(state(&harness), (Some(1), true));
    assert_eq!(
        harness.state().status(),
        Some("Transposed up a semitone"),
        "the status bar says what happened"
    );
    harness.get_by_label_contains("Transposition: +1");
    // The tuning fork switches it off and on again (the written notes stay).
    harness.get_by_label("Transposition").click();
    harness.run();
    assert_eq!(
        state(&harness),
        (Some(1), false),
        "bypassed, the value kept"
    );
    harness.get_by_label("Transposition").click();
    harness.run();
    assert_eq!(state(&harness), (Some(1), true));
    // One step down is back to none.
    harness.get_by_label("Transpose down a semitone").click();
    harness.run();
    assert_eq!(state(&harness), (Some(0), true));
}

#[test]
fn the_transpose_window_moves_the_notes_when_the_fingerings_change() {
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
    // The first note's string and fret before the edit.
    let first = |harness: &Harness<'static, GuitarScoreApp>| {
        let song = harness.state().document().unwrap().song();
        let note = song
            .track(gscore_model::TrackIndex::new(0))
            .unwrap()
            .measure(gscore_model::MeasureIndex::new(0))
            .unwrap()
            .voices()
            .next()
            .unwrap()
            .beats()
            .next()
            .unwrap()
            .notes()
            .next()
            .unwrap();
        (note.string().unwrap().get(), note.fret().unwrap().get())
    };
    let before = first(&harness);
    harness.get_by_label("Tools").click();
    harness.run();
    harness.get_by_label("Transpose…").click();
    harness.run();
    for _ in 0..2 {
        harness.get_by_label("A semitone higher").click();
        harness.run();
    }
    harness.get_by_label("Change the fingerings").click();
    harness.run();
    harness.get_by_label("Apply the transposition").click();
    harness.run();
    let after = first(&harness);
    assert_ne!(after, before, "the note moved on the fretboard");
    // Two semitones on the same string, when it can hold them.
    if after.0 == before.0 {
        assert_eq!(after.1, before.1.saturating_add(2));
    }
    // The window closed, and nothing was transposed in sound (the notes moved instead).
    assert!(harness.query_by_label("Change the fingerings").is_none());
    assert_eq!(
        harness.state().document().unwrap().song().transposition(),
        Some(0)
    );
}

#[test]
fn the_tuning_dialog_plays_a_string_and_stops_it_again() {
    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    harness.get_by_label_contains("Tuning:").click();
    harness.run();
    // A reference tone per string: the same button stops it (it rings until stopped).
    harness.get_by_label("Play string 1").click();
    harness.run();
    // Without an audio device the tone cannot start and says why; nothing hangs.
    let stopped = harness.query_by_label("Stop string 1").is_some();
    let failed = harness.query_all_by_label_contains("Play string 1").count() > 0;
    assert!(stopped || failed, "either it sounds or it says why not");
    if stopped {
        harness.get_by_label("Stop string 1").click();
        harness.run();
        harness.get_by_label("Play string 1");
    }
    harness.get_by_label("Cancel").click();
    harness.run();
}

#[test]
fn the_mixer_has_a_reverb_bus_the_tracks_send_into() {
    let mut harness = app_with(Some(testdata("basic/multi_staff.gp")));
    harness.get_by_label("Mixer view").click();
    harness.run();
    harness.get_by_label("Reverb bus level");
    // Its fader turns the return down (the tracks' Reverb knobs are the sends).
    let fader = harness.get_by_label("Reverb bus level").rect();
    let start = fader.center();
    harness.hover_at(start);
    harness.drag_at(start);
    harness.run();
    for step in 1..=4 {
        harness.hover_at(start + egui::vec2(0.0, 8.0 * step as f32));
        harness.run();
    }
    harness.drop_at(start + egui::vec2(0.0, 32.0));
    harness.run();
    assert!(
        harness.state().playback().reverb_bus().level < 127,
        "the fader turned the return down"
    );
    // And its mute.
    harness.get_by_label("Mute reverb bus").click();
    harness.run();
    assert!(harness.state().playback().reverb_bus().muted);
}

#[test]
fn the_reverb_bus_window_chooses_the_reverb() {
    use gscore_app::chain_settings::BlockChoice;

    let mut harness = app_with(Some(testdata("basic/multi_staff.gp")));
    harness.get_by_label("Mixer view").click();
    harness.run();
    harness
        .get_by_label("Sound chain for the reverb bus")
        .click();
    harness.run();
    harness.snapshot("reverb_bus_window");
    // A computed reverb by default, with its knobs and no mix (the bus runs all wet).
    assert_eq!(
        harness.state().playback().reverb_bus().block.choice,
        BlockChoice::Reverb
    );
    for knob in ["Size", "Damping", "Width"] {
        assert!(harness.query_all_by_label(knob).next().is_some(), "{knob}");
    }
    assert!(
        harness.query_all_by_label("Mix").next().is_none(),
        "an all-wet bus has no mix"
    );
    // It can be turned off, and back on.
    harness.get_by_label("None").click();
    harness.run();
    assert_eq!(
        harness.state().playback().reverb_bus().block.choice,
        BlockChoice::Empty
    );
    harness.get_by_label("Computed").click();
    harness.run();
    assert_eq!(
        harness.state().playback().reverb_bus().block.choice,
        BlockChoice::Reverb
    );
    // A room is offered too (it asks for its WAV).
    harness.get_by_label("Room (impulse response)");
}

#[test]
fn every_change_in_a_bus_window_is_its_own_undo_step_and_a_drag_is_one() {
    use gscore_app::chain_settings::BlockChoice;

    let mut harness = app_with(Some(testdata("basic/multi_staff.gp")));
    harness.get_by_label("Mixer view").click();
    harness.run();
    harness
        .get_by_label("Sound chain for the reverb bus")
        .click();
    harness.run();
    let undo_depth = |harness: &Harness<'_, GuitarScoreApp>| {
        harness.state().document().unwrap().editor().undo_depth()
    };
    let start = undo_depth(&harness);
    // Two clicks are two edits, not one merged step.
    harness.get_by_label("None").click();
    harness.run();
    harness.get_by_label("Computed").click();
    harness.run();
    assert_eq!(
        undo_depth(&harness),
        start + 2,
        "two clicks, two undo steps"
    );

    // Two drags of one knob are two steps; each drag is one.
    let drag = |harness: &mut Harness<'_, GuitarScoreApp>| {
        let from = harness.get_by_label("Size").rect().center();
        harness.hover_at(from);
        harness.drag_at(from);
        harness.run();
        // The knob turns with a sideways drag.
        for step in 1..=4 {
            harness.hover_at(from + egui::vec2(8.0 * step as f32, 0.0));
            harness.run();
        }
        harness.drop_at(from + egui::vec2(32.0, 0.0));
        harness.run();
    };
    let size = |harness: &Harness<'_, GuitarScoreApp>| harness.state().playback().reverb_bus();
    let before = size(&harness);
    drag(&mut harness);
    let after_first = size(&harness);
    assert_ne!(after_first, before, "the drag turned the knob");
    assert_eq!(undo_depth(&harness), start + 3, "one drag, one undo step");
    drag(&mut harness);
    assert_ne!(
        size(&harness),
        after_first,
        "the second drag turned it further"
    );
    assert_eq!(
        undo_depth(&harness),
        start + 4,
        "another drag, another step"
    );

    // Undo goes back one drag at a time.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run();
    assert_eq!(size(&harness), after_first);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run();
    assert_eq!(size(&harness), before);
    assert_eq!(size(&harness).block.choice, BlockChoice::Reverb);
}

#[test]
fn the_compressor_bus_is_a_strip_and_a_window() {
    use gscore_app::chain_settings::{BlockChoice, CompRatioSetting};

    // A wide window, so the strips after the tracks (the two buses) are on screen.
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 700.0))
        .build_eframe(|cc| {
            GuitarScoreApp::new(&cc.egui_ctx, Some(testdata("basic/multi_staff.gp")))
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
    harness.get_by_label("Mixer view").click();
    harness.run();
    harness.get_by_label("Comp bus level");
    // Its fader turns the parallel sum down.
    let fader = harness.get_by_label("Comp bus level").rect();
    let start = fader.center();
    harness.hover_at(start);
    harness.drag_at(start);
    harness.run();
    for step in 1..=4 {
        harness.hover_at(start + egui::vec2(0.0, 8.0 * step as f32));
        harness.run();
    }
    harness.drop_at(start + egui::vec2(0.0, 32.0));
    harness.run();
    assert!(harness.state().playback().compressor_bus().level < 127);
    // And its mute.
    harness.get_by_label("Mute comp bus").click();
    harness.run();
    assert!(harness.state().playback().compressor_bus().muted);
    harness.get_by_label("Mute comp bus").click();
    harness.run();
    // Its window holds the CA-76: a default at 4:1 for a whole kit (not "All", which
    // pumps), with its knobs and its gain-reduction meter.
    harness.get_by_label("Sound chain for the comp bus").click();
    harness.run();
    // A curve (or needle) drawn anti-aliased differs a little between GPUs: on the CI
    // runner a few pixels are off by up to 1.5 (YIQ distance), none above 2.
    harness.snapshot_options("compressor_bus_window", &curve_tolerance());
    let bus = harness.state().playback().compressor_bus();
    assert_eq!(bus.block.choice, BlockChoice::Compressor);
    assert_eq!(bus.block.comp_ratio, CompRatioSetting::Four);
    assert!(bus.block.input_db > 0.0 && bus.block.input_db < 18.0);
    for knob in ["Input", "Output", "Attack", "Release", "Mix"] {
        assert!(harness.query_all_by_label(knob).next().is_some(), "{knob}");
    }
    // It can be turned off, and back on.
    harness.get_by_label("None").click();
    harness.run();
    assert_eq!(
        harness.state().playback().compressor_bus().block.choice,
        BlockChoice::Empty
    );
    harness.get_by_label("CA-76").click();
    harness.run();
    assert_eq!(
        harness.state().playback().compressor_bus().block.choice,
        BlockChoice::Compressor
    );
    // A fresh CA-76 has its plain knobs; Default brings back the bus's built-in setting.
    assert_ne!(harness.state().playback().compressor_bus(), bus);
    harness.get_by_label("Default").click();
    harness.run();
    assert_eq!(harness.state().playback().compressor_bus().block, bus.block);
    harness.get_by_label("Close").click();
    harness.run();
}

#[test]
fn the_mixer_has_a_chorus_bus_the_tracks_send_into() {
    use gscore_app::chain_settings::BlockChoice;

    let mut harness = app_with(Some(testdata("basic/steel_guitar_dadgad.gp")));
    harness.get_by_label("Mixer view").click();
    harness.run();
    harness.get_by_label("Chorus bus level");
    // Its fader turns the chorused signal down, and its mute silences it.
    let fader = harness.get_by_label("Chorus bus level").rect();
    let start = fader.center();
    harness.hover_at(start);
    harness.drag_at(start);
    harness.run();
    for step in 1..=4 {
        harness.hover_at(start + egui::vec2(0.0, 8.0 * step as f32));
        harness.run();
    }
    harness.drop_at(start + egui::vec2(0.0, 32.0));
    harness.run();
    assert!(harness.state().playback().chorus_bus().level < 127);
    harness.get_by_label("Mute chorus bus").click();
    harness.run();
    assert!(harness.state().playback().chorus_bus().muted);
    harness.get_by_label("Mute chorus bus").click();
    harness.run();
    // Its window holds the chorus, all wet (the tracks' Chorus knobs are the sends).
    harness
        .get_by_label("Sound chain for the chorus bus")
        .click();
    harness.run();
    harness.snapshot("chorus_bus_window");
    let bus = harness.state().playback().chorus_bus();
    assert_eq!(bus.block.choice, BlockChoice::Chorus);
    assert!((bus.block.mix - 1.0).abs() < 1e-6);
    for knob in ["Rate", "Depth"] {
        assert!(harness.query_all_by_label(knob).next().is_some(), "{knob}");
    }
    assert!(
        harness.query_all_by_label("Mix").next().is_none(),
        "an all-wet bus has no mix"
    );
    // It can be turned off, and back on.
    harness.get_by_label("None").click();
    harness.run();
    assert_eq!(
        harness.state().playback().chorus_bus().block.choice,
        BlockChoice::Empty
    );
    harness.get_by_label("Stereo chorus").click();
    harness.run();
    assert_eq!(
        harness.state().playback().chorus_bus().block.choice,
        BlockChoice::Chorus
    );
    harness.get_by_label("Close").click();
    harness.run();
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
    let mut harness = wide_app_with(Some(testdata("basic/note_lengths.gp")));
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

#[test]
fn dismissing_an_open_error_shows_the_empty_workspace() {
    let mut harness = app_with(Some(testdata("basic/README.md")));
    assert!(harness.state().notice().is_some());
    harness.get_by_label("Dismiss").click();
    settle(&mut harness);
    assert_eq!(harness.state().notice(), None);
    harness.get_by_label("Open a song");
}

#[test]
fn the_speed_trainer_button_left_of_the_speed_opens_its_settings() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    assert!(!harness.state().playback().trainer().enabled);
    let button = harness.get_by_label("Speed trainer").rect();
    let speed = harness.get_by_label("Playback speed").rect();
    assert!(
        button.right() <= speed.left(),
        "the button sits left of the speed"
    );
    harness.get_by_label("Speed trainer").click();
    settle(&mut harness);
    harness
        .get_by_label("Raise the speed while a loop plays")
        .click();
    settle(&mut harness);
    assert!(harness.state().playback().trainer().enabled);
}

#[test]
fn file_keys_work_in_the_empty_workspace() {
    let mut harness = app_with(None);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::N);
    harness.run();
    // Ctrl+N opens the new-song dialog.
    harness.get_by_label("Create");
}

#[test]
fn the_view_menu_shows_one_key_per_entry() {
    let mut harness = app_with(Some(testdata("basic/note_lengths.gp")));
    harness.get_by_label("View").click();
    harness.run();
    // Zoom in has two keys (ctrl++ and ctrl+=); the menu shows the first.
    harness.get_by_label("Zoom in ctrl++");
    harness.get_by_label("Zoom out ctrl+-");
    harness.get_by_label("Actual size ctrl+0");
    harness.key_press(egui::Key::Escape);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Quit ctrl+q");
}

#[test]
fn a_songs_mixer_state_travels_in_its_notes() {
    // The comp sends and the tones a song plays are kept in the song's free-text notes
    // ("Notices" in Guitar Pro, which keeps that field): a saved and reopened song sounds
    // the same, even with default settings.
    let dir = std::env::temp_dir().join(format!("gscore-notes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temp dir");
    let path = dir.join("song.gp");
    std::fs::copy(testdata("basic/multi_staff.gp"), &path).expect("a copy of the song");

    // A tall window: the track panel's Sound section (the stomp box) is below the mixer.
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1100.0))
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(path.clone())));
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
    harness.get_by_label("Mixer view").click();
    harness.run();
    // A send into the compressor bus on the first track: the knob drags sideways.
    let knob = harness.get_by_label("Comp Clean Guitar").rect().center();
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
    let send = harness
        .state()
        .playback()
        .comp_sends()
        .iter()
        .find(|(track, _)| *track == 0)
        .map(|&(_, value)| value)
        .expect("the send was set");
    assert!(send > 0, "{send}");
    // And the song's own reverb bus (muted here): a bus the song sets travels too.
    harness.get_by_label("Mute reverb bus").click();
    harness.run();
    assert!(harness.state().playback().reverb_bus().muted);
    // And a tone of its own for the track's preset (27: Clean Guitar).
    harness.get_by_label("Sound chain").click();
    harness.run();
    harness.run();
    harness.get_by_label("Block 0: Strings").click();
    harness.run();
    // The picking hand's knobs are on the Strike tab.
    harness.get_by_label("Strike").click();
    harness.run();
    harness.get_by_label("Pick position value").click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.get_by_label("Pick position value").type_text("0.4");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    harness.get_by_label("Close").click();
    harness.run();
    // Saved, the song carries both in its notes.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::S);
    // The key is handled on the next frame: only then is the save running, and waited for.
    harness.step();
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "saving timed out"
        );
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    // The saved notice and the meters keep the interface repainting for a few frames.
    harness.run_steps(8);

    // A fresh app with default settings hears the same sound and sends.
    let reopened = app_with(Some(path));
    assert_eq!(
        reopened
            .state()
            .playback()
            .comp_sends()
            .iter()
            .find(|(track, _)| *track == 0)
            .map(|&(_, value)| value),
        Some(send)
    );
    let tone = reopened
        .state()
        .playback()
        .track_tones()
        .get(&0)
        .cloned()
        .expect("the song's tone came back");
    assert!((tone.sound.pick_position - 0.4).abs() < 1e-6, "{tone:?}");
    // The song's own bus came back and plays (the fresh app's default bus is not muted).
    assert!(reopened.state().playback().reverb_bus().muted);
    // The notes the user sees are their own: our block is not shown.
    assert!(
        reopened
            .state()
            .document()
            .expect("the song")
            .song()
            .notes()
            .all(|note| !note.starts_with("[GuitarScore"))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_drum_tracks_window_edits_the_kits_parts() {
    use gscore_app::chain_settings::{BlockChoice, DrumPart};

    // A tall window: the track panel's Sound section (the stomp box) is below the mixer.
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1100.0))
        .build_eframe(|cc| {
            GuitarScoreApp::new(&cc.egui_ctx, Some(testdata("basic/multi_staff.gp")))
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
    // The drum track (the sixth) and its Sound chain.
    harness.state_mut().set_shown_track(5);
    harness.run();
    harness.get_by_label("Sound chain").click();
    harness.run();
    harness.run();
    // The kit's own chain (the drum bus) is shown first, empty until the owner designs
    // the defaults: the kit plays as the SoundFont renders it.
    harness.get_by_label("Part");
    harness.get_by_label("Block 1: +");
    // A part's board is empty too.
    harness.get_by_label("Kick").click();
    harness.run();
    harness.get_by_label("Block 1: +");
    harness.snapshot("sound_chain_drum_kick");
    // A block added to the kick is the kick's tone in the song from then on (an edit with
    // undo), and this machine's kit keeps its own.
    harness.get_by_label("Block 1: +").click();
    harness.run();
    harness.get_by_label("Compressor").click();
    harness.run();
    harness.get_by_label("Block 1: Comp").click();
    harness.run();
    harness.get_by_label("Input value").click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.get_by_label("Input value").type_text("12");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    let kick = song_drum_part(&harness, "kick").expect("the kick's tone is in the song");
    assert!(
        (kick.chain.blocks[0].input_db - 12.0).abs() < 1e-6,
        "{kick:?}"
    );
    assert_eq!(kick.chain.blocks[0].choice, BlockChoice::Compressor);
    assert_eq!(kick.level, 127);
    assert_ne!(
        harness
            .state()
            .playback()
            .track_drum_part_chain(5, DrumPart::Kick),
        harness
            .state()
            .playback()
            .kit_drum_part_chain(DrumPart::Kick),
        "the track plays its own kick, not the machine's kit"
    );
    // Built-in forgets it again: the kit's setting (or the built-in tone) plays.
    harness.get_by_label("Built-in").click();
    harness.run();
    assert!(song_drum_part(&harness, "kick").is_none());
    assert_eq!(
        harness
            .state()
            .playback()
            .track_drum_part_chain(5, DrumPart::Kick),
        harness
            .state()
            .playback()
            .kit_drum_part_chain(DrumPart::Kick),
        "back to the kit's kick"
    );
    // Back to the kit: the track's own chain, the drum bus (empty).
    harness.get_by_label("Kit").click();
    harness.run();
    harness.get_by_label("Block 1: +");
    harness.get_by_label("Close").click();
    harness.run();
}

/// The drum track's kit room in its sound payload (the sixth track).
fn song_room(
    harness: &Harness<'static, GuitarScoreApp>,
) -> Option<gscore_app::chain_settings::BlockSetting> {
    let document = harness.state().document()?;
    let json = document
        .song()
        .track(gscore_model::TrackIndex::new(5))?
        .sound_state()?;
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    serde_json::from_value(value.get("drum_room")?.clone()).ok()
}

#[test]
fn the_kits_room_window_edits_the_room_and_the_default_brings_it_back() {
    // A tall window: the track panel's Sound section (the stomp box) is below the mixer.
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1100.0))
        .build_eframe(|cc| {
            GuitarScoreApp::new(&cc.egui_ctx, Some(testdata("basic/multi_staff.gp")))
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
    harness.state_mut().set_shown_track(5);
    harness.run();
    harness.get_by_label("Sound chain").click();
    harness.run();
    harness.run();
    // The kit's room has its own window, opened from the part row.
    harness.get_by_label("Kit room…").click();
    harness.run();
    harness.get_by_label("Default");
    let before = song_room(&harness);
    assert!(before.is_none(), "the kit room starts as the built-in one");
    let from = harness.get_by_label("Size").rect().center();
    harness.hover_at(from);
    harness.drag_at(from);
    harness.run();
    for step in 1..=4 {
        harness.hover_at(from + egui::vec2(8.0 * step as f32, 0.0));
        harness.run();
    }
    harness.drop_at(from + egui::vec2(32.0, 0.0));
    harness.run();
    let edited = song_room(&harness).expect("the song keeps the kit's room");
    assert!(
        (edited.size - 0.25).abs() > 1e-3,
        "the Size knob turned: {edited:?}"
    );
    // Default puts the built-in short room back.
    harness.get_by_label("Default").click();
    harness.run();
    let default = song_room(&harness).expect("the room is kept");
    assert!((default.size - 0.25).abs() < 1e-6, "{default:?}");
    assert!((default.decay - 0.6).abs() < 1e-6, "{default:?}");
    // Escape closes the window, leaving the chain window open.
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(harness.query_by_label("Default").is_none());
    harness.get_by_label("Close").click();
    harness.run();
}

/// Snapshot options for a picture with anti-aliased curves, whose edge pixels vary a
/// little between GPUs and drivers: a colour tolerance of 2 (YIQ distance) per pixel
/// instead of 0.6, and still no pixel allowed past it (a moved line or border differs by
/// far more).
fn curve_tolerance() -> egui_kittest::SnapshotOptions {
    egui_kittest::SnapshotOptions::new().threshold(2.0)
}
