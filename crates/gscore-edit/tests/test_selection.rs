//! Making a selection from the keyboard (Shift+arrows), selections that cross bar lines,
//! and clearing or copying what is selected.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

use gscore_edit::{
    Action, ActionId, BeatSpot, Command, Cursor, Keymap, Outcome, PasteOptions, Request, Selection,
    TabEditor,
};
use gscore_model::testing::{self, GUITAR_6};
use gscore_model::{
    BeatIndex, BeatPosition, Dynamic, MeasureIndex, NoteEffect, StringNumber, TrackIndex,
    VoiceIndex, differences,
};

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

fn spot(measure: usize, beat: usize) -> BeatSpot {
    BeatSpot {
        measure: MeasureIndex::new(measure),
        beat: BeatIndex::new(beat),
    }
}

fn at(measure: usize, beat: usize) -> BeatPosition {
    BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(beat),
    }
}

fn go(editor: &mut TabEditor, measure: usize, beat: usize) {
    editor.set_cursor(Cursor {
        measure: MeasureIndex::new(measure),
        beat: BeatIndex::new(beat),
        ..editor.cursor()
    });
}

fn notes_at(editor: &TabEditor, position: BeatPosition) -> Vec<(u8, u8)> {
    editor
        .song()
        .beat(position)
        .map(|beat| {
            beat.notes()
                .map(|n| (n.string().unwrap().get(), n.fret().unwrap().get()))
                .collect()
        })
        .unwrap_or_default()
}

/// Selected beats as (measure, beat) pairs.
fn selected(editor: &TabEditor) -> Vec<(usize, usize)> {
    editor
        .selection()
        .map(|s| s.positions(editor.song()))
        .unwrap_or_default()
        .into_iter()
        .map(|p| (p.measure.get(), p.beat.get()))
        .collect()
}

#[test]
fn shift_arrows_extend_the_selection_across_bar_lines() {
    let mut e = editor(2);
    go(&mut e, 0, 2);
    for _ in 0..3 {
        assert_eq!(plain(&mut e, ActionId::SelectRight), Outcome::Moved);
    }
    assert_eq!(selected(&e), vec![(0, 2), (0, 3), (1, 0), (1, 1)]);
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (1, 1));
    // Back again shrinks it; past the anchor it grows the other way.
    for _ in 0..4 {
        plain(&mut e, ActionId::SelectLeft);
    }
    assert_eq!(selected(&e), vec![(0, 1), (0, 2)]);
    assert_eq!(
        e.selection(),
        Some(Selection::Beats {
            staff: gscore_model::StaffIndex::new(0),
            track: TrackIndex::new(0),
            voice: VoiceIndex::new(0),
            anchor: spot(0, 2),
            focus: spot(0, 1),
        })
    );
    // Extending never adds beats, unlike a plain Right at the end of the song.
    go(&mut e, 1, 3);
    plain(&mut e, ActionId::SelectRight);
    assert_eq!(e.song().measure_count(), 2);
    // A plain arrow ends the selection.
    assert_eq!(plain(&mut e, ActionId::CursorLeft), Outcome::Moved);
    assert_eq!(e.selection(), None);
}

#[test]
fn select_beat_takes_the_whole_beat_and_delete_clears_its_notes() {
    let mut e = editor(1);
    for _ in 0..3 {
        e.perform(Action::Digit(5), 0).unwrap();
        plain(&mut e, ActionId::CursorDownString);
    }
    assert_eq!(notes_at(&e, at(0, 0)).len(), 3);
    let before = e.song().clone();
    let depth = e.undo_depth();

    assert_eq!(plain(&mut e, ActionId::SelectBeat), Outcome::Moved);
    assert_eq!(selected(&e), vec![(0, 0)]);
    assert_eq!(plain(&mut e, ActionId::DeleteBeat), Outcome::Edited);
    assert!(
        notes_at(&e, at(0, 0)).is_empty(),
        "every string was cleared"
    );
    let beats = |e: &TabEditor| {
        e.song()
            .track(TrackIndex::new(0))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .voice(VoiceIndex::new(0))
            .unwrap()
            .beats()
            .len()
    };
    assert_eq!(beats(&e), 4, "the beat stays, only its notes go");
    assert_eq!(e.undo_depth(), depth + 1, "one undo step");
    assert!(e.selection().is_some(), "the selection stays");

    plain(&mut e, ActionId::Undo);
    assert!(differences(&before, e.song()).is_empty());
    // A second time keeps the selection as it is.
    assert_eq!(plain(&mut e, ActionId::SelectBeat), Outcome::Nothing);
}

#[test]
fn select_all_takes_every_bar_and_delete_clears_them_all() {
    let mut e = TabEditor::new(
        testing::song(&[GUITAR_6, GUITAR_6], 2).unwrap(),
        TrackIndex::new(0),
    );
    for track in 0..2 {
        for measure in 0..2 {
            e.set_cursor(Cursor {
                track: TrackIndex::new(track),
                measure: MeasureIndex::new(measure),
                beat: BeatIndex::new(0),
                ..e.cursor()
            });
            e.perform(Action::Digit(3), 0).unwrap();
        }
    }
    let notes = |editor: &TabEditor, selection: Selection| {
        selection
            .positions(editor.song())
            .into_iter()
            .filter_map(|at| editor.song().beat(at))
            .map(|beat| beat.notes().len())
            .sum::<usize>()
    };
    let before = e.song().clone();
    assert_eq!(plain(&mut e, ActionId::SelectAll), Outcome::Moved);
    let selection = e.selection().expect("every bar is selected");
    assert_eq!(
        selection,
        Selection::Measures {
            anchor: MeasureIndex::new(0),
            focus: MeasureIndex::new(1),
            track: None,
        }
    );
    assert_eq!(
        selection.positions(e.song()).len(),
        2 * 2 * 4,
        "every beat of both tracks in both bars"
    );
    assert_eq!(notes(&e, selection), 4);

    let depth = e.undo_depth();
    assert_eq!(plain(&mut e, ActionId::Delete), Outcome::Edited);
    assert_eq!(notes(&e, selection), 0, "every track was cleared");
    assert_eq!(e.undo_depth(), depth + 1, "one undo step");
    plain(&mut e, ActionId::Undo);
    assert!(differences(&before, e.song()).is_empty());
}

