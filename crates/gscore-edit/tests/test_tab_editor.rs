//! Interactive editing at the cursor: moving, typing frets, durations, beats and measures,
//! and undo/redo putting the cursor back where the edit happened.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

mod common;

use common::test_songs;
use gscore_edit::{Action, ActionId, Cursor, EditError, Outcome, Request, TabEditor};
use gscore_model::testing::{self, GUITAR_6};
use gscore_model::{BeatIndex, BeatPosition, MeasureIndex, Song, StringNumber, TrackIndex};

fn editor(measures: usize) -> TabEditor {
    TabEditor::new(
        testing::song(&[GUITAR_6], measures).unwrap(),
        TrackIndex::new(0),
    )
}

fn plain(editor: &mut TabEditor, id: ActionId) -> Outcome {
    editor
        .perform(Action::Plain(id), 0)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// The fret on `string` of the beat at the cursor, if there is a note.
fn fret_at(editor: &TabEditor, string: u8) -> Option<u8> {
    fret_of(editor.song(), editor.cursor().position(), string)
}

fn fret_of(song: &Song, at: BeatPosition, string: u8) -> Option<u8> {
    let beat = song.beat(at)?;
    beat.notes()
        .find(|n| n.string().ok() == StringNumber::new(string).ok())
        .and_then(|n| n.fret().ok())
        .map(|f| f.get())
}

fn value_at(editor: &TabEditor) -> u16 {
    let beat = editor.song().beat(editor.cursor().position()).unwrap();
    beat.note_value().unwrap().value()
}

#[test]
fn arrows_walk_beats_across_measures_and_strings_within_the_track() {
    let mut e = editor(2);
    assert_eq!(plain(&mut e, ActionId::CursorLeft), Outcome::Nothing);
    assert_eq!(plain(&mut e, ActionId::CursorUpString), Outcome::Nothing);
    for _ in 0..4 {
        assert_eq!(plain(&mut e, ActionId::CursorRight), Outcome::Moved);
    }
    let c = e.cursor();
    assert_eq!((c.measure.get(), c.beat.get()), (1, 0));
    assert_eq!(plain(&mut e, ActionId::CursorLeft), Outcome::Moved);
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (0, 3));
    for _ in 0..10 {
        plain(&mut e, ActionId::CursorDownString);
    }
    assert_eq!(e.cursor().string, 6, "stops at the lowest string");
    assert_eq!(e.revision(), 0, "moving never edits");
}

#[test]
fn digits_enter_frets_and_a_quick_second_digit_makes_two_digit_frets() {
    let mut e = editor(1);
    e.perform(Action::Digit(1), 1000).unwrap();
    assert_eq!(fret_at(&e, 1), Some(1));
    e.perform(Action::Digit(2), 1300).unwrap();
    assert_eq!(fret_at(&e, 1), Some(12));
    // One undo removes the whole number.
    plain(&mut e, ActionId::Undo);
    assert_eq!(fret_at(&e, 1), None);

    // Too slow: the second digit replaces the first.
    e.perform(Action::Digit(1), 2000).unwrap();
    e.perform(Action::Digit(5), 2000 + 501).unwrap();
    assert_eq!(fret_at(&e, 1), Some(5));

    // Three digits never make a fret above 99; the third starts a new number.
    e.perform(Action::Digit(9), 5000).unwrap();
    e.perform(Action::Digit(9), 5100).unwrap();
    e.perform(Action::Digit(7), 5200).unwrap();
    assert_eq!(fret_at(&e, 1), Some(7));

    // Moving in between ends the number.
    e.perform(Action::Digit(1), 6000).unwrap();
    plain(&mut e, ActionId::CursorDownString);
    plain(&mut e, ActionId::CursorUpString);
    e.perform(Action::Digit(3), 6100).unwrap();
    assert_eq!(fret_at(&e, 1), Some(3));
}

#[test]
fn notes_go_on_the_cursor_string_and_can_be_cleared() {
    let mut e = editor(1);
    plain(&mut e, ActionId::CursorDownString);
    plain(&mut e, ActionId::CursorDownString);
    e.perform(Action::Digit(5), 0).unwrap();
    assert_eq!(fret_at(&e, 3), Some(5));
    assert_eq!(fret_at(&e, 1), None);
    assert_eq!(plain(&mut e, ActionId::ClearNote), Outcome::Edited);
    assert_eq!(fret_at(&e, 3), None);
}

