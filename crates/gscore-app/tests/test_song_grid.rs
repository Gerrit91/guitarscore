//! The song grid's scroll bars: a song wider than the grid shows its horizontal scroll bar
//! at the bottom of the panel, whatever the number of tracks, without the pointer having to
//! be over the grid first, and the bar scrolls the measures.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gscore_app::GuitarScoreApp;
use gscore_edit::Command;
use gscore_model::{MeasureIndex, TrackIndex};

mod common;

const WINDOW: egui::Vec2 = egui::vec2(1000.0, 700.0);

/// `note_lengths.gp` grown to `tracks` tracks and 60 measures: far wider than the grid,
/// and (beyond eight tracks) taller than the panel opens.
fn wide_song(tracks: usize) -> Harness<'static, GuitarScoreApp> {
    let file =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/basic/note_lengths.gp");
    let mut harness = Harness::builder()
        .with_size(WINDOW)
        .with_max_steps(64)
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(file)));
    let started = Instant::now();
    harness.run();
    while harness.state().is_busy() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    {
        let editor = harness.state_mut().document_mut().unwrap().editor_mut();
        for _ in 1..tracks {
            editor
                .execute_command(Command::DuplicateTrack {
                    track: TrackIndex::new(0),
                })
                .unwrap();
        }
        while editor.song().measure_count() < 60 {
            editor
                .execute_command(Command::InsertMeasure {
                    measure: MeasureIndex::new(0),
                })
                .unwrap();
        }
    }
    harness.run();
    harness
}

fn song_grid(harness: &Harness<'static, GuitarScoreApp>) -> egui::Rect {
    harness.get_by_label("Song grid").rect()
}

/// The song grid's horizontal scroll bar: a wide scroll bar below the grid's top, across
/// its columns.
fn horizontal_bar(harness: &Harness<'static, GuitarScoreApp>) -> egui::Rect {
    let grid = song_grid(harness);
    harness
        .query_all_by_role(Role::ScrollBar)
        .map(|node| node.rect())
        .find(|bar| {
            bar.width() > bar.height() && bar.top() > grid.top() && bar.left() >= grid.left() - 1.0
        })
        .expect("the song grid has a horizontal scroll bar")
}

fn park_pointer(harness: &mut Harness<'static, GuitarScoreApp>) {
    harness.hover_at(egui::pos2(500.0, 100.0));
    harness.run();
}

fn bar_is_on_screen_and_scrolls(tracks: usize) {
    let mut harness = wide_song(tracks);
    park_pointer(&mut harness);
    let bar = horizontal_bar(&harness);
    assert!(
        bar.bottom() <= WINDOW.y && bar.top() >= 0.0,
        "with {tracks} tracks the bar is at {bar:?}, outside the {WINDOW:?} window"
    );
    // Dragging the bar's handle (at the left, the grid being at its start) to the right
    // scrolls the measures left.
    let before = song_grid(&harness).left();
    let from = egui::pos2(bar.left() + 8.0, bar.center().y);
    harness.hover_at(from);
    harness.run();
    harness.drag_at(from);
    harness.step();
    harness.hover_at(from + egui::vec2(200.0, 0.0));
    harness.step();
    harness.drop_at(from + egui::vec2(200.0, 0.0));
    harness.run();
    let after = song_grid(&harness).left();
    assert!(
        after < before - 100.0,
        "with {tracks} tracks dragging the bar moved the grid from {before} to {after}"
    );
}

#[test]
fn the_horizontal_bar_scrolls_a_song_with_few_tracks() {
    bar_is_on_screen_and_scrolls(2);
}

/// More tracks than the panel shows: the bar stays at the panel's bottom instead of below
/// the last track, out of sight until the tracks are scrolled down.
#[test]
fn the_horizontal_bar_stays_in_sight_with_many_tracks() {
    bar_is_on_screen_and_scrolls(14);
}

/// The bar is drawn while the pointer is elsewhere: its handle (at the left) looks different
/// from the bar's empty end (at the right).
#[test]
fn the_horizontal_bar_shows_without_hovering() {
    let _gpu = common::lock_gpu();
    let mut harness = wide_song(2);
    park_pointer(&mut harness);
    let bar = horizontal_bar(&harness);
    let image = harness.render().expect("rendered");
    let ppp = harness.ctx.pixels_per_point();
    let pixel = |x: f32| {
        let px = (x * ppp) as u32;
        let py = (bar.center().y * ppp) as u32;
        *image.get_pixel(px, py)
    };
    let handle = pixel(bar.left() + 4.0);
    let end = pixel(bar.right() - 4.0);
    assert_ne!(
        handle, end,
        "the bar's handle and its empty end look the same: the bar is not drawn"
    );
}

fn wheel(harness: &mut Harness<'static, GuitarScoreApp>, at: egui::Pos2, down: f32) {
    harness.hover_at(at);
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -down),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();
}

/// How far the last track's row in the track column is from its row in the matrix.
fn last_row_offset(harness: &Harness<'static, GuitarScoreApp>, tracks: usize) -> f32 {
    let row = harness
        .query_by_label_contains(&format!("Track {tracks}: "))
        .expect("the last track's row")
        .rect();
    let in_matrix = song_grid(harness).top() + (tracks - 1) as f32 * 24.0;
    row.top() - in_matrix
}

/// The track column has no scroll bar of its own: it scrolls with the matrix, from a
/// wheel over either, and its rows stay beside theirs in the matrix, down to the last.
#[test]
fn the_track_column_scrolls_with_the_matrix() {
    let tracks = 14;
    let mut harness = wide_song(tracks);
    park_pointer(&mut harness);
    let top = song_grid(&harness).top();
    assert!(last_row_offset(&harness, tracks).abs() < 0.5);
    let column = harness
        .query_by_label_contains("Track 1: ")
        .unwrap()
        .rect()
        .center();
    for at in [
        song_grid(&harness).left_top() + egui::vec2(40.0, 40.0),
        column,
    ] {
        wheel(&mut harness, at, 1000.0);
        assert!(song_grid(&harness).top() < top - 100.0, "scrolled down");
        let offset = last_row_offset(&harness, tracks);
        assert!(
            offset.abs() < 0.5,
            "the rows are {offset} px off the matrix"
        );
        // Scrolled to the end, the last row is above the horizontal bar.
        let row = harness
            .query_by_label_contains(&format!("Track {tracks}: "))
            .unwrap()
            .rect();
        assert!(row.bottom() <= horizontal_bar(&harness).top() - 24.0 + 0.5);
        wheel(&mut harness, at, -1000.0);
        assert!(
            (song_grid(&harness).top() - top).abs() < 0.5,
            "back at the top"
        );
    }
}

/// The ruler ends with the matrix, before its vertical scroll bar, not above the bar.
#[test]
fn the_ruler_ends_at_the_vertical_bar() {
    let mut harness = wide_song(14);
    park_pointer(&mut harness);
    let grid = song_grid(&harness);
    let vertical = harness
        .query_all_by_role(Role::ScrollBar)
        .map(|node| node.rect())
        .find(|bar| {
            bar.height() > bar.width() && bar.left() > grid.left() && bar.top() >= grid.top()
        })
        .expect("the song grid has a vertical scroll bar");
    // The matrix ends 4 px (the solid bar's inner margin) before its bar; so does the ruler.
    let ruler = harness.get_by_label("Measure ruler").rect();
    assert!(
        ruler.right() <= vertical.left() && ruler.right() >= vertical.left() - 4.5,
        "the ruler ends at {}, the bar starts at {}",
        ruler.right(),
        vertical.left()
    );
}