#[test]
fn delete_without_a_selection_still_deletes_the_beat() {
    let mut e = editor(1);
    plain(&mut e, ActionId::DeleteBeat);
    assert_eq!(
        e.song()
            .track(TrackIndex::new(0))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .voice(VoiceIndex::new(0))
            .unwrap()
            .beats()
            .len(),
        3
    );
}

#[test]
fn a_selection_over_a_bar_line_copies_and_pastes_bar_by_bar() {
    let mut e = editor(4);
    go(&mut e, 0, 3);
    e.perform(Action::Digit(3), 0).unwrap();
    go(&mut e, 1, 0);
    e.perform(Action::Digit(7), 0).unwrap();
    e.set_selection(Some(Selection::Beats {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        voice: VoiceIndex::new(0),
        anchor: spot(0, 3),
        focus: spot(1, 0),
    }));
    plain(&mut e, ActionId::Copy);

    // Pasted over the same two beats further on, they replace them one for one.
    e.set_selection(Some(Selection::Beats {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        voice: VoiceIndex::new(0),
        anchor: spot(2, 3),
        focus: spot(3, 0),
    }));
    let before = e.song().clone();
    assert_eq!(plain(&mut e, ActionId::Paste), Outcome::Edited);
    let string = StringNumber::new(1).unwrap().get();
    assert_eq!(notes_at(&e, at(2, 3)), vec![(string, 3)]);
    assert_eq!(notes_at(&e, at(3, 0)), vec![(string, 7)]);
    assert_eq!(
        selected(&e),
        vec![(2, 3), (3, 0)],
        "the pasted beats are selected"
    );
    plain(&mut e, ActionId::Undo);
    assert!(differences(&before, e.song()).is_empty());

    // In the last measure there is no next one for the second bar: nothing changes.
    e.set_selection(Some(Selection::Beats {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        voice: VoiceIndex::new(0),
        anchor: spot(3, 0),
        focus: spot(3, 0),
    }));
    assert!(e.perform(Action::Plain(ActionId::Paste), 0).is_err());
    assert!(differences(&before, e.song()).is_empty());
}

#[test]
fn pasted_notes_keep_their_dynamic() {
    let mut e = editor(2);
    go(&mut e, 0, 0);
    e.perform(Action::Digit(3), 0).unwrap();
    e.execute_command(Command::SetDynamic {
        at: at(0, 0),
        dynamic: Dynamic::Pp,
    })
    .unwrap();
    // Later notes are entered pianissimo; make the target loud first.
    go(&mut e, 1, 0);
    e.perform(Action::Digit(5), 0).unwrap();
    e.execute_command(Command::SetDynamic {
        at: at(1, 0),
        dynamic: Dynamic::Ff,
    })
    .unwrap();
    e.set_selection(Some(Selection::Beats {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        voice: VoiceIndex::new(0),
        anchor: spot(0, 0),
        focus: spot(0, 0),
    }));
    plain(&mut e, ActionId::Copy);
    e.set_selection(Some(Selection::Beats {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        voice: VoiceIndex::new(0),
        anchor: spot(1, 2),
        focus: spot(1, 2),
    }));
    assert_eq!(plain(&mut e, ActionId::Paste), Outcome::Edited);
    let dynamic =
        |e: &TabEditor, measure, beat| e.song().beat(at(measure, beat)).unwrap().dynamic();
    assert_eq!(
        dynamic(&e, 1, 2),
        Some(Dynamic::Pp),
        "not the ff in effect there"
    );
}

/// How many beats the first voice of `measure` has.
fn beat_count(e: &TabEditor, measure: usize) -> usize {
    e.song()
        .track(TrackIndex::new(0))
        .unwrap()
        .measure(MeasureIndex::new(measure))
        .unwrap()
        .voice(VoiceIndex::new(0))
        .unwrap()
        .beats()
        .len()
}

#[test]
fn copy_with_only_the_cursor_takes_its_beat_and_paste_inserts_it_after() {
    // As Guitar Pro: with no selection Ctrl+C copies the cursor's whole beat and Ctrl+V
    // puts a copy after it; nothing is overwritten.
    let mut e = editor(1);
    let string = |string: u8| StringNumber::new(string).unwrap();
    for (string_number, fret) in [(1, 2), (2, 4)] {
        e.execute_command(Command::SetFret {
            at: at(0, 0),
            string: string(string_number),
            fret: gscore_model::Fret::new(fret).unwrap(),
        })
        .unwrap();
    }
    e.execute_command(Command::SetNoteEffect {
        at: at(0, 0),
        string: string(1),
        effect: NoteEffect::PalmMute(true),
    })
    .unwrap();
    e.execute_command(Command::SetFret {
        at: at(0, 1),
        string: string(1),
        fret: gscore_model::Fret::new(7).unwrap(),
    })
    .unwrap();
    assert_eq!(e.selection(), None);
    go(&mut e, 0, 0);
    assert_eq!(plain(&mut e, ActionId::Copy), Outcome::Nothing);
    let before = e.song().clone();
    let beats = beat_count(&e, 0);
    let depth = e.undo_depth();
    assert_eq!(plain(&mut e, ActionId::Paste), Outcome::Edited);
    assert_eq!(beat_count(&e, 0), beats + 1, "a beat was added");
    assert_eq!(
        notes_at(&e, at(0, 1)),
        vec![(1, 2), (2, 4)],
        "the copy, whole"
    );
    assert!(
        e.song()
            .beat(at(0, 1))
            .unwrap()
            .notes()
            .find(|n| n.string().unwrap().get() == 1)
            .unwrap()
            .is_palm_muted(),
        "the note effect came along"
    );
    assert_eq!(
        notes_at(&e, at(0, 2)),
        vec![(1, 7)],
        "the next beat moved on"
    );
    assert_eq!(selected(&e), vec![(0, 1)], "the pasted beat is selected");
    assert_eq!(e.undo_depth(), depth + 1, "one undo step");
    plain(&mut e, ActionId::Undo);
    assert!(differences(&before, e.song()).is_empty());
}

