//! Step entry (M23): typed frets, notes by name, pitches and places, rests and chords move
//! the cursor on by the entry length, each as one undo step; the fingering places pitches
//! on unusual tunings, extended ranges, basses and capos, and refuses what it cannot play.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

mod common;

use common::{note, test_songs};
use gscore_edit::{
    ACTIONS, Action, ActionId, Command, Cursor, EditError, FRET_DIGIT_WINDOW_MS, Letter, Neck,
    Outcome, Place, StepInput, TabEditor,
};
use gscore_model::testing::{self, BASS_4, GUITAR_6, GUITAR_7, GUITAR_8};
use gscore_model::{BeatKind, Fret, MeasureIndex, NoteValue, Song, StringNumber, TrackIndex};

fn stepping(tuning: &[i8], measures: usize) -> TabEditor {
    let mut editor = TabEditor::new(
        testing::song(&[tuning], measures).unwrap(),
        TrackIndex::new(0),
    );
    editor.set_step_entry(true);
    editor
}

fn act(editor: &mut TabEditor, action: Action, now_ms: u64) -> Outcome {
    editor
        .perform(action, now_ms)
        .unwrap_or_else(|e| panic!("{action:?}: {e}"))
}

fn plain(editor: &mut TabEditor, id: ActionId) -> Outcome {
    act(editor, Action::Plain(id), 0)
}

fn letter(editor: &mut TabEditor, letter: Letter, chord: bool) -> Outcome {
    act(editor, Action::Note { letter, chord }, 0)
}

/// (measure, beat) of the cursor.
fn at(editor: &TabEditor) -> (usize, usize) {
    let cursor = editor.cursor();
    (cursor.measure.get(), cursor.beat.get())
}

/// The notes of beat `beat` in measure `measure`, as (string, fret), string 1 first.
fn notes(song: &Song, measure: usize, beat: usize) -> Vec<(u8, u8)> {
    let Some(beat) = song.beat(common::pos(measure, 0, beat)) else {
        return Vec::new();
    };
    let mut notes: Vec<(u8, u8)> = beat
        .notes()
        .filter_map(|n| Some((n.string().ok()?.get(), n.fret().ok()?.get())))
        .collect();
    notes.sort_unstable();
    notes
}

/// The pitch every note of a beat sounds at.
fn pitches(song: &Song, measure: usize, beat: usize) -> Vec<u8> {
    let track = song.track(TrackIndex::new(0)).unwrap();
    let beat = song.beat(common::pos(measure, 0, beat)).unwrap();
    let mut pitches: Vec<u8> = beat
        .notes()
        .map(|n| track.sounding_pitch(&n).unwrap().get())
        .collect();
    pitches.sort_unstable();
    pitches
}

fn value(song: &Song, measure: usize, beat: usize) -> NoteValue {
    song.beat(common::pos(measure, 0, beat))
        .unwrap()
        .note_value()
        .unwrap()
}

fn place(string: u8, fret: u8) -> Place {
    Place {
        string: StringNumber::new(string).unwrap(),
        fret: Fret::new(fret).unwrap(),
    }
}

#[test]
fn a_typed_fret_advances_when_its_digit_window_ends() {
    let mut e = stepping(GUITAR_6, 2);
    act(&mut e, Action::Digit(3), 1_000);
    // Still on the beat: a second digit could follow.
    assert_eq!(at(&e), (0, 0));
    assert_eq!(e.step_deadline_ms(), Some(1_000 + FRET_DIGIT_WINDOW_MS));
    assert_eq!(e.tick(1_100), Outcome::Nothing);
    assert_eq!(e.tick(1_000 + FRET_DIGIT_WINDOW_MS), Outcome::Moved);
    assert_eq!(at(&e), (0, 1));
    assert_eq!(notes(e.song(), 0, 0), [(1, 3)]);
    assert_eq!(e.undo_depth(), 1);
}

#[test]
fn two_digits_make_one_fret_and_one_undo_step() {
    let mut e = stepping(GUITAR_6, 2);
    act(&mut e, Action::Digit(1), 0);
    act(&mut e, Action::Digit(2), 200);
    assert_eq!(notes(e.song(), 0, 0), [(1, 12)]);
    assert_eq!(e.undo_depth(), 1);
    e.tick(2_000);
    assert_eq!(at(&e), (0, 1));
    plain(&mut e, ActionId::Undo);
    assert_eq!(notes(e.song(), 0, 0), []);
    assert_eq!(at(&e), (0, 0));
}

