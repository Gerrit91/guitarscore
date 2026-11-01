# Development Log

Terse entries, newest last: what landed, numbers, what is still open.

## 2026-11-01 — Status reset; M0 foundation repaired (M0 in progress)

Earlier entries claimed M0–M2 complete and M3 under way. They were removed: the tree did not
pass its own checks (fmt failed, clippy failed on an invalid `clippy.toml`, `cargo xtask` did
not exist, the only test was trivial) and most modules were 2-line stubs.

Landed:
- Single workspace (removed the stray second workspace in `crates/Cargo.toml`); `xtask` is a
  member, `cargo xtask` alias in `.cargo/config.toml`.
- Lints: valid `clippy.toml` (panic-family lints allowed in tests only);
  `arithmetic_side_effects` is now per crate (`gscore-io`, `gscore-layout`, `gscore-play`)
  instead of an unknown rustc lint applied everywhere.
- Toolchain pinned to 1.99.1 (`rust-toolchain.toml`, also CI); `guitarpro` pinned to rev
  `b62be7c`.
- `cargo xtask layers`: layer table, restricted GUI/audio crates, `[lints] workspace = true`
  in every crate, no `std::fs`/`std::thread`/`Instant`/`SystemTime` in core crates, wasm
  check of the core crates. `cargo xtask ci` runs the whole definition of done.
- `gscore-io` is bytes-only (file reading moved to `gscore-cli`); import rejects out-of-range
  fret/string values instead of truncating them; export returns "unsupported" instead of
  writing a file with only the title (data loss).
- `gscore-cli` no longer indexes `args`/`measures` or calls `process::exit` from the library.
- Removed unused inter-crate dependencies from stub crates and `gscore-app -> gscore-cli`
  (same layer).
- Tests: 10 (xtask 6, io 2, cli 1, model 1).

Open (M0):
- wasm check not yet run: the local Rust (Arch package) lacks the `wasm32-unknown-unknown`
  target. Likely failure: `guitarpro` pulls `zip` 9 with default features, including
  `zstd-sys` (C code). See `docs/gaps.md`.
- Panic hook / crash log not set up yet (AGENTS.md "Containment").