#[test]
fn pasted_beats_replace_the_selection_whatever_its_size() {
    // Guitar Pro's rule: the copied beats take the selection's place. As many as were
    // copied: nothing changes; fewer selected: the rest are added; more: they go.
    let mut e = editor(2);
    for (beat, fret) in [(0, 1), (1, 2), (2, 3), (3, 4)] {
        go(&mut e, 0, beat);
        e.perform(Action::Digit(fret), 0).unwrap();
    }
    let frets = |e: &TabEditor| -> Vec<Vec<(u8, u8)>> {
        (0..beat_count(e, 0))
            .map(|beat| notes_at(e, at(0, beat)))
            .collect()
    };
    let select = |e: &mut TabEditor, from: usize, to: usize| {
        e.set_selection(Some(Selection::Beats {
            staff: gscore_model::StaffIndex::new(0),
            track: TrackIndex::new(0),
            voice: VoiceIndex::new(0),
            anchor: spot(0, from),
            focus: spot(0, to),
        }));
    };
    // Beats 2 and 3 copied.
    select(&mut e, 1, 2);
    plain(&mut e, ActionId::Copy);
    let original = frets(&e);
    // Pasted on the same selection: nothing changes.
    plain(&mut e, ActionId::Paste);
    assert_eq!(frets(&e), original);
    // Over one beat (the 4th): it is replaced and the second copied beat added.
    select(&mut e, 3, 3);
    plain(&mut e, ActionId::Paste);
    assert_eq!(
        frets(&e),
        vec![
            vec![(1, 1)],
            vec![(1, 2)],
            vec![(1, 3)],
            vec![(1, 2)],
            vec![(1, 3)]
        ]
    );
    assert_eq!(selected(&e), vec![(0, 3), (0, 4)]);
    // Over four beats (the 1st to 4th): two take their place, the other two go.
    select(&mut e, 0, 3);
    plain(&mut e, ActionId::Paste);
    assert_eq!(frets(&e), vec![vec![(1, 2)], vec![(1, 3)], vec![(1, 3)]]);
    // Each paste is one undo step.
    plain(&mut e, ActionId::Undo);
    plain(&mut e, ActionId::Undo);
    assert_eq!(frets(&e), original);
}

#[test]
fn a_selection_over_a_bar_line_replaces_existing_beats_whole() {
    let mut e = editor(2);
    let string = StringNumber::new(1).unwrap();
    // Two source beats, an eighth and a quarter; the second carries a palm mute.
    go(&mut e, 0, 0);
    e.perform(Action::Digit(3), 0).unwrap();
    plain(&mut e, ActionId::DurationShorter);
    go(&mut e, 0, 1);
    e.perform(Action::Digit(5), 0).unwrap();
    e.execute_command(Command::SetNoteEffect {
        at: at(0, 1),
        string,
        effect: NoteEffect::PalmMute(true),
    })
    .unwrap();
    e.set_selection(Some(Selection::Beats {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        voice: VoiceIndex::new(0),
        anchor: spot(0, 0),
        focus: spot(0, 1),
    }));
    plain(&mut e, ActionId::Copy);

    // Target: two existing beats with other notes.
    e.set_selection(None);
    go(&mut e, 1, 2);
    e.perform(Action::Digit(9), 0).unwrap();
    plain(&mut e, ActionId::CursorDownString);
    e.perform(Action::Digit(8), 0).unwrap();
    go(&mut e, 1, 3);
    e.perform(Action::Digit(7), 0).unwrap();
    let before = e.song().clone();
    let depth = e.undo_depth();
    e.set_selection(Some(Selection::Beats {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        voice: VoiceIndex::new(0),
        anchor: spot(1, 2),
        focus: spot(1, 3),
    }));
    assert_eq!(plain(&mut e, ActionId::Paste), Outcome::Edited);
    let first = e.song().beat(at(1, 2)).unwrap();
    assert_eq!(first.note_value().unwrap().value(), 8, "the copied length");
    assert_eq!(
        notes_at(&e, at(1, 2)),
        vec![(1, 3)],
        "the old notes are gone"
    );
    assert_eq!(notes_at(&e, at(1, 3)), vec![(1, 5)]);
    assert!(
        e.song()
            .beat(at(1, 3))
            .unwrap()
            .notes()
            .next()
            .unwrap()
            .is_palm_muted(),
        "the copied note effect came along"
    );
    assert_eq!(e.undo_depth(), depth + 1, "one undo step");
    plain(&mut e, ActionId::Undo);
    assert!(differences(&before, e.song()).is_empty());
}

