//! Guitar Pro's design mode in the layout: a bar's width scale and a moved beat column.

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

use gscore_layout::{Fonts, Layout, LayoutOptions, layout_track, layout_tracks};
use gscore_model::testing::{self, GUITAR_6};
use gscore_model::{
    BeatIndex, BeatPosition, MeasureIndex, ScaleScope, Song, StaffIndex, TrackIndex, VoiceIndex,
};

fn song(measures: usize) -> Song {
    testing::song(&[GUITAR_6], measures).expect("a song")
}

fn layout(song: &Song) -> Layout {
    let fonts = Fonts::bundled().expect("the bundled fonts");
    layout_track(song, TrackIndex::new(0), &LayoutOptions::default(), &fonts).expect("lays out")
}

/// The measures on the first system.
fn first_system(layout: &Layout) -> usize {
    let mut count = 0;
    for measure in &layout.places.measures {
        if measure.system != 0 {
            break;
        }
        count += 1;
    }
    count
}

#[test]
fn a_wider_bar_takes_more_room_and_a_line_holds_fewer_of_them() {
    let mut song = song(8);
    let before = layout(&song);
    let wide = before
        .places
        .measure(MeasureIndex::new(0))
        .map(|measure| measure.right - measure.left)
        .expect("the first measure");
    assert!(wide > 0.0);
    let measures_before = first_system(&before);

    let _ = song
        .set_measure_scale(ScaleScope::Song, MeasureIndex::new(0), Some(2.0))
        .expect("sets the scale");
    let after = layout(&song);
    let wider = after
        .places
        .measure(MeasureIndex::new(0))
        .map(|measure| measure.right - measure.left)
        .expect("the first measure");
    assert!(
        wider > wide * 1.5,
        "the bar is twice as wide at its content: {wide} became {wider}"
    );
    assert!(
        first_system(&after) <= measures_before,
        "a wider first bar cannot fit more bars: {measures_before} became {}",
        first_system(&after)
    );
}

#[test]
fn a_narrower_bar_takes_less_room() {
    let mut song = song(2);
    let before = layout(&song);
    let wide = before.places.measures[1].right - before.places.measures[1].left;
    let _ = song
        .set_measure_scale(ScaleScope::Song, MeasureIndex::new(1), Some(0.5))
        .expect("sets the scale");
    let after = layout(&song);
    let narrow = after.places.measures[1].right - after.places.measures[1].left;
    assert!(
        narrow < wide * 0.95,
        "a squeezed bar is narrower: {wide} became {narrow}"
    );
}

#[test]
fn a_staffs_own_scale_overrides_the_songs() {
    let mut song = song(2);
    let _ = song
        .set_measure_scale(ScaleScope::Song, MeasureIndex::new(0), Some(0.5))
        .expect("sets the song scale");
    let narrow = layout(&song).places.measures[0].right - layout(&song).places.measures[0].left;
    let _ = song
        .set_measure_scale(
            ScaleScope::Staff {
                track: TrackIndex::new(0),
                staff: StaffIndex::new(0),
            },
            MeasureIndex::new(0),
            Some(2.0),
        )
        .expect("sets the staff scale");
    let wide = layout(&song).places.measures[0].right - layout(&song).places.measures[0].left;
    assert!(wide > narrow * 2.0, "{narrow} became {wide}");
}

/// Where the beat's column starts in measure 0.
fn beat_x(layout: &Layout, beat: usize) -> f32 {
    layout
        .places
        .measure(MeasureIndex::new(0))
        .expect("the first measure")
        .beats
        .iter()
        .find(|place| place.voice == VoiceIndex::new(0) && place.beat == BeatIndex::new(beat))
        .map(|place| place.x)
        .expect("the beat's column")
}