#[test]
fn plus_and_minus_halve_and_double_the_note_value_within_range() {
    let mut e = editor(1);
    assert_eq!(value_at(&e), 4);
    plain(&mut e, ActionId::DurationShorter);
    assert_eq!(value_at(&e), 8);
    plain(&mut e, ActionId::DurationLonger);
    plain(&mut e, ActionId::DurationLonger);
    plain(&mut e, ActionId::DurationLonger);
    assert_eq!(value_at(&e), 1);
    assert_eq!(plain(&mut e, ActionId::DurationLonger), Outcome::Nothing);
    for _ in 0..10 {
        plain(&mut e, ActionId::DurationShorter);
    }
    assert_eq!(value_at(&e), 128);
}

#[test]
fn moving_right_from_the_last_beat_fills_the_measure_then_adds_one() {
    let mut e = editor(1);
    // Make the measure three quarters long by deleting a beat, then shorten the last one.
    plain(&mut e, ActionId::DeleteBeat);
    for _ in 0..2 {
        plain(&mut e, ActionId::CursorRight);
    }
    plain(&mut e, ActionId::DurationShorter);
    // Two quarters and an eighth: each step right adds an eighth rest (the current
    // value) until the 4/4 measure is full.
    for beat in 3..6 {
        assert_eq!(plain(&mut e, ActionId::CursorRight), Outcome::Edited);
        assert_eq!(e.cursor().beat.get(), beat);
        assert_eq!(value_at(&e), 8);
    }
    assert_eq!(e.song().measure_count(), 1);
    // Full now: the next step adds a measure and moves into it.
    assert_eq!(plain(&mut e, ActionId::CursorRight), Outcome::Edited);
    assert_eq!(e.song().measure_count(), 2);
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (1, 0));
    // Undo puts the cursor back where it was.
    plain(&mut e, ActionId::Undo);
    assert_eq!(e.song().measure_count(), 1);
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (0, 5));
}

#[test]
fn beats_and_measures_insert_and_delete_at_the_cursor() {
    let mut e = editor(2);
    plain(&mut e, ActionId::CursorRight);
    plain(&mut e, ActionId::InsertBeat);
    assert_eq!(e.cursor().beats_in(e.song(), MeasureIndex::new(0)), 5);
    plain(&mut e, ActionId::DeleteBeat);
    assert_eq!(e.cursor().beats_in(e.song(), MeasureIndex::new(0)), 4);

    plain(&mut e, ActionId::InsertMeasure);
    assert_eq!(e.song().measure_count(), 3);
    assert_eq!(e.cursor().beat.get(), 0);
    plain(&mut e, ActionId::DeleteMeasure);
    plain(&mut e, ActionId::DeleteMeasure);
    assert_eq!(e.song().measure_count(), 1);
    // The last measure stays: the model refuses, nothing changes.
    let before = e.revision();
    assert!(
        e.perform(Action::Plain(ActionId::DeleteMeasure), 0)
            .is_err()
    );
    assert_eq!(e.revision(), before);
}

#[test]
fn undo_and_redo_restore_song_and_cursor() {
    let mut e = editor(2);
    let original = e.song().clone();
    for _ in 0..5 {
        plain(&mut e, ActionId::CursorRight);
    }
    e.perform(Action::Digit(7), 0).unwrap();
    let edited_at = e.cursor();
    plain(&mut e, ActionId::CursorLeft);
    plain(&mut e, ActionId::CursorLeft);

    plain(&mut e, ActionId::Undo);
    assert!(gscore_model::differences(&original, e.song()).is_empty());
    assert_eq!(e.cursor(), edited_at);
    plain(&mut e, ActionId::CursorLeft);
    plain(&mut e, ActionId::Redo);
    assert_eq!(e.cursor(), edited_at);
    assert_eq!(fret_at(&e, 1), Some(7));
    assert!(matches!(
        e.perform(Action::Plain(ActionId::Redo), 0),
        Err(EditError::NothingToRedo)
    ));
}

#[test]
fn playback_and_saving_are_requests_for_the_app_and_clipboard_is_unsupported() {
    let mut e = editor(1);
    assert_eq!(
        plain(&mut e, ActionId::PlayPause),
        Outcome::Request(Request::PlayPause)
    );
    assert_eq!(
        plain(&mut e, ActionId::Save),
        Outcome::Request(Request::Save)
    );
    assert!(matches!(
        e.perform(Action::Plain(ActionId::Paste), 0),
        Err(EditError::Unsupported(_))
    ));
}

#[test]
fn an_empty_measure_gets_a_beat_for_its_first_note() {
    let mut e = editor(1);
    for _ in 0..4 {
        plain(&mut e, ActionId::DeleteBeat);
    }
    assert_eq!(e.cursor().beats_in(e.song(), MeasureIndex::new(0)), 0);
    e.perform(Action::Digit(3), 0).unwrap();
    assert_eq!(fret_at(&e, 1), Some(3));
    assert_eq!(value_at(&e), 4, "a quarter, the entry value");
    // Moving on fills the rest of the measure beat by beat.
    assert_eq!(plain(&mut e, ActionId::CursorRight), Outcome::Edited);
    assert_eq!(e.cursor().beat.get(), 1);
}