#[test]
fn a_one_track_measure_selection_copies_cuts_and_clears_only_that_track() {
    let mut e = TabEditor::new(
        testing::song(&[GUITAR_6, GUITAR_6], 2).unwrap(),
        TrackIndex::new(0),
    );
    let note = |track: usize, measure: usize| BeatPosition {
        track: TrackIndex::new(track),
        measure: MeasureIndex::new(measure),
        ..at(0, 0)
    };
    e.execute_command(Command::SetFret {
        at: note(0, 0),
        string: StringNumber::new(1).unwrap(),
        fret: gscore_model::Fret::new(3).unwrap(),
    })
    .unwrap();
    e.execute_command(Command::SetFret {
        at: note(1, 0),
        string: StringNumber::new(1).unwrap(),
        fret: gscore_model::Fret::new(7).unwrap(),
    })
    .unwrap();
    let one_track = |measure: usize| Selection::Measures {
        anchor: MeasureIndex::new(measure),
        focus: MeasureIndex::new(measure),
        track: Some(TrackIndex::new(0)),
    };

    // Copying the first bar of track 1 and pasting it into the second bar writes that
    // track only.
    e.set_selection(Some(one_track(0)));
    plain(&mut e, ActionId::Copy);
    e.set_selection(Some(one_track(1)));
    e.set_cursor(Cursor {
        measure: MeasureIndex::new(1),
        track: TrackIndex::new(0),
        ..e.cursor()
    });
    assert_eq!(plain(&mut e, ActionId::Paste), Outcome::Edited);
    assert_eq!(notes_at(&e, note(0, 1)), vec![(1, 3)]);
    assert!(notes_at(&e, note(1, 1)).is_empty(), "the other track stays");

    // Delete clears the selected track's notes only.
    e.set_selection(Some(one_track(1)));
    assert_eq!(plain(&mut e, ActionId::DeleteBeat), Outcome::Edited);
    assert!(notes_at(&e, note(0, 1)).is_empty());
    // Cut clears the first bar of that track without removing the bars.
    e.set_selection(Some(one_track(0)));
    assert_eq!(plain(&mut e, ActionId::Cut), Outcome::Edited);
    assert_eq!(e.song().measure_count(), 2);
    assert!(notes_at(&e, note(0, 0)).is_empty());
    assert_eq!(notes_at(&e, note(1, 0)), vec![(1, 7)]);
}

#[test]
fn cutting_a_selection_over_a_bar_line_leaves_rests() {
    let mut e = editor(2);
    go(&mut e, 0, 3);
    e.perform(Action::Digit(3), 0).unwrap();
    go(&mut e, 1, 0);
    e.perform(Action::Digit(7), 0).unwrap();
    e.set_selection(Some(Selection::Beats {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        voice: VoiceIndex::new(0),
        anchor: spot(1, 0),
        focus: spot(0, 3),
    }));
    assert_eq!(plain(&mut e, ActionId::Cut), Outcome::Edited);
    assert!(notes_at(&e, at(0, 3)).is_empty());
    assert!(notes_at(&e, at(1, 0)).is_empty());
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (0, 3));
}

#[test]
fn shift_up_and_down_ask_the_page_for_the_neighbouring_line() {
    // The lines are the layout's: the editor hands the move to the app, which answers
    // with the beat it found there.
    let keymap = Keymap::default();
    let shift = |key| {
        gscore_edit::KeyCombo::new(
            gscore_edit::Key::Named(key),
            gscore_edit::Modifiers {
                shift: true,
                ..gscore_edit::Modifiers::default()
            },
        )
    };
    assert_eq!(
        keymap.action(shift(gscore_edit::NamedKey::Down)),
        Some(Action::Plain(ActionId::SelectLineDown))
    );
    assert_eq!(
        keymap.action(shift(gscore_edit::NamedKey::Up)),
        Some(Action::Plain(ActionId::SelectLineUp))
    );
    let mut e = editor(4);
    assert_eq!(
        plain(&mut e, ActionId::SelectLineDown),
        Outcome::Request(Request::SelectLine { down: true })
    );
    assert_eq!(
        plain(&mut e, ActionId::SelectLineUp),
        Outcome::Request(Request::SelectLine { down: false })
    );
    assert!(e.selection().is_none(), "nothing changes before the answer");

    // The answer extends the selection from the cursor's beat and moves the cursor.
    plain(&mut e, ActionId::CursorRight);
    assert_eq!(e.extend_selection_to_spot(spot(2, 1)), Outcome::Moved);
    assert_eq!(
        e.selection(),
        Some(Selection::Beats {
            staff: gscore_model::StaffIndex::new(0),
            track: TrackIndex::new(0),
            voice: VoiceIndex::new(0),
            anchor: spot(0, 1),
            focus: spot(2, 1),
        })
    );
    assert_eq!(e.cursor().measure, MeasureIndex::new(2));
    // Again from there: the anchor stays.
    assert_eq!(e.extend_selection_to_spot(spot(1, 0)), Outcome::Moved);
    let Some(Selection::Beats { anchor, focus, .. }) = e.selection() else {
        panic!("a beat selection");
    };
    assert_eq!((anchor, focus), (spot(0, 1), spot(1, 0)));
    // A spot past the song changes nothing.
    assert_eq!(e.extend_selection_to_spot(spot(9, 0)), Outcome::Nothing);
    assert_eq!(e.cursor().measure, MeasureIndex::new(1));
}

#[test]
fn pasting_copied_bars_without_moving_the_selection_duplicates_them_after_it() {
    let mut e = TabEditor::new(
        testing::song(&[GUITAR_6, GUITAR_6], 2).unwrap(),
        TrackIndex::new(0),
    );
    // A note in the first bar of both tracks.
    for track in 0..2 {
        let position = BeatPosition {
            track: TrackIndex::new(track),
            ..at(0, 0)
        };
        e.execute_command(Command::SetFret {
            at: position,
            string: StringNumber::new(1).unwrap(),
            fret: gscore_model::Fret::new(3).unwrap(),
        })
        .unwrap();
    }
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(0),
        focus: MeasureIndex::new(0),
        track: None,
    }));
    plain(&mut e, ActionId::Copy);
    // Pasting without moving the selection puts a copy of the bar right after it, in every
    // track, and the song grows.
    assert_eq!(plain(&mut e, ActionId::Paste), Outcome::Edited);
    assert_eq!(e.song().measure_count(), 3);
    for track in 0..2 {
        let position = BeatPosition {
            track: TrackIndex::new(track),
            ..at(1, 0)
        };
        assert_eq!(notes_at(&e, position), vec![(1, 3)], "track {track}");
    }
    assert!(matches!(
        e.selection(),
        Some(Selection::Measures { anchor, focus, .. })
            if anchor == MeasureIndex::new(1) && focus == MeasureIndex::new(1)
    ));
    // The last bar can be duplicated past the song's end too.
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(2),
        focus: MeasureIndex::new(2),
        track: None,
    }));
    plain(&mut e, ActionId::Copy);
    assert_eq!(plain(&mut e, ActionId::Paste), Outcome::Edited);
    assert_eq!(e.song().measure_count(), 4);
    // Each paste is one undo step.
    plain(&mut e, ActionId::Undo);
    assert_eq!(e.song().measure_count(), 3);
}

