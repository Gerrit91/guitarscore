//! Command behaviour: each command edits what it says, undo restores the song, and every
//! successful edit still saves and reloads without differences.

// This whole crate is test code: failing loudly (panic, unwrap, indexing) is how tests
// report problems. clippy's in-test allowances only cover `#[test]` functions, not helpers.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

mod common;

use common::{
    apply_all_then_undo_all, apply_then_undo, assert_roundtrips, note, pos, quarter, test_songs,
};
use gscore_edit::{Command, EditError, Editor};
use gscore_model::{
    BeatKind, Duration, Fret, MeasureIndex, NoteValue, StringNumber, TimeSignature, differences,
    testing,
};

fn string(number: u8) -> StringNumber {
    StringNumber::new(number).unwrap()
}

fn fret(value: u8) -> Fret {
    Fret::new(value).unwrap()
}

#[test]
fn set_fret_adds_a_note_and_undo_restores_the_song() {
    let song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let command = Command::SetFret {
        at: pos(0, 0, 0),
        string: string(6),
        fret: fret(5),
    };
    let mut editor = Editor::new(song.clone());
    editor.execute(command.clone()).unwrap();
    let beat = editor.song().beat(pos(0, 0, 0)).unwrap();
    assert_eq!(beat.kind(), BeatKind::Normal);
    let notes: Vec<_> = beat.notes().collect();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].fret().unwrap(), fret(5));

    apply_then_undo(&song, &command);
}

#[test]
fn clear_note_removes_a_note_and_the_last_one_makes_a_rest() {
    let song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let mut editor = Editor::new(song.clone());
    editor
        .execute(Command::SetFret {
            at: pos(0, 0, 0),
            string: string(1),
            fret: fret(5),
        })
        .unwrap();
    let with_note = editor.song().clone();

    let clear = Command::ClearNote {
        at: pos(0, 0, 0),
        string: string(1),
    };
    editor.execute(clear.clone()).unwrap();
    assert_eq!(
        editor.song().beat(pos(0, 0, 0)).unwrap().kind(),
        BeatKind::Rest
    );

    apply_then_undo(&with_note, &clear);
}

#[test]
fn set_duration_changes_the_beat_length() {
    let song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let command = Command::SetDuration {
        at: pos(0, 0, 0),
        duration: NoteValue::new(8, 1, Some((3, 2))).unwrap(),
    };
    let mut editor = Editor::new(song.clone());
    editor.execute(command.clone()).unwrap();
    assert_eq!(
        editor
            .song()
            .beat(pos(0, 0, 0))
            .unwrap()
            .duration()
            .unwrap(),
        Duration::new(1, 8).unwrap()
    );

    apply_then_undo(&song, &command);
}

#[test]
fn insert_and_delete_beat_change_the_voice_and_undo_it() {
    let song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let mut editor = Editor::new(song.clone());
    let insert = Command::InsertBeat {
        at: pos(0, 0, 1),
        duration: quarter(),
    };
    editor.execute(insert.clone()).unwrap();
    assert_eq!(
        editor.song().beat(pos(0, 0, 1)).unwrap().kind(),
        BeatKind::Rest,
        "the new rest is at beat 2"
    );
    assert_eq!(
        editor
            .song()
            .beat(pos(0, 0, 0))
            .unwrap()
            .duration()
            .unwrap(),
        Duration::new(1, 4).unwrap()
    );
    apply_then_undo(&song, &insert);

    let delete = Command::DeleteBeat { at: pos(0, 0, 0) };
    apply_then_undo(&song, &delete);
}

