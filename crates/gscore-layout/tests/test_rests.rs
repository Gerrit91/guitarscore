//! Multi-bar rests: a run of bars where every shown staff rests is written as one bar with
//! a thick bar and the number of bars above it.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

mod common;

use gscore_layout::{DrawOp, Fonts, Glyph, Layout, LayoutOptions, layout_tracks};
use gscore_model::testing::{self, GUITAR_6};
use gscore_model::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, Song, StaffIndex, StringNumber, SystemBreak,
    TrackColor, TrackIndex, VoiceIndex,
};

/// A song of `tracks` guitar tracks and `bars` bars, with a note on track `track`'s bar
/// `bar` for each `(track, bar)`.
fn song(tracks: usize, bars: usize, notes: &[(usize, usize)]) -> Song {
    let tunings = vec![GUITAR_6; tracks];
    let mut song = testing::song(&tunings, bars).unwrap();
    for &(track, measure) in notes {
        let at = BeatPosition {
            staff: StaffIndex::new(0),
            track: TrackIndex::new(track),
            measure: MeasureIndex::new(measure),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(0),
        };
        let _ = song
            .set_note(at, StringNumber::new(1).unwrap(), Fret::new(3).unwrap())
            .unwrap();
    }
    song
}

fn layout(song: &Song, multi_bar_rests: bool) -> Layout {
    let tracks: Vec<TrackIndex> = song.tracks().map(|track| track.index()).collect();
    let options = LayoutOptions {
        multi_bar_rests,
        ..LayoutOptions::default()
    };
    layout_tracks(
        song,
        &tracks,
        tracks[0],
        &options,
        &Fonts::bundled().unwrap(),
    )
    .unwrap()
}

/// The counts written over multi-bar rests, left to right.
fn counts(layout: &Layout) -> Vec<String> {
    let mut counts = Vec::new();
    let mut current = String::new();
    let mut last_x = f32::NEG_INFINITY;
    for op in &layout.ops {
        if let DrawOp::Glyph {
            glyph: Glyph::TimeSigDigit(digit),
            at,
            ..
        } = op
        {
            // A count's digits follow each other closely; a time signature's are stacked.
            if !current.is_empty() && (at.x - last_x).abs() > 3.0 {
                counts.push(std::mem::take(&mut current));
            }
            current.push(char::from(b'0' + digit.get()));
            last_x = at.x;
        }
    }
    if !current.is_empty() {
        counts.push(current);
    }
    counts
}

/// The bars' places on the page, `(left, right)` per bar.
fn bars(layout: &Layout) -> Vec<(f32, f32)> {
    layout
        .places
        .measures
        .iter()
        .map(|place| (place.left, place.right))
        .collect()
}

#[test]
fn a_run_of_rests_is_one_bar_with_its_count() {
    // Bar 1 plays, bars 2 to 5 rest, bar 6 plays; the time signature of bar 1 is "44".
    let song = song(1, 6, &[(0, 0), (0, 5)]);
    let plain = layout(&song, false);
    assert_eq!(counts(&plain), ["44"], "without: only the time signature");
    let folded = layout(&song, true);
    assert_eq!(counts(&folded), ["44", "4"]);
    // The thick bar.
    assert!(
        folded
            .ops
            .iter()
            .any(|op| matches!(op, DrawOp::Rect { rect, .. } if rect.width > 5.0))
    );
    // Each bar of the run has its own equal slice of the rest, in order, so the cursor
    // finds it there and playback crosses the rest once over its four bars.
    let places = bars(&folded);
    assert_eq!(places.len(), 6);
    let contents: Vec<f32> = folded.places.measures.iter().map(|p| p.content).collect();
    let slice = places[1].1 - contents[1];
    assert!(slice > 1.0, "{slice}");
    for bar in 2..5 {
        let (left, right) = places[bar];
        assert!(
            (left - places[bar - 1].1).abs() < 1e-3,
            "bar {} follows",
            bar + 1
        );
        assert!(
            (right - left - slice).abs() < 1e-3,
            "bar {} is a slice",
            bar + 1
        );
    }
    // The run's last slice ends at the rest's barline, where bar 6 starts.
    assert!((places[4].1 - places[5].0).abs() < 1e-3);
    // The playback cursor moves on bar by bar and beat by beat through the rest.
    let mut last = f32::NEG_INFINITY;
    for bar in 1..5 {
        for beat in 0..4 {
            let rect = folded
                .places
                .column_rect(
                    MeasureIndex::new(bar),
                    VoiceIndex::new(0),
                    BeatIndex::new(beat),
                )
                .expect("a beat of the run");
            assert!(rect.origin.x > last, "bar {}, beat {}", bar + 1, beat + 1);
            last = rect.origin.x;
        }
    }
    // Four bars take the room of one.
    assert!(folded.height <= plain.height);
}

#[test]
fn a_mark_at_a_bars_start_ends_the_run_and_one_bar_is_no_run() {
    let mut song = song(1, 7, &[(0, 0)]);
    // A section at bar 4 splits bars 2-7 into 2-3 and 4-7.
    let _ = song
        .set_section(MeasureIndex::new(3), "Verse", TrackColor::DEFAULT)
        .unwrap();
    assert_eq!(counts(&layout(&song, true)), ["44", "2", "4"]);
    // A single resting bar stays as it is.
    let song = self::song(1, 3, &[(0, 0), (0, 2)]);
    assert_eq!(counts(&layout(&song, true)), ["44"]);
}

#[test]
fn a_double_barline_inside_ends_the_run_and_a_line_break_does_not() {
    let mut song = song(1, 6, &[(0, 0)]);
    let _ = song
        .set_system_break(TrackIndex::new(0), MeasureIndex::new(3), SystemBreak::Line)
        .unwrap();
    assert_eq!(
        counts(&layout(&song, true)),
        ["44", "5"],
        "the line break goes"
    );
    let _ = song.set_double_bar(MeasureIndex::new(2), true).unwrap();
    assert_eq!(counts(&layout(&song, true)), ["44", "2", "3"]);
}

#[test]
fn with_several_tracks_every_track_must_rest() {
    // Track 1 plays bar 1; track 2 plays bar 4: the runs are 2-3 and 5-6.
    let song = song(2, 6, &[(0, 0), (1, 3)]);
    let layout = layout(&song, true);
    // Each track's staff writes the time signature; the count is written on each staff.
    let counts = counts(&layout);
    assert_eq!(
        counts.iter().filter(|count| *count == "2").count(),
        4,
        "{counts:?}"
    );
}
