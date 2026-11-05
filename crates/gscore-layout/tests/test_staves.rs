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

/// Slash notation is a staff of its own: rhythm slashes under a clef, no fret numbers,
/// and it combines with the other two staves.
#[test]
fn slash_notation_is_a_staff_of_its_own_and_combines() {
    let slash = layout(TrackStaves::SLASH_ONLY);
    assert_eq!(fret_numbers(&slash), 0, "no tablature");
    assert!(
        slash.ops.iter().any(|op| matches!(
            op,
            DrawOp::Glyph {
                glyph: Glyph::GClef,
                ..
            }
        )),
        "the slash staff has a clef"
    );
    assert!(
        slash.ops.iter().any(
            |op| matches!(op, DrawOp::Line { thickness, .. } if (*thickness - 0.18).abs() < 1e-6)
        ),
        "a rhythm slash is drawn"
    );

    let tab_and_slash = layout(TrackStaves::TAB_AND_SLASH);
    assert!(fret_numbers(&tab_and_slash) > 0, "the tablature stays");
    assert!(tab_and_slash.height > slash.height);

    let all = layout(TrackStaves::ALL_STAVES);
    assert!(fret_numbers(&all) > 0);
    assert!(all.height > tab_and_slash.height, "notation adds a staff");
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