#[test]
fn copying_all_tracks_from_a_one_track_selection_and_inserting_the_bars() {
    let mut e = TabEditor::new(
        testing::song(&[GUITAR_6, GUITAR_6], 2).unwrap(),
        TrackIndex::new(0),
    );
    // A note in each track's first bar; only track 1 has one in the second bar.
    for track in 0..2 {
        let position = BeatPosition {
            track: TrackIndex::new(track),
            ..at(0, 0)
        };
        e.execute_command(Command::SetFret {
            at: position,
            string: StringNumber::new(1).unwrap(),
            fret: gscore_model::Fret::new(3).unwrap(),
        })
        .unwrap();
    }
    // Copying bar 1 of track 1 only, but from every track.
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(0),
        focus: MeasureIndex::new(0),
        track: Some(TrackIndex::new(0)),
    }));
    plain(&mut e, ActionId::CopyAllTracks);
    assert_eq!(e.clipboard_bars(), Some(1));
    // Inserting the copied bars after that bar grows the song by one, both tracks.
    assert!(matches!(
        e.paste_measures_inserting(MeasureIndex::new(1)),
        Ok(Outcome::Edited)
    ));
    assert_eq!(e.song().measure_count(), 3);
    for track in 0..2 {
        let position = BeatPosition {
            track: TrackIndex::new(track),
            ..at(1, 0)
        };
        assert_eq!(notes_at(&e, position), vec![(1, 3)], "track {track}");
    }
    // Inserting before the first bar shifts it and the copy to bars 2 and 3.
    e.set_selection(None);
    assert!(matches!(
        e.paste_measures_inserting(MeasureIndex::new(0)),
        Ok(Outcome::Edited)
    ));
    assert_eq!(e.song().measure_count(), 4);
    for track in 0..2 {
        let first = BeatPosition {
            track: TrackIndex::new(track),
            ..at(0, 0)
        };
        let third = BeatPosition {
            track: TrackIndex::new(track),
            ..at(2, 0)
        };
        assert_eq!(notes_at(&e, first), vec![(1, 3)], "track {track}");
        assert_eq!(notes_at(&e, third), vec![(1, 3)], "track {track}");
    }
    // One undo step per insert.
    plain(&mut e, ActionId::Undo);
    assert_eq!(e.song().measure_count(), 3);
}

/// Several sections selected together swap as one run with the section they are dropped
/// on; the sections between keep their order between them.
#[test]
fn a_run_of_sections_swaps_with_another_section() {
    // Seven bars, one section per bar or two: A (1), B (2-3), C (4), D (5-7).
    let mut e = editor(7);
    for (measure, title) in [(0, "A"), (1, "B"), (3, "C"), (4, "D")] {
        e.execute_command(Command::SetSection {
            measure: MeasureIndex::new(measure),
            title: title.to_owned(),
            color: gscore_model::Section::DEFAULT_COLOR,
        })
        .unwrap();
    }
    let order = |e: &TabEditor| -> Vec<String> {
        (0..e.song().measure_count())
            .filter_map(|measure| e.song().section(MeasureIndex::new(measure)))
            .map(|section| section.title)
            .collect()
    };
    // A and B (bars 1-3) dropped on D: D comes first, C stays between, A and B last.
    assert!(matches!(
        e.swap_section_run(
            MeasureIndex::new(0),
            MeasureIndex::new(2),
            MeasureIndex::new(4)
        ),
        Ok(Outcome::Edited)
    ));
    assert_eq!(order(&e), ["D", "C", "A", "B"]);
    // The run is selected where it went: bars 5-7.
    assert!(matches!(
        e.selection(),
        Some(Selection::Measures { anchor, focus, .. })
            if anchor == MeasureIndex::new(4) && focus == MeasureIndex::new(6)
    ));
    // One undo step.
    plain(&mut e, ActionId::Undo);
    assert_eq!(order(&e), ["A", "B", "C", "D"]);
    // Backwards: C and D dropped on A.
    e.swap_section_run(
        MeasureIndex::new(3),
        MeasureIndex::new(6),
        MeasureIndex::new(0),
    )
    .unwrap();
    assert_eq!(order(&e), ["C", "D", "B", "A"]);
    plain(&mut e, ActionId::Undo);
    // Not whole sections, or a target inside the run: refused, nothing changes.
    assert!(
        e.swap_section_run(
            MeasureIndex::new(0),
            MeasureIndex::new(1),
            MeasureIndex::new(4)
        )
        .is_err()
    );
    assert!(
        e.swap_section_run(
            MeasureIndex::new(0),
            MeasureIndex::new(2),
            MeasureIndex::new(1)
        )
        .is_err()
    );
    assert_eq!(order(&e), ["A", "B", "C", "D"]);
}

