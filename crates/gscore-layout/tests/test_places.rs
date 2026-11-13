//! The page's places: every measure and beat is found, the cursor box sits on its string,
//! and a click on a cursor box finds the same beat and string again.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;

use gscore_layout::{Fonts, Hit, LayoutOptions, Point, layout_track};
use gscore_model::testing::{self, BASS_4, GUITAR_6};
use gscore_model::{BeatIndex, MeasureIndex, TrackIndex, VoiceIndex};

fn fonts() -> Fonts {
    Fonts::bundled().unwrap()
}

#[test]
fn every_measure_and_beat_has_a_place_and_clicks_find_them_again() {
    for (tuning, notation) in [(GUITAR_6, true), (BASS_4, false)] {
        let song = testing::song(&[tuning], 12).unwrap();
        let options = LayoutOptions {
            notation,
            ..LayoutOptions::default()
        };
        let layout = layout_track(&song, TrackIndex::new(0), &options, &fonts()).unwrap();
        let places = &layout.places;
        assert_eq!(places.measures.len(), 12);
        assert!(
            places.systems.len() > 1,
            "twelve measures need several lines"
        );
        let strings = u8::try_from(tuning.len()).unwrap();
        for place in &places.measures {
            assert_eq!(place.beats.len(), 4, "four quarter rests");
            assert!(place.left < place.content && place.content < place.right);
            for beat in &place.beats {
                for string in 1..=strings {
                    let rect = places
                        .cursor_rect(place.measure, beat.voice, beat.beat, string)
                        .unwrap();
                    assert!(rect.origin.x >= 0.0 && rect.origin.x + rect.width <= layout.width);
                    let center = Point::new(
                        rect.origin.x + rect.width / 2.0,
                        rect.origin.y + rect.height / 2.0,
                    );
                    assert_eq!(
                        places.hit(center),
                        Some(Hit {
                            measure: place.measure,
                            voice: beat.voice,
                            beat: beat.beat,
                            string,
                            staff: places.staff,
                        })
                    );
                }
            }
        }
        // Off the page, between systems or beyond the last string: no hit, or the nearest
        // string.
        assert_eq!(places.hit(Point::new(-5.0, -5.0)), None);
        let first = &places.systems[0];
        let below = Point::new(places.measures[0].content, first.tab_bottom() + 1.5);
        assert_eq!(places.hit(below).unwrap().string, strings);
    }
}

#[test]
fn a_beat_the_page_does_not_show_sits_at_the_first_column() {
    let song = testing::song(&[GUITAR_6], 1).unwrap();
    let layout = layout_track(
        &song,
        TrackIndex::new(0),
        &LayoutOptions::default(),
        &fonts(),
    )
    .unwrap();
    let place = layout.places.measure(MeasureIndex::new(0)).unwrap();
    let (x, _) = layout
        .places
        .beat_x(MeasureIndex::new(0), VoiceIndex::new(1), BeatIndex::new(7))
        .unwrap();
    assert_eq!(x, place.content);
    assert!(layout.places.measure(MeasureIndex::new(1)).is_none());
}

#[test]
fn a_real_file_places_every_measure() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/basic/ornaments.gp");
    let bytes = std::fs::read(path).unwrap();
    let song = gscore_io::import_guitar_pro_from_bytes(&bytes)
        .unwrap()
        .song;
    let layout = layout_track(
        &song,
        TrackIndex::new(0),
        &LayoutOptions::default(),
        &fonts(),
    )
    .unwrap();
    assert_eq!(layout.places.measures.len(), song.measure_count());
    for (index, place) in layout.places.measures.iter().enumerate() {
        assert_eq!(place.measure, MeasureIndex::new(index));
    }
}

#[test]
fn the_gliding_cursor_moves_from_beat_to_beat_and_on_to_the_next_bar() {
    let song = testing::song(&[GUITAR_6], 12).unwrap();
    let layout = layout_track(
        &song,
        TrackIndex::new(0),
        &LayoutOptions::default(),
        &fonts(),
    )
    .unwrap();
    let places = &layout.places;
    let voice = VoiceIndex::new(0);
    let m0 = MeasureIndex::new(0);
    let center = |rect: gscore_layout::Rect| rect.origin.x + rect.width / 2.0;
    let x = |measure, beat| {
        center(
            places
                .column_rect(measure, voice, BeatIndex::new(beat))
                .unwrap(),
        )
    };
    let glide = |measure, beat, progress, next| {
        center(
            places
                .glide_rect(measure, voice, BeatIndex::new(beat), progress, next)
                .unwrap(),
        )
    };
    // Halfway between two beats, at the start, and all the way (never past the next).
    let half = glide(m0, 0, 0.5, None);
    assert!((half - (x(m0, 0) + x(m0, 1)) / 2.0).abs() < 1e-3);
    assert!((glide(m0, 0, 0.0, None) - x(m0, 0)).abs() < 1e-3);
    assert!((glide(m0, 0, 7.0, None) - x(m0, 1)).abs() < 1e-3);
    assert!((glide(m0, 0, f32::NAN, None) - x(m0, 0)).abs() < 1e-3);
    // The last beat glides on to the next bar's first beat on the same line...
    let m1 = MeasureIndex::new(1);
    assert!((glide(m0, 3, 1.0, Some(m1)) - x(m1, 0)).abs() < 1e-3);
    // ...or to its own barline when another bar is played next (a repeat, a new line).
    let barline = places.measure(m0).unwrap().right;
    assert!((glide(m0, 3, 1.0, Some(MeasureIndex::new(2))) - barline).abs() < 1e-3);
    assert!((glide(m0, 3, 1.0, Some(MeasureIndex::new(5))) - barline).abs() < 1e-3);
    assert!((glide(m0, 3, 1.0, None) - barline).abs() < 1e-3);
}
