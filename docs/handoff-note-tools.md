# Handoff: the note tools (fretboard, keyboard, drum map)

Owner's request of 2026-11-14: three tools for entering guitar, piano and drum notes, as
in Guitar Pro, each a button at the top right and an entry under Tools, each opening its
own window. Built on `feature/note-input-tools` (from `main` after the editor polish was
merged). This file says what is there and what is left.

## Done

- `crates/gscore-app/src/app/note_tools.rs` holds all three windows, their geometry
  (`Board`, `keys`, `key_at`), the drum names and the click handling
  (`GuitarScoreApp::show_note_tools`). State: `GuitarScoreApp::note_tools` (which windows
  are open, where they sit); not saved in the settings.
- **Fretboard:** the shown staff's strings (string 1 at the top), 24 frets, fret numbers,
  the cursor beat's notes as circles with their note names. A click on a string and fret
  enters that note; a click on a note removes it.
- **Keyboard:** C1 to C7, the beat's sounding pitches in blue. A click enters the key on
  the lowest fret of a string the beat does not use (`BeatNotes::place_for`); a click on a
  sounding key removes it.
- **Drum map:** MIDI note, General MIDI name and kit part (`gscore_audio::drum_part`) for
  notes 35 to 81, the beat's drums marked. On a drum track a click on a name enters or
  removes that drum; on another track the map is only shown.
- Toolbar buttons (`Icon::Fretboard`, `Icon::PianoKeys`, `Icon::DrumMap`, shown from a
  toolbar width of 1100 points), Tools menu checkboxes, Escape closes the front window.
- Tests: unit tests in `note_tools.rs`, `crates/gscore-app/tests/test_note_tools.rs`
  (with the `note_tools` snapshot). Devlog and `docs/parity.md` updated.

## Not done (none of it asked for yet; each is the owner's call)

- The windows show only the cursor's beat, not a selection of several beats, and not the
  notes sounding while playing.
- An entered note is heard (like a typed fret), but there is no "listen only" mode that
  sounds a fret, key or drum without entering it.
- The drum tab is the track's own tablature switch (Track panel › Appearance on a drum
  track; `gscore-layout/src/score/part.rs`), stored in the file like a guitar's. It shows
  the stored MIDI numbers on the kit's "strings" as they are, with a TAB clef; there are
  no drum names beside the lines. A file whose kit has the switch on (older Guitar Pro
  files often do) now opens with the drum tab.
- The fretboard is right-handed only, shows no capo, no scale or chord overlay, and its
  note names have no octave. A keyboard track with two staves shows the cursor's staff.
- The keyboard's range is fixed (C1 to C7); pitches outside it are not shown.
- The drum map is General MIDI's, as the synthesizer splits it into parts; it cannot be
  edited, and the notation's own map (`gscore-layout/src/notation/drums.rs`) is separate.
- Window positions and open state are not remembered between runs.
- The icons are first drafts (see `icons.rs`); the owner's style is flat, few details.

## Known state of the checks

`cargo fmt`, clippy and `cargo xtask layers` pass. All tests pass except
`dismissing_an_open_error_shows_the_empty_workspace`, which also fails on `main` when the
checkout sits in a very long path (as the temporary worktree did); it was not run in the
owner's normal folder.
