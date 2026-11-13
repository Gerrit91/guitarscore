//! Drum tracks: written on a percussion staff, each drum sound in its place with its own
//! notehead, stems up, no tablature.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use gscore_layout::{ColorToken, DrawOp, Fonts, Glyph, LayoutCache, LayoutOptions, layout_track};
use gscore_model::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, Song, StringNumber, TrackIndex, VoiceIndex,
    testing,
};

fn at(beat: usize) -> BeatPosition {
    BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(beat),
    }
}

/// One measure of a drum kit: bass drum and closed hi-hat, hi-hat, snare and hi-hat, rest.
fn drum_song() -> Song {
    let mut song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let _ = song.set_track_drums(TrackIndex::new(0), true).unwrap();
    let mut put = |beat: usize, string: u8, sound: u8| {
        let _ = song
            .set_note(
                at(beat),
                StringNumber::new(string).unwrap(),
                Fret::new(sound).unwrap(),
            )
            .unwrap();
    };
    put(0, 1, 42);
    put(0, 2, 36);
    put(1, 1, 42);
    put(2, 1, 42);
    put(2, 2, 38);
    song
}

fn glyphs(ops: &[DrawOp]) -> Vec<(Glyph, f32, f32)> {
    ops.iter()
        .filter_map(|op| match op {
            DrawOp::Glyph { glyph, at, .. } => Some((*glyph, at.x, at.y)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_drum_kit_is_written_on_a_percussion_staff() {
    let song = drum_song();
    let fonts = Fonts::bundled().unwrap();
    let layout =
        layout_track(&song, TrackIndex::new(0), &LayoutOptions::default(), &fonts).unwrap();
    let glyphs = glyphs(&layout.ops);
    let has = |g: Glyph| glyphs.iter().any(|(glyph, _, _)| *glyph == g);
    assert!(has(Glyph::PercussionClef));
    assert!(
        !has(Glyph::TabClef6) && !has(Glyph::GClef),
        "no tab and no pitched clef"
    );
    // Three hi-hats with cross heads, a bass drum and a snare with normal heads.
    let crosses: Vec<_> = glyphs.iter().filter(|g| g.0 == Glyph::NoteheadX).collect();
    let normal: Vec<_> = glyphs
        .iter()
        .filter(|g| g.0 == Glyph::NoteheadBlack)
        .collect();
    assert_eq!((crosses.len(), normal.len()), (3, 2));
    // The hi-hat sits above the snare, the snare above the bass drum (y grows down).
    let bass = normal.iter().map(|g| g.2).fold(f32::MIN, f32::max);
    let snare = normal.iter().map(|g| g.2).fold(f32::MAX, f32::min);
    assert!(crosses.iter().all(|g| g.2 < snare) && snare < bass);
    // No fret numbers: the drum sounds are the noteheads.
    assert!(!layout.ops.iter().any(|op| matches!(
        op,
        DrawOp::Text {
            color: ColorToken::FretNumber,
            ..
        }
    )));
    // Five staff lines per system, one system.
    let staff_lines = layout
        .ops
        .iter()
        .filter(|op| {
            matches!(
                op,
                DrawOp::Line {
                    color: ColorToken::Staff,
                    ..
                }
            )
        })
        .count();
    assert_eq!(staff_lines, 5);
    // The cursor can still reach every "string" of the kit.
    assert_eq!(layout.places.systems[0].strings, 6);
    assert_eq!(layout.places.measures[0].beats.len(), 4);
}

#[test]
fn drum_stems_point_up() {
    let song = drum_song();
    let fonts = Fonts::bundled().unwrap();
    let layout =
        layout_track(&song, TrackIndex::new(0), &LayoutOptions::default(), &fonts).unwrap();
    let heads: Vec<f32> = glyphs(&layout.ops)
        .iter()
        .filter(|g| matches!(g.0, Glyph::NoteheadX | Glyph::NoteheadBlack))
        .map(|g| g.2)
        .collect();
    let lowest_head = heads.iter().copied().fold(f32::MIN, f32::max);
    let stem = fonts.music.engraving().stem_thickness;
    let stems: Vec<(f32, f32)> = layout
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Line {
                from,
                to,
                thickness,
                color: ColorToken::Notation,
            } if (*thickness - stem).abs() < 1e-6 && (from.x - to.x).abs() < 1e-6 => {
                Some((from.y, to.y))
            }
            _ => None,
        })
        .collect();
    assert_eq!(stems.len(), 3, "{stems:?}");
    for (from, to) in stems {
        assert!(to < from, "the free end is above the heads");
        assert!(from <= lowest_head + 1.0);
    }
}

#[test]
fn switching_a_track_to_drums_lays_it_out_again() {
    let mut song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let _ = song
        .set_note(at(0), StringNumber::new(1).unwrap(), Fret::new(42).unwrap())
        .unwrap();
    let fonts = Fonts::bundled().unwrap();
    let options = LayoutOptions::default();
    let mut cache = LayoutCache::default();
    let guitar = cache
        .layout(&song, TrackIndex::new(0), &options, &fonts)
        .unwrap();
    assert!(glyphs(&guitar.ops).iter().any(|g| g.0 == Glyph::GClef));
    let _ = song.set_track_drums(TrackIndex::new(0), true).unwrap();
    let drums = cache
        .layout(&song, TrackIndex::new(0), &options, &fonts)
        .unwrap();
    assert_eq!(
        drums,
        layout_track(&song, TrackIndex::new(0), &options, &fonts).unwrap()
    );
    assert!(glyphs(&drums.ops).iter().any(|g| g.0 == Glyph::NoteheadX));
}
