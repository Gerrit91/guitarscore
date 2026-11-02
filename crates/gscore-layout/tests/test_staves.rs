//! A track's staves switch: notation and tab, tab only, or notation only.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

use gscore_layout::{ColorToken, DrawOp, Fonts, Layout, LayoutOptions, layout_track};
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
    let both = layout(TrackStaves::NotationAndTab);
    let tab = layout(TrackStaves::TabOnly);
    let notation = layout(TrackStaves::NotationOnly);

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
