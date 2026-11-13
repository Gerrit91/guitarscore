//! Shared helpers for the layout integration tests.

#![allow(dead_code)]
// Integration tests are test code: failing loudly is how they report a problem.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::path::PathBuf;

use gscore_layout::{Fonts, Layout, LayoutOptions, layout_track};
use gscore_model::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, Song, StringNumber, TrackIndex, VoiceIndex,
};

/// Every file in `testdata/basic` that Guitar Pro exported.
pub(crate) const TESTDATA: [&str; 15] = [
    "basic/steel_guitar_dadgad.gp",
    "basic/steel_guitar_dadgad.gpx",
    "basic/steel_guitar_dadgad.gp5",
    "basic/note_lengths.gp",
    "basic/note_lengths.gpx",
    "basic/note_lengths.gp5",
    "basic/ornaments.gp",
    "basic/ornaments.gpx",
    "basic/ornaments.gp5",
    "basic/advanced_ornaments.gp",
    "basic/advanced_ornaments.gpx",
    "basic/advanced_ornaments.gp5",
    "basic/multi_staff.gp",
    "basic/multi_staff.gpx",
    "basic/staff.gp",
];

pub(crate) fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Opens a file from `testdata/`.
pub(crate) fn open(relative: &str) -> Song {
    let path = crate_dir().join("../../testdata").join(relative);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    gscore_io::import_guitar_pro_from_bytes(&bytes)
        .unwrap_or_else(|e| panic!("{relative}: {e}"))
        .song
}

/// Lays out `track` with the default options and the bundled fonts.
pub(crate) fn layout(song: &Song, track: usize) -> Layout {
    let fonts = Fonts::bundled().unwrap();
    layout_track(
        song,
        TrackIndex::new(track),
        &LayoutOptions::default(),
        &fonts,
    )
    .unwrap()
}

/// Lays out `track` as tab only, without the notation staff.
pub(crate) fn layout_tab_only(song: &Song, track: usize) -> Layout {
    let fonts = Fonts::bundled().unwrap();
    let options = LayoutOptions {
        notation: false,
        ..LayoutOptions::default()
    };
    layout_track(song, TrackIndex::new(track), &options, &fonts).unwrap()
}

/// Sets frets in the first track: (measure, beat, string, fret), all 0-based except the
/// 1-based string.
pub(crate) fn with_notes(mut song: Song, notes: &[(usize, usize, u8, u8)]) -> Song {
    for &(measure, beat, string, fret) in notes {
        let at = BeatPosition {
            staff: gscore_model::StaffIndex::new(0),
            track: TrackIndex::new(0),
            measure: MeasureIndex::new(measure),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(beat),
        };
        // The `Change` (for undo) is not needed here.
        let _change = song
            .set_note(
                at,
                StringNumber::new(string).unwrap(),
                Fret::new(fret).unwrap(),
            )
            .unwrap();
    }
    song
}
