# Handoff: drawing and editing a track's further staves

Owner decision (2026-11-13): a full staves model, every staff of a track equal
everywhere (model, layout, editing, playback, commands). Done on branch
`staves-gp6-drums`:

- **Data** (`gscore-gp`): `Track::staves` holds the staves after the first (measures,
  strings, tuning name, capo); the first stays on the track as in GP3-5. Read from and
  written to GP6/GP7 (`<Staves>`, a bar per staff per master bar). `docs/guitarpro.md`
  item 45.
- **Model** (`gscore-model`): `StaffIndex`; `TrackView::on_staff(staff)` is the track
  seen through one staff (its measures, strings, tuning and capo; name, sound and
  instrument are the track's); `StaffView` wraps it; `TrackView::staff_count`, `staff`,
  `all_staves`. Measure insert/delete, time and key signatures and their undo act on
  every staff (undo data: one measure per staff, every track's staves in turn); a new bar
  keeps its staff's clef. Validation and the save comparison cover every staff.
- **Playback** (`gscore-play` `build::collect`): every staff plays at its own pitches; a
  staff's strings are numbered from `STAFF_STRINGS` (12) × its index in `PlayedNote`, so
  staves never stop each other's notes.
- Test helper: `gscore_model::testing::song_with_two_staves`.

## Done on 2026-11-13 (all of the three stages below)

1. **Layout.** A row of a system is a (track, staff) pair: `score.rs` `layout_with` makes
   one `Part` per staff, laid out through `TrackView::on_staff`, so the measure code
   needed no change. A track's staves stack under each other with a smaller gap
   (`STAFF_GAP` 4 spaces) than between tracks (`TRACK_GAP` 5), each with its own clef and
   key/time signatures; one line joins them at the system start and a SMuFL `brace`
   (`Glyph::Brace`, stretched over the group) stands beside it; the track's name is
   written once, centered on the group. A track with several staves is written in
   standard notation only (no tab, no slash), as Guitar Pro shows a keyboard. The measures
   of a track's staves line up with each other like those of different tracks.
   `LayoutCache` keys per (track, staff); `TrackPlaces` and `Places` carry the staff;
   `Hit` carries it too, so `Layout::hit` says which staff the pointer is on
   (`Layout::staff_places`, `Layout::hit_in_voice_on`). One more fix: notation no longer
   transposes a keyboard's notes an octave up (`Notation::octave_up` is set for string
   instruments only), so a piano's bass staff is written at pitch. Goldens:
   `crates/gscore-layout/tests/golden/two_staves.all.svg` (new; **the owner still has to
   approve the picture**), and `tests/test_second_staff.rs`.
2. **Positions and editing.** `BeatPosition` gains `staff: StaffIndex`; `Song::beat`,
   `Song::measure_at` and `Song::staff` address it, `Song::edit_measure_at` (and
   `edit_measure_of`) edit the right staff's measure, and every model edit that takes a
   position goes through it. `Change`/undo data carry positions, so undo works per staff.
   `Command::to_line` writes `staff=` only when it is not the first (a line without one,
   from an older journal, means staff 1); `parse::position` reads it. `Cursor` gains the
   staff: Up moves from a staff's top string to the lowest string of the staff above,
   Down from a staff's lowest to the top of the staff below, and `clamp` pulls a cursor
   onto a staff its track has. Clicks (and right-clicks and drags) take the staff under
   the pointer; the cursor and its scroll use that staff's places. `Selection::Beats`
   gains the staff. Tests: `tests/test_second_staff.rs` (layout), `test_staves.rs` in
   `gscore-model` and `gscore-edit`, the two-staff song in `common::test_songs()` (so
   every command's apply/undo test and `panic_hunt` run on it), and a hunt over `staff=2`
   for every command with a position.
3. **The rest.** Copy/cut/paste across staves: a beat selection carries its staff, and a
   measure copy now takes every staff of the track and pastes them together. `Song::issues`
   walks every staff with that staff's strings and measures (`Issue` carries the staff;
   its `Display` names it when it is not the first). The track panel lists each staff's
   tuning under the tuning button. `Sequence::beat_excerpt` plays only the staff being
   edited, so entering a note on a piano's bass staff hears the bass.

## Still open

- Editing a second staff's tuning, string count and capo from the track panel (only
  display for now: the tuning dialog acts on the first staff).
- MusicXML import/export with second staves.
- The ASCII tab of `gscore-cli dump` and the lyrics editor show the first staff only.

Watch: `TrackView::staves()` (which kinds of staff a track's page shows: notation,
tab, slash) is an older, unrelated name; the staves API says `staff`, `all_staves`.