#[test]
fn the_cursor_follows_track_changes_and_stays_inside_the_song() {
    let song = testing::song(&[GUITAR_6, testing::BASS_4], 3).unwrap();
    let mut e = TabEditor::new(song, TrackIndex::new(0));
    for _ in 0..9 {
        plain(&mut e, ActionId::CursorRight);
    }
    for _ in 0..5 {
        plain(&mut e, ActionId::CursorDownString);
    }
    e.set_track(TrackIndex::new(1));
    let c = e.cursor();
    assert_eq!((c.track.get(), c.measure.get(), c.string), (1, 2, 1));
    e.set_cursor(Cursor {
        measure: MeasureIndex::new(99),
        beat: BeatIndex::new(99),
        string: 99,
        ..c
    });
    let c = e.cursor();
    assert_eq!((c.measure.get(), c.beat.get(), c.string), (2, 3, 4));
}

/// Random actions on varied songs never panic, keep the cursor valid, and undoing every
/// edit restores the song (AGENTS.md "panic_hunt" for the interactive layer).
#[test]
fn random_actions_never_panic_and_undo_restores_the_song() {
    let mut seed: u64 = 0x5eed;
    let mut next = |bound: usize| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(seed >> 33).unwrap() % bound
    };
    let actions: Vec<ActionId> = gscore_edit::ACTIONS.iter().map(|a| a.action).collect();
    // The owner's files add several voices, tuplets, ties and placeholder beats.
    let mut songs = test_songs();
    for file in ["note_lengths.gp", "ornaments.gp", "steel_guitar_dadgad.gp"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/basic")
            .join(file);
        let bytes = std::fs::read(path).unwrap();
        let song = gscore_io::import_guitar_pro_from_bytes(&bytes)
            .unwrap()
            .song;
        songs.push((file, song));
    }
    for (name, song) in songs {
        if song.track_count() == 0 {
            continue;
        }
        let mut e = TabEditor::new(song.clone(), TrackIndex::new(0));
        let mut now = 0;
        for _ in 0..400 {
            now += next(700) as u64;
            let action = if next(3) == 0 {
                Action::Digit(u8::try_from(next(10)).unwrap())
            } else {
                Action::Plain(actions[next(actions.len())])
            };
            let _ = e.perform(action, now);
            let c = e.cursor();
            assert!(c.measure.get() < e.song().measure_count().max(1), "{name}");
            assert!(c.string >= 1, "{name}");
        }

        // The recorded commands, written as text and read back (as the recovery journal
        // does), rebuild the same song and the same undo history on a fresh editor.
        let applied = e.take_applied();
        assert!(e.take_applied().is_empty(), "taking empties the record");
        let mut replayed = TabEditor::new(song.clone(), TrackIndex::new(0));
        for entry in &applied {
            let line = entry.to_line();
            let parsed = gscore_edit::Recorded::parse(&line).unwrap().unwrap();
            replayed
                .replay(parsed)
                .unwrap_or_else(|err| panic!("{name}: replaying {line}: {err}"));
        }
        assert_eq!(
            replayed.take_applied(),
            applied,
            "{name}: replay records the same"
        );
        assert!(
            gscore_model::differences(e.song(), replayed.song()).is_empty(),
            "{name}: the replay differs"
        );
        while replayed.can_undo() {
            e.undo().unwrap();
            replayed.undo().unwrap();
            assert!(
                gscore_model::differences(e.song(), replayed.song()).is_empty(),
                "{name}: the undo histories differ"
            );
        }
        assert!(!e.can_undo(), "{name}: the replayed history is as deep");
        while e.can_undo() {
            e.undo().unwrap_or_else(|err| panic!("{name}: undo: {err}"));
        }
        assert!(
            gscore_model::differences(&song, e.song()).is_empty(),
            "{name}: undoing everything did not restore the song"
        );
    }
}

#[test]
fn a_new_beat_at_the_measure_end_is_only_as_long_as_what_still_fits() {
    let mut e = editor(1);
    // Eighth, quarter, quarter, quarter: an eighth is missing, the last beat is a quarter.
    plain(&mut e, ActionId::DurationShorter);
    for _ in 0..3 {
        plain(&mut e, ActionId::CursorRight);
    }
    assert_eq!(plain(&mut e, ActionId::CursorRight), Outcome::Edited);
    assert_eq!((e.cursor().beat.get(), value_at(&e)), (4, 8));
}
