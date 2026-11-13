//! Second-voice notation and ties across barlines.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

mod common;

use gscore_layout::{DrawOp, Glyph, Layout};
use gscore_model::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, NoteValue, StringNumber, SystemBreak, TrackIndex,
    VoiceIndex, testing,
};

fn at(measure: usize, beat: usize, voice: usize) -> BeatPosition {
    BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(voice),
        beat: BeatIndex::new(beat),
    }
}

fn string(number: u8) -> StringNumber {
    StringNumber::new(number).unwrap()
}

/// The positions of a glyph in the layout.
fn glyphs(layout: &Layout, glyph: Glyph) -> Vec<(f32, f32)> {
    layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Glyph {
                glyph: drawn, at, ..
            } if *drawn == glyph => Some((at.x, at.y)),
            _ => None,
        })
        .collect()
}

/// The tie curves that cross `x`.
fn crossing_curves(layout: &Layout, x: f32) -> usize {
    layout
        .ops
        .iter()
        .filter(|op| match op {
            DrawOp::Curve { from, to, .. } => from.x < x && to.x > x,
            _ => false,
        })
        .count()
}

#[test]
fn a_second_voice_is_written_on_the_notation_staff() {
    let mut song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let _ = song
        .set_note(at(0, 0, 0), string(1), Fret::new(5).unwrap())
        .unwrap();
    let _ = song
        .insert_beat(at(0, 0, 1), NoteValue::new(4, 0, None).unwrap())
        .unwrap();
    let _ = song
        .set_note(at(0, 0, 1), string(6), Fret::new(3).unwrap())
        .unwrap();
    let laid = common::layout(&song, 0);
    // Both voices have a quarter note; both are drawn.
    let heads = glyphs(&laid, Glyph::NoteheadBlack);
    assert_eq!(heads.len(), 2);
    assert_ne!(
        heads[0].1, heads[1].1,
        "the voices are at different pitches"
    );
    // They do not collide, so they share the column's center.
    assert!((heads[0].0 - heads[1].0).abs() < 0.01);
}

#[test]
fn colliding_voices_shift_the_second_notehead() {
    let mut song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    // The same written pitch (A4) on two strings.
    let _ = song
        .set_note(at(0, 0, 0), string(1), Fret::new(5).unwrap())
        .unwrap();
    let _ = song
        .insert_beat(at(0, 0, 1), NoteValue::new(4, 0, None).unwrap())
        .unwrap();
    let _ = song
        .set_note(at(0, 0, 1), string(2), Fret::new(10).unwrap())
        .unwrap();
    let laid = common::layout(&song, 0);
    let heads = glyphs(&laid, Glyph::NoteheadBlack);
    assert_eq!(heads.len(), 2);
    let shift = (heads[1].0 - heads[0].0).abs();
    assert!(
        (0.55..=0.65).contains(&shift),
        "the second voice's head is offset (got {shift})"
    );
}

#[test]
fn a_tie_across_a_barline_is_drawn_on_both_staves() {
    let mut song = testing::song(&[testing::GUITAR_6], 2).unwrap();
    // A quarter note on the last beat of bar 1, tied to the first beat of bar 2.
    let _ = song
        .set_note(at(0, 3, 0), string(1), Fret::new(5).unwrap())
        .unwrap();
    let _ = song
        .set_note(at(1, 0, 0), string(1), Fret::new(5).unwrap())
        .unwrap();
    let _ = song.set_note_tie(at(1, 0, 0), string(1), true).unwrap();
    let laid = common::layout(&song, 0);
    let boundary = laid.places.measures[1].left;
    // One tie on the notation staff, one on the tab.
    assert_eq!(crossing_curves(&laid, boundary), 2);
}

#[test]
fn a_tie_across_a_system_break_is_split_into_fragments() {
    let mut song = testing::song(&[testing::GUITAR_6], 2).unwrap();
    let _ = song
        .set_note(at(0, 3, 0), string(1), Fret::new(5).unwrap())
        .unwrap();
    let _ = song
        .set_note(at(1, 0, 0), string(1), Fret::new(5).unwrap())
        .unwrap();
    let _ = song.set_note_tie(at(1, 0, 0), string(1), true).unwrap();
    let _ = song
        .set_system_break(TrackIndex::new(0), MeasureIndex::new(1), SystemBreak::Line)
        .unwrap();
    let laid = common::layout(&song, 0);
    assert_eq!(laid.places.systems.len(), 2);
    let boundary = laid.places.measures[1].left;
    // No curve runs from one system to the other.
    assert_eq!(crossing_curves(&laid, boundary), 0);
    // Each staff has an outgoing fragment on the first system and an incoming one on the
    // second.
    let first = &laid.places.measures[0];
    let second = &laid.places.measures[1];
    let outgoing = laid
        .ops
        .iter()
        .filter(|op| {
            matches!(op, DrawOp::Curve { from, to, .. }
                if from.x >= first.content && to.x <= first.right)
        })
        .count();
    let incoming = laid
        .ops
        .iter()
        .filter(|op| {
            matches!(op, DrawOp::Curve { from, to, .. }
                if from.x >= second.left && to.x <= second.content)
        })
        .count();
    assert!(outgoing >= 2, "an outgoing tie fragment per staff");
    assert!(incoming >= 2, "an incoming tie fragment per staff");
}

#[test]
fn a_tie_in_the_second_voice_crosses_the_barline() {
    let mut song = testing::song(&[testing::GUITAR_6], 2).unwrap();
    // Voice 2: a whole note in bar 1, tied to the first beat of bar 2.
    let _ = song
        .insert_beat(at(0, 0, 1), NoteValue::new(1, 0, None).unwrap())
        .unwrap();
    let _ = song
        .set_note(at(0, 0, 1), string(6), Fret::new(3).unwrap())
        .unwrap();
    let _ = song
        .insert_beat(at(1, 0, 1), NoteValue::new(1, 0, None).unwrap())
        .unwrap();
    let _ = song
        .set_note(at(1, 0, 1), string(6), Fret::new(3).unwrap())
        .unwrap();
    let _ = song.set_note_tie(at(1, 0, 1), string(6), true).unwrap();
    let laid = common::layout(&song, 0);
    let boundary = laid.places.measures[1].left;
    assert!(
        crossing_curves(&laid, boundary) >= 1,
        "the second voice's tie continues across the barline"
    );
}
