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
    gscore-cli/               # L5: headless tool (info, dump, render, convert, roundtrip-check)
    gscore-app/               # L5: egui/eframe application shell (thin); produces the `guitarscore` binary
```

## Dependency Restrictions

- Only `gscore-app` may use egui, eframe, winit, or rfd
- Only `gscore-audio` may use `cpal`, `midir`, or `rustysynth`
- Nothing below `gscore-app` may use a GUI crate
