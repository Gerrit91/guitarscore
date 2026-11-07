# Handoff: selections and keyboard navigation over empty bars, repeats; grid selection

Owner's list of 2026-11-07. Reproduce each point first (a song with empty bars, one with
repeats and alternative endings: repository test data only, never `~/svn/bigfive`), then
fix it with a test. Pointers below are where the review found the likely causes; check
them, they are not proven.

## 1. A selection over empty bars is drawn

Today a selection over several bars only shows where the bars have notes.

- Likely cause: the selection's bands come from the layout's beat places
  (`crates/gscore-app/src/app/page.rs`, `selection_bands(&layout.places, …)`); an empty
  bar has no beat places, so no band. The bands must come from the **bars' extents**
  (each selected bar's x range on its system line, the staff's height), so an empty bar
  inside or at the end of a selection is covered like any other. A selection of beats
  that starts or ends in a bar keeps starting or ending at that beat.
- The same for every view (pages, screen vertical and horizontal) and for several tracks
  shown at once.

## 2. The arrow keys move through empty bars

Today Left and Right do not get past (or into) empty bars.

- Likely cause: `crates/gscore-edit/src/cursor.rs` `Cursor::left` and `Cursor::right` skip
  a measure with no beats (`if self.beats_in(song, measure) == 0 { continue; }` and the
  `checked_sub(1)` on an empty measure in `left`). An empty bar must be a stop: the
  cursor sits at its start (the place where typing a fret fills it; the layout already
  draws a cursor in an empty measure, see `Sequence::tick_of`'s "the cursor in an empty
  measure"). Right from the last beat of a bar goes into the next bar even when it is
  empty, and on from there; Left likewise backwards.
- Shift+Left/Right extends the selection through empty bars the same way.
- Grace-note beats stay skipped as now.

## 3. Repeats and alternative endings do not disturb navigation

The owner suspects problems with the arrow keys around repeats and alternative endings.

- Navigation goes through the bars **in written order** (bar 1, 2, 3, …), ignoring
  repeats, voltas and Da Capo / Segno directions: the cursor never jumps back to a repeat
  start or skips an ending. Only playback follows the played order.
- Find where the cursor or the selection uses the played order or measure spans
  (`gscore-play` `measure_spans`, `MeasureSpan`, the playback cursor following) and make
  the keys use bar indices only. Test with a song that has a repeat with two endings: Right
  from the last beat before the repeat sign goes into the next written bar.

## 4. Home and End

- **Home** moves the cursor to the first beat of the track (bar 1), **End** to the last
  beat of the last bar (or the start of the last bar if it is empty).
- **Shift+Home / Shift+End** extend the selection from where it is anchored to the track's
  start or end.
- Key names exist (`crates/gscore-edit/src/keymap.rs` `NamedKey::Home`, `NamedKey::End`,
  mapped in `crates/gscore-app/src/keys.rs`), but no action is bound to them as far as the
  review saw: add the actions (cursor commands in `gscore-edit`, so the CLI and the
  keymap can use them too), default bindings, and the shortcuts list
  (`app/shortcuts.rs`). Check that they do not clash with an existing binding on macOS
  (Cmd+Left/Right are the usual Home/End there; offer those too if free).

## 5. Selecting bars of one track in the song grid

Today a selection in the song grid is a range of bars across **all** tracks
(`crates/gscore-app/src/state.rs` `MeasureSelection { anchor, focus }`,
`app/grid/matrix.rs`).

- Pressing on a bar cell and dragging **within that track's row** selects bars of that
  track only (a range of bars on one track; the score shows that track's bars selected).
- When the drag leaves the row (up or down into another track's row, or onto the measure
  ruler), the selection becomes the bars across **all tracks**, as today. Dragging back
  into the starting row makes it one track again.
- Shift+click extends the selection on the same track (or across all, if the selection
  is across all). A click on the measure ruler keeps selecting across all tracks.
- The selection must stay one with the score's (the editor's `Selection::Measures`
  carries a track, or a new variant for one track's bars): copy, paste, delete, loop
  playback and the palette act on what is selected (one track or all). Check each of them.

## 6. Copy and paste of notes and beats

The owner, 2026-11-07: copying and pasting selected beats does not work. Two kinds are
needed, as in Guitar Pro:

- **A single note:** with the cursor on a note and nothing selected, Copy takes that note
  (its fret and its note effects); Paste puts it on the string under the cursor, in the
  beat under the cursor, and leaves the beat's other notes alone. Guitar Pro does this: copy
  fret 2 on one string, paste 2 onto another string. Whether the pasted note keeps the
  fret or the pitch (the same note on another string needs another fret) follows Guitar
  Pro: check what it does, the owner's example keeps the fret.
- **One beat or several beats** (a selection): Paste replaces whole beats from the cursor
  on (length, notes, beat effects), as `paste_beats` intends.

Pointers (from the review, not proven):

- `crates/gscore-edit/src/selection/clipboard.rs` `copy_selection` needs a selection
  (`EditError::NothingSelected` without one), so copying with only the cursor on a note
  does nothing today. The clipboard (`Clipboard`) has no single-note kind yet: add one,
  and a paste that writes one note (`Command::SetFret` and its effects) into the
  cursor's beat on the cursor's string.
- The keys reach the editor (`app/keys.rs` maps egui's `Event::Copy`, `Event::Cut` and
  `Event::Paste` to Ctrl+C/X/V; `tab_editor.rs` runs `ActionId::Copy` and `Paste`), so first
  reproduce in the app why a beat selection does not paste: the selection being made in
  the score but not reaching the editor's (`set_selection`), the paste landing somewhere
  else, or the copy failing silently (an error that is never shown). The menu entries
  (`page_menu.rs`, `bars.rs`) must work the same way as the keys.
- A paste that cannot happen (too few bars left: `paste_beats` refuses) says so in the
  status line instead of doing nothing.
- Copy and paste of bars in the grid (section 5) stay as they are and act on one track or
  all.

## Tests

- Edit tests (`crates/gscore-edit/tests/`): Left/Right through an empty bar in the middle
  and at the end; Shift+arrows across empty bars; navigation across a repeat with two
  endings in written order; Home/End and Shift+Home/End.
- Edit tests for copy and paste: a single note copied from one string and pasted onto
  another string of another beat (the beat's other notes stay, the effects come along);
  one beat and several beats over a bar line pasted over existing beats (replaced whole);
  undo of each paste is one step.
- App tests (`crates/gscore-app/tests/`): a selection over empty bars draws a band over
  each (snapshot); dragging in the grid within one row selects one track, out of the row
  all tracks, back in one track again; copy and delete act on one track's selection only;
  Ctrl+C / Ctrl+V and the menu entries copy and paste a note and a beat selection made in
  the score.