#[test]
fn a_digit_the_neck_has_no_fret_for_is_the_next_note() {
    // 3 then 5: no fret 35 on a 24-fret neck, so 3 and then 5.
    let mut e = stepping(GUITAR_6, 2);
    act(&mut e, Action::Digit(3), 0);
    act(&mut e, Action::Digit(5), 100);
    assert_eq!(notes(e.song(), 0, 0), [(1, 3)]);
    assert_eq!(notes(e.song(), 0, 1), [(1, 5)]);
    assert_eq!(at(&e), (0, 1));
    // After the window, a digit on the same string is the next note too.
    act(&mut e, Action::Digit(1), 5_000);
    assert_eq!(notes(e.song(), 0, 2), [(1, 1)]);
    assert_eq!(e.undo_depth(), 3);
}

#[test]
fn moving_strings_ends_a_typed_note_but_not_a_held_chord() {
    // A riff: 3 on string 1, then 5 on string 2.
    let mut e = stepping(GUITAR_6, 2);
    act(&mut e, Action::Digit(3), 0);
    plain(&mut e, ActionId::CursorDownString);
    act(&mut e, Action::Digit(5), 100);
    assert_eq!(notes(e.song(), 0, 0), [(1, 3)]);
    assert_eq!(notes(e.song(), 0, 1), [(2, 5)]);

    // A chord: the chord modifier holds the beat open across string moves.
    let mut e = stepping(GUITAR_6, 2);
    act(&mut e, Action::ChordDigit(3), 0);
    plain(&mut e, ActionId::CursorDownString);
    act(&mut e, Action::ChordDigit(1), 10_000);
    assert_eq!(e.step_deadline_ms(), None);
    assert_eq!(at(&e), (0, 0));
    plain(&mut e, ActionId::CursorDownString);
    // A plain digit ends the chord: it advances after its window.
    act(&mut e, Action::Digit(0), 20_000);
    e.tick(30_000);
    assert_eq!(notes(e.song(), 0, 0), [(1, 3), (2, 1), (3, 0)]);
    assert_eq!(at(&e), (0, 1));
    // The cursor stays on the string it was on.
    assert_eq!(e.cursor().string, 3);
    assert_eq!(e.undo_depth(), 1);
}

#[test]
fn notes_by_name_are_placed_near_each_other_and_advance_at_once() {
    let mut e = stepping(GUITAR_6, 2);
    letter(&mut e, Letter::E, false);
    assert_eq!(at(&e), (0, 1));
    letter(&mut e, Letter::G, false);
    letter(&mut e, Letter::A, false);
    // E3, then the G and A above it.
    assert_eq!(pitches(e.song(), 0, 0), [52]);
    assert_eq!(pitches(e.song(), 0, 1), [55]);
    assert_eq!(pitches(e.song(), 0, 2), [57]);
    assert_eq!(e.undo_depth(), 3);
    // The cursor follows the notes to their string.
    let last = notes(e.song(), 0, 2);
    assert_eq!(e.cursor().string, last[0].0);
}

#[test]
fn the_key_signature_gives_letters_their_accidentals() {
    let mut e = stepping(GUITAR_6, 2);
    e.execute_command(Command::SetKeySignature {
        measure: MeasureIndex::new(0),
        key: 2,
        minor: false,
    })
    .unwrap();
    letter(&mut e, Letter::F, false);
    letter(&mut e, Letter::C, false);
    letter(&mut e, Letter::G, false);
    let classes: Vec<u8> = (0..3).map(|b| pitches(e.song(), 0, b)[0] % 12).collect();
    // F# and C# in D major; G stays.
    assert_eq!(classes, [6, 1, 7]);
}

#[test]
fn capitals_add_notes_to_the_chord_as_one_step() {
    let mut e = stepping(GUITAR_6, 2);
    letter(&mut e, Letter::A, false);
    letter(&mut e, Letter::C, true);
    letter(&mut e, Letter::E, true);
    assert_eq!(pitches(e.song(), 0, 0), [57, 60, 64]);
    let chord = notes(e.song(), 0, 0);
    assert_eq!(chord.len(), 3);
    assert_eq!(at(&e), (0, 1));
    assert_eq!(e.undo_depth(), 1);
    plain(&mut e, ActionId::Undo);
    assert_eq!(notes(e.song(), 0, 0), []);
}

