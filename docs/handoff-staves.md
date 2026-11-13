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

## Still to do, in this order

1. **Layout.** A row of a system is a track today (`score.rs` `layout_with`, rows keyed
   by `TrackIndex`; `LayoutCache` per track; `Layout::tracks` / `TrackPlaces` per track).
   Make a row a (track, staff): lay out each staff through `TrackView::on_staff`, so the
   measure layout code needs no change; stack a track's staves directly under each other
   (a smaller gap than between tracks), each with its own clef (the bass staff's bars are
   bass clef); join them with a brace (SMuFL `brace`) and one barline through both; the
   track name once, beside the pair. A keyboard's staves are notation only (no tab), as
   Guitar Pro shows them. The cache key and places gain the staff. Goldens: a two-staff
   case in `gscore-layout/tests` (owner approves the picture).
2. **Positions and editing.** `BeatPosition` (and the note and voice positions built on
   it) gains `staff: StaffIndex`; `Song::beat`, the edit methods and the commands'
   positions address it, `Command::to_line` writes it (journal compatibility: a line
   without a staff means staff 0). The cursor moves from the lowest string of one staff
   to the highest of the next (and back); clicks hit the staff under the pointer. Every
   command's apply/undo test and `panic_hunt` over a two-staff song.
3. **The rest.** Copy and paste across staves; the track panel shows each staff's
   tuning; issues (`Song::issues`) per staff; the excerpt played on note entry uses the
   staff; MusicXML later.

Watch: `TrackView::staves()` (which kinds of staff a track's page shows: notation,
tab, slash) is an older, unrelated name; the new API says `staff`, `all_staves`.
