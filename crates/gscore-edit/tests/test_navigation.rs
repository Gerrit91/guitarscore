//! Cursor navigation: through empty measures, across repeats in written order, and to the
//! start or end of the track.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

use gscore_edit::{Action, ActionId, BeatSpot, Command, Cursor, Outcome, Selection, TabEditor};
use gscore_model::testing::{self, GUITAR_6};
use gscore_model::{BeatIndex, MeasureIndex, TrackIndex, VoiceIndex};

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

fn go(editor: &mut TabEditor, measure: usize, beat: usize) {
    editor.set_cursor(Cursor {
        measure: MeasureIndex::new(measure),
        beat: BeatIndex::new(beat),
        ..editor.cursor()
    });
}

/// Removes every beat of `measure`, as Guitar Pro's empty GP7 bar has none.
fn empty_measure(editor: &mut TabEditor, measure: usize) {
    let at = gscore_model::BeatPosition {
        track: editor.cursor().track,
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(0),
    };
    for _ in 0..4 {
        editor
            .execute_command(Command::DeleteBeat { at })
            .unwrap_or_else(|e| panic!("emptying measure {measure}: {e}"));
    }
    assert_eq!(
        editor.cursor().beats_in(editor.song(), MeasureIndex::new(measure)),
        0
    );
}

fn spot(measure: usize, beat: usize) -> BeatSpot {
    BeatSpot {
        measure: MeasureIndex::new(measure),
        beat: BeatIndex::new(beat),
    }
}

#[test]
fn arrows_stop_in_an_empty_measure_in_the_middle_and_at_the_end() {
    let mut e = editor(3);
    empty_measure(&mut e, 1);
    empty_measure(&mut e, 2);

    // Right from the last beat before the empty bar: its start.
    go(&mut e, 0, 3);
    assert_eq!(plain(&mut e, ActionId::CursorRight), Outcome::Moved);
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (1, 0));
    // On from there into the next empty bar, and on into a bar with beats.
    assert_eq!(plain(&mut e, ActionId::CursorRight), Outcome::Moved);
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (2, 0));
    // In the last (empty) bar Right adds a measure, as at the end of any bar.
    assert_eq!(plain(&mut e, ActionId::CursorRight), Outcome::Edited);
    assert_eq!(e.song().measure_count(), 4);
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (3, 0));

    // Left from an empty bar goes back into the previous empty bar, and on into the
    // last beat of the bar before it.
    go(&mut e, 2, 0);
    assert_eq!(plain(&mut e, ActionId::CursorLeft), Outcome::Moved);
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (1, 0));
    assert_eq!(plain(&mut e, ActionId::CursorLeft), Outcome::Moved);
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (0, 3));
}

#[test]
fn shift_arrows_extend_the_selection_through_empty_measures() {
    let mut e = editor(3);
    empty_measure(&mut e, 1);
    empty_measure(&mut e, 2);
    go(&mut e, 0, 2);

    for _ in 0..3 {
        assert_eq!(plain(&mut e, ActionId::SelectRight), Outcome::Moved);
    }
    assert_eq!(
        e.selection(),
        Some(Selection::Beats {
            track: TrackIndex::new(0),
            voice: VoiceIndex::new(0),
            anchor: spot(0, 2),
            focus: spot(2, 0),
        })
    );
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (2, 0));

    // Back through the empty bars to the anchor and beyond it.
    for _ in 0..4 {
        assert_eq!(plain(&mut e, ActionId::SelectLeft), Outcome::Moved);
    }
    assert_eq!(
        e.selection(),
        Some(Selection::Beats {
            track: TrackIndex::new(0),
            voice: VoiceIndex::new(0),
            anchor: spot(0, 2),
            focus: spot(0, 1),
        })
    );
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (0, 1));
}
