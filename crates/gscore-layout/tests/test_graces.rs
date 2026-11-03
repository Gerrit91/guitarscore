//! Grace notes: they take no time and are drawn small before their main note.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

mod common;

use gscore_layout::{DrawOp, Glyph};
use gscore_model::{BeatIndex, MeasureIndex, TrackIndex, VoiceIndex, testing};

#[test]
fn a_grace_beat_takes_no_time() {
    let song = testing::song_with_grace_beat().unwrap();
    let measure = song
        .track(TrackIndex::new(0))
        .unwrap()
        .measure(MeasureIndex::new(0))
        .unwrap();
    let voice = measure.voice(VoiceIndex::new(0)).unwrap();
    assert!(voice.beat(BeatIndex::new(0)).unwrap().is_grace());
    assert_eq!(
        voice.total_duration().unwrap(),
        measure.time_signature().unwrap().measure_length(),
        "the grace beat does not make the measure too long"
    );
}

#[test]
fn a_gp5_grace_attached_to_a_note_is_drawn() {
    let song = testing::song_with_attached_grace().unwrap();
    let laid = common::layout(&song, 0);
    let scales: Vec<f32> = laid
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Glyph {
                glyph: Glyph::NoteheadBlack,
                scale,
                ..
            } => Some(*scale),
            _ => None,
        })
        .collect();
    assert!(
        scales.iter().any(|scale| (*scale - 0.6).abs() < 0.01),
        "the attached grace note is drawn small: {scales:?}"
    );
    // The grace note takes no length: the measure is still one whole note long.
    let measure = song
        .track(TrackIndex::new(0))
        .unwrap()
        .measure(MeasureIndex::new(0))
        .unwrap();
    let voice = measure.voice(VoiceIndex::new(0)).unwrap();
    assert_eq!(
        voice.total_duration().unwrap(),
        measure.time_signature().unwrap().measure_length()
    );
}

#[test]
fn a_grace_note_is_drawn_small_before_the_main_note() {
    let song = testing::song_with_grace_beat().unwrap();
    let laid = common::layout(&song, 0);
    // The main note is the first timed beat of the measure, at its start.
    let place = &laid.places.measures[0];
    assert_eq!(place.beats[0].beat, BeatIndex::new(1));
    // One small grace notehead and one normal one.
    let scales: Vec<f32> = laid
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Glyph {
                glyph: Glyph::NoteheadBlack,
                scale,
                ..
            } => Some(*scale),
            _ => None,
        })
        .collect();
    assert!(
        scales.iter().any(|scale| (*scale - 0.6).abs() < 0.01),
        "a small grace notehead: {scales:?}"
    );
    assert!(
        scales.iter().any(|scale| (*scale - 1.0).abs() < 0.01),
        "the main notehead: {scales:?}"
    );
    // On the tab, a small grace fret number before the main one.
    let fret_sizes: Vec<f32> = laid
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { text, size, .. } if text == "5" || text == "3" => Some(*size),
            _ => None,
        })
        .collect();
    assert!(
        fret_sizes.iter().filter(|size| **size < 1.0).count() >= 1,
        "a small grace fret number: {fret_sizes:?}"
    );
}
