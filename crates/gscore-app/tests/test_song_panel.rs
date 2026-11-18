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

/// Clicks at `pos` with `modifiers` held for the click (the modifiers stay held until the
/// frame has run, so the widget sees them).
fn click_at(
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
    if modifiers != egui::Modifiers::NONE {
        harness
            .input_mut()
            .events
            .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        harness.run();
    }
}

/// A spot on the sections in the grid's ruler (under the measure numbers) on `measure`.
fn section_band(harness: &Harness<'static, GuitarScoreApp>, measure: usize) -> egui::Pos2 {
    let ruler = harness.get_by_label("Measure ruler").rect();
    ruler.left_top() + egui::vec2(24.0 * measure as f32 + 12.0, 21.0)
}

#[test]
fn a_click_on_a_section_in_the_grid_ruler_selects_all_its_bars() {
    let mut harness = song_with_sections(false);
    // A click on the Verse's name (bar 4, between its start at 3 and the Chorus) selects
    // the whole section, in every track, and puts the cursor on its first bar.
    let pos = section_band(&harness, 3);
    click_at(&mut harness, pos, egui::Modifiers::NONE);
    assert_eq!(
        harness.state().selection(),
        Some(MeasureSelection {
            anchor: 2,
            focus: 4,
            track: None,
        })
    );
    assert_eq!(
        harness
            .state()
            .document()
            .unwrap()
            .editor()
            .cursor()
            .measure,
        MeasureIndex::new(2)
    );
    // A shift click on the Chorus's name extends the selection over the whole Chorus
    // (bars 6-9).
    let pos = section_band(&harness, 6);
    click_at(&mut harness, pos, egui::Modifiers::SHIFT);
    assert_eq!(
        harness.state().selection(),
        Some(MeasureSelection {
            anchor: 2,
            focus: 8,
            track: None,
        })
    );
}

#[test]
fn shift_up_and_down_narrow_a_sections_selection_to_the_cursor_track() {
    let mut harness = song_with_sections(false);
    harness.get_by_label("Song tab").click();
    harness.run();
    harness
        .get_by_label("Select the bars of section Verse")
        .click();
    harness.run();
    assert_eq!(
        harness
            .state()
            .selection()
            .and_then(|selection| selection.track),
        None
    );
    // Every track is selected already: the key narrows the selection to the cursor's
    // track, and the other direction takes in every track again.
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowUp);
    harness.run();
    assert_eq!(
        harness
            .state()
            .selection()
            .and_then(|selection| selection.track),
        Some(0)
    );
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowDown);
    harness.run();
    assert_eq!(
        harness
            .state()
            .selection()
            .and_then(|selection| selection.track),
        None
    );
}

/// The notes of `track`'s `measure` (string and fret).
fn notes_in(
    harness: &Harness<'static, GuitarScoreApp>,
    track: usize,
    measure: usize,
) -> Vec<(u8, u8)> {
    let song = harness.state().document().unwrap().song();
    let position = gscore_model::BeatPosition {
        track: gscore_model::TrackIndex::new(track),
        staff: gscore_model::StaffIndex::new(0),
        measure: MeasureIndex::new(measure),
        voice: gscore_model::VoiceIndex::new(0),
        beat: gscore_model::BeatIndex::new(0),
    };
    let Some(measure) = song.measure_at(position) else {
        return Vec::new();
    };
    measure
        .voices()
        .flat_map(|voice| voice.beats())
        .flat_map(|beat| beat.notes())
        .map(|note| {
            (
                note.string().map(|string| string.get()).unwrap_or(0),
                note.fret().map(|fret| fret.get()).unwrap_or(0),
            )
        })
        .collect()
}

