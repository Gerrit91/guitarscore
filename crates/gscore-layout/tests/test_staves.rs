//! A track's staves switch: notation and tab, tab only, or notation only.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

mod common;

use gscore_layout::{ColorToken, DrawOp, Fonts, Glyph, Layout, LayoutOptions, layout_track};
use gscore_model::testing::{self, GUITAR_6};
use gscore_model::{Fret, MeasureIndex, Song, StringNumber, TrackIndex, TrackStaves};

fn song_with(staves: TrackStaves) -> Song {
    let mut song = testing::song(&[GUITAR_6], 2).unwrap();
    let at = gscore_model::BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: gscore_model::VoiceIndex::new(0),
        beat: gscore_model::BeatIndex::new(0),
    };
    let _note = song
        .set_note(at, StringNumber::new(2).unwrap(), Fret::new(5).unwrap())
        .unwrap();
    let _staves = song.set_track_staves(TrackIndex::new(0), staves).unwrap();
    song
}

fn layout(staves: TrackStaves) -> Layout {
    let song = song_with(staves);
    let fonts = Fonts::bundled().unwrap();
    layout_track(&song, TrackIndex::new(0), &LayoutOptions::default(), &fonts).unwrap()
}

/// Fret numbers on the page.
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

#[test]
fn each_choice_shows_its_staves() {
    let both = layout(TrackStaves::NOTATION_AND_TAB);
    let tab = layout(TrackStaves::TAB_ONLY);
    let notation = layout(TrackStaves::NOTATION_ONLY);

    assert!(fret_numbers(&both) > 0);
    assert_eq!(fret_numbers(&tab), fret_numbers(&both));
    assert_eq!(fret_numbers(&notation), 0, "no tablature at all");
    // Each single staff takes less room than both.
    assert!(tab.height < both.height);
    assert!(notation.height < both.height);

    // With notation only, the cursor rows lie on the notation staff.
    let system = &notation.places.systems[0];
    assert!((system.tab_top - system.top).abs() < 1e-4);
    assert_eq!(system.strings, 6);
    let both_system = &both.places.systems[0];
    assert!(system.tab_bottom() < both_system.tab_top);
}

/// A tie within a measure draws its curve on the slash staff too.
#[test]
fn slash_notation_shows_a_tie_within_the_measure() {
    let mut song = testing::song(&[GUITAR_6], 1).unwrap();
    let at = |beat| gscore_model::BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: gscore_model::VoiceIndex::new(0),
        beat: gscore_model::BeatIndex::new(beat),
    };
    let _ = song
        .set_note(at(0), StringNumber::new(1).unwrap(), Fret::new(5).unwrap())
        .unwrap();
    let _ = song
        .set_note(at(1), StringNumber::new(1).unwrap(), Fret::new(5).unwrap())
        .unwrap();
    let _ = song
        .set_note_tie(at(1), StringNumber::new(1).unwrap(), true)
        .unwrap();
    let _ = song.set_track_staves(TrackIndex::new(0), TrackStaves::SLASH_ONLY);
    let laid = slash_layout(&song);
    let curves = laid
        .ops
        .iter()
        .filter(|op| matches!(op, DrawOp::Curve { .. }))
        .count();
    assert!(curves >= 1, "the tied slashes are joined by a curve");
}

/// A tie across a barline reaches the next measure's first slash.
#[test]
fn slash_notation_shows_a_tie_across_a_barline() {
    let mut song = testing::song(&[GUITAR_6], 2).unwrap();
    let at = |measure, beat| gscore_model::BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(measure),
        voice: gscore_model::VoiceIndex::new(0),
        beat: gscore_model::BeatIndex::new(beat),
    };
    let _ = song
        .set_note(
            at(0, 3),
            StringNumber::new(1).unwrap(),
            Fret::new(5).unwrap(),
        )
        .unwrap();
    let _ = song
        .set_note(
            at(1, 0),
            StringNumber::new(1).unwrap(),
            Fret::new(5).unwrap(),
        )
        .unwrap();
    let _ = song
        .set_note_tie(at(1, 0), StringNumber::new(1).unwrap(), true)
        .unwrap();
    let _ = song.set_track_staves(TrackIndex::new(0), TrackStaves::SLASH_ONLY);
    let laid = slash_layout(&song);
    let boundary = laid.places.measures[1].left;
    let crossing = laid
        .ops
        .iter()
        .filter(|op| match op {
            DrawOp::Curve { from, to, .. } => from.x < boundary && to.x > boundary,
            _ => false,
        })
        .count();
    assert!(
        crossing >= 1,
        "the tie crosses the barline on the slash staff"
    );
}

fn slash_layout(song: &Song) -> Layout {
    let fonts = Fonts::bundled().unwrap();
    layout_track(song, TrackIndex::new(0), &LayoutOptions::default(), &fonts).unwrap()
}

/// A tab-only staff still shows the tuplets the notation staff would carry.
#[test]
fn tab_only_shows_tuplet_numbers() {
    let song = common::open("basic/note_lengths.gp");
    let layout = common::layout_tab_only(&song, 0);
    let texts: Vec<&str> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    for tuplet in ["3", "5", "6", "7"] {
        assert!(
            texts.contains(&tuplet),
            "missing tuplet {tuplet:?} in the tab: {texts:?}"
        );
    }
}