#[test]
fn swapping_two_sections_swaps_their_bars_and_names() {
    // Six bars: section A at bar 1 (bars 1-3) and B at bar 4 (bars 4-6).
    let mut e = editor(6);
    for (measure, title) in [(0, "A"), (3, "B")] {
        e.execute_command(Command::SetSection {
            measure: MeasureIndex::new(measure),
            title: title.to_owned(),
            color: gscore_model::Section::DEFAULT_COLOR,
        })
        .unwrap();
    }
    for (measure, fret) in [(0, 3), (3, 7)] {
        e.execute_command(Command::SetFret {
            at: at(measure, 0),
            string: StringNumber::new(1).unwrap(),
            fret: gscore_model::Fret::new(fret).unwrap(),
        })
        .unwrap();
    }
    // Dragging A onto B swaps the two blocks: B's bars come first, A's last.
    assert!(matches!(
        e.swap_sections(MeasureIndex::new(0), MeasureIndex::new(3)),
        Ok(Outcome::Edited)
    ));
    assert_eq!(
        e.song().section(MeasureIndex::new(0)).map(|s| s.title),
        Some("B".to_owned())
    );
    assert_eq!(
        e.song().section(MeasureIndex::new(3)).map(|s| s.title),
        Some("A".to_owned())
    );
    assert_eq!(notes_at(&e, at(0, 0)), vec![(1, 7)], "B's note came along");
    assert_eq!(notes_at(&e, at(3, 0)), vec![(1, 3)], "A's note came along");
    // The dragged section (A) is selected where it went: bars 4-6.
    assert!(matches!(
        e.selection(),
        Some(Selection::Measures { anchor, focus, .. })
            if anchor == MeasureIndex::new(3) && focus == MeasureIndex::new(5)
    ));
    // One undo step puts both sections back.
    plain(&mut e, ActionId::Undo);
    assert_eq!(
        e.song().section(MeasureIndex::new(0)).map(|s| s.title),
        Some("A".to_owned())
    );
    assert_eq!(notes_at(&e, at(0, 0)), vec![(1, 3)]);
}

/// What a bar holds that a swap must carry: its section, tempo change, key signature,
/// repeat signs and endings, and its first beat's notes.
fn bar_data(e: &TabEditor, measure: usize) -> String {
    let index = MeasureIndex::new(measure);
    let song = e.song();
    format!(
        "{:?} {:?} {:?} {:?} {:?}",
        song.section(index).map(|section| section.title),
        song.tempo_change(index),
        song.key_signature(index),
        song.repeat(index),
        notes_at(e, at(measure, 0)),
    )
}

#[test]
fn swapping_sections_carries_every_bar_setting_with_the_bars() {
    // Seven bars: A (bars 1-3) with a repeat, a one-bar bridge, B (bars 5-7) starting in
    // another key and tempo.
    let mut e = editor(7);
    for (measure, title) in [(0, "A"), (3, "Bridge"), (4, "B")] {
        e.execute_command(Command::SetSection {
            measure: MeasureIndex::new(measure),
            title: title.to_owned(),
            color: gscore_model::Section::DEFAULT_COLOR,
        })
        .unwrap();
    }
    for (measure, fret) in [(0, 3), (3, 5), (4, 7), (6, 9)] {
        e.execute_command(Command::SetFret {
            at: at(measure, 0),
            string: StringNumber::new(1).unwrap(),
            fret: gscore_model::Fret::new(fret).unwrap(),
        })
        .unwrap();
    }
    for command in [
        Command::SetRepeat {
            measure: MeasureIndex::new(1),
            repeat: gscore_model::Repeat {
                start: true,
                end_repeats: None,
                endings: 0,
            },
        },
        Command::SetRepeat {
            measure: MeasureIndex::new(2),
            repeat: gscore_model::Repeat {
                start: false,
                end_repeats: Some(1),
                endings: 0,
            },
        },
        Command::SetMeasureTempo {
            measure: MeasureIndex::new(4),
            tempo: 140,
        },
        Command::SetKeySignature {
            measure: MeasureIndex::new(4),
            key: 2,
            minor: false,
        },
    ] {
        e.execute_command(command).unwrap();
    }
    let before: Vec<String> = (0..7).map(|measure| bar_data(&e, measure)).collect();
    assert!(matches!(
        e.swap_sections(MeasureIndex::new(0), MeasureIndex::new(4)),
        Ok(Outcome::Edited)
    ));
    // B, the bridge, then A: each bar exactly as it was, only somewhere else. The tempo
    // change that started B leaves with it instead of staying behind at bar 5.
    let after: Vec<String> = (0..7).map(|measure| bar_data(&e, measure)).collect();
    let order = [4, 5, 6, 3, 0, 1, 2];
    let expected: Vec<String> = order.iter().map(|&index| before[index].clone()).collect();
    assert_eq!(after, expected);
    assert_eq!(e.song().tempo_change(MeasureIndex::new(4)), None);
    // A is selected where it went: bars 5-7.
    assert!(matches!(
        e.selection(),
        Some(Selection::Measures { anchor, focus, .. })
            if anchor == MeasureIndex::new(4) && focus == MeasureIndex::new(6)
    ));
    // One undo step puts everything back.
    plain(&mut e, ActionId::Undo);
    let undone: Vec<String> = (0..7).map(|measure| bar_data(&e, measure)).collect();
    assert_eq!(undone, before);
}

#[test]
fn cut_bars_pasted_over_the_same_selection_go_there_and_are_not_duplicated() {
    let mut e = editor(6);
    e.execute_command(Command::SetFret {
        at: at(0, 0),
        string: StringNumber::new(1).unwrap(),
        fret: gscore_model::Fret::new(3).unwrap(),
    })
    .unwrap();
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(0),
        focus: MeasureIndex::new(1),
        track: None,
    }));
    plain(&mut e, ActionId::Cut);
    assert_eq!(e.song().measure_count(), 4);
    // The selection still covers bars 1-2, which now hold what came after the cut: the
    // paste writes the cut bars there, as before copying learned to duplicate.
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(0),
        focus: MeasureIndex::new(1),
        track: None,
    }));
    plain(&mut e, ActionId::Paste);
    assert_eq!(
        e.song().measure_count(),
        4,
        "nothing was added after the selection"
    );
    assert_eq!(notes_at(&e, at(0, 0)), vec![(1, 3)]);
}

