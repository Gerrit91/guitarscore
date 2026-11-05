//! Several tracks in one score (M21): stacked staves, beats lined up across tracks, shared
//! line breaks, the multitrack view's own breaks, places per track, and the cache.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::float_cmp
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

mod common;

use gscore_layout::{
    DrawOp, EmptyStaves, Fonts, Layout, LayoutCache, LayoutOptions, Point, layout_track,
    layout_tracks,
};
use gscore_model::{MeasureIndex, Song, TrackIndex};

fn all_tracks(song: &Song) -> Vec<TrackIndex> {
    song.tracks().map(|track| track.index()).collect()
}

fn layout_all(song: &Song, options: &LayoutOptions) -> Layout {
    let tracks = all_tracks(song);
    layout_tracks(
        song,
        &tracks,
        tracks[0],
        options,
        &Fonts::bundled().unwrap(),
    )
    .unwrap()
}

#[test]
fn every_track_gets_its_places_in_the_order_given() {
    let song = common::open("basic/multi_staff.gp");
    let layout = layout_all(&song, &LayoutOptions::default());
    let shown: Vec<TrackIndex> = layout.tracks.iter().map(|t| t.track).collect();
    assert_eq!(shown, all_tracks(&song));
    // The primary track's places are the top-level ones.
    assert_eq!(layout.places, layout.tracks[0].places);
    // Every track has the same systems and measures, stacked top to bottom.
    let systems = layout.tracks[0].places.systems.len();
    assert!(systems > 1);
    for track in &layout.tracks {
        assert_eq!(track.places.systems.len(), systems);
        assert_eq!(track.places.measures.len(), song.measure_count());
    }
    for system in 0..systems {
        for pair in layout.tracks.windows(2) {
            let upper = &pair[0].places.systems[system];
            let lower = &pair[1].places.systems[system];
            assert!(
                lower.top > upper.tab_bottom() + 2.0,
                "system {system}: track {} starts above track {}'s bottom",
                pair[1].track,
                pair[0].track
            );
        }
    }
}

#[test]
fn measures_and_beats_line_up_across_tracks() {
    let song = common::open("basic/multi_staff.gp");
    let layout = layout_all(&song, &LayoutOptions::default());
    let first = &layout.tracks[0].places;
    for track in &layout.tracks[1..] {
        for (a, b) in first.measures.iter().zip(&track.places.measures) {
            assert_eq!(a.system, b.system, "{}", a.measure);
            assert!((a.left - b.left).abs() < 1e-3, "{}", a.measure);
            assert!((a.right - b.right).abs() < 1e-3, "{}", a.measure);
        }
    }
    // Bar 1: the clean guitar and the rhythm guitar both play eight eighths, and the
    // drums' beats fall on the same eighths: their columns are at the same x.
    let xs = |track: usize| -> Vec<f32> {
        layout.tracks[track].places.measures[0]
            .beats
            .iter()
            .map(|beat| beat.x)
            .collect()
    };
    let (clean, rhythm) = (xs(0), xs(1));
    assert_eq!(clean.len(), 8);
    for (a, b) in clean.iter().zip(&rhythm) {
        assert!((a - b).abs() < 1e-3, "{clean:?} vs {rhythm:?}");
    }
    for x in xs(5) {
        assert!(
            clean.iter().any(|c| (c - x).abs() < 1e-3),
            "a drum beat at {x} is between the guitar's columns {clean:?}"
        );
    }
}

#[test]
fn several_tracks_break_where_the_multitrack_layout_allows() {
    let song = common::open("basic/multi_staff.gp");
    // Guitar Pro's multitrack layout is "4 4 2": no line runs across bar 5 or bar 9.
    let breaks = song.score_line_breaks();
    assert!(breaks[4] && breaks[8]);
    let layout = layout_all(&song, &LayoutOptions::default());
    let places = &layout.tracks[0].places;
    let system_of = |measure: usize| places.measure(MeasureIndex::new(measure)).unwrap().system;
    assert_ne!(system_of(3), system_of(4));
    assert_ne!(system_of(7), system_of(8));
}

#[test]
fn a_point_on_a_staff_finds_that_track() {
    let song = common::open("basic/multi_staff.gp");
    let layout = layout_all(&song, &LayoutOptions::default());
    for track in &layout.tracks {
        let system = &track.places.systems[0];
        let measure = &track.places.measures[0];
        let point = Point::new(
            (measure.left + measure.right) / 2.0,
            (system.top + system.tab_bottom()) / 2.0,
        );
        let (found, hit) = layout.hit(point).unwrap();
        assert_eq!(found, track.track);
        assert_eq!(hit.measure, MeasureIndex::new(0));
    }
}

#[test]
fn only_the_top_track_carries_the_measure_numbers_and_sections() {
    let song = common::open("basic/multi_staff.gp");
    let options = LayoutOptions::default();
    // Section names, and measure numbers (in their own size, unlike fret numbers).
    let texts = |layout: &Layout, wanted: &str| {
        layout
            .ops
            .iter()
            .filter(|op| {
                matches!(op, DrawOp::Text { text, size, .. }
                    if text == wanted
                        && (text.parse::<u32>().is_err() || *size == options.measure_number_size))
            })
            .count()
    };
    let all = layout_all(&song, &options);
    let one = layout_track(
        &song,
        TrackIndex::new(0),
        &options,
        &Fonts::bundled().unwrap(),
    )
    .unwrap();
    assert_eq!(texts(&all, "Intro"), texts(&one, "Intro"));
    assert_eq!(texts(&all, "Opening Theme"), 1);
    // The measure number "2" stands once per score, not once per track.
    assert_eq!(texts(&all, "2"), texts(&one, "2"));
}

