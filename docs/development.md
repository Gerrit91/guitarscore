# Development Guide

This document provides guidelines for contributing to the GuitarScore project.

## Workspace Structure

The workspace is layered; a crate may only depend on crates in lower layers
(`cargo xtask layers` enforces this, using the table in `xtask/src/layers.rs`):

0. `gscore-gp`: Guitar Pro song data and reading/writing (derived from guitarpro)
1. `gscore-model`: document model wrapping the Guitar Pro song: views, positions, checks
2. `gscore-io`, `gscore-layout`, `gscore-play`: file entry point, layout, playback
3. `gscore-edit`, `gscore-audio`: editing commands, audio output
4. `gscore-session`: open document, crash reports, recovery, autosave
5. `gscore-cli`, `gscore-app`: command-line tool, desktop application

`gscore-play` and `gscore-audio` are empty until their milestone (AGENTS.md section 9); the
other crates contain code.

## Coding Standards

- Follow Rust idioms and best practices
- Prefer small, focused crates with well-defined APIs
- Avoid "big ball of mud" architecture
- The core crates (`gscore-gp` up to `gscore-edit`) must compile for `wasm32-unknown-unknown`
- Never panic in non-test code

## Testing Requirements

All changes must pass `cargo xtask ci`, which runs:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo xtask layers`

Additionally, specific tests apply based on what's modified.

## Developer tools

- `gscore-cli roundtrip-check <file> [--to gp|gpx|gp5|gp4|gp3]` (hidden from `--help`):
  writes the song, reads it back and lists what changed. Use it whenever reading or
  writing code changes.
- `GSCORE_UPDATE_GOLDEN=1 cargo test -p gscore-cli --test test_golden` regenerates the
  expected `info`/`dump` outputs in `crates/gscore-cli/tests/golden/` (review the diff).
- `gscore-cli commands` lists the editing commands; `gscore-cli edit ... --script <file>`
  applies a script of them. `crates/gscore-cli/tests/scripts/` holds scripts that are also
  regression tests (one edits a test file, one undoes everything back to the input).
- `gscore-cli render <file> -o out.svg [--track N] [--theme light|dark]` draws a track's
  tablature. `GSCORE_UPDATE_GOLDEN=1 cargo test -p gscore-layout --test test_golden`
  regenerates the SVG references in `crates/gscore-layout/tests/golden/` (the owner reviews
  visual changes before they are committed).

### Looking at the app without a window

`cargo run -p gscore-app --example snapshot -- <file> <out.png> [--dark] [--track N]
[--zoom Z] [--size WxH]` renders the whole app offscreen (wgpu, no window, no focus
stealing) and saves a PNG. `crates/gscore-app/tests/test_app.rs` drives the app through its
accessibility tree (clicking menus and buttons as a user would) and compares two
snapshots with `crates/gscore-app/tests/snapshots/`; regenerate them with
`UPDATE_SNAPSHOTS=1 cargo test -p gscore-app --test test_app` and have the owner review the
images.

### Looking at rendered SVG

The SVG names its fonts (Bravura, Noto Sans) instead of embedding them. To view it without
installing the fonts system-wide, point fontconfig at `assets/fonts/` and rasterize with
`rsvg-convert` (librsvg):

```bash
cat > /tmp/gscore-fonts.conf <<CONF
<?xml version="1.0"?><!DOCTYPE fontconfig SYSTEM "fonts.dtd">
<fontconfig><dir>$PWD/assets/fonts</dir><include>/etc/fonts/fonts.conf</include></fontconfig>
CONF
cargo run -p gscore-cli -- render testdata/basic/note_lengths.gp -o /tmp/note_lengths.svg
FONTCONFIG_FILE=/tmp/gscore-fonts.conf rsvg-convert -w 1414 /tmp/note_lengths.svg -o /tmp/note_lengths.png
```

Compare the PNG with Guitar Pro's export next to the test file (`testdata/basic/*.png`).