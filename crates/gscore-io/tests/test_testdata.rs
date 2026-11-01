//! Import tests over the owner's files in `testdata/`.
//!
//! Expected values come from the owner's Guitar Pro session (`testdata/basic/*.png` and
//! `*.pdf` show the score). The same song is exported as GP7, GP6 and GP5; all must import
//! to the same music. Change expected values only together with the owner.

// This whole crate is test code: failing loudly (panic, unwrap, indexing) is how tests
// report problems. clippy's in-test allowances only cover `#[test]` functions, not helpers.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;

use gscore_io::{FileFormat, Imported, IoError, import_guitar_pro_from_bytes};
use gscore_model::{
    BeatKind, Duration, InstrumentKind, MeasureIndex, NoteKind, TrackIndex, VoiceIndex,
};

fn testdata(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative)
}

fn read(relative: &str) -> Vec<u8> {
    let path = testdata(relative);
    std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn import(relative: &str) -> Imported {
    import_guitar_pro_from_bytes(&read(relative))
        .unwrap_or_else(|e| panic!("cannot import {relative}: {e}"))
}

/// The DADGAD test file, exported by Guitar Pro in each format we read, with what each
/// export is expected to contain beyond the shared music.
struct DadgadExport {
    path: &'static str,
    format: FileFormat,
    /// GP5 exports drop the notes ("notices").
    notes: &'static [&'static str],
    /// Only GP7 stores an instrument name; GP5 has none, GP6 only an instrument id.
    instrument_name: Option<&'static str>,
}

const DADGAD_EXPORTS: [DadgadExport; 3] = [
    DadgadExport {
        path: "basic/steel_guitar_dadgad.gp",
        format: FileFormat::Gp,
        notes: &["Used for testing"],
        instrument_name: Some("Steel Guitar"),
    },
    DadgadExport {
        path: "basic/steel_guitar_dadgad.gpx",
        format: FileFormat::Gpx,
        notes: &["Used for testing"],
        instrument_name: None,
    },
    DadgadExport {
        path: "basic/steel_guitar_dadgad.gp5",
        format: FileFormat::Gp5,
        notes: &[],
        instrument_name: None,
    },
];

#[test]
fn dadgad_song_information() {
    for export in &DADGAD_EXPORTS {
        let imported = import(export.path);
        let path = export.path;
        assert_eq!(imported.format, export.format, "{path}");
        let song = &imported.song;
        assert_eq!(song.title(), "Steel Guitar DADGAD", "{path}");
        assert_eq!(song.subtitle(), "Test Material", "{path}");
        assert_eq!(song.artist(), "GuitarScore", "{path}");
        assert_eq!(song.album(), "Basic Tests", "{path}");
        assert_eq!(song.lyricist(), "Gerrit91", "{path}");
        assert_eq!(song.composer(), "Gerrit91", "{path}");
        assert_eq!(song.copyright(), "Gerrit91", "{path}");
        assert_eq!(song.tab_author(), "Gerrit91", "{path}");
        assert_eq!(song.instructions(), "", "{path}");
        assert_eq!(song.notes().collect::<Vec<_>>(), export.notes, "{path}");
        assert_eq!(song.tempo(), 120, "{path}");
        assert_eq!(song.measure_count(), 1, "{path}");
        assert_eq!(song.track_count(), 1, "{path}");
    }
}

#[test]
fn dadgad_track_instrument_and_tuning() {
    for export in &DADGAD_EXPORTS {
        let path = export.path;
        let imported = import(path);
        let track = imported.song.track(TrackIndex::new(0)).expect("one track");
        assert_eq!(track.name(), "Steel Guitar", "{path}");
        assert_eq!(track.instrument_kind(), InstrumentKind::Strings, "{path}");
        assert_eq!(track.instrument_name(), export.instrument_name, "{path}");
        assert_eq!(track.string_count(), 6, "{path}");

        let tuning = track.tuning().unwrap().expect("a guitar has a tuning");
        let low_to_high: Vec<String> = tuning.iter().rev().map(|s| s.pitch.to_string()).collect();
        assert_eq!(low_to_high, ["D2", "A2", "D3", "G3", "A3", "D4"], "{path}");
        // String 1 is the highest.
        let numbers: Vec<u8> = tuning.iter().map(|s| s.string.get()).collect();
        assert_eq!(numbers, [1, 2, 3, 4, 5, 6], "{path}");
        assert_eq!(
            track.tuning_name().unwrap().as_deref(),
            Some("DADGAD"),
            "{path}"
        );
    }
}

