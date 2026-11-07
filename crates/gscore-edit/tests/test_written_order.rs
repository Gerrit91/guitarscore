//! Cursor navigation follows the bars in written order: repeats, alternative endings and
//! navigation signs belong to playback, not to the arrow keys.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

use gscore_edit::{Action, ActionId, Cursor, Outcome, TabEditor};
use gscore_model::testing;
use gscore_model::{BeatIndex, MeasureIndex, TrackIndex};

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

#[test]
fn a_repeat_with_two_endings_does_not_change_the_written_order() {
    // A repeat from bar 2 to bar 3 with two alternatives in bars 3 and 4.
    let song =
        testing::song_with_repeats(5, &[(1, true, -1, 0), (2, false, 2, 1), (3, false, -1, 2)])
            .unwrap();
    let mut e = TabEditor::new(song, TrackIndex::new(0));
    go(&mut e, 1, 3);
    assert_eq!(plain(&mut e, ActionId::CursorRight), Outcome::Moved);
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (2, 0));
    // The repeat sign at the end of bar 3 does not send the cursor back to bar 2.
    go(&mut e, 2, 3);
    assert_eq!(plain(&mut e, ActionId::CursorRight), Outcome::Moved);
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (3, 0));
    // Nor does Left jump to an ending or a repeat's end.
    go(&mut e, 4, 0);
    assert_eq!(plain(&mut e, ActionId::CursorLeft), Outcome::Moved);
    assert_eq!((e.cursor().measure.get(), e.cursor().beat.get()), (3, 3));
}
