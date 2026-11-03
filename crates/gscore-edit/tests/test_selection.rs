//! Making a selection from the keyboard (Shift+arrows), selections that cross bar lines,
//! and clearing or copying what is selected.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

use gscore_edit::{Action, ActionId, BeatSpot, Command, Cursor, Outcome, Selection, TabEditor};
use gscore_model::testing::{self, GUITAR_6};
use gscore_model::{
    BeatIndex, BeatPosition, Dynamic, MeasureIndex, StringNumber, TrackIndex, VoiceIndex,
    differences,
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
fn shift_up_selects_the_whole_beat_and_delete_clears_its_notes() {
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
    // Shift+Down does the same; a second press keeps the selection as it is.
    assert_eq!(plain(&mut e, ActionId::SelectBeat), Outcome::Nothing);
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
        track: TrackIndex::new(0),
        voice: VoiceIndex::new(0),
        anchor: spot(0, 3),
        focus: spot(1, 0),
    }));
    plain(&mut e, ActionId::Copy);

    go(&mut e, 2, 3);
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
    go(&mut e, 3, 0);
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
        track: TrackIndex::new(0),
        voice: VoiceIndex::new(0),
        anchor: spot(0, 0),
        focus: spot(0, 0),
    }));
    plain(&mut e, ActionId::Copy);
    go(&mut e, 1, 2);
    assert_eq!(plain(&mut e, ActionId::Paste), Outcome::Edited);
    let dynamic =
        |e: &TabEditor, measure, beat| e.song().beat(at(measure, beat)).unwrap().dynamic();
    assert_eq!(
        dynamic(&e, 1, 2),
        Some(Dynamic::Pp),
        "not the ff in effect there"
    );
}

#[test]
fn cutting_a_selection_over_a_bar_line_leaves_rests() {
    let mut e = editor(2);
    go(&mut e, 0, 3);
    e.perform(Action::Digit(3), 0).unwrap();
    go(&mut e, 1, 0);
    e.perform(Action::Digit(7), 0).unwrap();
    e.set_selection(Some(Selection::Beats {
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