#[test]
fn one_track_through_the_multitrack_entry_is_the_single_track_layout() {
    let song = common::open("basic/multi_staff.gp");
    let options = LayoutOptions::default();
    let fonts = Fonts::bundled().unwrap();
    let track = TrackIndex::new(2);
    let one = layout_track(&song, track, &options, &fonts).unwrap();
    let same = layout_tracks(&song, &[track], track, &options, &fonts).unwrap();
    assert_eq!(one, same);
}

#[test]
fn the_cache_keeps_every_shown_track_and_gives_the_same_layout() {
    let song = common::open("basic/multi_staff.gp");
    let options = LayoutOptions::default().on_pages();
    let fonts = Fonts::bundled().unwrap();
    let tracks = all_tracks(&song);
    let mut cache = LayoutCache::default();
    let first = cache
        .layout_tracks(&song, &tracks, tracks[0], &options, &fonts)
        .unwrap();
    assert_eq!(cache.laid_out(), tracks.len() * song.measure_count());
    assert_eq!(
        first,
        layout_tracks(&song, &tracks, tracks[0], &options, &fonts).unwrap()
    );
    // Back to one track and to all again: nothing is laid out afresh.
    cache.layout(&song, tracks[1], &options, &fonts).unwrap();
    assert_eq!(cache.laid_out(), 0);
    cache
        .layout_tracks(&song, &tracks, tracks[3], &options, &fonts)
        .unwrap();
    assert_eq!(cache.laid_out(), 0);
}

#[test]
fn a_missing_track_or_none_is_an_error() {
    let song = common::open("basic/multi_staff.gp");
    let options = LayoutOptions::default();
    let fonts = Fonts::bundled().unwrap();
    assert!(layout_tracks(&song, &[], TrackIndex::new(0), &options, &fonts).is_err());
    let missing = [TrackIndex::new(0), TrackIndex::new(99)];
    assert!(layout_tracks(&song, &missing, TrackIndex::new(0), &options, &fonts).is_err());
}

/// How many of each system's tracks are drawn: tracks with measure places there.
fn drawn_per_system(layout: &Layout) -> Vec<usize> {
    let systems = layout.tracks[0].places.systems.len();
    (0..systems)
        .map(|system| {
            layout
                .tracks
                .iter()
                .filter(|t| t.places.measures.iter().any(|m| m.system == system))
                .count()
        })
        .collect()
}

#[test]
fn empty_staves_are_left_out_after_the_first_system() {
    let song = common::open("basic/multi_staff.gp");
    let fonts = Fonts::bundled().unwrap();
    let tracks = all_tracks(&song);
    let shown = layout_all(&song, &LayoutOptions::default());
    let hidden_options = LayoutOptions {
        empty_staves: EmptyStaves::Hidden,
        ..LayoutOptions::default()
    };
    let hidden = layout_tracks(&song, &tracks, tracks[0], &hidden_options, &fonts).unwrap();
    assert!(drawn_per_system(&shown).iter().all(|&count| count == 6));
    let counts = drawn_per_system(&hidden);
    // The first system shows every track; bars 5-8 have no clean or rhythm guitar.
    assert_eq!(counts[0], 6);
    assert!(counts.iter().any(|&count| count < 6), "{counts:?}");
    assert!(hidden.height < shown.height, "hiding saves room");
    // Every track keeps one place per system, so system numbers stay in step.
    for track in &hidden.tracks {
        assert_eq!(track.places.systems.len(), counts.len());
    }
    // The system's marks go to the topmost track still drawn.
    let sections = hidden
        .ops
        .iter()
        .filter(|op| matches!(op, DrawOp::Text { text, .. } if text == "Opening Theme"))
        .count();
    assert_eq!(sections, 1);
    // A point where a hidden track would be finds a drawn one.
    let bar5 = MeasureIndex::new(4);
    let clean = hidden.places_of(tracks[0]).unwrap();
    assert!(
        clean.measure(bar5).is_none(),
        "the clean guitar is hidden at bar 5"
    );
    let crunch = hidden.places_of(tracks[2]).unwrap();
    let place = crunch.measure(bar5).unwrap();
    let system = &crunch.systems[place.system];
    let (track, _) = hidden
        .hit(Point::new(place.content, system.tab_bottom()))
        .unwrap();
    assert_eq!(track, tracks[2]);
}

#[test]
fn the_primary_track_is_kept_when_asked() {
    let song = common::open("basic/multi_staff.gp");
    let fonts = Fonts::bundled().unwrap();
    let tracks = all_tracks(&song);
    let options = LayoutOptions {
        empty_staves: EmptyStaves::HiddenButPrimary,
        ..LayoutOptions::default()
    };
    let layout = layout_tracks(&song, &tracks, tracks[0], &options, &fonts).unwrap();
    // The cursor's track (the clean guitar) has every measure, even its empty ones.
    assert_eq!(layout.places.measures.len(), song.measure_count());
    assert!(layout.places.measure(MeasureIndex::new(4)).is_some());
    // Another empty track is still left out.
    let rhythm = layout.places_of(tracks[1]).unwrap();
    assert!(rhythm.measure(MeasureIndex::new(4)).is_none());
}