#[test]
fn corrections_change_the_note_entered_last() {
    let mut e = stepping(GUITAR_6, 2);
    letter(&mut e, Letter::C, false);
    let c = pitches(e.song(), 0, 0)[0];
    let string = notes(e.song(), 0, 0)[0].0;
    plain(&mut e, ActionId::StepSemitoneUp);
    assert_eq!(pitches(e.song(), 0, 0), [c + 1]);
    // A semitone stays on its string.
    assert_eq!(notes(e.song(), 0, 0)[0].0, string);
    plain(&mut e, ActionId::StepOctaveUp);
    assert_eq!(pitches(e.song(), 0, 0), [c + 13]);
    plain(&mut e, ActionId::StepSemitoneDown);
    plain(&mut e, ActionId::StepOctaveDown);
    assert_eq!(pitches(e.song(), 0, 0), [c]);
    let before = notes(e.song(), 0, 0);
    plain(&mut e, ActionId::StepStringDown);
    let after = notes(e.song(), 0, 0);
    assert_ne!(before, after);
    assert_eq!(pitches(e.song(), 0, 0), [c]);
    // Every correction rewrote the same step; the cursor stayed after it.
    assert_eq!(e.undo_depth(), 1);
    assert_eq!(at(&e), (0, 1));
    // A move elsewhere ends what a correction can reach.
    plain(&mut e, ActionId::CursorLeft);
    assert_eq!(plain(&mut e, ActionId::StepSemitoneUp), Outcome::Nothing);
}

#[test]
fn rests_and_entry_lengths_fill_bars_and_add_new_ones() {
    let mut e = stepping(GUITAR_6, 1);
    e.set_entry_value(note(8));
    for i in 0..8 {
        if i == 3 {
            plain(&mut e, ActionId::StepRest);
        } else {
            letter(&mut e, Letter::E, false);
        }
    }
    let song = e.song();
    for beat in 0..8 {
        assert_eq!(value(song, 0, beat), note(8), "beat {beat}");
    }
    assert_eq!(
        song.beat(common::pos(0, 0, 3)).unwrap().kind(),
        BeatKind::Rest
    );
    // The bar is full: the cursor went on to a new last bar.
    assert_eq!(song.measure_count(), 2);
    assert_eq!(at(&e), (1, 0));
    assert_eq!(e.undo_depth(), 8);
}

#[test]
fn tuplets_fill_a_bar_too() {
    let mut e = stepping(GUITAR_6, 1);
    let triplet = NoteValue::new(8, 0, Some((3, 2))).unwrap();
    e.set_entry_value(triplet);
    for _ in 0..12 {
        letter(&mut e, Letter::A, false);
    }
    assert_eq!(at(&e), (1, 0));
    for beat in 0..12 {
        assert_eq!(value(e.song(), 0, beat), triplet);
    }
}

#[test]
fn free_time_bars_and_the_last_bar_never_fail() {
    let mut e = stepping(GUITAR_6, 2);
    e.execute_command(Command::SetFreeTime {
        measure: MeasureIndex::new(1),
        free_time: true,
    })
    .unwrap();
    for _ in 0..12 {
        letter(&mut e, Letter::D, false);
    }
    assert!(e.song().measure_count() >= 3);
}

#[test]
fn pitches_and_places_enter_steps_directly() {
    let mut e = stepping(GUITAR_6, 2);
    // An open E major chord from a MIDI keyboard.
    e.enter_step(StepInput::Pitches(vec![40, 47, 52, 56, 59, 64]), false, 0)
        .unwrap();
    assert_eq!(
        notes(e.song(), 0, 0),
        [(1, 0), (2, 0), (3, 1), (4, 2), (5, 2), (6, 0)]
    );
    // A MIDI guitar gives the places.
    e.enter_step(StepInput::Places(vec![place(6, 3)]), false, 0)
        .unwrap();
    assert_eq!(notes(e.song(), 0, 1), [(6, 3)]);
    // The next note of the chord comes a moment later.
    e.enter_step(StepInput::Places(vec![place(5, 5)]), true, 0)
        .unwrap();
    assert_eq!(notes(e.song(), 0, 1), [(5, 5), (6, 3)]);
    e.enter_step(StepInput::Rest, false, 0).unwrap();
    assert_eq!(
        e.song().beat(common::pos(0, 0, 2)).unwrap().kind(),
        BeatKind::Rest
    );
    assert_eq!(at(&e), (0, 3));
    assert_eq!(e.undo_depth(), 3);
}