#[test]
fn a_moved_column_takes_its_neighbours_and_the_barline_with_it() {
    let mut song = song(2);
    let before = layout(&song);
    let second = beat_x(&before, 1);
    let last = beat_x(&before, 3);
    let barline = before.places.measures[0].right;
    let _ = song
        .set_beat_spacing(
            BeatPosition {
                track: TrackIndex::new(0),
                staff: StaffIndex::new(0),
                measure: MeasureIndex::new(0),
                voice: VoiceIndex::new(0),
                beat: BeatIndex::new(1),
            },
            Some(1.0),
        )
        .expect("moves the column");
    let after = layout(&song);
    assert!(
        (beat_x(&after, 1) - (second + 1.0)).abs() < 0.01,
        "the column moved a staff space right: {second} became {}",
        beat_x(&after, 1)
    );
    assert!(
        (beat_x(&after, 0) - beat_x(&before, 0)).abs() < 0.01,
        "the column before it stays where it was"
    );
    assert!(
        (beat_x(&after, 3) - (last + 1.0)).abs() < 0.01,
        "the columns after it move with it"
    );
    assert!(
        (after.places.measures[0].right - (barline + 1.0)).abs() < 0.01,
        "the barline moves with them"
    );
}

#[test]
fn a_hostile_offset_cannot_pass_the_column_before_it() {
    let mut song = song(2);
    let _ = song
        .set_beat_spacing(
            BeatPosition {
                track: TrackIndex::new(0),
                staff: StaffIndex::new(0),
                measure: MeasureIndex::new(0),
                voice: VoiceIndex::new(0),
                beat: BeatIndex::new(1),
            },
            Some(-4.0),
        )
        .expect("moves the column as far left as allowed");
    let after = layout(&song);
    let mut previous = beat_x(&after, 0);
    for beat in 1..4 {
        let x = beat_x(&after, beat);
        assert!(
            x.is_finite() && x > previous,
            "beat {beat}: {previous} then {x}"
        );
        previous = x;
    }
    assert!(after.places.measures[0].right > previous);
}

#[test]
fn a_moved_column_and_a_bar_width_survive_the_layout_cache() {
    let fonts = Fonts::bundled().expect("the bundled fonts");
    let options = LayoutOptions::default();
    let mut cache = gscore_layout::LayoutCache::default();
    let mut song = song(4);
    let before = cache
        .layout(&song, TrackIndex::new(0), &options, &fonts)
        .expect("lays out");
    let barline = before.places.measures[0].right;
    // The song-wide scale lives in the measure header, so the layout cache must notice it.
    let _ = song
        .set_measure_scale(ScaleScope::Song, MeasureIndex::new(0), Some(2.0))
        .expect("sets the scale");
    let after = cache
        .layout(&song, TrackIndex::new(0), &options, &fonts)
        .expect("lays out again");
    assert!(
        after.places.measures[0].right > barline * 1.2,
        "the cached measure was laid out again: {barline} became {}",
        after.places.measures[0].right
    );
}

#[test]
fn a_wider_bar_of_a_later_track_widens_it_for_every_track() {
    let song = testing::song(&[GUITAR_6, GUITAR_6], 2).expect("a song");
    let fonts = Fonts::bundled().expect("the bundled fonts");
    let options = LayoutOptions::default();
    let tracks = [TrackIndex::new(0), TrackIndex::new(1)];
    let width = |song: &Song| {
        let layout = layout_tracks(song, &tracks, tracks[0], &options, &fonts).expect("lays out");
        let measure = &layout.places.measures[0];
        measure.right - measure.left
    };
    let before = width(&song);
    // The second track's own bar is twice as wide; the first track has no scale.
    let mut widened = song.clone();
    let _ = widened
        .set_measure_scale(
            ScaleScope::Staff {
                track: TrackIndex::new(1),
                staff: StaffIndex::new(0),
            },
            MeasureIndex::new(0),
            Some(2.0),
        )
        .expect("sets the scale");
    let after = width(&widened);
    assert!(
        after > before * 1.3,
        "the shared bar line follows the widest bar: {before} became {after}"
    );
}
