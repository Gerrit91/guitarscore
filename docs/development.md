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

Only `gscore-gp`, `gscore-model`, `gscore-io`, `gscore-session` and `gscore-cli` contain code
so far; the other crates are empty until their milestone (AGENTS.md section 9).

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