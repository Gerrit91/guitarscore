//! Lyrics under the staves: one syllable per beat, on as many rows as the lines take.

#![allow(unused_crate_dependencies)]

use gscore_layout::{DrawOp, Fonts, LayoutOptions, TextFont, layout_track};
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

/// The lyric syllables (baseline and font) and the baselines of the fret numbers "5".
#[allow(clippy::unwrap_used)]
fn placed(options: &LayoutOptions) -> (Vec<(f32, TextFont)>, Vec<f32>) {
    let mut song = testing::song_with_notes(&[
        (4, &[TestNote::new(6, 5)]),
        (4, &[TestNote::new(6, 5)]),
        (4, &[TestNote::new(6, 5)]),
        (4, &[]),
    ])
    .unwrap();
    let _line = song
        .set_lyric_line(0, MeasureIndex::new(0), "Hel lo+you")
        .unwrap();
    let fonts = Fonts::bundled().unwrap();
    let layout = layout_track(&song, TrackIndex::new(0), options, &fonts).unwrap();
    let mut lyrics = Vec::new();
    let mut frets = Vec::new();
    for op in &layout.ops {
        if let DrawOp::Text { text, at, font, .. } = op {
            match text.as_str() {
                "Hel" | "lo you" => lyrics.push((at.y, *font)),
                "5" => frets.push(at.y),
                _ => {}
            }
        }
    }
    (lyrics, frets)
}

#[test]
fn lyrics_go_between_notation_and_tab_in_bold() {
    let (lyrics, frets) = placed(&LayoutOptions::default());
    // Two syllables: "lo+you" shares one beat, with a space.
    assert_eq!(lyrics.len(), 2, "{lyrics:?}");
    assert!(lyrics.iter().all(|(_, font)| *font == TextFont::Bold));
    let lowest_fret = frets.iter().copied().fold(f32::MIN, f32::max);
    let tab_top = lowest_fret - 5.0 * LayoutOptions::default().tab_line_spacing;
    // Above the tab (its top line), not under it.
    assert!(
        lyrics.iter().all(|(y, _)| *y < tab_top),
        "{lyrics:?} {tab_top}"
    );
}

#[test]
fn without_notation_lyrics_go_under_the_tab() {
    let options = LayoutOptions {
        notation: false,
        ..LayoutOptions::default()
    };
    let (lyrics, frets) = placed(&options);
    let lowest_fret = frets.iter().copied().fold(f32::MIN, f32::max);
    // Below the fret numbers on the lowest string, clear of them.
    assert!(
        lyrics.iter().all(|(y, _)| *y > lowest_fret + 1.0),
        "{lyrics:?} {lowest_fret}"
    );
}