#[test]
fn an_unplayable_pitch_is_an_error_and_changes_nothing() {
    let mut e = stepping(GUITAR_6, 1);
    let before = e.song().clone();
    for input in [
        StepInput::Pitches(vec![20]),
        StepInput::Pitches(vec![120]),
        // Two notes only the low E string can play.
        StepInput::Pitches(vec![40, 41]),
        StepInput::Pitches((40..60).collect()),
    ] {
        let error = e.enter_step(input.clone(), false, 0).unwrap_err();
        assert!(
            matches!(error, EditError::Unplayable(_)),
            "{input:?}: {error}"
        );
        assert!(!error.to_string().is_empty());
    }
    assert!(gscore_model::differences(&before, e.song()).is_empty());
    assert_eq!(e.undo_depth(), 0);
    assert_eq!(at(&e), (0, 0));
}

#[test]
fn the_fingering_follows_the_tuning_strings_and_capo() {
    for (tuning, lowest) in [
        (GUITAR_7, 35_u8),
        (GUITAR_8, 30),
        (BASS_4, 28),
        // Drop C: the low string is two semitones under drop D.
        (&[62, 57, 53, 48, 43, 36][..], 36),
    ] {
        let mut e = stepping(tuning, 1);
        e.enter_step(StepInput::Pitches(vec![lowest]), false, 0)
            .unwrap();
        let strings = u8::try_from(tuning.len()).unwrap();
        assert_eq!(notes(e.song(), 0, 0), [(strings, 0)], "{tuning:?}");
        // One semitone under the lowest open string: unplayable.
        assert!(
            e.enter_step(StepInput::Pitches(vec![lowest - 1]), false, 0)
                .is_err()
        );
    }
    // With a capo at the 2nd fret, the open low string sounds F#2 and fret 0 is the capo.
    let mut e = stepping(GUITAR_6, 1);
    e.execute_command(Command::SetTrackCapo {
        track: TrackIndex::new(0),
        capo: 2,
    })
    .unwrap();
    let neck = Neck::of_staff(e.song(), &e.cursor()).unwrap();
    assert_eq!(neck.pitch(place(6, 0)), Some(42));
    e.enter_step(StepInput::Pitches(vec![42]), false, 0)
        .unwrap();
    assert_eq!(notes(e.song(), 0, 0), [(6, 0)]);
    assert!(
        e.enter_step(StepInput::Pitches(vec![40]), false, 0)
            .is_err()
    );
}

#[test]
fn step_entry_keys_do_nothing_outside_the_mode() {
    let mut e = TabEditor::new(testing::song(&[GUITAR_6], 1).unwrap(), TrackIndex::new(0));
    assert_eq!(letter(&mut e, Letter::C, false), Outcome::Nothing);
    assert_eq!(act(&mut e, Action::ChordDigit(3), 0), Outcome::Nothing);
    assert_eq!(plain(&mut e, ActionId::StepRest), Outcome::Nothing);
    assert_eq!(e.undo_depth(), 0);
    // Plain digits keep their old behaviour: no advance.
    act(&mut e, Action::Digit(3), 0);
    e.tick(10_000);
    assert_eq!(at(&e), (0, 0));
    // The toggle switches the mode on and off.
    assert_eq!(plain(&mut e, ActionId::ToggleStepEntry), Outcome::Moved);
    assert!(e.step_entry());
    plain(&mut e, ActionId::LeaveStepEntry);
    assert!(!e.step_entry());
}

#[test]
fn leaving_step_entry_advances_a_waiting_beat() {
    let mut e = stepping(GUITAR_6, 1);
    act(&mut e, Action::Digit(7), 0);
    assert_eq!(at(&e), (0, 0));
    e.set_step_entry(false);
    assert_eq!(at(&e), (0, 1));
    assert_eq!(e.step_deadline_ms(), None);
}

#[test]
fn steps_replay_from_the_journal_to_the_same_song() {
    let mut e = stepping(GUITAR_6, 1);
    act(&mut e, Action::Digit(1), 0);
    act(&mut e, Action::Digit(0), 100);
    letter(&mut e, Letter::A, false);
    letter(&mut e, Letter::C, true);
    plain(&mut e, ActionId::StepSemitoneUp);
    for _ in 0..4 {
        letter(&mut e, Letter::E, false);
    }
    let recorded = e.take_applied();
    let mut replayed = TabEditor::new(testing::song(&[GUITAR_6], 1).unwrap(), TrackIndex::new(0));
    for entry in recorded {
        let line = entry.to_line();
        let entry = gscore_edit::Recorded::parse(&line).unwrap().unwrap();
        replayed.replay(entry).unwrap();
    }
    assert!(gscore_model::differences(e.song(), replayed.song()).is_empty());
    assert_eq!(e.undo_depth(), replayed.undo_depth());
}

