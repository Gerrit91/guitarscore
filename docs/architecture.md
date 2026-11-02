# Architecture Overview

GuitarScore follows a layered architecture pattern:

```
guitarscore/
  Cargo.toml                 # workspace, [workspace.lints]
  rust-toolchain.toml        # pin a stable toolchain
  clippy.toml
  CLAUDE.md
  THIRD_PARTY.md
  docs/                      # architecture.md, development.md, parity.md, gaps.md, devlog.md
  testdata/                  # owner's own test tabs; testdata/corrupt/ for broken files
  corpus/                    # gitignored: owner's larger private collection of GP files (opt-in tests)
  assets/fonts/              # SMuFL fonts + metadata, text font
  xtask/                     # project tasks: layers, corpus, ci
  crates/
    gscore-gp/                # L0: Guitar Pro song data + reading/writing GP3-5, GPX, GP7+ (derived from guitarpro, MIT)
    gscore-model/             # L1: document model (wraps gscore-gp's song), positions, queries, invariants
    gscore-io/                # L2: GP files <-> model: format detection, limits, catch_unwind (bytes in, bytes out)
    gscore-layout/            # L2: model -> positioned draw ops. No GUI imports.
    gscore-play/              # L2: sequencer, MIDI events, channel allocator, Player trait. No device crates.
    gscore-edit/              # L3: cursor, selection, commands, undo/redo, keymap
    gscore-audio/             # L3: output backends (midir, cpal, rustysynth) implementing Player
    gscore-session/           # L4: open document, recovery journal, autosave, safe-save orchestration
    gscore-cli/               # L5: headless tool (info, dump, render, commands, edit, convert, roundtrip-check)
    gscore-app/               # L5: egui/eframe application shell (thin); produces the `guitarscore` binary
```

## Dependency Restrictions

- Only `gscore-app` may use egui, eframe, winit, or rfd
- Only `gscore-audio` may use `cpal`, `midir`, or `rustysynth`
- Nothing below `gscore-app` may use a GUI crate

## Layout (`gscore-layout`)

Layout is a pure function of the song, the settings and the font metrics:
`layout_track(song, track, options, fonts) -> Layout { width, height, ops: Vec<DrawOp> }`.
Coordinates are in staff spaces (y down); colors are `ColorToken`s that a `Theme` (light,
dark) maps to RGB. Glyphs are referred to by SMuFL name (`Glyph`); the renderer resolves the
code point. Engraving values (line thicknesses, glyph boxes and advances) come from the
bundled Bravura metadata, read once with `MusicFont::bravura()`.

The steps:

Tracks with a tuning get standard notation above the tab (`notation`: spelling and clef,
chords and accidentals per measure, noteheads/stems/flags/dots/rests, beams and ties),
sharing the tab's columns so a note and its fret number line up.

1. `header`: title block, credits, tuning, tempo mark.
2. `measure`: each measure on its own. Beats of all voices are merged into columns by
   start time; each column gets space by the length until the next column (a quarter gets
   `Spacing::quarter`, each doubling multiplies by `doubling_ratio`), but never so little
   that fret numbers collide. A `MeasureLayout` does not depend on its position, so it is
   the unit to cache per measure and document revision for incremental relayout (still
   open: the app lays out the whole track again after an edit).
3. `system`: greedy line breaking; every line but the last is justified by stretching the
   columns (not the clef, time signatures or leads); the last only if it is at least 75%
   full.
4. Drawing: staff lines first, then clef, measure content (masks behind fret numbers so the
   staff line does not run through them) and barlines.

`svg::to_svg` writes the operations as SVG for golden tests and `gscore-cli render`; the
app paints the same operations with egui. Besides the operations, a `Layout` carries
`Places`: where each system, measure and beat column is. The app draws the cursor and
turns clicks into beats and strings with it, without looking into the draw operations.

## Interactive editing (`gscore-edit`)

The M4 command layer (`Command`, `Editor` with undo/redo) is driven from the keyboard by:

- `Cursor`: track, measure, voice, beat and string; plain data, moves only read the song,
  `clamp` pulls it back into a song that changed.
- `ActionId`/`Action`: what a key asks for at the cursor (`cursor_left`, `enter_fret`,
  `duration_shorter`, `undo`, `play_pause`, ...); the ids are the keymap's.
- `Keymap`: typed characters and key names (with modifiers) to action ids, parsed from a
  text file; `DEFAULT_KEYMAP` is the built-in file. Characters ignore Shift unless Ctrl or
  Alt is held, so a binding works on every layout.
- `TabEditor`: owns the `Editor` and the cursor, turns actions into commands, remembers
  the cursor of each edit for undo/redo, and joins two quick fret digits into one undo step.
  Time comes in as milliseconds from the caller (no clock in core crates). Playback and
  saving come back as `Outcome::Request` for the app.

## The app (`gscore-app`)

A thin eframe/egui shell (wgpu renderer) that holds state and delegates:

- `state`: `ViewState` (theme, zoom, track): plain data, no widgets, unit-tested.
- `tasks`: the file dialogs, opening and saving run on background threads and report
  through a channel polled once per frame (AGENTS.md rule 9); a panicking job still reports
  back.
- `document`: the `TabEditor`, the saved revision (for "changed") and the layouts, cached
  per track and editor revision; layout runs inside `catch_unwind`.
- `keys`: egui's key and text events to keymap combos (characters from text events, named
  keys and Ctrl combinations from key events, clipboard events back to Ctrl+C/X/V, nothing
  while an input method composes).
- `canvas`: paints `DrawOp`s (staff spaces times zoom), culling what is off screen; text
  and glyphs are placed by their baseline as layout specifies.
- `fonts`: Noto Sans (also the UI font), Noto Sans Bold and Bravura, compiled in.
- `app`: `bars` (menus, toolbar, status bar, notices), `page` (page view, cursor, clicks),
  `editing` (keys to actions, before any widget sees them), `files` (open, save, the
  unsaved-changes and lossy-save questions); drawing the page is wrapped in
  `catch_unwind` so a bug shows a notice instead of closing the window.

The keymap file is found and read by `gscore_session::load_user_keymap` (config
directory, size cap, fallback to the built-in keymap with the problem described).

Opening a file (`gscore_session::open`: capped read, then import) is shared with the CLI.
