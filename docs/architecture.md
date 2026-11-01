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

1. `header`: title block, credits, tuning, tempo mark.
2. `measure`: each measure on its own. Beats of all voices are merged into columns by
   start time; each column gets space by the length until the next column (a quarter gets
   `Spacing::quarter`, each doubling multiplies by `doubling_ratio`), but never so little
   that fret numbers collide. A `MeasureLayout` does not depend on its position, so it is
   the unit to cache per measure and document revision when editing needs incremental
   relayout (M8).
3. `system`: greedy line breaking; every line but the last is justified by stretching the
   columns (not the clef, time signatures or leads); the last only if it is at least 75%
   full.
4. Drawing: staff lines first, then clef, measure content (masks behind fret numbers so the
   staff line does not run through them) and barlines.

`svg::to_svg` writes the operations as SVG for golden tests and `gscore-cli render`; the
app paints the same operations with egui.

## The app (`gscore-app`)

A thin eframe/egui shell (wgpu renderer) that holds state and delegates:

- `state`: `ViewState` (theme, zoom, track): plain data, no widgets, unit-tested.
- `tasks`: the file dialog and opening files run on background threads and report through
  a channel polled once per frame (AGENTS.md rule 9); a panicking job still reports back.
- `document`: the song plus its layouts, cached per track; layout runs inside
  `catch_unwind`.
- `canvas`: paints `DrawOp`s (staff spaces times zoom), culling what is off screen; text
  and glyphs are placed by their baseline as layout specifies.
- `fonts`: Noto Sans (also the UI font), Noto Sans Bold and Bravura, compiled in.
- `app`: menus, toolbar, page view, status bar, notices; drawing the page is wrapped in
  `catch_unwind` so a bug shows a notice instead of closing the window.

Opening a file (`gscore_session::open`: capped read, then import) is shared with the CLI.
