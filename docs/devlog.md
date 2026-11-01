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

## 2026-11-01 — Instruments and tunings

- Tunings print as note names (scientific pitch notation, sharps), lowest string first.
- Vendored guitarpro: reads GP7 `<InstrumentSet>` (type id and name) into the track; marks
  `drumKit` tracks as percussion (before: only via MIDI channel 10, which GP7 does not
  store there); `Track::has_tuning` is false when the file has no tuning, and the exporter
  then writes none.
- Model: `TrackView::instrument_kind()` (`Strings`, `Drums`, `Other`, from the last camelCase
  word of the type id), `instrument_name()`; `tuning()` returns `Option` and is `None` for
  non-string instruments (GP7 stores E standard on vocal and piano tracks), tracks without a
  stored tuning, and all-zero tunings.
- Owner's files: instrument types found: electricGuitar 126, drumKit 48, voice 28,
  electricBass 21, padSynthesizer 7, electricPiano 4, trumpet 2, acousticPiano 1,
  electricOrgan 1.

## 2026-11-01 — Song information, tuning names, first testdata tests

- `info` shows every song field (subtitle, album, lyrics by, music by, copyright, tab by,
  instructions, notes), aligned, `(not set)` when empty; model accessors on `Song`.
- Tuning names: the file's tuning label if set, else recognized from the interval pattern
  (standard incl. basses and 7/8-string, drop, double drop, open D/E/G/A/C, DADGAD family).
  Owner's band files: 134 tracks in Drop A, plus D#, C#, E, B standard and Drop B.
- Vendored guitarpro: reads tuning `<Label>`; GP7 metadata now uses the GP3-5 fields
  (tab author was stored as transcriber, composer overwrote the tab author, instructions
  went to comments; `WordsAndMusic` read).
- Found by the owner's test file: GPIF note strings count from the lowest string; upstream
  kept that order, which only matched its reversed tunings. After the tuning fix the notes
  sat on mirrored strings. Now converted on import and export (`flip_note_strings`).
  The new note test fails without this fix (checked).
- Tests over `testdata/`: song information, track/instrument/tuning, notes (pitch from string
  and fret), all `testdata/corrupt/` files rejected, ~1,500 truncated and byte-flipped
  copies of the test file never panic (also not internally). `info` reference file
  `testdata/basic/steel_guitar_dadgad.info.txt` (regenerate with GSCORE_UPDATE_GOLDEN=1).
- Tests: 51 own + 166 vendored.

## 2026-11-01 — Test file in all formats; parser hardening

- Owner exported the DADGAD file as GP7, GPX, GP5, ASCII tab, MIDI, PDF. Import tests now
  run over GP7/GPX/GP5 (same music; GP5 lacks notes, GP5/GPX lack instrument names). This
  verified the GP6 tuning order. `info` references moved to
  `crates/gscore-cli/tests/golden/` (one per format); `testdata/` holds only owner files,
  described in `testdata/basic/README.md`.
- Robustness test over damaged copies of all three files (4,357 inputs) found caught
  panics: GPX block table read past the end, GP5 direction pointing to a missing measure.
  Both fixed in the vendored crate; now zero internal panics.
- Memory limits in the vendored crate (128 MiB unpacked): GP7 zip entries (zip bombs),
  BCFZ declared size (up to 2 GB before), BCFS file sizes (negative became huge). Failed
  allocations abort the process and cannot be caught. `read_gpx` accepts plain BCFS.
- Tests: 53 own + 170 vendored.

## 2026-11-01 — guitarpro integrated into gscore-model and gscore-io

- Owner decision: stop treating guitarpro as vendored third-party code. Removed the unused
  modules (~21k lines), then split the rest (~10.4k lines, by a scanner script): data types
  to `gscore_model::gp` (no dependencies), reading/writing to `gscore_io::gp` (private).
  Inherent methods needing the reader's error type became extension traits; `Song`'s
  read/write methods are the `SongCodec` trait. `vendor/` is gone.
- `cargo xtask layers`: `gscore_model::gp` may only be used by gscore-io (like `interop`);
  the vendored-crate mechanism and the `guitarpro` dependency rule were removed.
- Workspace moved to edition 2024 (the derived code uses let-chains); four of our `if`s
  became let-chains on clippy's advice.
