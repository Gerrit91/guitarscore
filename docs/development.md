# Development Guide

This document provides guidelines for contributing to the GuitarScore project.

## Building

`rust-toolchain.toml` pins the Rust version and installs `rustfmt`, `clippy` and the
`wasm32-unknown-unknown` target. With a distribution's Rust instead, install the wasm
target yourself (Arch: `pacman -S rust-wasm`). Building the Guitar Pro reader for wasm also
needs `clang`.

```bash
cargo build
cargo xtask ci   # everything a change must pass: fmt, clippy, tests, layer and wasm checks
cargo gui testdata/basic/note_lengths.gp      # build and start the app (a cargo alias)
gscore-cli gui testdata/basic/note_lengths.gp # start the built app from the command line
cargo xtask web                                # the browser build in dist/web (docs/web.md)
cargo xtask web --smoke                        # also check it in headless Chromium
cargo xtask web --soundfont PATH               # ship another .sf2; --no-soundfont for silence
cargo xtask tones list                         # the saved modelled-guitar tones and their programs
cargo xtask tones import --preset NAME --program N  # copy one into the built-in library
cargo xtask tones programs                     # the programs, their built-in tones and where those came from
cargo xtask tones update                       # copy changed saved tones into the built-in library again
cargo xtask tones rse ~/tabs                   # the RSE sounds Guitar Pro files use, and the tone each starts on
cargo xtask tones rse --map "Distortion Guitar/Lead" --tone "Heavy Lead"   # start that RSE sound on a built-in tone
cargo xtask tones normalize                    # level the built-in tones with each other
```

The app renders with wgpu (Vulkan, Metal or DirectX 12). Its snapshot tests render
offscreen, so a headless machine needs a Vulkan adapter; without a GPU, Mesa's lavapipe
does it (`mesa-vulkan-drivers` on Debian/Ubuntu, `vulkan-swrast` on Arch, plus
`libvulkan1`/`vulkan-icd-loader`).

## Workspace Structure

The workspace is layered; a crate may only depend on crates in lower layers
(`cargo xtask layers` enforces this, using the table in `xtask/src/layers.rs`). What each
crate does is in `docs/architecture.md`:

0. `gscore-version`, `gscore-gp`: build version; Guitar Pro song data and reading/writing
1. `gscore-model`: document model wrapping the Guitar Pro song
2. `gscore-io`, `gscore-layout`, `gscore-play`: file entry point, layout, sequencer
3. `gscore-edit`, `gscore-audio`: editing commands, audio output
4. `gscore-session`, `gscore-worklet`: files, recovery and settings; the browser's synth
5. `gscore-cli`, `gscore-app`: command-line tool, desktop and browser application

## Coding Standards

- Follow Rust idioms and best practices
- Prefer small, focused crates with well-defined APIs
- Avoid "big ball of mud" architecture
- The core crates (`gscore-gp` up to `gscore-edit`), `gscore-audio` and `gscore-worklet` must compile for `wasm32-unknown-unknown`
- Never panic in non-test code

## Testing Requirements

All changes must pass `cargo xtask ci`, which runs:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo xtask layers`

Additionally, specific tests apply based on what's modified.

Changes to the model, editing, layout or the app's frame loop also run `cargo xtask bench`
(release build) on large files, before and after; it fails when a budget is missed: from a
key press to the new layout within 16 ms, opening within 2 s. Point it at the owner's files
with `GSCORE_BENCH_FILES=a.gp:b.gp` (they are not in the repository) and note the numbers
in the devlog.

## Developer tools

- `gscore-cli gui [FILE]` starts the desktop application (from the directory of the
  `gscore-cli` executable, so `target/<profile>/guitarscore` after a workspace build, or
  from `PATH`) and waits for it to exit. Build both with
  `cargo build -p gscore-cli -p gscore-app`; `cargo gui <FILE>` builds and starts a
  release build.
- `gscore-cli roundtrip-check <file> [--to gp|gpx|gp5|gp4|gp3]` (hidden from `--help`):
  writes the song, reads it back and lists what changed. Use it whenever reading or
  writing code changes.
- `GSCORE_UPDATE_GOLDEN=1 cargo test -p gscore-cli --test test_golden` regenerates the
  expected `info`/`dump` outputs in `crates/gscore-cli/tests/golden/` (review the diff).
- `gscore-cli commands` lists the editing commands; `gscore-cli edit ... --script <file>`
  applies a script of them. `crates/gscore-cli/tests/scripts/` holds scripts that are also
  regression tests (one edits a test file, one undoes everything back to the input).
- `gscore-cli render <file> -o out.svg [--track N] [--all-tracks] [--theme light|dark]`
  draws the score as SVG, or as PDF pages when the output ends in `.pdf`. `GSCORE_UPDATE_GOLDEN=1 cargo test -p gscore-layout --test test_golden`
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

### Hearing and checking playback

`gscore-cli play <file> --port "Midi Through"` plays silently through ALSA's through port;
`aseqdump -p 14:0` in another terminal shows the messages. `crates/gscore-audio/tests/`
does this automatically (and skips where there is no such port). `gscore-play`'s tests
compare the sequenced notes with Guitar Pro's own MIDI export in `testdata/basic/*.mid`.

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