#[test]
fn copying_every_track_takes_the_whole_bars_a_beat_selection_touches() {
    let mut e = TabEditor::new(
        testing::song(&[GUITAR_6, GUITAR_6], 4).unwrap(),
        TrackIndex::new(0),
    );
    // Beats of the first track, from bar 2 into bar 3.
    e.set_selection(Some(Selection::Beats {
        track: TrackIndex::new(0),
        staff: gscore_model::StaffIndex::new(0),
        voice: VoiceIndex::new(0),
        anchor: spot(1, 0),
        focus: spot(2, 0),
    }));
    plain(&mut e, ActionId::CopyAllTracks);
    assert_eq!(e.clipboard_bars(), Some(2));
    assert_eq!(
        e.clipboard_text().as_deref(),
        Some("GuitarScore: 2 bars of 2 tracks")
    );
    // Nothing selected: the cursor's bar.
    e.set_selection(None);
    plain(&mut e, ActionId::CopyAllTracks);
    assert_eq!(e.clipboard_bars(), Some(1));
}

#[test]
fn copied_bars_of_one_track_pasted_on_their_selection_overwrite_them() {
    // A bar selection in one track (as the score makes it) is pasted over, not after.
    let mut e = editor(3);
    e.execute_command(Command::SetFret {
        at: at(0, 0),
        string: StringNumber::new(1).unwrap(),
        fret: gscore_model::Fret::new(3).unwrap(),
    })
    .unwrap();
    let one_track = Selection::Measures {
        anchor: MeasureIndex::new(0),
        focus: MeasureIndex::new(0),
        track: Some(TrackIndex::new(0)),
    };
    e.set_selection(Some(one_track));
    plain(&mut e, ActionId::Copy);
    assert_eq!(plain(&mut e, ActionId::Paste), Outcome::Edited);
    assert_eq!(e.song().measure_count(), 3, "no bar was added");
    assert_eq!(notes_at(&e, at(0, 0)), vec![(1, 3)]);
    assert!(notes_at(&e, at(1, 0)).is_empty());
    // Moved to another bar, the paste writes over that one.
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(2),
        focus: MeasureIndex::new(2),
        track: Some(TrackIndex::new(0)),
    }));
    plain(&mut e, ActionId::Paste);
    assert_eq!(e.song().measure_count(), 3);
    assert_eq!(notes_at(&e, at(2, 0)), vec![(1, 3)]);
}

#[test]
fn pasting_over_bars_replaces_them_and_only_grows_the_song_at_its_end() {
    let mut e = TabEditor::new(
        testing::song(&[GUITAR_6, GUITAR_6], 3).unwrap(),
        TrackIndex::new(0),
    );
    // A note in the first bar of both tracks, then a copy of that bar.
    for track in 0..2 {
        let position = BeatPosition {
            track: TrackIndex::new(track),
            ..at(0, 0)
        };
        e.execute_command(Command::SetFret {
            at: position,
            string: StringNumber::new(1).unwrap(),
            fret: gscore_model::Fret::new(3).unwrap(),
        })
        .unwrap();
    }
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(0),
        focus: MeasureIndex::new(0),
        track: None,
    }));
    plain(&mut e, ActionId::Copy);
    // Overwriting the last bar writes over it; the song keeps its length.
    assert!(matches!(
        e.paste_measures_overwriting(MeasureIndex::new(2)),
        Ok(Outcome::Edited)
    ));
    assert_eq!(e.song().measure_count(), 3);
    for track in 0..2 {
        let position = BeatPosition {
            track: TrackIndex::new(track),
            ..at(2, 0)
        };
        assert_eq!(notes_at(&e, position), vec![(1, 3)], "track {track}");
    }
    // Overwriting past the end adds the bars it needs.
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(0),
        focus: MeasureIndex::new(1),
        track: None,
    }));
    plain(&mut e, ActionId::Copy);
    assert!(matches!(
        e.paste_measures_overwriting(MeasureIndex::new(2)),
        Ok(Outcome::Edited)
    ));
    assert_eq!(e.song().measure_count(), 4);
    // One undo step puts the three bars back.
    plain(&mut e, ActionId::Undo);
    assert_eq!(e.song().measure_count(), 3);
}

