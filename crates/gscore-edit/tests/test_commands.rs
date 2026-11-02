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
    BeatKind, Duration, Fret, MeasureIndex, NoteValue, StringNumber, TimeSignature, TrackColor,
    TrackIndex, differences, testing,
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
        .track(TrackIndex::new(0))
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
fn the_history_is_capped_by_memory_and_keeps_the_newest_edit() {
    let song = testing::song(&[testing::GUITAR_6, testing::GUITAR_6], 4).unwrap();
    let mut editor = Editor::new(song);
    assert_eq!(editor.history_bytes(), 0);
    let note = Command::SetFret {
        at: pos(0, 0, 0),
        string: string(1),
        fret: fret(5),
    };
    editor.execute(note).unwrap();
    let small = editor.history_bytes();
    assert!(small > 0);
    // Deleting a measure keeps both tracks' measures: a larger entry.
    editor
        .execute(Command::DeleteMeasure {
            measure: MeasureIndex::new(3),
        })
        .unwrap();
    let both = editor.history_bytes();
    assert!(both > 2 * small, "{both} vs {small}");
    // Undo gives the memory back; redo takes it again.
    editor.undo().unwrap();
    assert_eq!(editor.history_bytes(), small);
    editor.redo().unwrap();
    assert_eq!(editor.history_bytes(), both);

    // Room for about one entry: the oldest go, the newest stays even when it is too big.
    let mut editor = editor.with_history_bytes(both - small);
    assert_eq!(editor.undo_depth(), 1);
    editor
        .execute(Command::DeleteMeasure {
            measure: MeasureIndex::new(2),
        })
        .unwrap();
    assert_eq!(editor.undo_depth(), 1);
    let mut tiny = editor.with_history_bytes(1);
    assert_eq!(tiny.undo_depth(), 1, "the newest entry stays");
    tiny.undo().unwrap();
    assert_eq!(tiny.history_bytes(), 0);
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

#[test]
fn every_track_command_edits_and_undoes() {
    let song = testing::song(&[testing::GUITAR_6, testing::BASS_4], 2).unwrap();
    for command in [
        Command::InsertTrack {
            at: TrackIndex::new(1),
        },
        Command::DeleteTrack {
            track: TrackIndex::new(0),
        },
        Command::DuplicateTrack {
            track: TrackIndex::new(0),
        },
        Command::MoveTrack {
            from: TrackIndex::new(0),
            to: TrackIndex::new(1),
        },
        Command::RenameTrack {
            track: TrackIndex::new(0),
            name: "Renamed".to_owned(),
        },
        Command::SetTrackColor {
            track: TrackIndex::new(0),
            color: TrackColor::new(0x00FF00),
        },
        Command::SetTrackVolume {
            track: TrackIndex::new(0),
            volume: 30,
        },
        Command::SetTrackPan {
            track: TrackIndex::new(0),
            pan: 20,
        },
        Command::SetTrackMute {
            track: TrackIndex::new(0),
            mute: true,
        },
        Command::SetTrackSolo {
            track: TrackIndex::new(1),
            solo: true,
        },
    ] {
        apply_then_undo(&song, &command);
    }
}

#[test]
fn adding_a_track_keeps_the_song_consistent() {
    let song = testing::song(&[testing::GUITAR_6], 3).unwrap();
    let mut editor = Editor::new(song);
    editor
        .execute(Command::InsertTrack {
            at: TrackIndex::new(1),
        })
        .unwrap();
    assert_eq!(editor.song().track_count(), 2);
    let inserted = editor.song().track(TrackIndex::new(1)).unwrap();
    assert_eq!(inserted.measures().len(), 3);
    assert!(
        inserted
            .measures()
            .all(|measure| !measure.has_notes() && measure.time_signature().is_ok())
    );
    assert!(editor.song().issues().is_empty());

    editor
        .execute(Command::DuplicateTrack {
            track: TrackIndex::new(1),
        })
        .unwrap();
    let names: Vec<String> = editor
        .song()
        .tracks()
        .map(|track| track.name().to_owned())
        .collect();
    assert_eq!(names.len(), 3);
    assert_ne!(names[1], names[2], "the copy gets its own name");

    // Moving the last track to the front keeps every measure addressable.
    let last = editor.song().track_count().saturating_sub(1);
    editor
        .execute(Command::MoveTrack {
            from: TrackIndex::new(last),
            to: TrackIndex::new(0),
        })
        .unwrap();
    assert!(editor.song().issues().is_empty());
    assert_eq!(
        editor.song().track(TrackIndex::new(0)).unwrap().index(),
        TrackIndex::new(0)
    );
}

#[test]
fn track_commands_report_impossible_requests() {
    let song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let mut editor = Editor::new(song);
    for command in [
        Command::DeleteTrack {
            track: TrackIndex::new(0),
        },
        Command::MoveTrack {
            from: TrackIndex::new(0),
            to: TrackIndex::new(1),
        },
        Command::RenameTrack {
            track: TrackIndex::new(5),
            name: "Nobody".to_owned(),
        },
        Command::SetTrackVolume {
            track: TrackIndex::new(5),
            volume: 20,
        },
    ] {
        let result = editor.execute(command.clone());
        assert!(result.is_err(), "{} should fail", command.id());
        assert_eq!(
            editor.song().track_count(),
            1,
            "{} must not change the song",
            command.id()
        );
    }
}

#[test]
fn dragging_a_volume_slider_is_one_undo_step() {
    let song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let before = song
        .track_sound(TrackIndex::new(0))
        .map(|sound| sound.volume);
    let mut editor = Editor::new(song);
    for volume in [100, 80, 60, 40] {
        editor
            .execute_with_merge(
                Command::SetTrackVolume {
                    track: TrackIndex::new(0),
                    volume,
                },
                true,
            )
            .unwrap();
    }
    assert_eq!(editor.undo_depth(), 1, "one drag is one undo step");
    assert_eq!(
        editor
            .song()
            .track_sound(TrackIndex::new(0))
            .map(|sound| sound.volume),
        Some(40)
    );
    editor.undo().unwrap();
    assert_eq!(
        editor
            .song()
            .track_sound(TrackIndex::new(0))
            .map(|sound| sound.volume),
        before
    );
}

#[test]
fn two_separate_slider_drags_are_two_undo_steps() {
    let song = testing::song(&[testing::GUITAR_6], 1).unwrap();
    let before = song
        .track_sound(TrackIndex::new(0))
        .map(|sound| sound.volume);
    let mut editor = Editor::new(song);
    let volume = |editor: &Editor| {
        editor
            .song()
            .track_sound(TrackIndex::new(0))
            .map(|sound| sound.volume)
    };
    // First drag: start a new entry, then merge within the drag.
    editor
        .execute_with_merge(
            Command::SetTrackVolume {
                track: TrackIndex::new(0),
                volume: 100,
            },
            false,
        )
        .unwrap();
    editor
        .execute_with_merge(
            Command::SetTrackVolume {
                track: TrackIndex::new(0),
                volume: 80,
            },
            true,
        )
        .unwrap();
    // The button is released (a merge barrier) and the next press is a second drag; the
    // commands during it ask to merge, but the barrier keeps it a separate undo step.
    editor.break_merge();
    editor
        .execute_with_merge(
            Command::SetTrackVolume {
                track: TrackIndex::new(0),
                volume: 60,
            },
            true,
        )
        .unwrap();
    editor
        .execute_with_merge(
            Command::SetTrackVolume {
                track: TrackIndex::new(0),
                volume: 40,
            },
            true,
        )
        .unwrap();
    assert_eq!(editor.undo_depth(), 2, "two drags are two undo steps");
    editor.undo().unwrap();
    assert_eq!(volume(&editor), Some(80), "undo ends the second drag");
    editor.undo().unwrap();
    assert_eq!(volume(&editor), before, "undo ends the first drag");
}
