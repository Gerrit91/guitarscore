//! Shared helpers for the `gscore-edit` integration tests.

#![allow(dead_code)]
// Integration tests are test code: failing loudly is how they report a problem.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use gscore_edit::{Command, Editor};
use gscore_io::{FileFormat, export_to_bytes, import_guitar_pro_from_bytes};
use gscore_model::{
    BeatIndex, BeatPosition, MeasureIndex, NoteValue, Song, TrackIndex, VoiceIndex, differences,
    settings_differences, testing,
};

/// A position in the first track.
pub(crate) fn pos(measure: usize, voice: usize, beat: usize) -> BeatPosition {
    BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(voice),
        beat: BeatIndex::new(beat),
    }
}

/// A quarter note.
pub(crate) fn quarter() -> NoteValue {
    NoteValue::new(4, 0, None).unwrap()
}

/// A plain note length.
pub(crate) fn note(value: u16) -> NoteValue {
    NoteValue::new(value, 0, None).unwrap()
}

/// Whether `after` equals `before` in everything the command tests compare: the musical
/// content, plus the track settings that a save may not keep (color, mute, solo, sound).
fn same_state(before: &Song, after: &Song) -> bool {
    differences(before, after).is_empty() && settings_differences(before, after).is_empty()
}

/// Applies `command` to `song`, checks that it changed something, undoes it, and checks that
/// the song is exactly as it was. The one helper every command test uses (AGENTS.md M4).
pub(crate) fn apply_then_undo(song: &Song, command: &Command) {
    let mut editor = Editor::new(song.clone());
    let before = editor.song().clone();
    editor
        .execute(command.clone())
        .unwrap_or_else(|error| panic!("{} on {:?}: {error}", command.id(), song.title()));
    assert!(
        !same_state(&before, editor.song()),
        "{} changed nothing",
        command.id()
    );
    editor
        .undo()
        .unwrap_or_else(|error| panic!("undo {}: {error}", command.id()));
    assert!(
        same_state(&before, editor.song()),
        "undo of {} did not restore the song: {:?}",
        command.id(),
        differences(&before, editor.song())
    );
}

/// Applies every command that succeeds, then undoes everything; the song must be back to its
/// original content.
pub(crate) fn apply_all_then_undo_all(song: &Song, commands: &[Command]) {
    let mut editor = Editor::new(song.clone());
    for command in commands {
        let _ = editor.execute(command.clone());
    }
    while editor.can_undo() {
        editor.undo().unwrap();
    }
    assert!(
        same_state(song, editor.song()),
        "undoing everything did not restore the song: {:?}",
        differences(song, editor.song())
    );
}

/// Writes the song as GP7, reads it back and checks that nothing the model compares changed.
pub(crate) fn assert_roundtrips(song: &Song) {
    let bytes = export_to_bytes(song, FileFormat::Gp).expect("write as GP7");
    let reread = import_guitar_pro_from_bytes(&bytes).expect("read back the written GP7");
    let diffs = differences(song, &reread.song);
    assert!(diffs.is_empty(), "round trip changed the song: {diffs:?}");
}

/// Songs of several shapes: empty, one measure, several measures, bass, 7- and 8-string
/// guitars, and a large score. Commands must handle all of them without panicking.
pub(crate) fn test_songs() -> Vec<(&'static str, Song)> {
    vec![
        ("empty", testing::empty()),
        (
            "one measure",
            testing::song(&[testing::GUITAR_6], 1).unwrap(),
        ),
        (
            "three measures",
            testing::song(&[testing::GUITAR_6], 3).unwrap(),
        ),
        (
            "two tracks",
            testing::song(&[testing::GUITAR_6, testing::BASS_4], 2).unwrap(),
        ),
        (
            "four-string bass",
            testing::song(&[testing::BASS_4], 2).unwrap(),
        ),
        (
            "seven-string",
            testing::song(&[testing::GUITAR_7], 2).unwrap(),
        ),
        (
            "eight-string",
            testing::song(&[testing::GUITAR_8], 2).unwrap(),
        ),
        ("huge", testing::song(&[testing::GUITAR_6], 256).unwrap()),
    ]
}