#[test]
fn a_section_copies_and_pastes_its_bars_after_itself_in_every_track() {
    let mut harness = song_with_sections(false);
    // A second track, so the copy proves it takes every track.
    {
        let editor = harness
            .state_mut()
            .document_mut()
            .expect("a document")
            .editor_mut();
        editor
            .execute_command(Command::DuplicateTrack {
                track: gscore_model::TrackIndex::new(0),
            })
            .expect("the track is duplicated");
    }
    harness.run();
    harness.get_by_label("Song tab").click();
    harness.run();
    harness
        .get_by_label("Select the bars of section Verse")
        .click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::C);
    harness.run();
    assert_eq!(
        harness
            .state()
            .document()
            .unwrap()
            .editor()
            .clipboard_text()
            .as_deref(),
        Some("GuitarScore: 3 bars of 2 tracks")
    );
    // Pasting with the selection still on the section puts a copy right after it.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::V);
    harness.run();
    let song = harness.state().document().unwrap().song();
    assert_eq!(song.measure_count(), 12);
    for track in 0..2 {
        assert_eq!(
            notes_in(&harness, track, 5),
            notes_in(&harness, track, 2),
            "track {track}'s copy"
        );
        assert_eq!(notes_in(&harness, track, 7), notes_in(&harness, track, 4));
    }
    assert_eq!(
        harness.state().selection(),
        Some(MeasureSelection {
            anchor: 5,
            focus: 7,
            track: None,
        })
    );
}

#[test]
fn ctrl_shift_c_and_v_copy_every_track_and_ask_where_to_paste() {
    let mut harness = song_with_sections(false);
    // A second track, to prove that copying one track's bars takes every track.
    {
        let editor = harness
            .state_mut()
            .document_mut()
            .expect("a document")
            .editor_mut();
        editor
            .execute_command(Command::DuplicateTrack {
                track: gscore_model::TrackIndex::new(0),
            })
            .expect("the track is duplicated");
    }
    harness.run();
    let grid = harness.get_by_label("Song grid").rect();
    let cell = |measure: usize| grid.left_top() + egui::vec2(24.0 * measure as f32 + 12.0, 12.0);
    // A bar selection in the first track only: bar 3, shift-clicked to bar 4.
    click_at(&mut harness, cell(2), egui::Modifiers::NONE);
    click_at(&mut harness, cell(3), egui::Modifiers::SHIFT);
    assert_eq!(
        harness
            .state()
            .selection()
            .and_then(|selection| selection.track),
        Some(0)
    );
    let ctrl_shift = egui::Modifiers {
        command: true,
        shift: true,
        ..egui::Modifiers::NONE
    };
    harness.key_press_modifiers(ctrl_shift, egui::Key::C);
    harness.run();
    assert_eq!(
        harness
            .state()
            .document()
            .unwrap()
            .editor()
            .clipboard_text()
            .as_deref(),
        Some("GuitarScore: 2 bars of 2 tracks")
    );
    harness.key_press_modifiers(ctrl_shift, egui::Key::V);
    harness.run();
    // The dialog asks where the bars go.
    harness.get_by_label("Before the selection");
    harness.get_by_label("After the selection");
    harness.get_by_label("After the selection").click();
    harness.run();
    harness.get_by_label("Paste").click();
    harness.run();
    let song = harness.state().document().unwrap().song();
    assert_eq!(song.measure_count(), 11);
    for track in 0..2 {
        assert_eq!(
            notes_in(&harness, track, 4),
            notes_in(&harness, track, 2),
            "track {track}'s pasted bar"
        );
        assert_eq!(notes_in(&harness, track, 5), notes_in(&harness, track, 3));
    }
}

