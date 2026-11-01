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

`gscore-gp`, `gscore-model`, `gscore-io`, `gscore-edit`, `gscore-session` and `gscore-cli`
contain code so far; `gscore-layout`, `gscore-play`, `gscore-audio` and `gscore-app` are
empty until their milestone (AGENTS.md section 9).

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