- Credits: README acknowledgements, `LICENSE-guitarpro` in both `gp` directories,
  `THIRD_PARTY.md`, history and change list in `docs/guitarpro.md`.
- Tests: 78 (the derived code's 22 tests now live in gscore-io and gscore-model). All
  owner files still import.

## 2026-11-01 — gscore-gp crate

- Owner decision: the Guitar Pro data and its reading/writing belong together, so the split
  from the previous entry became one crate, `gscore-gp` (layer 0): `src/model` (data, public)
  and `src/io` (reading/writing, private; entry points `Song::read_*`/`write*`). The
  extension traits are inherent methods again. gscore-model wraps its `Song` as before;
  gscore-io keeps format detection, limits and `catch_unwind`. Layers shift up by one.
- xtask: only gscore-model and gscore-io may depend on gscore-gp; gscore-gp is in the wasm
  check; the lint exemption is confined to that crate.
- AGENTS.md (architecture, decision 1 marked decided, rule 7, M2 note), README.md
  (rewritten: status, building, CLI, layout, credits), CLAUDE.md, architecture.md,
  THIRD_PARTY.md and docs/guitarpro.md updated.
- Tests: 78, unchanged in number (the derived code's 22 now run in gscore-gp).

## 2026-11-01 — Cleanup of leftovers

- Removed scaffold placeholders: `*Placeholder` structs and empty stub modules in
  gscore-play, -edit, -audio, -app, -layout and -session (the crates keep a doc comment on
  their future job; gscore-session keeps the crash reports); gscore-layout's unused
  dependency on gscore-model.
- gscore-io: removed the unused export stub and the `IoError` variants only it or nothing
  used (`Unsupported`, `ParseError`, `WriteError`).
- gscore-gp: removed 11 unused underscore-prefixed items, 3 unused trait methods, a
  test-only wrapper, and about 120 lines of commented-out code (incl. debug `println!`s).
- Removed stray files from the initial scaffolding: `test_gscore_io.rs` and `src/main.rs`
  in the repo root (never compiled), and the stale `crates/gscore-cli/README.md`.
  `docs/development.md` shows the current layers.
- Tests: 77 (the export stub's test went with it).

## 2026-11-01 — Milestones reordered

Owner decision: headless editing and saving before the UI. New order: M2 import + `info`,
`dump`, `roundtrip-check` (current); M3 save; M4 headless editing (command registry,
`gscore-cli commands` / `edit -c ... --script ...`); M5 layout; M6 read-only app view; M7
interactive editing in the app (cursor, keymap, recovery journal); M8 MIDI playback; M9
internal synth. `cargo-fuzz` deferred to "Later". Milestone references in crate docs,
gaps, parity, testdata README and CLAUDE.md updated.

## 2026-11-01 — `gscore-cli dump` (M2)

- ASCII tablature: one row per string (highest first, labelled with note names), beats as
  wide as their length (whole note = 24 columns, rounded up, at least text + 1), two
  dashes at each barline, `||` at the end, all voices merged on one exact time grid, dead
  notes `x`, ties `(5)`; wraps at measure boundaries (`--width`, default 80); `--track N`;
  non-string tracks are listed without tablature.
- For the DADGAD test file the grid is identical to Guitar Pro's own ASCII export, in all
  three formats (test compares against `testdata/basic/steel_guitar_dadgad.txt`).
- `Duration::checked_sub`; shared input reading (`gscore-cli/src/input.rs`); golden tests
  generalized to `tests/test_golden.rs` (`<file>.<command>.txt`).
- All 26 owner band files dump without errors.
- Tests: 85. M2 still open: `roundtrip-check`.

## 2026-11-01 — `roundtrip-check`; M2 complete

- `gscore_io::export_to_bytes(song, format)` (GP7, GPX, GP5 5.10, GP4 4.06, GP3 3.00) behind
  `catch_unwind`; import and export share `contain()`. Bytes only; safe-save is M3.
- `gscore_model::differences(a, b)`: song information, tracks (name, instrument, strings,
  tuning, tuning name), time signatures, and every beat (kind, length, notes); placeholder
  beats and empty voices ignored. M3 will reuse it to verify saved files.
- `gscore-cli roundtrip-check <file> [--to ...]`, hidden from `--help` (owner's wish);
  exit 1 with the list of differences.
- Findings (docs/gaps.md #2): GP7/GPX writer loses instruments and tuning labels (all 26
  band files differ); GP5 writer panics on every GPIF-sourced song (contained); GP5 ->
  GPIF adds a beat to the empty second voice; one band file gives unreadable GPX. Test
  file: GPX -> GPX and GP5 -> GP5 are clean.
- The panic hook now says "internal error, report written to ..." (it also runs for
  contained panics, so "crash report" was misleading).
- Tests: 90 (+3 ignored known gaps).

M2 (import, `info`, `dump`, `roundtrip-check`, corrupt-file and damaged-input tests) is
complete.

## 2026-11-01 — M3: writer fixes (first step)

Test-first with `roundtrip-check` and the formerly ignored tests:
- GP3-5 writer: lyric lines (panic), 64-entry MIDI channel table generated for songs from
  GP6/GP7 (panic or unreadable file; drums on channel 10), equalizer value counts (shifted
  the rest of the file).
- GPIF writer: placeholder-only voices stay empty; `<InstrumentSet>` and the tuning label
  are written (drum tracks from GP3-5 as `drumKit`).
- GPX container: files too large for the block table are refused instead of corrupted.
- Results on the 26 band files: GP7 -> GP7 all clean (was 0); -> GPX 25 clean, 1 too large;
  -> GP5 still failing (deferred, docs/gaps.md #2). Test file: all formats and the
  conversions between GP7, GPX and GP5 clean, except the expected loss of the instrument
  name in GP7 -> GP5.
- Tests: no ignored tests left.

## 2026-11-01 — testdata: note lengths and ornaments

- Added `testdata/basic/note_lengths.*` and `ornaments.*` (GP7, GPX, GP5 plus Guitar
  Pro's `.txt`, `.mid`, `.pdf` and `.png` exports; the README describes the music).
- `test_golden` now covers all nine files with `info` and `dump` references, 12 new
  golden files. `ornaments` must dump the same note grid in all three formats.
- A test pins the GP5 double-dot limitation: Guitar Pro 5 writes the 7/16 note (a
  double-dotted quarter) as 3/8, so measure 9 of `note_lengths.gp5` is 15/16 and `info`
  reports the issue. GP7 and GPX keep it. The future GP5 writer should drop the second
  dot and warn instead of failing (gaps #2).
- Checked both dumps against the PDF and Guitar Pro's tab export: durations, 3/5/6/7
  tuplets, dots, ties, dead notes and string assignment all match. Effects still do not
  appear by design (gaps #4).
- Found while comparing the exports: the GP6/GP7 reader ignores `<Whammy>`, so the dive
  bomb in `ornaments` measure 9 is lost on import (GP5 keeps it); recorded in gaps #4.
  GP5 also stores hammer-on flags only on the origin note, while GPIF has origin and
  destination; the model keeps one `hammer` flag.

## 2026-11-01 — dump: note and beat effect symbols

- `gscore-model` views now expose note effects (hammer, bend, ghost, accent, palm mute,
  staccato, trill, tremolo picking, vibrato, slide) and beat effects (pick/brush stroke,
  tapping/slap/pop, fade-in, whammy bar), so `gscore-cli dump` can print Guitar Pro's
  ASCII symbols without leaking `gscore-gp`.
- `dump` writes `L` on the origin of a tie (the continuation stays a plain fret), `h` on
  the first note of each hammer/pull pair, and `b`, `s`, `~`, `g`, `>`, `t`, `M`, `.`,
  `+`, `S`, `P`, `<`, `w`, `=`, `v`/`^`, `V`/`n` after the fret. Harmonics, let ring,
  grace notes and fingerings have no symbol in Guitar Pro's export.
- Reader fixes in `gscore-gp`: `HopoDestination` no longer sets `hammer` (GP7/GPX had two
  `h` per pair, GP5 one), and the whammy bar is read from GP7 `<Whammy>` and GP6
  `WhammyBar*` beat properties, so the dive bomb in `ornaments` survives and GP7, GPX and
  GP5 dump the same effects. Regression tests in `effect_tests`; docs/guitarpro.md 17-18.
- `ornaments` goldens now contain the effects. A test asserts the two `h`, the single
  `L` and every reference cell (`15b`, `15s`, `0g`, `0>`, `0.`, `2M`, `15w`). The
  note-grid comparison ignores symbols because GP5 also sets the accent bit for heavy
  accents and staccato. Testdata README and gaps #4 updated.

## 2026-11-01 — dump: Wikipedia ASCII-tab symbols, palm mutes under the staff

- The dump no longer copies Guitar Pro's ASCII legend. Note effects now use the usual
  ASCII-tab symbols from Wikipedia's table: `h`/`p` (direction from the next note's pitch
  on the same string, since Guitar Pro stores one hammer flag), `b` up / `r` release, `/`
  up / `\` down for slides (falling back to `S`/`s` when the target pitch is unknown),
  `~`, `tr`, `TP`, `t` for tapping, `*` for harmonics and `(fret)` for ghost notes.
  Concepts the table does not cover keep their letters: slap `S`, pop `P`, fade-in `<`,
  whammy `w`, brush `v`/`^`, pick stroke `V`/`n`; the tie `L` stays on the origin. Wah is
  not drawn at all.
- Palm mutes moved from a suffix (`2M`) to a row under the staff, aligned with the notes:
  `PM` starts a run, `-` carries it and `|` closes it (`PM------|`). A system without palm
  mutes gets no extra row. The row uses spaces where the staff has padding, so `PM` sits
  above the first muted fret.
- `gscore-model` exposes `NoteView::bend()`, `slides()` and `is_harmonic()`, plus
  `BendKind` (up/release) and `SlideKind` (in/out directions and shift/legato), so the
  dump can choose symbols without seeing `gscore-gp` types.
- Goldens for `ornaments` (GP7, GPX, GP5) regenerated; `test_golden` expects `0h`, `2p`,
  `(0)`, `15\`, `12/`, `7*`, `7~`, `15w`, asserts no `M` in the tab lines, and checks the
  `PM...|` row is aligned with the first muted fret. The DADGAD cross-format grid and the
  Guitar Pro layout comparison still pass. Testdata README and gaps #4 updated.
- Still open: hammer/pull and slide targets in the next measure fall back (`h`, `S`/`s`),
  and ties are matched within one measure only; both need a cross-measure pass.

## 2026-11-01 — dump: bend targets, harmonic nodes, let ring, legend

- Bends now name the target fret: the last bend point is the distance in semitones, so a
  whole-step bend on fret 15 prints `15b17` (odd semitones print `.5`). Release bends keep
  `r` and get a target only when the pitch moves. The `ornaments` bend now reads `15b17`.
- Harmonics use the `Harmonic`/`HarmonicKind` view: natural harmonics stay `*`, pinch and
  artificial harmonics print the node after the fret (`7[19]`) and tapped harmonics print
  `7(19)`. The node is the fretted note plus the stored harmonic offset; a file without
  one (GP5 pinch) prints `[?]`. Fixed a GPIF reader bug: Guitar Pro writes `HarmonicFret`
  before `HarmonicType`, so the fret used to be dropped; a regression test covers it.
- Let ring joins palm mute on rows under the staff, so `PalmMute`/`LetRing` share one
  annotation-row implementation (`PM----|`, `LR----|`). The rows are independent and
  stacked in `Annotation::ALL` order, so a system can print several at once (palm mute
  and let ring now; up/down picking later), and a row only appears where it occurs. Both
  marks are aligned with their first note.
- Direct shift/legato slides of at least an eighth note sit right against the note they
  slide to (`15\12` instead of `15\-12`) when every note in the column qualifies. Other
  effects still keep their separating dash.
- `gscore-cli legend` prints every symbol from the `Symbol`/`Annotation` enums the dump
  uses, so the tablature and its documentation cannot drift apart. The dump help text is
  current again.
- `test_golden` compares `legend.txt`, checks `15b17`, `7[19]`, `15\12` and both annotation
  rows, and strips bracketed harmonic nodes from the cross-format note grid (GP5 cannot
  store them). A wide dump also proves palm mute and let ring stack as two separate
  under-staff lines. Goldens regenerated for all three `ornaments` exports.
- Still open: `[?]` for GP5 pinch harmonics, and the cross-measure target/ties gap in
  gaps #4.