#[test]
fn dragging_a_section_name_onto_another_swaps_their_bars() {
    let mut harness = song_with_sections(false);
    // The Verse's bars (3-5) and the Chorus's (6-9), as they are now.
    let verse: Vec<Vec<(u8, u8)>> = (2..=4)
        .map(|measure| notes_in(&harness, 0, measure))
        .collect();
    let chorus: Vec<Vec<(u8, u8)>> = (5..=8)
        .map(|measure| notes_in(&harness, 0, measure))
        .collect();
    // Drag the Verse's name in the ruler onto the Chorus's.
    let from = section_band(&harness, 3);
    let to = section_band(&harness, 6);
    harness.hover_at(from);
    harness.run();
    harness.drag_at(from);
    harness.step();
    harness.hover_at(from + egui::vec2(10.0, 0.0));
    harness.step();
    harness.hover_at(to);
    harness.step();
    harness.drop_at(to);
    harness.run();
    // The two sections trade places, bars and names with them.
    let song = harness.state().document().unwrap().song();
    assert_eq!(
        song.section(MeasureIndex::new(2))
            .map(|section| section.title),
        Some("Chorus".to_owned())
    );
    assert_eq!(
        song.section(MeasureIndex::new(6))
            .map(|section| section.title),
        Some("Verse".to_owned())
    );
    for (index, notes) in chorus.iter().enumerate() {
        assert_eq!(
            &notes_in(&harness, 0, 2 + index),
            notes,
            "the Chorus's bar {index} moved"
        );
    }
    for (index, notes) in verse.iter().enumerate() {
        assert_eq!(
            &notes_in(&harness, 0, 6 + index),
            notes,
            "the Verse's bar {index} moved"
        );
    }
    // The dragged section (the Verse) is selected where it went.
    assert_eq!(
        harness.state().selection(),
        Some(MeasureSelection {
            anchor: 6,
            focus: 8,
            track: None,
        })
    );
}

#[test]
fn the_paste_dialog_matches_its_snapshot() {
    let _gpu = common::lock_gpu();
    let mut harness = song_with_sections(true);
    harness.get_by_label("Song tab").click();
    harness.run();
    harness
        .get_by_label("Select the bars of section Verse")
        .click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::C);
    harness.run();
    let ctrl_shift = egui::Modifiers {
        command: true,
        shift: true,
        ..egui::Modifiers::NONE
    };
    harness.key_press_modifiers(ctrl_shift, egui::Key::V);
    harness.run();
    harness.snapshot("paste_around");
}

/// The center of a section's row in the Song tab's table.
fn section_row(harness: &Harness<'static, GuitarScoreApp>, title: &str) -> egui::Pos2 {
    harness
        .get_by_label(&format!("Select the bars of section {title}"))
        .rect()
        .center()
}

#[test]
fn dragging_a_section_row_in_the_song_tab_swaps_the_sections() {
    let mut harness = song_with_sections(false);
    harness.get_by_label("Song tab").click();
    harness.run();
    // Selecting the Verse first shows the drag dropping that selection.
    harness
        .get_by_label("Select the bars of section Verse")
        .click();
    harness.run();
    assert!(harness.state().selection().is_some());
    // Drag the Chorus's row onto the Verse's.
    let from = section_row(&harness, "Chorus");
    let to = section_row(&harness, "Verse");
    harness.hover_at(from);
    harness.run();
    harness.drag_at(from);
    harness.step();
    harness.hover_at(from + egui::vec2(10.0, 0.0));
    harness.step();
    // While dragging, the old selection is gone: only the dragged section is marked.
    assert_eq!(harness.state().selection(), None);
    harness.hover_at(to);
    harness.step();
    harness.drop_at(to);
    harness.run();
    // The Chorus's bars now start where the Verse did, and the Verse's after them.
    let song = harness.state().document().unwrap().song();
    assert_eq!(
        song.section(MeasureIndex::new(2))
            .map(|section| section.title),
        Some("Chorus".to_owned())
    );
    assert_eq!(
        song.section(MeasureIndex::new(6))
            .map(|section| section.title),
        Some("Verse".to_owned())
    );
    assert_eq!(
        harness.state().selection(),
        Some(MeasureSelection {
            anchor: 2,
            focus: 5,
            track: None,
        })
    );
}