/// Every action, every step input and adversarial pitches on every test song, with step
/// entry on: `Ok` or `Err`, never a panic, and undoing everything restores the song.
#[test]
fn every_step_entry_input_survives_every_song() {
    let mut inputs: Vec<Action> = ACTIONS
        .iter()
        .map(|info| Action::Plain(info.action))
        // Quitting and the like do nothing in the editor; undo would undo the hunt.
        .filter(|a| !matches!(a.id(), ActionId::Undo | ActionId::Redo))
        .collect();
    inputs.extend((0..10).map(Action::Digit));
    inputs.extend((0..10).map(Action::ChordDigit));
    for l in [
        Letter::C,
        Letter::D,
        Letter::E,
        Letter::F,
        Letter::G,
        Letter::A,
        Letter::B,
    ] {
        inputs.push(Action::Note {
            letter: l,
            chord: false,
        });
        inputs.push(Action::Note {
            letter: l,
            chord: true,
        });
    }
    for (name, song) in test_songs() {
        let mut e = TabEditor::new(song.clone(), TrackIndex::new(0));
        e.set_step_entry(true);
        let mut now = 0_u64;
        for (i, action) in inputs.iter().enumerate() {
            now += if i % 3 == 0 { 700 } else { 50 };
            let _ = e.perform(*action, now);
            if !e.step_entry() {
                e.set_step_entry(true);
            }
            let _ = e.tick(now);
            if i % 7 == 0 {
                for pitches in [vec![0], vec![255], vec![40, 40, 40], (0..=255).collect()] {
                    let _ = e.enter_step(StepInput::Pitches(pitches), i % 2 == 0, now);
                }
                let _ = e.enter_step(
                    StepInput::Places(vec![place(8, 127), place(1, 0), place(1, 5)]),
                    i % 2 == 1,
                    now,
                );
            }
            let _ = e.song().issues();
            let mut cursor: Cursor = e.cursor();
            cursor.clamp(e.song());
            assert_eq!(cursor, e.cursor(), "{name}: cursor outside the song");
        }
        while e.can_undo() {
            e.undo().unwrap();
        }
        assert!(
            gscore_model::differences(&song, e.song()).is_empty(),
            "{name}: undoing every step did not restore the song"
        );
    }
}

#[test]
fn a_typed_fret_is_complete_once_no_second_digit_can_change_it() {
    // Outside step entry: 1 may become 10-19 within the window; 3 cannot grow on 24 frets.
    let mut e = TabEditor::new(testing::song(&[GUITAR_6], 1).unwrap(), TrackIndex::new(0));
    act(&mut e, Action::Digit(1), 1_000);
    assert_eq!(
        e.fret_complete_at(1_100),
        Some(1_000 + FRET_DIGIT_WINDOW_MS)
    );
    assert_eq!(e.fret_complete_at(1_000 + FRET_DIGIT_WINDOW_MS), None);
    act(&mut e, Action::Digit(2), 1_200);
    assert_eq!(e.fret_complete_at(1_250), None, "12 is final");
    plain(&mut e, ActionId::CursorRight);
    act(&mut e, Action::Digit(3), 2_000);
    assert_eq!(e.fret_complete_at(2_000), None);
    act(&mut e, Action::Digit(0), 3_000);
    assert_eq!(e.fret_complete_at(3_000), None, "0 never grows");
    // A digit that would pass the neck's last fret is not joined outside step entry
    // either: 3 then 5 is 5.
    act(&mut e, Action::Digit(3), 4_000);
    act(&mut e, Action::Digit(5), 4_100);
    assert_eq!(notes(e.song(), 0, 1), [(1, 5)]);
    // Something else than a digit makes it final.
    act(&mut e, Action::Digit(2), 5_000);
    assert!(e.fret_complete_at(5_000).is_some());
    plain(&mut e, ActionId::CursorDownString);
    assert_eq!(e.fret_complete_at(5_010), None);

    // In step entry: likewise, and a chord held open is never complete.
    let mut e = stepping(GUITAR_6, 1);
    act(&mut e, Action::Digit(2), 0);
    assert_eq!(e.fret_complete_at(10), Some(FRET_DIGIT_WINDOW_MS));
    act(&mut e, Action::Digit(4), 20);
    assert_eq!(e.fret_complete_at(30), None, "24 is final");
    e.tick(10_000);
    act(&mut e, Action::ChordDigit(5), 10_000);
    assert_eq!(e.fret_complete_at(99_999), Some(u64::MAX));
    act(&mut e, Action::Plain(ActionId::CursorRight), 100_000);
    assert_eq!(e.fret_complete_at(100_000), None);
}
