//! The track panel's two tabs: the Song tab (the song's information and its section list,
//! whose rows select all their bars) and the Track tab.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gscore_app::{GuitarScoreApp, MeasureSelection};
use gscore_edit::{Command, Selection};
use gscore_model::{MeasureIndex, Section};

mod common;

/// `note_lengths.gp` (nine bars, one track) with sections at bars 1, 3 and 6, rendered
/// offscreen when `gpu` (for a snapshot).
fn song_with_sections(gpu: bool) -> Harness<'static, GuitarScoreApp> {
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/basic/note_lengths.gp");
    let builder = Harness::builder().with_size(egui::vec2(1000.0, 700.0));
    let builder = if gpu { builder.wgpu() } else { builder };
    let mut harness = builder.build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(file)));
    let started = Instant::now();
    harness.run();
    while harness.state().is_busy() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    {
        let editor = harness
            .state_mut()
            .document_mut()
            .expect("a document")
            .editor_mut();
        for (measure, title) in [(0, "Intro"), (2, "Verse"), (5, "Chorus")] {
            editor
                .execute_command(Command::SetSection {
                    measure: MeasureIndex::new(measure),
                    title: title.to_owned(),
                    color: Section::DEFAULT_COLOR,
                })
                .expect("the section is set");
        }
    }
    harness.run();
    harness
}

#[test]
fn the_panel_offers_the_song_and_the_track_and_the_song_has_its_sections() {
    let mut harness = song_with_sections(false);
    // The track's settings are shown at first: the panel opens on the Track tab.
    harness.get_by_label("Track tab");
    harness.get_by_label("Instrument");
    assert!(harness.query_by_label("Title field").is_none());
    // The Song tab has the song's information and one row per section, each with its bars.
    harness.get_by_label("Song tab").click();
    harness.run();
    harness.get_by_label("Title field");
    harness.get_by_label("Artist field");
    assert!(harness.query_by_label("Instrument").is_none());
    harness.get_by_label("Select the bars of section Intro");
    harness.get_by_label("Select the bars of section Verse");
    harness.get_by_label("Select the bars of section Chorus");
    // And back: the track's settings are as they were.
    harness.get_by_label("Track tab").click();
    harness.run();
    harness.get_by_label("Instrument");
}

#[test]
fn a_section_row_selects_all_its_bars_in_every_track() {
    let mut harness = song_with_sections(false);
    harness.get_by_label("Song tab").click();
    harness.run();
    // The Verse lasts from bar 3 to the bar before the Chorus (bar 5).
    harness
        .get_by_label("Select the bars of section Verse")
        .click();
    harness.run();
    let app = harness.state();
    assert_eq!(
        app.selection(),
        Some(MeasureSelection {
            anchor: 2,
            focus: 4,
            track: None,
        })
    );
    let editor = app.document().expect("a document").editor();
    assert_eq!(
        editor.selection(),
        Some(Selection::Measures {
            anchor: MeasureIndex::new(2),
            focus: MeasureIndex::new(4),
            track: None,
        })
    );
    // The cursor is on the section's first bar, so the score shows it.
    assert_eq!(editor.cursor().measure, MeasureIndex::new(2));
    // The last section runs to the song's last bar.
    harness
        .get_by_label("Select the bars of section Chorus")
        .click();
    harness.run();
    assert_eq!(
        harness.state().selection(),
        Some(MeasureSelection {
            anchor: 5,
            focus: 8,
            track: None,
        })
    );
}

#[test]
fn the_song_tab_edits_the_song_and_undoes_a_field_as_one_step() {
    let mut harness = song_with_sections(false);
    harness.get_by_label("Song tab").click();
    harness.run();
    harness.get_by_label("Title field").click();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.run();
    harness.get_by_label("Title field").type_text("A new title");
    harness.run();
    assert_eq!(
        harness.state().document().unwrap().song().title(),
        "A new title"
    );
    // Enter leaves the field; Undo then takes the whole typing back, not one letter.
    harness.key_press(egui::Key::Enter);
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run();
    assert_eq!(
        harness.state().document().unwrap().song().title(),
        "Note Lengths"
    );
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Y);
    harness.run();
    assert_eq!(
        harness.state().document().unwrap().song().title(),
        "A new title"
    );
}

#[test]
fn the_song_tab_matches_its_snapshot() {
    let _gpu = common::lock_gpu();
    let mut harness = song_with_sections(true);
    harness.get_by_label("Song tab").click();
    harness.run();
    harness.snapshot("song_panel");
}