#[test]
fn starting_a_section_drag_in_the_ruler_drops_the_selection() {
    let mut harness = song_with_sections(false);
    // Select the Verse from the ruler, then drag the Chorus.
    let verse = section_band(&harness, 3);
    click_at(&mut harness, verse, egui::Modifiers::NONE);
    assert_eq!(
        harness.state().selection(),
        Some(MeasureSelection {
            anchor: 2,
            focus: 4,
            track: None,
        })
    );
    let from = section_band(&harness, 6);
    let to = section_band(&harness, 3);
    harness.hover_at(from);
    harness.run();
    harness.drag_at(from);
    harness.step();
    harness.hover_at(from + egui::vec2(10.0, 0.0));
    harness.step();
    // Only the dragged section is marked while it moves.
    assert_eq!(harness.state().selection(), None);
    harness.hover_at(to);
    harness.step();
    harness.drop_at(to);
    harness.run();
    // The Chorus was dropped on the Verse, so their bars trade places.
    let song = harness.state().document().unwrap().song();
    assert_eq!(
        song.section(MeasureIndex::new(2))
            .map(|section| section.title),
        Some("Chorus".to_owned())
    );
    assert_eq!(
        song.section(MeasureIndex::new(6))
            .map(|section| section.title),
        Some("Verse".to_owned())
    );
}

#[test]
fn the_paste_dialog_pastes_at_a_typed_bar_and_can_overwrite() {
    let mut harness = song_with_sections(false);
    harness.get_by_label("Song tab").click();
    harness.run();
    harness
        .get_by_label("Select the bars of section Verse")
        .click();
    harness.run();
    let verse: Vec<Vec<(u8, u8)>> = (2..=4)
        .map(|measure| notes_in(&harness, 0, measure))
        .collect();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::C);
    harness.run();
    let ctrl_shift = egui::Modifiers {
        command: true,
        shift: true,
        ..egui::Modifiers::NONE
    };
    harness.key_press_modifiers(ctrl_shift, egui::Key::V);
    harness.run();
    // At bar 6, writing over the bars there: the song keeps its nine bars.
    harness.get_by_label("At bar").click();
    harness.get_by_label("Paste bar").click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.get_by_label("Paste bar").type_text("6");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    harness
        .get_by_label_contains("Overwrite the bars there")
        .click();
    harness.run();
    harness.get_by_label("Paste").click();
    harness.run();
    let song = harness.state().document().unwrap().song();
    assert_eq!(song.measure_count(), 9);
    for (index, notes) in verse.iter().enumerate() {
        assert_eq!(&notes_in(&harness, 0, 5 + index), notes);
    }
    // One track, so the paste selects that track's row.
    assert_eq!(
        harness.state().selection(),
        Some(MeasureSelection {
            anchor: 5,
            focus: 7,
            track: Some(0),
        })
    );
}

#[test]
fn a_section_row_dropped_off_the_table_or_on_itself_swaps_nothing() {
    let mut harness = song_with_sections(false);
    harness.get_by_label("Song tab").click();
    harness.run();
    let before = harness.state().document().unwrap().editor().undo_depth();
    let from = section_row(&harness, "Chorus");
    let over = section_row(&harness, "Verse");
    // Over the Verse's row, then off the table (onto the score) before dropping.
    let off = egui::pos2(200.0, 300.0);
    // And over the Verse's row, then back onto the dragged row itself.
    for drop in [off, from] {
        harness.hover_at(from);
        harness.run();
        harness.drag_at(from);
        harness.step();
        harness.hover_at(from + egui::vec2(10.0, 0.0));
        harness.step();
        harness.hover_at(over);
        harness.step();
        harness.hover_at(drop);
        harness.step();
        harness.drop_at(drop);
        harness.run();
        let song = harness.state().document().unwrap().song();
        assert_eq!(
            song.section(MeasureIndex::new(2))
                .map(|section| section.title),
            Some("Verse".to_owned()),
            "dropped at {drop:?}"
        );
        assert_eq!(
            harness.state().document().unwrap().editor().undo_depth(),
            before
        );
    }
}

