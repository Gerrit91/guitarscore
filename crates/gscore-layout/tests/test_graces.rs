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

use gscore_layout::{DrawOp, Fonts, Glyph};
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
        scales.iter().any(|scale| *scale < 0.9),
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
        scales.iter().any(|scale| *scale < 0.9),
        "a small grace notehead: {scales:?}"
    );
    assert!(
        scales.iter().any(|scale| (*scale - 1.0).abs() < 0.01),
        "the main notehead: {scales:?}"
    );
    // On the tab, a smaller grace fret number (5) than the main one (3); sizes are
    // checked in detail below.
    let size_of = |wanted: &str| {
        laid.ops.iter().find_map(|op| match op {
            DrawOp::Text { text, size, .. } if text == wanted => Some(*size),
            _ => None,
        })
    };
    let fret_sizes = [size_of("5").unwrap(), size_of("3").unwrap()];
    assert!(
        fret_sizes[0] < fret_sizes[1],
        "a small grace fret number: {fret_sizes:?}"
    );
}

/// The glyphs of `layout` as `(glyph, x, scale)`.
fn glyphs(layout: &gscore_layout::Layout) -> Vec<(Glyph, f32, f32)> {
    layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Glyph {
                glyph, at, scale, ..
            } => Some((*glyph, at.x, *scale)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_grace_note_keeps_clear_of_the_main_notes_accidental() {
    // Grace F#4 (string 1, fret 2) before G#4 (fret 4): both need a sharp.
    let song = testing::song_with_graces(&[(&[(1, 2)], &[(1, 4)])]).unwrap();
    let laid = common::layout(&song, 0);
    let music = Fonts::bundled().unwrap().music;
    let glyphs = glyphs(&laid);
    let main_sharp = glyphs
        .iter()
        .find(|(glyph, _, scale)| *glyph == Glyph::Sharp && *scale == 1.0)
        .map(|(_, x, _)| *x)
        .expect("the main note's sharp");
    let small: Vec<&(Glyph, f32, f32)> = glyphs.iter().filter(|(_, _, s)| *s < 1.0).collect();
    assert!(
        small.iter().any(|(glyph, _, _)| *glyph == Glyph::Sharp),
        "the grace note has its own sharp: {small:?}"
    );
    for (glyph, x, scale) in &small {
        let right = x + music.advance(*glyph) * scale;
        assert!(
            right < main_sharp,
            "{glyph:?} of the grace note ends at {right}, the main sharp starts at {main_sharp}"
        );
    }
}

#[test]
fn a_grace_fret_number_is_smaller_but_readable_and_unslashed() {
    let song = testing::song_with_grace_beat().unwrap();
    let laid = common::layout(&song, 0);
    let sizes = |wanted: &str| -> Vec<(f32, f32, f32)> {
        laid.ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Text { text, size, at, .. } if text == wanted => Some((*size, at.x, at.y)),
                _ => None,
            })
            .collect()
    };
    let (grace_size, grace_x, grace_y) = sizes("5")[0];
    let (main_size, main_x, _) = sizes("3")[0];
    assert!(
        grace_size < main_size && grace_size >= 0.7 * main_size,
        "{grace_size} against {main_size}"
    );
    assert!(main_x - grace_x > 1.0, "a clear gap before the main note");
    // No stroke crosses the grace fret number on the tab.
    let crossing = laid.ops.iter().any(|op| match op {
        DrawOp::Line { from, to, .. } => {
            let (low, high) = (from.y.min(to.y), from.y.max(to.y));
            from.y != to.y
                && from.x < grace_x
                && to.x > grace_x
                && from.x > grace_x - 1.0
                && low < grace_y
                && high > grace_y - 1.5
        }
        _ => false,
    });
    assert!(!crossing, "the tab carries no slash");
}

#[test]
fn a_low_grace_note_gets_ledger_lines() {
    // The open low E is written below the treble staff, three ledger lines down.
    let song = testing::song_with_graces(&[(&[(6, 0)], &[(6, 1)])]).unwrap();
    let laid = common::layout(&song, 0);
    let short_lines = laid
        .ops
        .iter()
        .filter(|op| match op {
            DrawOp::Line { from, to, .. } => from.y == to.y && (to.x - from.x) < 2.0,
            _ => false,
        })
        .count();
    // Three for the main note and three for the grace note.
    assert!(short_lines >= 6, "{short_lines} ledger lines");
}
