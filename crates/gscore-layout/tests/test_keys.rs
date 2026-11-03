//! Key signatures: their accidentals at the start of each system, and the accidentals that
//! the key signature makes unnecessary.

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
use gscore_model::{MeasureIndex, testing};

/// Where the layout draws `glyph`.
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

#[test]
fn a_sharp_key_signature_is_written_at_each_system_start() {
    let mut song = testing::song(&[testing::GUITAR_6], 4).unwrap();
    let _ = song
        .set_key_signature(MeasureIndex::new(0), 2, false)
        .unwrap();
    let laid = common::layout(&song, 0);
    let systems = laid.places.systems.len();
    assert!(systems >= 1);
    // D major has F sharp and C sharp, at the start of every system.
    assert_eq!(glyphs(&laid, Glyph::Sharp).len(), 2 * systems);
    assert!(glyphs(&laid, Glyph::Flat).is_empty());
}

#[test]
fn a_key_signature_removes_the_accidentals_of_its_notes() {
    // String 1, fret 2 sounds F sharp.
    let song = common::with_notes(
        testing::song(&[testing::GUITAR_6], 1).unwrap(),
        &[(0, 0, 1, 2)],
    );
    // In C major the note needs a sharp accidental.
    let plain = common::layout(&song, 0);
    assert_eq!(glyphs(&plain, Glyph::Sharp).len(), 1);
    // In D major the key signature has both sharps; the note adds none.
    let mut keyed = song.clone();
    let _ = keyed
        .set_key_signature(MeasureIndex::new(0), 2, false)
        .unwrap();
    let laid = common::layout(&keyed, 0);
    assert_eq!(glyphs(&laid, Glyph::Sharp).len(), 2);
}

#[test]
fn a_flat_key_writes_flats_and_removes_their_accidentals() {
    // String 3, fret 3 sounds B flat.
    let song = common::with_notes(
        testing::song(&[testing::GUITAR_6], 1).unwrap(),
        &[(0, 0, 3, 3)],
    );
    // C major spells it A sharp and shows the accidental.
    let plain = common::layout(&song, 0);
    assert_eq!(glyphs(&plain, Glyph::Sharp).len(), 1);
    assert!(glyphs(&plain, Glyph::Flat).is_empty());
    // F major's signature has the B flat; the note adds no accidental.
    let mut keyed = song.clone();
    let _ = keyed
        .set_key_signature(MeasureIndex::new(0), -1, false)
        .unwrap();
    let laid = common::layout(&keyed, 0);
    assert_eq!(glyphs(&laid, Glyph::Flat).len(), 1);
    assert!(glyphs(&laid, Glyph::Sharp).is_empty());
}

#[test]
fn a_key_change_cancels_the_accidentals_it_drops() {
    let mut song = testing::song(&[testing::GUITAR_6], 2).unwrap();
    let _ = song
        .set_key_signature(MeasureIndex::new(0), 2, false)
        .unwrap();
    let _ = song
        .set_key_signature(MeasureIndex::new(1), 0, false)
        .unwrap();
    let laid = common::layout(&song, 0);
    // D major's two sharps at the start, then F natural and C natural to cancel them.
    assert_eq!(glyphs(&laid, Glyph::Sharp).len(), 2);
    assert_eq!(glyphs(&laid, Glyph::Natural).len(), 2);
}

#[test]
fn a_tab_only_score_shows_the_key_signature_on_the_tab_staff() {
    let mut song = testing::song(&[testing::GUITAR_6], 2).unwrap();
    let _ = song
        .set_key_signature(MeasureIndex::new(0), -2, false)
        .unwrap();
    let laid = common::layout_tab_only(&song, 0);
    let systems = laid.places.systems.len();
    let flats = glyphs(&laid, Glyph::Flat);
    assert_eq!(flats.len(), 2 * systems);
    // They stay within the tab staff's height.
    let system = &laid.places.systems[0];
    let bottom = system.top + 5.0 * system.string_spacing;
    for (_, y) in flats {
        assert!(
            (system.top..=bottom).contains(&y),
            "a key signature at y {y} is outside the staff {system:?}"
        );
    }
}

#[test]
fn a_drum_track_has_no_key_signature() {
    let mut song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let _ = song
        .set_track_drums(gscore_model::TrackIndex::new(0), true)
        .unwrap();
    let _ = song
        .set_key_signature(MeasureIndex::new(0), 3, false)
        .unwrap();
    let laid = common::layout(&song, 0);
    assert!(glyphs(&laid, Glyph::Sharp).is_empty());
}