#[test]
fn insert_measure_adds_a_bar_to_every_track_and_delete_removes_it() {
    let song = testing::song(&[testing::GUITAR_6, testing::BASS_4], 2).unwrap();
    let insert = Command::InsertMeasure {
        measure: MeasureIndex::new(1),
    };
    let mut editor = Editor::new(song.clone());
    editor.execute(insert.clone()).unwrap();
    assert_eq!(editor.song().measure_count(), 3);
    for track in editor.song().tracks() {
        assert_eq!(track.measures().len(), 3);
    }
    let added = editor
        .song()
        .track(gscore_model::TrackIndex::new(0))
        .unwrap()
        .measure(MeasureIndex::new(1))
        .unwrap();
    assert_eq!(
        added.time_signature().unwrap(),
        TimeSignature::new(4, 4).unwrap()
    );
    apply_then_undo(&song, &insert);

    let delete = Command::DeleteMeasure {
        measure: MeasureIndex::new(0),
    };
    apply_then_undo(&song, &delete);
}

#[test]
fn failed_commands_change_nothing_and_are_not_in_the_history() {
    let song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let mut editor = Editor::new(song.clone());
    let bad = Command::SetFret {
        at: pos(0, 0, 99),
        string: string(1),
        fret: fret(5),
    };
    assert!(editor.execute(bad).is_err());
    assert!(!editor.can_undo());
    assert!(!editor.can_redo());
    assert!(differences(&song, editor.song()).is_empty());
}

#[test]
fn undo_and_redo_walk_the_history() {
    let song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let mut editor = Editor::new(song.clone());
    assert!(matches!(editor.undo(), Err(EditError::NothingToUndo)));
    assert!(matches!(editor.redo(), Err(EditError::NothingToRedo)));

    editor
        .execute(Command::SetFret {
            at: pos(0, 0, 0),
            string: string(1),
            fret: fret(5),
        })
        .unwrap();
    assert!(editor.can_undo());
    editor.undo().unwrap();
    assert!(editor.can_redo());
    assert!(differences(&song, editor.song()).is_empty());
    editor.redo().unwrap();
    assert!(!differences(&song, editor.song()).is_empty());
    editor.undo().unwrap();
    assert!(differences(&song, editor.song()).is_empty());
}

#[test]
fn a_new_edit_clears_the_redo_stack() {
    let song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let mut editor = Editor::new(song.clone());
    editor
        .execute(Command::SetFret {
            at: pos(0, 0, 0),
            string: string(1),
            fret: fret(5),
        })
        .unwrap();
    editor.undo().unwrap();
    assert!(editor.can_redo());
    editor
        .execute(Command::SetFret {
            at: pos(0, 0, 0),
            string: string(2),
            fret: fret(7),
        })
        .unwrap();
    assert!(!editor.can_redo());
}

#[test]
fn the_history_limit_drops_the_oldest_edits() {
    let song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let mut editor = Editor::with_history_limit(song, 2);
    for (beat, value) in [(0, 1), (1, 2), (2, 3)] {
        editor
            .execute(Command::SetFret {
                at: pos(0, 0, beat),
                string: string(1),
                fret: fret(value),
            })
            .unwrap();
    }
    assert_eq!(editor.undo_depth(), 2);
}

#[test]
fn enabled_matches_whether_the_command_applies() {
    let commands = [
        Command::SetFret {
            at: pos(0, 0, 0),
            string: string(1),
            fret: fret(5),
        },
        Command::SetFret {
            at: pos(0, 0, 0),
            string: string(99),
            fret: fret(5),
        },
        Command::SetFret {
            at: pos(999, 0, 0),
            string: string(1),
            fret: fret(5),
        },
        Command::ClearNote {
            at: pos(0, 0, 0),
            string: string(1),
        },
        Command::SetDuration {
            at: pos(0, 0, 0),
            duration: quarter(),
        },
        Command::SetDuration {
            at: pos(999, 0, 0),
            duration: quarter(),
        },
        Command::InsertBeat {
            at: pos(0, 0, 0),
            duration: quarter(),
        },
        Command::InsertBeat {
            at: pos(0, 0, 999),
            duration: quarter(),
        },
        Command::DeleteBeat { at: pos(0, 0, 0) },
        Command::InsertMeasure {
            measure: MeasureIndex::new(0),
        },
        Command::InsertMeasure {
            measure: MeasureIndex::new(999),
        },
        Command::DeleteMeasure {
            measure: MeasureIndex::new(0),
        },
        Command::DeleteMeasure {
            measure: MeasureIndex::new(999),
        },
    ];
    for (name, song) in test_songs() {
        for command in &commands {
            let enabled = command.enabled(&song);
            let mut edited = song.clone();
            let result = command.apply(&mut edited);
            assert_eq!(
                enabled,
                result.is_ok(),
                "{name}: {} said enabled={enabled}, apply={result:?}",
                command.id()
            );
        }
    }
}

