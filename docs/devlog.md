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
- Panic hook / crash log not set up yet (AGENTS.md "Containment").

## 2026-11-01 — wasm check verified

- Installed `rust-wasm`; `cargo xtask layers` now runs the wasm check of all five core crates
  and passes. `gscore-io` passes too, contrary to the expected `zstd-sys` failure: clang
  compiles the C code for wasm. Building it therefore needs clang (present on GitHub's
  ubuntu-latest runners).
- Vendoring `guitarpro` stays a long-term goal (docs/gaps.md #1).

## 2026-11-01 — Crash reports; M0 complete

- `gscore-session`: `install_panic_hook(dir)` writes one report per panic (version, time,
  thread, location, message, backtrace) atomically (temp file + rename), then runs the
  previous hook. `default_crash_dir()`: XDG state dir on Linux, `~/Library/Logs` on macOS,
  `%LOCALAPPDATA%` on Windows, no extra dependency. Installed by `gscore-cli`; `gscore-app`
  installs it once it has a `main` (M4).
- `panic = "unwind"` spelled out in the dev and release profiles.
- Tests: 12 (+2: hook writes a report from a panicking thread; crash dir path).
- `cargo xtask ci` green, wasm check included.

M0 done. Open questions for M1, from the end-of-milestone architecture review:
- `gscore-model` is a separate model, while AGENTS.md's default plan is to wrap the
  `guitarpro` model directly so saving cannot drop fields. Decide before writing M1.
- Durations are `f32` (`Beat::duration`, `Note::duration`); rational durations are needed
  for exact measure-fill invariants.
- All model fields are `pub`, so invariants cannot be enforced by the type.

## 2026-11-01 — M1: gscore-model wraps the guitarpro song

Decision (owner): wrap `guitarpro::Song` (the "legacy" model, which every GP format reads
into) instead of converting to an own model, so saving cannot drop unmapped fields. Vendoring
`guitarpro` stays a long-term goal.

- `Song { inner: guitarpro::Song }`, private. Read-only views `TrackView`, `MeasureView`,
  `VoiceView`, `BeatView`, `NoteView`; value types `Duration` (exact reduced fraction of a
  whole note, dots and tuplets), `TimeSignature`, `StringNumber`, `Fret`, `Pitch`; index
  types `TrackIndex` ... `BeatIndex`, `BeatPosition`.
- Invariants checked on creation (`interop::from_guitarpro`): positive tempo, 1..=12 strings
  numbered 1..=n, MIDI-range pitches and frets, notes only on existing strings and at most one
  per string, note values powers of two up to 1/128, tuplets 1..=64, valid time signatures,
  every track has the song's measure count. Errors name the location
  ("track 1: measure 1: voice 1, beat 1: ...").
- `Song::issues()`: under- and overfilled voices (placeholder beats count as zero).
- `cargo xtask layers`: only gscore-model and gscore-io may depend on `guitarpro`; only
  gscore-io may use `gscore_model::interop`. Both verified by injecting violations.
- gscore-io no longer converts (the lossy note mapping is gone); `IoError::InvalidSong`.
  `gscore-cli info` prints tuning per track and the number of issues.
- Tests: 30 (model 17, xtask 7, io 2, cli 2, session 2). Tested instruments: 4-string
  bass, 6-, 7- and 8-string guitars; time signatures 4/4, 3/4, 7/8, 2/4 with triplets.

Open: edit methods (M5), more accessors (tempo changes, repeats, effects) as layout and
playback need them.

## 2026-11-01 — CLI help, all GP formats, note problems become issues

- `gscore-cli`: `--help`/`-h`/`help`, `help <command>`/`<command> --help`, `--version`;
  commands come from one table (`COMMANDS`), so new ones appear in the help automatically.
  Exit codes: 0 ok, 1 command failed, 2 wrong usage. `info` shows the format and lists the
  first 10 issues.
- `gscore-io`: format detected from the content (zip = GP7+, BCFZ/BCFS = GPX,
  "FICHIER GUITAR PRO v3/4/5" = GP3/4/5), not the extension; import returns
  `Imported { song, format }`; inputs over 64 MiB are refused.
- Owner's 26 `.gp` files: before, all failed (GP5 parser on zip data); then 21 failed the
  new validation. Note-level problems (several notes on one string, notes on missing
  strings, frets outside 0..=127) are now `Song::issues()` instead of load errors: they come
  from drum and piano tracks mapped onto tab strings. Now all 26 open; 4 have no issues.
- Found: guitarpro ignores GP7 tunings (see docs/gaps.md #1).
- Tests: 39.

## 2026-11-01 — guitarpro vendored; GP7 tunings fixed

- `vendor/guitarpro/`: copy of upstream commit `b62be7c` (library only; upstream tests that
  need unvendored files removed), workspace member with all lints allowed. Provenance and
  every change in `vendor/guitarpro/VENDORED.md`; row in `THIRD_PARTY.md`. Same dependency
  versions as before (lockfile only lost the git source).
- Fixed: GP6/GP7 import ignored the file's tuning (every track E standard) and numbered
  strings in reverse; export wrote them reversed. Regression tests fail before the fix.
  Owner's files: 238 tracks went from 1 tuning to 13 distinct ones (4-, 5- and 6-string).
- `cargo xtask layers`: vendored crates have no layer, are exempt from the workspace-lints
  check, and must not depend on GuitarScore crates.
- Tests: 39 own + 161 in the vendored crate.

## 2026-11-01 — gscore-cli on clap

- Owner approved clap 4.6.7 (derive) for `gscore-cli` only. Subcommands are a typed enum
  whose doc comments are the help; `try_parse_from` keeps parsing testable and never exits
  the process. Removed the hand-written parser, command table and help module.
- Gains: typo suggestions, standard `help <command>`, `-h` short / `--help` long help.
  Exit codes unchanged (0 ok, 1 failed, 2 usage). `Cli::command().debug_assert()` test.
