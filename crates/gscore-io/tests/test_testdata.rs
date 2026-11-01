//! Import tests over the owner's files in `testdata/`.
//!
//! Expected values come from the owner's Guitar Pro session (`testdata/basic/*.png` shows
//! the score). Change them only together with the owner.

// This whole crate is test code: failing loudly (panic, unwrap, indexing) is how tests
// report problems. clippy's in-test allowances only cover `#[test]` functions, not helpers.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

// Integration tests are separate crates; these dependencies are used by the library only.
use guitarpro as _;

use std::path::PathBuf;

use gscore_io::{import_guitar_pro_from_bytes, FileFormat, Imported, IoError};
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

const DADGAD: &str = "basic/steel_guitar_dadgad.gp";

#[test]
fn dadgad_song_information() {
    let imported = import(DADGAD);
    assert_eq!(imported.format, FileFormat::Gp);
    let song = &imported.song;
    assert_eq!(song.title(), "Steel Guitar DADGAD");
    assert_eq!(song.subtitle(), "Test Material");
    assert_eq!(song.artist(), "GuitarScore");
    assert_eq!(song.album(), "Basic Tests");
    assert_eq!(song.lyricist(), "Gerrit91");
    assert_eq!(song.composer(), "Gerrit91");
    assert_eq!(song.copyright(), "Gerrit91");
    assert_eq!(song.tab_author(), "Gerrit91");
    assert_eq!(song.instructions(), "");
    assert_eq!(song.notes().collect::<Vec<_>>(), ["Used for testing"]);
    assert_eq!(song.tempo(), 120);
    assert_eq!(song.measure_count(), 1);
    assert_eq!(song.track_count(), 1);
}

#[test]
fn dadgad_track_instrument_and_tuning() {
    let imported = import(DADGAD);
    let track = imported.song.track(TrackIndex::new(0)).expect("one track");
    assert_eq!(track.name(), "Steel Guitar");
    assert_eq!(track.instrument_kind(), InstrumentKind::Strings);
    assert_eq!(track.instrument_name(), Some("Steel Guitar"));
    assert_eq!(track.string_count(), 6);

    let tuning = track.tuning().unwrap().expect("a guitar has a tuning");
    let low_to_high: Vec<String> = tuning.iter().rev().map(|s| s.pitch.to_string()).collect();
    assert_eq!(low_to_high, ["D2", "A2", "D3", "G3", "A3", "D4"]);
    // String 1 is the highest.
    let numbers: Vec<u8> = tuning.iter().map(|s| s.string.get()).collect();
    assert_eq!(numbers, [1, 2, 3, 4, 5, 6]);
    assert_eq!(track.tuning_name().unwrap().as_deref(), Some("DADGAD"));
}

#[test]
fn dadgad_notes_are_the_open_strings_from_low_to_high() {
    let imported = import(DADGAD);
    let track = imported.song.track(TrackIndex::new(0)).expect("one track");
    let tuning = track.tuning().unwrap().expect("tuning");
    let measure = track.measure(MeasureIndex::new(0)).expect("one measure");
    assert_eq!(measure.time_signature().unwrap().to_string(), "4/4");

    let voice = measure.voice(VoiceIndex::new(0)).expect("first voice");
    let eighth = Duration::new(1, 8).unwrap();
    let mut played = Vec::new();
    for beat in voice.beats() {
        match beat.kind() {
            BeatKind::Normal => {
                assert_eq!(beat.duration().unwrap(), eighth, "{}", beat.index());
                let notes: Vec<_> = beat.notes().collect();
                assert_eq!(notes.len(), 1, "{}", beat.index());
                let note = notes[0];
                assert_eq!(note.kind(), NoteKind::Normal);
                assert_eq!(note.fret().unwrap().get(), 0);
                let string = note.string().unwrap();
                let open = tuning
                    .iter()
                    .find(|s| s.string == string)
                    .expect("note on an existing string");
                played.push((string.get(), open.pitch.to_string()));
            }
            BeatKind::Rest => {
                assert_eq!(beat.duration().unwrap(), Duration::new(1, 4).unwrap());
                played.push((0, "rest".to_owned()));
            }
            BeatKind::Empty => {}
        }
    }
    let expected = [
        (6, "D2"),
        (5, "A2"),
        (4, "D3"),
        (3, "G3"),
        (2, "A3"),
        (1, "D4"),
        (0, "rest"),
    ];
    let expected: Vec<(u8, String)> = expected.iter().map(|&(s, p)| (s, p.to_owned())).collect();
    assert_eq!(played, expected);
    // The other voices are unused, and the measure is exactly full.
    assert!(measure.voices().skip(1).all(|v| v.is_empty()));
    assert_eq!(imported.song.issues(), []);
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
    let original = read(DADGAD);
    let mut inputs: Vec<Vec<u8>> = Vec::new();
    // Every prefix of the first 1 KiB (headers), then every 97th length.
    let lengths = (0..1024.min(original.len())).chain((1024..original.len()).step_by(97));
    for len in lengths {
        inputs.push(original[..len].to_vec());
    }
    // Flip one byte at a time, spread over the whole file.
    for pos in (0..original.len()).step_by(31) {
        let mut mutated = original.clone();
        mutated[pos] ^= 0xFF;
        inputs.push(mutated);
    }

    let mut internal_panics = Vec::new();
    for (i, bytes) in inputs.iter().enumerate() {
        if let Err(IoError::GuitarProError(msg)) = import_guitar_pro_from_bytes(bytes) {
            if msg.contains("panicked") {
                internal_panics.push(format!("input {i} ({} bytes): {msg}", bytes.len()));
            }
        }
    }
    assert!(
        internal_panics.is_empty(),
        "the parser panicked (caught) on {} of {} inputs, e.g. {:?}",
        internal_panics.len(),
        inputs.len(),
        internal_panics.first()
    );
}
