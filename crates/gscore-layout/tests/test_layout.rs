//! Properties every layout must have, whatever the song: no panic, finite coordinates on
//! the page, one fret number per note, one staff line per string.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

mod common;

use gscore_layout::{
    ColorToken, DrawOp, Fonts, Layout, LayoutError, LayoutOptions, Point, layout_track,
};
use gscore_model::testing::{self, BASS_4, GUITAR_6, GUITAR_7, GUITAR_8};
use gscore_model::{BeatKind, NoteKind, Song, TrackIndex};

/// Every point of every operation.
fn points(op: &DrawOp) -> Vec<Point> {
    match op {
        DrawOp::Line { from, to, .. } => vec![*from, *to],
        DrawOp::Rect { rect, .. } => vec![
            rect.origin,
            Point::new(rect.origin.x + rect.width, rect.origin.y + rect.height),
        ],
        DrawOp::Glyph { at, .. } | DrawOp::Text { at, .. } => vec![*at],
    }
}

/// Everything lies on the page and no number is NaN or infinite.
fn assert_on_page(layout: &Layout, what: &str) {
    assert!(
        layout.width.is_finite() && layout.height.is_finite(),
        "{what}"
    );
    for op in &layout.ops {
        for p in points(op) {
            assert!(p.x.is_finite() && p.y.is_finite(), "{what}: {op:?}");
            assert!(
                (0.0..=layout.width).contains(&p.x) && (0.0..=layout.height).contains(&p.y),
                "{what}: {op:?} is off the page ({} x {})",
                layout.width,
                layout.height
            );
        }
    }
}

/// The notes layout should show as fret numbers.
fn shown_notes(song: &Song, track: usize) -> usize {
    let track = song.track(TrackIndex::new(track)).unwrap();
    track
        .measures()
        .flat_map(|m| m.voices())
        .flat_map(|v| v.beats())
        .filter(|b| b.kind() == BeatKind::Normal)
        .flat_map(|b| b.notes())
        .filter(|n| matches!(n.kind(), NoteKind::Normal | NoteKind::Tie | NoteKind::Dead))
        .count()
}

fn fret_numbers(layout: &Layout) -> usize {
    layout
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                DrawOp::Text {
                    color: ColorToken::FretNumber,
                    ..
                }
            )
        })
        .count()
}

fn staff_lines(layout: &Layout) -> usize {
    layout
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                DrawOp::Line {
                    color: ColorToken::Staff,
                    ..
                }
            )
        })
        .count()
}

/// Rows of staff lines (distinct y positions), which is strings times systems.
fn systems(layout: &Layout, strings: usize) -> usize {
    staff_lines(layout) / strings
}

#[test]
fn every_test_file_lays_out_on_the_page_with_one_fret_number_per_note() {
    for file in common::TESTDATA {
        let song = common::open(file);
        for track in 0..song.track_count() {
            let layout = common::layout(&song, track);
            let what = format!("{file} track {}", track + 1);
            assert_on_page(&layout, &what);
            assert_eq!(fret_numbers(&layout), shown_notes(&song, track), "{what}");
            assert_eq!(
                layout,
                common::layout(&song, track),
                "{what}: not deterministic"
            );
        }
    }
}

#[test]
fn every_string_count_gets_one_line_per_string() {
    for tuning in [BASS_4, GUITAR_6, GUITAR_7, GUITAR_8] {
        let song = testing::song(&[tuning], 3).unwrap();
        let layout = common::layout(&song, 0);
        assert_on_page(&layout, &format!("{} strings", tuning.len()));
        assert_eq!(staff_lines(&layout) % tuning.len(), 0);
        assert_eq!(
            systems(&layout, tuning.len()),
            1,
            "three empty measures fit one line"
        );
    }
}

#[test]
fn long_songs_break_into_lines_that_fit_the_page() {
    let song = testing::song(&[GUITAR_6], 200).unwrap();
    let layout = common::layout(&song, 0);
    assert_on_page(&layout, "200 measures");
    assert!(systems(&layout, 6) > 10, "{} systems", systems(&layout, 6));
    let right_margin = LayoutOptions::default().page_width - LayoutOptions::default().margin_x;
    for op in &layout.ops {
        if let DrawOp::Line { to, .. } = op {
            assert!(to.x <= right_margin + 1e-3, "{op:?} runs into the margin");
        }
    }
}

#[test]
fn songs_without_tracks_or_measures_are_handled() {
    let fonts = Fonts::bundled().unwrap();
    let options = LayoutOptions::default();
    let missing = layout_track(&testing::empty(), TrackIndex::new(0), &options, &fonts);
    assert_eq!(missing, Err(LayoutError::NoSuchTrack(TrackIndex::new(0))));
    let one = testing::one_measure().unwrap();
    assert!(layout_track(&one, TrackIndex::new(1), &options, &fonts).is_err());

    let no_measures = testing::song(&[GUITAR_6], 0).unwrap();
    let layout = common::layout(&no_measures, 0);
    assert_on_page(&layout, "no measures");
    assert_eq!(staff_lines(&layout), 0, "no measures, no staff");
}

#[test]
fn odd_options_do_not_panic() {
    let fonts = Fonts::bundled().unwrap();
    let song = common::open("basic/note_lengths.gp");
    for options in [
        LayoutOptions {
            page_width: 1.0,
            ..LayoutOptions::default()
        },
        LayoutOptions {
            page_width: 0.0,
            margin_x: 100.0,
            ..LayoutOptions::default()
        },
        LayoutOptions {
            tab_line_spacing: f32::NAN,
            fret_size: f32::INFINITY,
            ..LayoutOptions::default()
        },
        LayoutOptions {
            spacing: gscore_layout::Spacing {
                quarter: -5.0,
                doubling_ratio: 0.0,
                min_space: f32::NAN,
                min_gap: -1.0,
            },
            ..LayoutOptions::default()
        },
    ] {
        // Nonsense settings may give a nonsense picture, but never a panic.
        let _ = layout_track(&song, TrackIndex::new(0), &options, &fonts);
    }
}

#[test]
fn unreadable_notes_show_as_question_marks_instead_of_failing_the_track() {
    // A negative fret is shown as `?`; a note on a 9th string of a 6-string guitar has no
    // line and is left out. Both are listed by the model as issues.
    let song = testing::one_beat_with_raw_notes(&[(1, -7), (2, 5), (9, 3)]).unwrap();
    assert_eq!(song.issues().len(), 2, "{:?}", song.issues());
    let layout = common::layout(&song, 0);
    assert_on_page(&layout, "odd notes");
    let texts: Vec<&str> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text {
                text,
                color: ColorToken::FretNumber,
                ..
            } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["?", "5"]);
}
