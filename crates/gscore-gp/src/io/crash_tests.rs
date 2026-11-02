//! Regression tests for crashes fixed while bringing this crate under the workspace's
//! no-panic rules (AGENTS.md rule 10): each one panicked before its fix.

use crate::io::Song;
use crate::model::{Beat, Chord, Measure, Note, NoteType, Voice};

/// A one-track, one-measure song whose only beat is `beat`.
fn song_with(beat: Beat) -> Song {
    let mut song = Song::default();
    song.tracks.push(Default::default());
    song.measure_headers.push(Default::default());
    song.tracks[0].measures.push(Measure {
        voices: vec![Voice {
            beats: vec![beat],
            ..Voice::default()
        }],
        ..Default::default()
    });
    song
}

fn read_back(bytes: &[u8], version: u8) -> Song {
    let mut song = Song::default();
    match version {
        3 => song.read_gp3(bytes),
        4 => song.read_gp4(bytes),
        _ => song.read_gp5(bytes),
    }
    .expect("read back");
    song
}

/// Track names longer than their 40-byte field underflowed the padding (`40 - len`).
#[test]
fn long_track_names_are_cut_to_their_field() {
    let long = "A very long track name that does not fit in forty bytes";
    for (version, number) in [((3, 0, 0), 3), ((4, 0, 0), 4), ((5, 1, 0), 5)] {
        let mut song = song_with(Beat::default());
        song.tracks[0].name = long.to_owned();
        let bytes = song.write(version, None).expect("written");
        assert_eq!(read_back(&bytes, number).tracks[0].name, long[..40]);
    }
}

/// Chord names longer than their 22-byte field underflowed the padding too.
#[test]
fn long_chord_names_are_cut_to_their_field() {
    let chord = Chord {
        name: "Cmaj7(add9) over G, played softly".to_owned(),
        new_format: Some(true),
        ..Chord::default()
    };
    for (version, number) in [((3, 0, 0), 3), ((5, 1, 0), 5)] {
        let mut beat = Beat::default();
        beat.effect.chord = Some(chord.clone());
        let bytes = song_with(beat).write(version, None).expect("written");
        let reread = read_back(&bytes, number);
        let name = reread.tracks[0].measures[0].voices[0].beats[0]
            .effect
            .chord
            .as_ref()
            .map(|c| c.name.clone());
        assert_eq!(name.as_deref(), Some(&chord.name[..22]), "GP{number}");
    }
}

/// GP5 counts plays, so writing added 1 to the repeat count: 127 overflowed.
#[test]
fn the_largest_repeat_count_writes_to_gp5() {
    let mut song = song_with(Beat::default());
    song.measure_headers[0].repeat_open = true;
    song.measure_headers[0].repeat_close = i8::MAX;
    let bytes = song.write((5, 1, 0), None).expect("written");
    assert_eq!(
        read_back(&bytes, 5).measure_headers[0].repeat_close,
        i8::MAX - 1
    );
}

/// A note on string 8 (an 8-string guitar from GP7) overflowed `1 << (7 - string)`. The
/// binary formats store 7 strings at most, so writing is refused with an error.
#[test]
fn notes_beyond_string_seven_are_refused_not_a_panic() {
    let beat = Beat {
        notes: vec![Note {
            string: 8,
            value: 3,
            kind: NoteType::Normal,
            ..Note::default()
        }],
        ..Beat::default()
    };
    let error = song_with(beat).write((5, 1, 0), None).unwrap_err();
    assert!(error.to_string().contains("string number"), "{error}");
}

/// The page setup's copyright is two lines; one without a line break was indexed past
/// its end.
#[test]
fn a_copyright_without_a_line_break_writes_to_gp5() {
    let mut song = song_with(Beat::default());
    song.page_setup.copyright = "All mine".to_owned();
    let bytes = song.write((5, 1, 0), None).expect("written");
    assert_eq!(read_back(&bytes, 5).page_setup.copyright, "All mine\n");
}

/// The sounding pitch of a note on a string the track does not have indexed past the
/// tuning.
#[test]
fn a_note_on_a_missing_string_has_no_pitch_instead_of_a_panic() {
    let note = Note {
        string: 9,
        value: 2,
        ..Note::default()
    };
    let strings = [(1, 64), (2, 59), (3, 55), (4, 50), (5, 45), (6, 40)];
    assert!(note.real_value(&strings).is_err());
    let on_string = Note { string: 2, ..note };
    assert_eq!(on_string.real_value(&strings).unwrap(), 61);
}
