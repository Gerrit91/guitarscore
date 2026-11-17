//! Multi-bar rests: a run of bars where every shown staff rests is written as one bar with
//! a thick bar and the number of bars above it.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

mod common;

use gscore_layout::{ColorToken, DrawOp, Fonts, Glyph, Layout, LayoutOptions, layout_tracks};
use gscore_model::testing::{self, GUITAR_6};
use gscore_model::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, Repeat, Song, StaffIndex, StringNumber,
    SystemBreak, TrackColor, TrackIndex, VoiceIndex,
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

/// The right end of the thick bar of a multi-bar rest.
fn rest_bar_right(layout: &Layout) -> f32 {
    layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Rect { rect, .. } if rect.height < 2.0 && rect.width > 5.0 => {
                Some(rect.origin.x + rect.width)
            }
            _ => None,
        })
        .fold(f32::NEG_INFINITY, f32::max)
}

/// The vertical barline strokes as `(center x, thickness)`, one per stroke (a stroke is
/// drawn once per shown staff).
fn barlines(layout: &Layout) -> Vec<(f32, f32)> {
    let mut strokes: Vec<(f32, f32)> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Line {
                from,
                to,
                thickness,
                color: ColorToken::Barline,
            } if (from.x - to.x).abs() < 1e-3 && (to.y - from.y).abs() > 1.0 => {
                Some((from.x, *thickness))
            }
            _ => None,
        })
        .collect();
    strokes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    strokes.dedup_by(|a, b| (a.0 - b.0).abs() < 0.01);
    strokes
}

/// The visible gap from the right end of a multi-bar rest's thick bar to the left edge of
/// the barline that closes it.
fn gap_to_closing_barline(layout: &Layout) -> f32 {
    let right = rest_bar_right(layout);
    let ink = barlines(layout)
        .into_iter()
        .filter(|&(x, _)| x > right)
        .map(|(x, thickness)| x - thickness / 2.0)
        .fold(f32::INFINITY, f32::min);
    ink - right
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

#[test]
fn the_thick_bar_keeps_clear_of_a_final_or_double_barline() {
    // Bars 2-3 rest and bar 4 plays: the run ends at a plain thin barline.
    let plain = layout(&song(1, 4, &[(0, 0), (0, 3)]), true);
    let plain_gap = gap_to_closing_barline(&plain);
    assert!(plain_gap > 1.0, "{plain_gap}");
    // The same run as the score's last bars ends at the thin-thick final barline...
    let final_run = layout(&song(1, 4, &[(0, 0)]), true);
    let final_gap = gap_to_closing_barline(&final_run);
    // ...or at a double barline.
    let mut song = song(1, 4, &[(0, 0), (0, 3)]);
    let _ = song.set_double_bar(MeasureIndex::new(2), true).unwrap();
    let double = layout(&song, true);
    let double_gap = gap_to_closing_barline(&double);
    // The thick bar leaves the same visible room before either: the barline's ink, not its
    // nominal x, is what it must clear.
    assert!(
        (final_gap - plain_gap).abs() < 1e-3,
        "final {final_gap}, plain {plain_gap}"
    );
    assert!(
        (double_gap - plain_gap).abs() < 1e-3,
        "double {double_gap}, plain {plain_gap}"
    );
}

#[test]
fn a_repeat_closing_a_run_is_the_only_barline_there() {
    // Bars 2-3 rest, bar 3 repeats at its end, bar 4 plays.
    let mut song = song(1, 4, &[(0, 0), (0, 3)]);
    let _ = song
        .set_repeat(
            MeasureIndex::new(2),
            Repeat {
                start: false,
                end_repeats: Some(1),
                endings: 0,
            },
        )
        .unwrap();
    let layout = layout(&song, true);
    let run_end = bars(&layout)[2].1;
    // The repeat sign's thin and thick line, and nothing else: no plain barline of the
    // run's lead or of a folded bar cutting through the dots.
    let closing: Vec<(f32, f32)> = barlines(&layout)
        .into_iter()
        .filter(|&(x, _)| x > run_end - 1.0 && x < run_end + 3.0)
        .collect();
    assert_eq!(closing.len(), 2, "{closing:?}");
    // The sign stands clear of the thick bar (its dots have room).
    assert!(closing[0].0 - rest_bar_right(&layout) > 1.5, "{closing:?}");
}