/// Starts playback to ALSA's silent "Midi Through" port; false (test skipped) without one.
fn start_playing(harness: &mut Harness<'static, GuitarScoreApp>) -> bool {
    const THROUGH: &str = "Midi Through";
    let playback = harness.state_mut().playback_mut();
    let port = playback.refresh_ports().ok().and_then(|()| {
        playback
            .ports()
            .iter()
            .find(|p| p.contains(THROUGH))
            .cloned()
    });
    let Some(port) = port else {
        eprintln!("skipped: no \"{THROUGH}\" MIDI port");
        return false;
    };
    harness.state_mut().playback_mut().choose_port(Some(port));
    harness.get_by_label("Play").click();
    harness.step();
    let started = Instant::now();
    while harness.state().playback().state() != gscore_play::PlayState::Playing {
        assert!(started.elapsed() < Duration::from_secs(5), "did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

#[test]
fn a_paste_from_the_paste_dialog_stops_playback_like_any_edit() {
    let mut harness = song_with_sections(false);
    harness.get_by_label("Song tab").click();
    harness.run();
    harness
        .get_by_label("Select the bars of section Verse")
        .click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::C);
    harness.run();
    if !start_playing(&mut harness) {
        return;
    }
    assert!(harness.state().playback().sequence().is_some());
    let ctrl_shift = egui::Modifiers {
        command: true,
        shift: true,
        ..egui::Modifiers::NONE
    };
    harness.key_press_modifiers(ctrl_shift, egui::Key::V);
    harness.run();
    harness.get_by_label("Paste").click();
    harness.step();
    assert_eq!(
        harness.state().document().unwrap().song().measure_count(),
        12
    );
    // The old song's sequence is gone: the next play builds the song as it is now.
    assert!(harness.state().playback().sequence().is_none());
    // The player stops on its own thread.
    let stopping = Instant::now();
    while harness.state().playback().state() == gscore_play::PlayState::Playing {
        assert!(stopping.elapsed() < Duration::from_secs(5), "still playing");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A spot in the one track's row of the grid on `measure` (1-based, as the ruler counts).
fn track_row(harness: &Harness<'static, GuitarScoreApp>, measure: usize) -> egui::Pos2 {
    let grid = harness.get_by_label("Song grid").rect();
    grid.left_top() + egui::vec2(24.0 * (measure - 1) as f32 + 12.0, 12.0)
}

/// Drags in the grid from `from` to `to`, a frame per step as a hand would.
fn drag_in_grid(harness: &mut Harness<'static, GuitarScoreApp>, from: egui::Pos2, to: egui::Pos2) {
    harness.hover_at(from);
    harness.run();
    harness.drag_at(from);
    harness.step();
    harness.hover_at(from + egui::vec2(6.0, 0.0));
    harness.step();
    harness.hover_at(to);
    harness.step();
    harness.drop_at(to);
    harness.run();
}

#[test]
fn dragging_the_selected_bars_moves_them_by_as_many_bars() {
    let mut harness = song_with_sections(false);
    let bars: Vec<Vec<(u8, u8)>> = (0..9)
        .map(|measure| notes_in(&harness, 0, measure))
        .collect();
    // Select bars 2-3 by dragging over them.
    let (two, three) = (track_row(&harness, 2), track_row(&harness, 3));
    drag_in_grid(&mut harness, two, three);
    assert_eq!(
        harness.state().selection(),
        Some(MeasureSelection {
            anchor: 1,
            focus: 2,
            track: Some(0),
        })
    );
    // Grab them at bar 3 and drop four bars to the right: they now start at bar 6.
    let seven = track_row(&harness, 7);
    drag_in_grid(&mut harness, three, seven);
    let song = harness.state().document().unwrap().song();
    assert_eq!(song.measure_count(), 9, "bars move, none are added");
    // Bars 4-7 moved left by two; the dragged ones follow them.
    let order = [0, 3, 4, 5, 6, 1, 2, 7, 8];
    for (index, &was) in order.iter().enumerate() {
        assert_eq!(notes_in(&harness, 0, index), bars[was], "bar {}", index + 1);
    }
    // The moved bars stay selected where they went, and the section names moved with
    // their bars: the Verse (bar 3) is at bar 7 now, the Chorus (bar 6) at bar 4.
    assert_eq!(
        harness.state().selection(),
        Some(MeasureSelection {
            anchor: 5,
            focus: 6,
            track: Some(0),
        })
    );
    assert_eq!(
        song.section(MeasureIndex::new(6))
            .map(|section| section.title),
        Some("Verse".to_owned())
    );
    assert_eq!(
        song.section(MeasureIndex::new(3))
            .map(|section| section.title),
        Some("Chorus".to_owned())
    );
    // One undo step puts them back.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run();
    for (measure, notes) in bars.iter().enumerate() {
        assert_eq!(&notes_in(&harness, 0, measure), notes);
    }
}

#[test]
fn while_playing_the_hidden_grid_selection_cannot_be_dragged() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = std::env::temp_dir().join(format!("gscore-grid-playing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sine.sf2");
    std::fs::write(&path, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = song_with_sections(false);
    harness.state_mut().playback_mut().use_soundfont(path);
    let bars: Vec<Vec<(u8, u8)>> = (0..9)
        .map(|measure| notes_in(&harness, 0, measure))
        .collect();
    let (two, three) = (track_row(&harness, 2), track_row(&harness, 3));
    drag_in_grid(&mut harness, two, three);
    assert!(harness.state().selection().is_some());

    harness.state_mut().play_pause();
    let started = Instant::now();
    while harness.state().playback().state() != gscore_play::PlayState::Playing {
        if let Some(notice) = harness.state().notice() {
            eprintln!("skipped: no usable audio device ({notice})");
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(10), "did not start");
        harness.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    // The selection stays (a loop plays it) but is not shown, so a drag from its bars
    // does not move them, as the score hides its selection while playing.
    let seven = track_row(&harness, 7);
    drag_in_grid(&mut harness, three, seven);
    harness.state_mut().play_pause();
    harness.run();
    for (measure, notes) in bars.iter().enumerate() {
        assert_eq!(
            &notes_in(&harness, 0, measure),
            notes,
            "bar {}",
            measure + 1
        );
    }
}

#[test]
fn a_drag_that_starts_outside_the_selection_still_selects() {
    let mut harness = song_with_sections(false);
    let (two, three) = (track_row(&harness, 2), track_row(&harness, 3));
    drag_in_grid(&mut harness, two, three);
    let depth = harness.state().document().unwrap().editor().undo_depth();
    let (five, seven) = (track_row(&harness, 5), track_row(&harness, 7));
    drag_in_grid(&mut harness, five, seven);
    assert_eq!(
        harness.state().selection(),
        Some(MeasureSelection {
            anchor: 4,
            focus: 6,
            track: Some(0),
        })
    );
    assert_eq!(
        harness.state().document().unwrap().editor().undo_depth(),
        depth,
        "nothing moved"
    );
}

#[test]
fn the_paste_dialog_repeats_copies_and_leaves_attributes_out() {
    let mut harness = song_with_sections(false);
    harness.get_by_label("Song tab").click();
    harness.run();
    harness
        .get_by_label("Select the bars of section Verse")
        .click();
    harness.run();
    let verse: Vec<Vec<(u8, u8)>> = (2..=4)
        .map(|measure| notes_in(&harness, 0, measure))
        .collect();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::C);
    harness.run();
    let ctrl_shift = egui::Modifiers {
        command: true,
        shift: true,
        ..egui::Modifiers::NONE
    };
    harness.key_press_modifiers(ctrl_shift, egui::Key::V);
    harness.run();
    // Two copies, without their section markers, after the selection (bars 6-8).
    harness.get_by_label("Paste copies").click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.get_by_label("Paste copies").type_text("2");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    harness.get_by_label("Sections").click();
    harness.run();
    harness.get_by_label("Paste").click();
    harness.run();
    let song = harness.state().document().unwrap().song();
    assert_eq!(
        song.measure_count(),
        15,
        "nine bars and two copies of three"
    );
    // The copies carry no section marker; the Chorus moved right with its own.
    assert!(song.section(MeasureIndex::new(5)).is_none());
    assert_eq!(
        song.section(MeasureIndex::new(11))
            .map(|section| section.title),
        Some("Chorus".to_owned())
    );
    for copy in 0..2 {
        for (index, notes) in verse.iter().enumerate() {
            let measure = 5 + copy * verse.len() + index;
            assert_eq!(&notes_in(&harness, 0, measure), notes, "bar {measure}");
        }
    }
}

#[test]
fn ctrl_shift_insert_opens_the_paste_dialog_too() {
    let mut harness = song_with_sections(false);
    harness.get_by_label("Song tab").click();
    harness.run();
    harness
        .get_by_label("Select the bars of section Verse")
        .click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::C);
    harness.run();
    let ctrl_shift = egui::Modifiers {
        command: true,
        shift: true,
        ..egui::Modifiers::NONE
    };
    harness.key_press_modifiers(ctrl_shift, egui::Key::Insert);
    harness.run();
    harness.get_by_label("After the selection");
}

#[test]
fn a_paste_event_with_shift_opens_the_paste_dialog() {
    // Some platforms send Ctrl+Shift+V as a paste event rather than a key; the held
    // Shift still asks for the dialog.
    let mut harness = song_with_sections(false);
    harness.get_by_label("Song tab").click();
    harness.run();
    harness
        .get_by_label("Select the bars of section Verse")
        .click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::C);
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers {
            shift: true,
            ..egui::Modifiers::NONE
        }));
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Paste("GuitarScore: 3 bars".to_owned()));
    harness.run();
    harness.get_by_label("After the selection");
    harness
        .input_mut()
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    harness.run();
}

