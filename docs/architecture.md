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
    gscore-model/             # L0: document model (wrapper over the Guitar Pro song data in `gp`), positions, queries
    gscore-io/                # L1: GP files <-> model, reader/writer in `gp` (bytes in, bytes out; no file system)
    gscore-layout/            # L1: model -> positioned draw ops. No GUI imports.
    gscore-play/              # L1: sequencer, MIDI events, channel allocator, Player trait. No device crates.
    gscore-edit/              # L2: cursor, selection, commands, undo/redo, keymap
    gscore-audio/             # L2: output backends (midir, cpal, rustysynth) implementing Player
    gscore-session/           # L3: open document, recovery journal, autosave, safe-save orchestration
    gscore-cli/               # L4: headless tool (info, dump, render, convert, roundtrip-check)
    gscore-app/               # L4: egui/eframe application shell (thin); produces the `guitarscore` binary
```

## Dependency Restrictions

- Only `gscore-app` may use egui, eframe, winit, or rfd
- Only `gscore-audio` may use `cpal`, `midir`, or `rustysynth`
- Nothing below `gscore-app` may use a GUI crate