/// A single staff shown as several rows (standard notation and TAB) is joined by a square
/// bracket at the system's start, like Guitar Pro draws it; one row gets none.
#[test]
fn a_staff_shown_as_notation_and_tab_gets_a_square_bracket() {
    let bracket_lines = |layout: &Layout| -> Vec<(gscore_layout::Point, gscore_layout::Point)> {
        layout
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Line {
                    from,
                    to,
                    thickness,
                    color: ColorToken::Barline,
                } if (*thickness - 0.22).abs() < 1e-6 => Some((*from, *to)),
                _ => None,
            })
            .collect()
    };
    let both = layout(TrackStaves::NOTATION_AND_TAB);
    let lines = bracket_lines(&both);
    assert_eq!(lines.len(), 3, "a vertical line and two hooks: {lines:?}");
    let vertical = lines
        .iter()
        .find(|(from, to)| (from.x - to.x).abs() < 1e-6)
        .expect("the bracket's vertical line");
    assert!(vertical.1.y - vertical.0.y > 8.0, "it spans both rows");
    for (from, to) in &lines {
        if (from.x - to.x).abs() < 1e-6 {
            continue;
        }
        assert!((from.y - to.y).abs() < 1e-6, "a horizontal hook");
        assert!(to.x > from.x, "the hooks reach towards the staves");
    }
    // One row alone has no bracket.
    let tab = layout(TrackStaves::TAB_ONLY);
    assert!(bracket_lines(&tab).is_empty());
    let notation = layout(TrackStaves::NOTATION_ONLY);
    assert!(bracket_lines(&notation).is_empty());
}

/// Slash notation is a staff of its own: rhythm slashes on one line, without a clef, and
/// it combines with the other two staves as the top one, joined to them at the start.
#[test]
fn slash_notation_is_a_staff_of_its_own_and_combines() {
    let is_slash = |op: &DrawOp| matches!(op, DrawOp::Line { thickness, .. } if (*thickness - 0.24).abs() < 1e-6);
    let slash = layout(TrackStaves::SLASH_ONLY);
    assert_eq!(fret_numbers(&slash), 0, "no tablature");
    assert!(
        !slash.ops.iter().any(|op| matches!(
            op,
            DrawOp::Glyph {
                glyph: Glyph::GClef,
                ..
            }
        )),
        "the slash staff has no clef"
    );
    assert!(slash.ops.iter().any(is_slash), "a rhythm slash is drawn");

    let tab_and_slash = layout(TrackStaves::TAB_AND_SLASH);
    assert!(fret_numbers(&tab_and_slash) > 0, "the tablature stays");
    assert!(tab_and_slash.height > slash.height);

    let all = layout(TrackStaves::ALL_STAVES);
    assert!(fret_numbers(&all) > 0);
    assert!(all.height > tab_and_slash.height, "notation adds a staff");
    // The slashes are above the notation's clef, and the tab's numbers below it.
    let lowest_slash = all
        .ops
        .iter()
        .filter(|op| is_slash(op))
        .filter_map(|op| match op {
            DrawOp::Line { from, to, .. } => Some(from.y.max(to.y)),
            _ => None,
        })
        .fold(f32::MIN, f32::max);
    let clef = all
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::Glyph {
                glyph: Glyph::GClef,
                at,
                ..
            } => Some(at.y),
            _ => None,
        })
        .unwrap();
    assert!(lowest_slash < clef - 4.0, "the slash staff is on top");
    // One line at the system's start joins the slash staff to the tab below.
    let lowest_fret = all
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text {
                at,
                color: ColorToken::FretNumber,
                ..
            } => Some(at.y),
            _ => None,
        })
        .fold(f32::MIN, f32::max);
    let start = all.ops.iter().find(|op| {
        matches!(op, DrawOp::Line { from, to, color: ColorToken::Barline, .. }
            if (from.x - to.x).abs() < 1e-3
                && from.y.min(to.y) < lowest_slash - 1.0
                && from.y.max(to.y) > lowest_fret)
    });
    assert!(
        start.is_some(),
        "a line joins the slash staff to the others"
    );
}

/// The horizontal screen view puts every measure on one line and widens the page to it.
#[test]
fn a_single_line_layout_is_wider_than_the_page() {
    let song = common::open("basic/note_lengths.gp");
    let fonts = Fonts::bundled().unwrap();
    let endless = |single_line| {
        let options = LayoutOptions {
            single_line,
            ..LayoutOptions::default()
        };
        layout_track(&song, TrackIndex::new(0), &options, &fonts).unwrap()
    };
    let broken = endless(false);
    let one_line = endless(true);
    assert!(broken.places.systems.len() > 1, "the score wraps normally");
    assert_eq!(one_line.places.systems.len(), 1, "everything on one line");
    assert!(
        one_line.width > broken.width,
        "the line is as wide as the whole score: {} vs {}",
        one_line.width,
        broken.width
    );
}