#[test]
fn dadgad_notes_are_the_open_strings_from_low_to_high() {
    for export in &DADGAD_EXPORTS {
        let path = export.path;
        assert_eq!(played_notes(path), expected_dadgad_notes(), "{path}");
        assert_eq!(import(path).song.issues(), [], "{path}");
    }
}

/// (string, open-string pitch) per beat of the first voice; rests as (0, "rest").
fn expected_dadgad_notes() -> Vec<(u8, String)> {
    [
        (6, "D2"),
        (5, "A2"),
        (4, "D3"),
        (3, "G3"),
        (2, "A3"),
        (1, "D4"),
        (0, "rest"),
    ]
    .iter()
    .map(|&(s, p)| (s, p.to_owned()))
    .collect()
}

/// The beats of the first voice of the only measure, checking durations and that every
/// other voice is unused.
fn played_notes(path: &str) -> Vec<(u8, String)> {
    let imported = import(path);
    let track = imported.song.track(TrackIndex::new(0)).expect("one track");
    let tuning = track.tuning().unwrap().expect("tuning");
    let measure = track.measure(MeasureIndex::new(0)).expect("one measure");
    assert_eq!(
        measure.time_signature().unwrap().to_string(),
        "4/4",
        "{path}"
    );
    assert!(measure.voices().skip(1).all(|v| v.is_empty()), "{path}");

    let voice = measure.voice(VoiceIndex::new(0)).expect("first voice");
    let mut played = Vec::new();
    for beat in voice.beats() {
        let at = format!("{path}, {}", beat.index());
        match beat.kind() {
            BeatKind::Normal => {
                assert_eq!(
                    beat.duration().unwrap(),
                    Duration::new(1, 8).unwrap(),
                    "{at}"
                );
                let notes: Vec<_> = beat.notes().collect();
                assert_eq!(notes.len(), 1, "{at}");
                let note = notes[0];
                assert_eq!(note.kind(), NoteKind::Normal, "{at}");
                assert_eq!(note.fret().unwrap().get(), 0, "{at}");
                let string = note.string().unwrap();
                let open = tuning
                    .iter()
                    .find(|s| s.string == string)
                    .expect("note on an existing string");
                played.push((string.get(), open.pitch.to_string()));
            }
            BeatKind::Rest => {
                assert_eq!(
                    beat.duration().unwrap(),
                    Duration::new(1, 4).unwrap(),
                    "{at}"
                );
                played.push((0, "rest".to_owned()));
            }
            BeatKind::Empty => {}
        }
    }
    played
}

#[test]
fn corrupt_files_are_rejected_cleanly() {
    let dir = testdata("corrupt");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("testdata/corrupt exists") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|e| e == "md") {
            continue;
        }
        let bytes = std::fs::read(&path).expect("readable");
        assert!(
            import_guitar_pro_from_bytes(&bytes).is_err(),
            "{} imported, but should fail",
            path.display()
        );
        checked += 1;
    }
    assert!(checked > 0, "no corrupt test files found");
}

/// Truncated and byte-flipped copies of a valid file must give `Ok` or `Err`, never a panic,
/// and the parser must not even panic internally (`catch_unwind` is only a safety net).
#[test]
fn truncated_and_mutated_files_never_panic() {
    let mut inputs: Vec<Vec<u8>> = Vec::new();
    for export in &DADGAD_EXPORTS {
        add_damaged_copies(&read(export.path), &mut inputs);
    }
    check_no_panics(&inputs);
}

/// Adds truncated and byte-flipped copies of `original` to `inputs`.
fn add_damaged_copies(original: &[u8], inputs: &mut Vec<Vec<u8>>) {
    // Every prefix of the first 1 KiB (headers), then every 97th length.
    let lengths = (0..1024.min(original.len())).chain((1024..original.len()).step_by(97));
    for len in lengths {
        inputs.push(original[..len].to_vec());
    }
    // Flip one byte at a time, spread over the whole file.
    for pos in (0..original.len()).step_by(31) {
        let mut mutated = original.to_vec();
        mutated[pos] ^= 0xFF;
        inputs.push(mutated);
    }
}

/// Every input must import to `Ok` or `Err` without the parser panicking, even internally.
fn check_no_panics(inputs: &[Vec<u8>]) {
    let mut internal_panics = Vec::new();
    for (i, bytes) in inputs.iter().enumerate() {
        if let Err(IoError::GuitarProError(msg)) = import_guitar_pro_from_bytes(bytes)
            && msg.contains("panicked")
        {
            internal_panics.push(format!("input {i} ({} bytes): {msg}", bytes.len()));
        }
    }
    assert!(
        internal_panics.is_empty(),
        "the parser panicked (caught) on {} of {} inputs:\n{}",
        internal_panics.len(),
        inputs.len(),
        internal_panics.join("\n")
    );
}