#[test]
fn every_successful_edit_still_saves_and_reloads() {
    let commands = [
        Command::SetFret {
            at: pos(0, 0, 0),
            string: string(1),
            fret: fret(5),
        },
        Command::SetFret {
            at: pos(0, 0, 0),
            string: string(1),
            fret: fret(7),
        },
        Command::SetDuration {
            at: pos(0, 0, 0),
            duration: NoteValue::new(8, 1, None).unwrap(),
        },
        Command::InsertBeat {
            at: pos(0, 0, 1),
            duration: quarter(),
        },
        Command::DeleteBeat { at: pos(0, 0, 1) },
        Command::ClearNote {
            at: pos(0, 0, 0),
            string: string(1),
        },
        Command::InsertMeasure {
            measure: MeasureIndex::new(1),
        },
        Command::SetFret {
            at: pos(1, 0, 0),
            string: string(2),
            fret: fret(3),
        },
        Command::DeleteMeasure {
            measure: MeasureIndex::new(1),
        },
    ];
    for (_, song) in test_songs() {
        let mut editor = Editor::new(song);
        for command in &commands {
            if editor.execute(command.clone()).is_ok() {
                assert_roundtrips(editor.song());
            }
        }
    }
}

#[test]
fn tuplets_and_double_dots_round_trip_after_editing() {
    let song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let mut editor = Editor::new(song);
    for duration in [
        NoteValue::new(8, 0, Some((3, 2))).unwrap(),
        NoteValue::new(16, 2, None).unwrap(),
        NoteValue::new(32, 0, Some((5, 4))).unwrap(),
    ] {
        editor
            .execute(Command::SetDuration {
                at: pos(0, 0, 0),
                duration,
            })
            .unwrap();
        assert_roundtrips(editor.song());
        editor
            .execute(Command::InsertBeat {
                at: pos(0, 0, 1),
                duration,
            })
            .unwrap();
        assert_roundtrips(editor.song());
    }
}

#[test]
fn a_first_measure_can_be_edited_into_a_track_with_no_measures() {
    use gscore_model::testing;

    let song = testing::song(&[testing::GUITAR_6], 0).unwrap();
    let mut editor = Editor::new(song);
    editor
        .execute(Command::InsertMeasure {
            measure: MeasureIndex::new(0),
        })
        .unwrap();
    assert_eq!(editor.song().measure_count(), 1);
    assert!(editor.song().issues().is_empty());
    assert_roundtrips(editor.song());
}

#[test]
fn applying_everything_and_undoing_everything_restores_the_song() {
    let commands = [
        Command::SetFret {
            at: pos(0, 0, 0),
            string: string(3),
            fret: fret(4),
        },
        Command::SetDuration {
            at: pos(0, 0, 0),
            duration: note(8),
        },
        Command::InsertBeat {
            at: pos(0, 0, 1),
            duration: note(16),
        },
        Command::DeleteBeat { at: pos(0, 0, 2) },
        Command::InsertMeasure {
            measure: MeasureIndex::new(1),
        },
        Command::SetFret {
            at: pos(1, 0, 0),
            string: string(1),
            fret: fret(12),
        },
        Command::DeleteMeasure {
            measure: MeasureIndex::new(1),
        },
    ];
    for (_, song) in test_songs() {
        apply_all_then_undo_all(&song, &commands);
    }
}
