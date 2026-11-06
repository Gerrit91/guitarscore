//! Lyrics under the staves: one syllable per beat, on as many rows as the lines take.

#![allow(unused_crate_dependencies)]

use gscore_layout::{DrawOp, Fonts, LayoutOptions, layout_track};
use gscore_model::testing::{self, TestNote};
use gscore_model::{MeasureIndex, TrackIndex};

#[test]
fn lyric_syllables_are_drawn_under_the_staff() {
    let mut song = testing::song_with_notes(&[
        (4, &[TestNote::new(1, 5)]),
        (4, &[TestNote::new(1, 5)]),
        (4, &[]),
        (4, &[]),
    ])
    .unwrap();
    let _line = song
        .set_lyric_line(0, MeasureIndex::new(0), "Hel lo world")
        .unwrap();
    let fonts = Fonts::bundled().unwrap();
    let layout =
        layout_track(&song, TrackIndex::new(0), &LayoutOptions::default(), &fonts).unwrap();
    let texts: Vec<(String, f32, f32)> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Text { text, at, .. } => Some((text.clone(), at.x, at.y)),
            _ => None,
        })
        .collect();
    let syllables: Vec<&(String, f32, f32)> = texts
        .iter()
        .filter(|(text, _, _)| matches!(text.as_str(), "Hel" | "lo" | "world"))
        .collect();
    assert_eq!(syllables.len(), 3, "{texts:?}");
    // The syllables run left to right.
    assert!(syllables[1].1 > syllables[0].1);
    assert!(syllables[2].1 > syllables[1].1);
    // They share a row under the staff.
    assert!((syllables[0].2 - syllables[1].2).abs() < f32::EPSILON);
}
