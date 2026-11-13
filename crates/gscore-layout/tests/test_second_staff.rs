//! A track's second staff (a piano's bass) in the score: a row of its own under the first,
//! in its own clef, joined by one line and a brace, named once, its measures lined up with
//! the treble's, and found by clicks on its own staves.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use gscore_layout::{DrawOp, Fonts, Glyph, Layout, LayoutOptions, Point, layout_tracks};
use gscore_model::{MeasureIndex, Song, StaffIndex, TrackIndex, testing};

/// The piano (two staves) and the guitar after it, laid out together.
fn layout(song: &Song) -> Layout {
    let tracks: Vec<TrackIndex> = song.tracks().map(|track| track.index()).collect();
    layout_tracks(
        song,
        &tracks,
        tracks[0],
        &LayoutOptions::default(),
        &Fonts::bundled().unwrap(),
    )
    .unwrap()
}

#[test]
fn a_two_staff_track_takes_one_row_per_staff() {
    let song = testing::song_with_two_staves().unwrap();
    let layout = layout(&song);
    let rows: Vec<(TrackIndex, StaffIndex)> = layout
        .tracks
        .iter()
        .map(|places| (places.track, places.staff))
        .collect();
    assert_eq!(
        rows,
        [
            (TrackIndex::new(0), StaffIndex::new(0)),
            (TrackIndex::new(0), StaffIndex::new(1)),
            (TrackIndex::new(1), StaffIndex::new(0)),
        ]
    );
    // The primary places are the piano's treble.
    assert_eq!(layout.places, layout.tracks[0].places);
    assert_eq!(layout.places.staff, StaffIndex::new(0));
    let bass = layout
        .staff_places(TrackIndex::new(0), StaffIndex::new(1))
        .unwrap();
    assert_eq!(bass.measures.len(), song.measure_count());
    for pair in layout.tracks.windows(2) {
        let upper = &pair[0].places.systems[0];
        let lower = &pair[1].places.systems[0];
        assert!(
            lower.top > upper.tab_bottom(),
            "staff {} of track {} starts below track {}'s staff",
            pair[1].staff,
            pair[1].track,
            pair[0].track
        );
    }
    // The piano's own two staves stand closer than the guitar's staff below them.
    let treble = &layout.tracks[0].places.systems[0];
    let bass_row = &layout.tracks[1].places.systems[0];
    let guitar = &layout.tracks[2].places.systems[0];
    let own_gap = bass_row.top - treble.tab_bottom();
    let track_gap = guitar.top - bass_row.tab_bottom();
    assert!(own_gap < track_gap, "{own_gap} vs {track_gap}");
}

#[test]
fn the_bass_staff_has_its_own_clef_and_the_two_are_braced() {
    let song = testing::song_with_two_staves().unwrap();
    let layout = layout(&song);
    let clefs = |glyph: Glyph, from: f32, to: f32| {
        layout
            .ops
            .iter()
            .filter(|op| {
                matches!(op, DrawOp::Glyph { glyph: drawn, at, .. }
                    if *drawn == glyph && at.y >= from && at.y <= to)
            })
            .count()
    };
    let treble = &layout.tracks[0].places.systems[0];
    let bass = &layout.tracks[1].places.systems[0];
    assert_eq!(clefs(Glyph::GClef, treble.top, treble.tab_bottom()), 1);
    assert_eq!(clefs(Glyph::FClef, bass.top, bass.tab_bottom()), 1);
    // One brace for the group, stretched over both staves, and one line joining them.
    let braces: Vec<&DrawOp> = layout
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                DrawOp::Glyph {
                    glyph: Glyph::Brace,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(braces.len(), 1, "{braces:?}");
    let DrawOp::Glyph { at, scale, .. } = braces[0] else {
        panic!("a brace is a glyph");
    };
    let height = 4.0 * *scale;
    assert!(
        height > treble.tab_bottom() - treble.top,
        "the brace reaches over the group"
    );
    assert!(at.y >= bass.tab_bottom() - height);
    // The line at the system's start runs from the treble's top to the bass's bottom.
    // (The line joining all tracks starts there too, so there are two that tall.)
    assert!(layout.ops.iter().any(|op| {
        matches!(op, DrawOp::Line { from, to, .. }
            if (from.x - to.x).abs() < 0.01
                && from.y <= treble.top + 0.01
                && to.y >= bass.tab_bottom() - 0.01)
    }));
}

#[test]
fn a_click_on_the_bass_staff_hits_it() {
    let song = testing::song_with_two_staves().unwrap();
    let layout = layout(&song);
    let bass = &layout.tracks[1].places.systems[0];
    let measure = &layout.tracks[1].places.measures[0];
    let point = Point::new(measure.content, (bass.top + bass.tab_bottom()) / 2.0);
    let (track, hit) = layout.hit(point).unwrap();
    assert_eq!(track, TrackIndex::new(0));
    assert_eq!(hit.staff, StaffIndex::new(1));
    assert_eq!(hit.measure, MeasureIndex::new(0));
    // The treble above it is the same track's first staff.
    let treble = &layout.tracks[0].places.systems[0];
    let point = Point::new(measure.content, (treble.top + treble.tab_bottom()) / 2.0);
    let (track, hit) = layout.hit(point).unwrap();
    assert_eq!(track, TrackIndex::new(0));
    assert_eq!(hit.staff, StaffIndex::new(0));
}

#[test]
fn the_bass_notes_are_written_on_the_bass_staff() {
    let song = testing::song_with_two_staves().unwrap();
    let layout = layout(&song);
    let bass = &layout.tracks[1].places.systems[0];
    // The bass staff shows a whole note: its head sits on the staff, not with the treble's.
    let heads: Vec<f32> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Glyph {
                glyph: Glyph::NoteheadWhole,
                at,
                ..
            } => Some(at.y),
            _ => None,
        })
        .collect();
    assert_eq!(heads.len(), 2, "{heads:?}");
    let in_bass = heads
        .iter()
        .filter(|&&y| y >= bass.top - 1.0 && y <= bass.tab_bottom() + 1.0)
        .count();
    assert_eq!(in_bass, 1, "{heads:?}");
}

#[test]
fn a_hit_carries_the_staff_its_places_are_of() {
    let song = testing::song_with_two_staves().unwrap();
    let layout = layout(&song);
    let bass = layout
        .staff_places(TrackIndex::new(0), StaffIndex::new(1))
        .unwrap();
    let system = &bass.systems[0];
    let point = Point::new(bass.measures[0].content, system.tab_bottom());
    assert_eq!(
        bass.hit(point).map(|hit| hit.staff),
        Some(StaffIndex::new(1))
    );
}