/// Selects from the cursor over the first bar lines in the score, as Shift+Right does: a
/// beat selection, not a bar selection.
fn select_beats_in_the_score(harness: &mut Harness<'static, GuitarScoreApp>) {
    for _ in 0..12 {
        harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowRight);
        harness.run();
    }
    assert!(matches!(
        harness.state().document().unwrap().editor().selection(),
        Some(Selection::Beats { .. })
    ));
}

#[test]
fn ctrl_shift_c_on_a_score_selection_copies_its_bars_for_the_paste_dialog() {
    let mut harness = song_with_sections(false);
    select_beats_in_the_score(&mut harness);
    let ctrl_shift = egui::Modifiers {
        command: true,
        shift: true,
        ..egui::Modifiers::NONE
    };
    harness.key_press_modifiers(ctrl_shift, egui::Key::C);
    harness.run();
    // The beats from bar 1 into bar 3: those three bars, whole.
    assert_eq!(
        harness
            .state()
            .document()
            .unwrap()
            .editor()
            .clipboard_bars(),
        Some(3)
    );
    harness.key_press_modifiers(ctrl_shift, egui::Key::V);
    harness.run();
    harness.get_by_label("After the selection");
}

#[test]
fn the_paste_dialog_key_with_beats_copied_says_how_to_copy_bars_and_changes_nothing() {
    let mut harness = song_with_sections(false);
    select_beats_in_the_score(&mut harness);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::C);
    harness.run();
    let depth = harness.state().document().unwrap().editor().undo_depth();
    let ctrl_shift = egui::Modifiers {
        command: true,
        shift: true,
        ..egui::Modifiers::NONE
    };
    harness.key_press_modifiers(ctrl_shift, egui::Key::V);
    harness.run();
    assert!(harness.query_by_label("After the selection").is_none());
    assert_eq!(
        harness.state().document().unwrap().editor().undo_depth(),
        depth,
        "nothing was pasted"
    );
    assert!(
        harness
            .state()
            .status()
            .is_some_and(|status| status.contains("Ctrl+Shift+C")),
        "{:?}",
        harness.state().status()
    );
}