#[test]
fn pasting_bars_repeats_them_and_takes_only_the_attributes_asked_for() {
    let mut e = editor(3);
    // The source bar carries a 3/4 signature, a tempo, a section and a beat text.
    e.execute_command(Command::SetTimeSignature {
        measure: MeasureIndex::new(0),
        numerator: 3,
        denominator: 4,
    })
    .unwrap();
    e.execute_command(Command::SetMeasureTempo {
        measure: MeasureIndex::new(0),
        tempo: 90,
    })
    .unwrap();
    e.execute_command(Command::SetSection {
        measure: MeasureIndex::new(0),
        title: "Source".to_owned(),
        color: gscore_model::Section::DEFAULT_COLOR,
    })
    .unwrap();
    e.execute_command(Command::SetBeatText {
        at: at(0, 0),
        text: "Source text".to_owned(),
    })
    .unwrap();
    e.execute_command(Command::SetFret {
        at: at(0, 0),
        string: StringNumber::new(1).unwrap(),
        fret: gscore_model::Fret::new(5).unwrap(),
    })
    .unwrap();
    // The target bar has a signature, tempo, section and text of its own.
    e.execute_command(Command::SetTimeSignature {
        measure: MeasureIndex::new(2),
        numerator: 4,
        denominator: 4,
    })
    .unwrap();
    e.execute_command(Command::SetMeasureTempo {
        measure: MeasureIndex::new(2),
        tempo: 120,
    })
    .unwrap();
    e.execute_command(Command::SetSection {
        measure: MeasureIndex::new(2),
        title: "Target".to_owned(),
        color: gscore_model::Section::DEFAULT_COLOR,
    })
    .unwrap();
    e.execute_command(Command::SetBeatText {
        at: at(2, 0),
        text: "Target text".to_owned(),
    })
    .unwrap();
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(0),
        focus: MeasureIndex::new(0),
        track: None,
    }));
    plain(&mut e, ActionId::Copy);
    // Without the attributes, only the music comes over.
    let bare = PasteOptions {
        time_signatures: false,
        tempo: false,
        sections: false,
        texts: false,
        ..PasteOptions::default()
    };
    assert!(matches!(
        e.paste_measures_overwriting_with(MeasureIndex::new(2), bare),
        Ok(Outcome::Edited)
    ));
    assert_eq!(notes_at(&e, at(2, 0)), vec![(1, 5)]);
    let measure = e
        .song()
        .track(TrackIndex::new(0))
        .and_then(|track| track.measure(MeasureIndex::new(2)))
        .expect("the target bar");
    assert_eq!(
        measure
            .time_signature()
            .map(|signature| signature.numerator()),
        Ok(4)
    );
    assert_eq!(e.song().tempo_change(MeasureIndex::new(2)), Some(120));
    assert_eq!(
        e.song().section(MeasureIndex::new(2)).map(|s| s.title),
        Some("Target".to_owned())
    );
    // The bars are replaced, so without the texts the copied bars' texts are not taken
    // and the old text goes with the bar it was on.
    assert_eq!(
        e.song().beat(at(2, 0)).map(|beat| beat.text().to_owned()),
        Some(String::new())
    );
    // With them, the bar's own headers and text come too.
    assert!(matches!(
        e.paste_measures_overwriting_with(MeasureIndex::new(2), PasteOptions::default()),
        Ok(Outcome::Edited)
    ));
    let measure = e
        .song()
        .track(TrackIndex::new(0))
        .and_then(|track| track.measure(MeasureIndex::new(2)))
        .expect("the target bar");
    assert_eq!(
        measure
            .time_signature()
            .map(|signature| signature.numerator()),
        Ok(3)
    );
    assert_eq!(e.song().tempo_change(MeasureIndex::new(2)), Some(90));
    assert_eq!(
        e.song().section(MeasureIndex::new(2)).map(|s| s.title),
        Some("Source".to_owned())
    );
    assert_eq!(
        e.song().beat(at(2, 0)).map(|beat| beat.text().to_owned()),
        Some("Source text".to_owned())
    );
    // Three copies of the bar, inserted after the second: the song grows by three.
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(0),
        focus: MeasureIndex::new(0),
        track: None,
    }));
    plain(&mut e, ActionId::Copy);
    assert!(matches!(
        e.paste_measures_inserting_with(
            MeasureIndex::new(1),
            PasteOptions::default().with_copies(3)
        ),
        Ok(Outcome::Edited)
    ));
    assert_eq!(e.song().measure_count(), 6);
    for measure in 1..=3 {
        assert_eq!(
            notes_at(&e, at(measure, 0)),
            vec![(1, 5)],
            "the copy at bar {}",
            measure.saturating_add(1)
        );
    }
}

/// Copying one track's bars and pasting them in another track used to write them back
/// into the track they were copied from: the other track stayed empty.
#[test]
fn bars_of_one_track_paste_into_the_track_pasted_into() {
    let mut e = TabEditor::new(
        testing::song(&[GUITAR_6, GUITAR_6], 3).unwrap(),
        TrackIndex::new(0),
    );
    e.execute_command(Command::SetFret {
        at: at(0, 0),
        string: StringNumber::new(2).unwrap(),
        fret: gscore_model::Fret::new(5).unwrap(),
    })
    .unwrap();
    let second = |measure| BeatPosition {
        track: TrackIndex::new(1),
        ..at(measure, 0)
    };
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(0),
        focus: MeasureIndex::new(0),
        track: Some(TrackIndex::new(0)),
    }));
    plain(&mut e, ActionId::Copy);

    // The second track's bar 2 selected (a click in the score, or its grid row).
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(1),
        focus: MeasureIndex::new(1),
        track: Some(TrackIndex::new(1)),
    }));
    assert_eq!(plain(&mut e, ActionId::Paste), Outcome::Edited);
    assert_eq!(notes_at(&e, second(1)), vec![(2, 5)]);
    assert!(
        notes_at(&e, at(1, 0)).is_empty(),
        "the first track is left alone"
    );
    assert_eq!(e.cursor().track, TrackIndex::new(1));

    // Only the cursor in the second track, through the Paste bars dialog's path.
    e.set_selection(None);
    e.set_cursor(Cursor {
        track: TrackIndex::new(1),
        ..e.cursor()
    });
    e.paste_measures_overwriting(MeasureIndex::new(2)).unwrap();
    assert_eq!(notes_at(&e, second(2)), vec![(2, 5)]);
    assert!(notes_at(&e, at(2, 0)).is_empty());
}

/// What was copied in one song pastes in another (the app hands the clipboard over when
/// another tab is shown); copied bars do not count as standing on their source there.
#[test]
fn a_clipboard_handed_to_another_song_pastes_there() {
    let mut from = editor(2);
    from.execute_command(Command::SetFret {
        at: at(0, 0),
        string: StringNumber::new(1).unwrap(),
        fret: gscore_model::Fret::new(7).unwrap(),
    })
    .unwrap();
    let bar = Selection::Measures {
        anchor: MeasureIndex::new(0),
        focus: MeasureIndex::new(0),
        track: None,
    };
    from.set_selection(Some(bar));
    plain(&mut from, ActionId::Copy);

    let mut to = editor(2);
    to.set_clipboard(from.clipboard());
    to.set_selection(Some(bar));
    assert_eq!(plain(&mut to, ActionId::Paste), Outcome::Edited);
    assert_eq!(
        to.song().measure_count(),
        2,
        "written over, not inserted after"
    );
    assert_eq!(notes_at(&to, at(0, 0)), vec![(1, 7)]);
}
