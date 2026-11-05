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

## 2026-11-01 — Review of the `more-tests` branch

- Bend unit: bend points are stored in quarter tones (`GP_BEND_SEMITONE` = 25 hundredths),
  but `Bend::semitones` passed them through and `dump` silently compensated. Now
  `Bend::quarter_tones` (a full bend is 4) taken from the highest point, so a bend that is
  released again keeps its reach (it reported 0); `dump`'s `bend_target` uses the same
  unit. Output unchanged.
- Legend: slap is `T` (thumb) instead of a second `S`; the legend shows examples (`(5)`,
  `7[19]`, `7(19)`) instead of `n` placeholders that clashed with `n` (pick stroke down). A
  test checks that every marker is unique.
- `Annotation::marks(note)` decides which notes a row covers, so a new row needs no change
  in the grid code.
- File sizes: effect types and accessors moved from `views.rs` (545 lines) to
  `gscore-model/src/effects.rs`; `dump.rs` (904 lines) split into `dump/{mod, symbols,
  grid, notation}.rs`. Unused `has_slide`/`is_harmonic` removed.
- Model tests for the effect accessors (bends, harmonic nodes, slides, simple note and beat
  effects); docs/guitarpro.md lists the harmonic-fret fix (19).
- Goldens: only `legend.txt` changed; all dump goldens are byte-identical after the split.

## 2026-11-01 — M3 complete: saving

- Verification: `differences()` compares beat and note effects field by field. That found
  the GP7 writer dropping whammy dips (10 band files) and, behind it, the reader dropping
  bend-and-release (214 notes): both ignored the GPIF middle value. Reader and writer now
  keep origin/middle/destination for bends and whammy (docs/guitarpro.md 20, 21). Complex
  bends (model kinds, `dump` release notation) are postponed by the owner (gaps #4).
- `gscore_session::save`: GP7 only; verify in memory, refuse losses unless allowed; temp
  file + fsync, `.bak` on first overwrite, atomic rename, directory fsync, byte-compare
  after re-reading from disk. `gscore-cli convert <file> -o <out> [--to] [--allow-losses]`.
- The GP7 XML writer is under the no-panic lints; `write_gpif_xml` returns `GpResult`;
  simile marks no longer round-trip through string slicing (docs/guitarpro.md 22).
- Results: all 26 band files and 9 test files round-trip and `convert` to GP7 with no
  differences; converted test files show identical `info` (except the format) and `dump`.
- Tests: 126.

End-of-M3 review: gscore-session now depends on gscore-io/gscore-model (layers 2/1, fine);
the CLI gained `format_arg` shared by `convert` and `roundtrip-check`. Open risks: whether
Guitar Pro opens our GP7 output (unverified), and the remaining unlinted `gscore-gp` code.

## 2026-11-01 — M4 complete: headless editing

- `gscore-model` gained checked editing methods (`src/edit/`): set/clear a note, set a beat
  length, insert/delete a beat, insert/delete a measure. Every method runs on a copy of the
  wrapped `gscore_gp::Song` and swaps it in only after validation, so a failed edit changes
  nothing (AGENTS.md rule 8). Each returns an opaque `Change`; `Song::revert` undoes it with
  the exact previous data, effects included. New `NoteValue` (note value + dots + tuplet)
  parses and prints `1/8`, `1/16..`, `1/8*3:2` and keeps the exact `Duration`.
- New measures decompose the time signature into exact rests (4/4 is one whole rest, 7/8 is
  a double-dotted half), inherit time and key signature, and are inserted in every track plus
  the measure header. Deleting the only measure is refused; insert/delete renumber headers,
  measure numbers and track links.
- `gscore-edit` is implemented: `Command` (id, label, `enabled`, `apply`), a static registry
  (`COMMANDS` with aliases, help and parameter metadata), a text parser (`id key=value ...`,
  1-based positions, `#` comments) and `Editor` (undo/redo, bounded history; failed commands
  never enter it). `set_fret` is the CLI name; `enter_fret` is an alias.
- `gscore-cli commands` lists every command and parameter; `gscore-cli edit <in> -o <out>`
  takes repeatable `-c "<command> ..."` and/or `--script <file>` and saves through
  `gscore_session::save`. A bad command or script line aborts before anything is written.
- Tests: apply-then-undo helper (undo must leave `differences()` empty), per-command tests,
  `panic_hunt` (every command x 8 song shapes x adversarial text and typed extremes),
  property test (random sequences on 8 songs: invariants hold, undo-all restores, redo-all
  returns), and a save round trip after every successful edit. CLI script
  `tests/scripts/edit_basic.gscript` + golden dump; `undo_all.gscript` restores the input.
- Tests: 179 (+53). `cargo xtask ci` green, wasm check included.

End-of-M4 review: `gscore-edit` parses text itself (clap stays in the CLI), and the model
owns undo data behind an opaque `Change`, so the command layer never sees `gscore-gp`. Open:
copy/cut/paste and the cursor/selection belong to M7; every model edit clones the whole song
(including embedded audio), which is correct but should become a per-region copy; tempo,
time signature and effect editing are not commands yet; undo history is in memory only.

## 2026-11-01 — Review of M4

- New measures: only voice 1 is filled with rests; further voices get an empty placeholder
  beat, as Guitar Pro stores unused voices. Before, voice 2 showed a full set of rests too.
- Undo of `insert_beat` into a measure without voices (GP7 empty bars) also removes the
  voice the insert created; before, an empty voice stayed behind. Test added.
- `Editor` keeps its undo history in a `VecDeque`, so trimming the oldest entry at the
  limit no longer shifts 10,000 entries per edit.
- `gscore-cli`: `report.rs` holds the difference listing and save messages shared by
  `convert`, `edit` and `roundtrip-check` (three copies before). Accepted losses now end
  with "... and N more" like refused ones, instead of stopping silently at 20.
- `gscore-edit/src/parse.rs` tests moved to `parse/tests.rs` (file under 400 lines).
- Measured: a single-note edit takes about 3.6 ms in release on `DrDr.gp` (whole-song clone
  plus full validation); noted in docs/gaps.md #5 for M7.
- Open question for the owner: AGENTS.md section 7 says undo history should not be limited,
  rule 9 says to cap it by memory. The code caps by entry count (10,000); unchanged.

## 2026-11-01 — M5 started: first tablature layout

- Owner decisions: Bravura (SMuFL, with its metadata JSON) as the music font, Noto Sans as
  the text font, both SIL OFL 1.1 in `assets/fonts/` (rows in THIRD_PARTY.md); `serde_json`
  added to `gscore-layout` to read the metadata.
- `gscore-layout` implemented: `layout_track` turns one track into `DrawOp`s (lines,
  rectangles, glyphs by SMuFL name, text) in staff spaces with semantic `ColorToken`s.
  Modules: `header` (title block, credits, tuning, tempo), `measure` (columns of beats
  across voices, duration-based spacing, fret numbers with masks, rests, time
  signatures), `system` (greedy line breaking, justification), `staff` (lines, barlines,
  tab clef), `metrics` (Bravura metadata, Noto Sans digit widths), `theme` (light and
  dark palettes), `svg` (SVG output with stable two-decimal numbers).
- `gscore-cli render <file> -o out.svg [--track N] [--theme light|dark]`.
- Model: `BeatView::note_value()` (written note value, dots, tuplet); the view iterators
  (`measures`, `voices`, `beats`, `notes`) no longer borrow the view itself (`+ use<'a>`),
  so they chain with `flat_map`; `testing::one_beat_with_raw_notes` for odd-note tests.
- Robustness: an unreadable fret (DrDr.gp's piano and vocal tracks store `-7`) shows as
  `?` instead of failing the whole track; drum tracks return an "unsupported" error.
- Tests: SVG goldens for the three owner files (light, plus one dark), a 4-string bass and
  an 8-string guitar; properties over every test file and track (all coordinates finite
  and on the page, one fret number per shown note, deterministic), every string count,
  200-measure line breaking, empty songs, nonsense options. Tests: 205 (+26).
- Compared with Guitar Pro's PNG exports: header, fret positions, rests and line breaking
  look right; Guitar Pro shows standard notation above the tab, which is out of scope.
- Release timing on DrDr.gp (172 measures): 0.03-1.1 ms per track layout (8.7k draw ops for
  the busiest track), 2.2 ms to read the Bravura metadata once, up to 4.8 ms to write SVG.

Open (rest of M5, docs/gaps.md #9): effects, rhythm under the tab, repeats, pages,
measure-layout cache, PNG output (a rasterizer such as resvg needs owner approval).

## 2026-11-01 — M6 started: read-only app

- Owner decisions: eframe/egui 0.37 with the wgpu renderer (checked against PhotoCraft,
  which uses the same setup), egui_kittest for headless UI tests and offscreen snapshots,
  rfd 0.17 for file dialogs (XDG portal, no GTK). Rows in THIRD_PARTY.md.
- `gscore-session::open`: capped read plus import, shared by the CLI and the app (the read
  itself is now capped too, not only the size check).
- `gscore-app` (binary `guitarscore [FILE]`): menus (File › Open…, Quit; View › theme,
  zoom), track picker, zoom, page view with culling, status bar, dismissible notices.
  Files open on a background thread; layout, drawing and background jobs are wrapped in
  `catch_unwind`. Fonts (Noto Sans, Bravura) are compiled in; Noto Sans is also the UI
  font.
- `cargo run -p gscore-app --example snapshot -- <file> <out.png> [--dark] ...` renders the
  app offscreen to PNG.
- Tests: view-state unit tests; UI tests through the accessibility tree (open at start,
  no file, unreadable and non-GP files give notices, View menu switches theme, toolbar
  zooms); two snapshots (light and dark) in `crates/gscore-app/tests/snapshots/`, stable
  over repeated runs. Snapshots showed two bugs, fixed: `set_visuals` left egui's widgets
  light (now `set_theme`), and the absolute path in the status bar made images depend on
  the checkout location (now the file name). Tests: 216 (+11).

Open (rest of M6, docs/gaps.md #10): persist view state and window, recent files, zoom
shortcuts (with the keymap, M7), macOS file dialog on the main thread.

## 2026-11-01 — M7 started: MIDI playback and export

- Owner decisions: MIDI playback moves before interactive editing (M7 now, editing M8);
  `midir` 0.11 for MIDI output (ALSA via libasound on Linux).
- `gscore-cli gui [FILE]` starts the app as a process (the CLI cannot link the GUI: both
  are top-layer crates); `cargo gui` builds and runs it.
- Data-loss fixes in `gscore-gp` found on the way (docs/guitarpro.md 23-25): GP3-5 tempo
  changes above 255 BPM were clamped; GP7 repeat counts were off by one (plays vs.
  repeats) in both directions; GP7 programs (in `<Sounds>`) were not read, so every GP7
  track had the default instrument. Each with a regression test.
- Model (`playback.rs`): `Song::track_sound` (program, volume, pan, drums), `Song::repeat`,
  `Song::tempo_change`, `BeatView::tempo_change`, `TrackView::capo`,
  `TrackView::sounding_pitch` (open string + fret + capo; natural and artificial
  harmonics by their node), `NoteView::velocity`.
- `gscore-play`: playback order with repeats and alternative endings (bounded);
  `Sequence` (exact tick positions at 960 PPQ, ties held, dead notes short and soft, one
  channel per track, drums on channel 10); tempo map; SMF writer (format 1); `Player`
  trait and a pure `Scheduler` that backends drive with their own clock.
- `gscore-audio`: `MidiOutPlayer` on its own thread, commands through a bounded channel;
  output errors stop playback and are reported, never panic.
- CLI: `midi` (export), `ports`, `play [--port] [--track N] [--speed F]`.
- App: Play/Pause and Stop in the toolbar, a "MIDI out" port picker, the playing measure's
  number. Snapshots regenerated for the new toolbar.
- Checks: the sequenced notes of all three owner files match Guitar Pro's own MIDI export
  (pitches, start times within one tick of 480 per quarter, programs); end-to-end tests
  play through ALSA's "Midi Through" port and listen on it (CLI, audio crate, app).

Open (rest of M7, docs/gaps.md #11): bends/slides with channel borrowing, other techniques,
a playhead on the page (after the layout work is merged), mute/solo.

## 2026-11-01 — `gscore-cli gui` starts the desktop application

- New `gui [FILE]` subcommand: finds the `guitarscore` binary (next to the running
  `gscore-cli` first, so a workspace build's `target/<profile>/` pair works, then `PATH`),
  starts it with the optional file, and waits; a missing binary, a start failure and a
  non-zero application exit are all reported (exit code 1) instead of being swallowed.
  Still a child process, not a crate dependency, so `gscore-cli` and `gscore-app` stay in
  the same layer (AGENTS.md section 5).
- Tests: 4 (`gui` discovery order, directories are not binaries, missing binary, launch
  forwards the file and reports a failing application) plus help assertions.

## 2026-11-01 — layout: ornaments on the tab staff

- `gscore-layout` now draws the effects the model exposes, in tab conventions: tie
  continuation number is hidden and a tie curve joins it to the origin (the old `(5)` was a
  ghost-note mix-up), ghost notes are parenthesized, dead notes stay `x`. Hammer-ons and
  pull-offs get a slur below the numbers and an `H`/`P` letter (from the two frets); shift
  slides a line between them; legato slides a slur with `sl.`; slides in/out get a slash
  before/after the fret; bends a curve with an arrow and `1/2`/`full`/`1 1/2`; slight and
  wide vibrato are vector waves above the staff (no music-font glyph needed, so the golden
  SVG shows them anywhere); harmonics print `<7>` and `7 <19>`; accents use `>` and `v`
  (heavy), staccato a dot; trill/tremolo glyphs; bold `P.M.`/`let ring` rows 2.2 staff
  spaces below the staff with dashed runs and a closing tick; the whammy line ends with an
  arrow and its whole-step offset; beat-level legato is one slur over the run.
- Owner review: measures now reserve `Spacing::barline_padding` (1.2 staff spaces by
  default) at both ends, so a fret number keeps its distance from the previous and next
  barline. The fret's mask counts as part of its width, so the padding is the visible gap.
  Regression test `notes_keep_their_distance_from_barlines`.
- Second owner review: bend and whammy arrowheads are vector V strokes now (up for a bend,
  down for a dive), so they no longer depend on a music-font glyph; the shift-slide line
  moved 0.3 staff spaces above the string line so the staff line cannot hide it; vibrato is
  collected per column, so note vibrato and the beat's `VibratoWTremBar` both appear (all
  four wiggles in `ornaments` measure 6 now show); pinch harmonics print `PH` above the
  staff. Beat vibrato is imported and written back (`BeatEffects::vibrato`/`wide_vibrato`,
  docs/guitarpro.md 28).
- New `DrawOp::Curve` (cubic Bézier) for slurs and bends, drawn by the SVG writer and the
  egui canvas. The model gained `NoteView::accent()`, `vibrato()`, `BeatView::whammy_bar()`
  and `legato()`, plus `AccentKind`, `Vibrato`, `Whammy` and `Legato`.
- Data that used to be lost on import/save is kept: GPIF wide vibrato (`wide_vibrato` in
  `NoteEffect`) and beat-level `<Legato>` (`BeatEffects::legato`). The save comparison now
  checks accent kind, vibrato kind, legato and the whammy shape. docs/guitarpro.md 26-27;
  tests in `effect_tests`.
- Goldens: `ornaments.svg` regenerated (the only layout golden that changes; the other
  files have no effects). Rendered with `gscore-cli render` + `rsvg-convert` (Bravura
  installed through a temporary fontconfig dir) and compared with Guitar Pro's reference:
  slurs, slides, bend, vibrato, harmonics, let ring, whammy and palm mute all line up.
- Tests: layout now counts only notes shown in tab (ties excluded) and asserts the
  ornaments file really contains its texts, `H`/`P`/`PH`, the dot glyph and curves. Tests:
  221 (+5). Workspace fmt/clippy/tests/xtask layers green.
- Open (docs/gaps.md #9): cross-measure/system ornaments, rhythm under the tab, grace
  notes/fingerings, beat-level pick/brush/slap/fade-in marks, and per-chord ornament
  layout. Note positioning is the owner's next topic.

## 2026-11-01 — Layout polish and standard notation above the tab

- Polish (owner review of the goldens): ornaments above the tab get rows above the
  measure-number band; bends, vibrato and the whammy line follow their note's space; slides
  slant with the pitch; slide slashes keep a gap; rests are spaced by their drawn width; the
  tab time signature is smaller.
- Standard notation above the tab, as in Guitar Pro (new `notation` module): treble clef
  (bass clef below F1), written an octave above sounding; notes spelled with sharps and
  placed by staff position; accidentals per measure (ties keep theirs); noteheads by value
  (cross for dead notes), seconds side by side, ledger lines; stems from the font's
  `stemUpSE`/`stemDownNW` anchors, at least to the middle line; flags; beams per beat
  (dotted quarter in 6/8, 9/8, 12/8) with partial beams; dots; rests; ties; tuplet
  numbers. With notation, rests and the time signature appear in the notation only and
  measure numbers sit above it, as in Guitar Pro. `LayoutOptions::notation` turns it off.
- Spacing now uses each column's left and right extent (accidentals left, dots and flags
  right; beamed notes need no flag room), so 64th runs stay compact.
- Font: engraving defaults for stems, beams and ledger lines, and glyph anchors, are read
  from Bravura's metadata; new glyphs for clefs, noteheads, sharp, natural and the flags.
- Compared with Guitar Pro's PNGs: DADGAD identical in beams and stem directions; note
  lengths and ornaments match measure by measure (accidentals in ornaments m6 and m10 as in
  Guitar Pro). Goldens and app snapshots regenerated (owner review). Tests: 266 (+6).

Open (docs/gaps.md #9): key signatures, second voice, ties across barlines, tuplet groups
across beats, harmonic diamonds, notation-staff articulations.

## 2026-11-01 — M7: bends and slides on borrowed MIDI channels

- `gscore-play` now plays bends and slides. `MidiEvent::PitchBend` writes a 14-bit bend;
  a note with a bend or slide borrows a free melodic channel (AGENTS.md section 5, decision
  10), receives the track's program, volume and pan plus a 6-semitone bend range (RPN 0,0,
  then RPN null), and is released after a center reset. The rest of the chord keeps its
  pitch. Guitar Pro's own MIDI export sets the same six-semitone range (its first bend is
  at tick 5760 at 480 PPQ with a 2621 offset, ours at 11520 at 960 PPQ with 2730).
- Bends ramp to their highest point (a quarter note at most) and hold; release bends start
  at the bent pitch and return. Shift/legato slides go to the next note on the same string;
  in slides use the previous note when it lies in the slide's direction, otherwise a whole
  step; out slides move a whole step. Notes that cannot get a channel play without the pitch
  movement: `Sequence::degraded_pitch_bends` counts them and `gscore-cli midi`/`play` print a
  warning.
- `ChannelAllocator::free_channels`/`used_channels`; `MidiEvent::silence` also resets a
  stale bend (but not volume/pan, so pause/resume keeps its mix). The test support gained
  `TestNote`/`song_with_notes`/`song_with_notes_on_tracks` so bends, slides and chords can be
  built without a fixture.
- Tests: pitch-bend bytes and ramp/release shapes, RPN setup, synthetic bends/slides/chords,
  graceful degradation with all 15 melodic channels in use, SMF output, and the owner's
  `ornaments` file: 133 bend events on borrowed channels (Guitar Pro's own MIDI starts its
  bends at tick 5760 at 480 PPQ, ours at 11520 at 960 PPQ, the same bar), and an end-to-end
  test hears a pitch bend on ALSA's "Midi Through" port. Tests: 282 (+16).
- Open (docs/gaps.md #11): bend curves are simplified (the model keeps only the highest
  point), and whammy, vibrato, let ring, palm mute, grace notes and trills are still silent.
- Review fix: slides wider than six semitones (fret 5 to 12, say) stopped short. The range
  is sent per borrowed note anyway, so such a slide now sets a wider range (up to 24
  semitones); bends keep Guitar Pro's six. Tests: 284 (+2).

## 2026-11-01 — M8: interactive editing in the app (without the recovery journal)

- Owner decision: M8 now covers the cursor, keyboard entry, keymap and undo/redo in the
  app; the recovery journal and crash recovery are deferred.
- `gscore-edit`: `Cursor`, `ActionId`/`Action` (22 actions; ids are the keymap's),
  `Keymap` (text file `<key> = <action>`, typed characters and key names with modifiers,
  every problem reported with its line), `TabEditor` (actions to commands at the cursor;
  undo/redo put the cursor back where the edit happened; two fret digits within 500 ms make
  one fret and one undo step; Right at the end of an unfilled measure adds a rest as long as
  the current beat or what still fits, at the end of the last full measure a new measure;
  a digit in a measure without beats inserts its beat first). Copy/cut/paste answer
  "not implemented yet" (no selection).
- `gscore-model`: `NoteValue::filling` (the measure-filling rests, now public).
- `gscore-layout`: `Layout::places` (systems, measures, beat columns) with `cursor_rect`,
  `column_rect` and `hit`; `ColorToken::Cursor` and a cursor color in both themes. Goldens
  unchanged.
- `gscore-session`: `load_user_keymap`/`load_keymap` (`~/.config/guitarscore/keymap.txt`,
  capped, falls back to the built-in keymap and says why); re-exports `FileFormat`.
- `gscore-cli keymap [--check FILE]`: the actions with their keys and the built-in keymap,
  or a check of a keymap file.
- App: keys go to the editor before any widget sees them (not while a menu, dialog or text
  field has the keyboard); cursor box and beat column in the cursor color, scrolled into
  view; click to place the cursor; Edit menu with the keys from the keymap; File › Save
  (in place for `.gp`, otherwise Save As) and Save As… in the background with the
  safe-save procedure; a lossy save asks; closing the window or opening a file with
  unsaved changes asks (Save / Discard / Cancel); "(changed)" in the status bar and "•" in
  the title; the cursor position in the status bar; playback rebuilds after an edit.
- Layout after an edit: the whole shown track is laid out again (war.gp, 236 measures:
  about 15 ms in release including SVG output); per-measure caching is still open.
- Tests: cursor moves, fret entry and the digit window, durations, measure filling and
  appending, undo/redo cursors, random actions on synthetic songs and the three owner files
  (undoing everything restores the song), keymap parsing and QWERTY/QWERTZ/AZERTY
  characters, egui event translation and IME, places and hits, keymap file loading, and the
  app driven by key events (typing, Shift+digit, `+` with and without Shift, Ctrl+Z, Ctrl+S
  saving in place with a backup, the unsaved-changes question, the Edit menu). App
  snapshots regenerated: they now show the cursor. Tests: 313 (+29).
- Open (docs/gaps.md #5, #10): selection and copy/paste, voice 2 entry, effects, playback
  from the cursor, incremental layout and per-region edits, default keys for
  `clear_note`/`previous_measure`/`next_measure`, the recovery journal.

## 2026-11-02 — M9: internal synthesizer

- Crates checked before starting: `rustysynth` 1.3.8 (2026-10-20, MIT, active: releases in
  October 2026, pure Rust, 16 channels with channel 10 as drums, RPN 0 pitch-bend range
  supported, `.sf3` refused) and `cpal` 0.18.2 (2026-08-16, Apache-2.0; ALSA on Linux,
  which PipeWire and PulseAudio serve). No bundled SoundFont yet.
- `gscore-play`: `Scheduler::jump` and `Scheduler::setup` (seek without allocating, for
  the audio callback); `PlayError::Device` reads "playback: …" (no longer MIDI only).
- `gscore-audio`: a shared `Transport` (play/pause/stop/seek/tempo over a `Sink`) now runs
  both the MIDI thread and the synth, so they behave the same. `SoundBank` (a loaded
  `.sf2`), `SynthEngine` (renders sequences offline or in a callback; its clock is the
  frames rendered, so timing is exact to one 64-frame block), `SynthPlayer` (loads the
  SoundFont and opens the default device on its own thread; the callback takes commands
  from a bounded channel, never allocates, locks or does I/O, hands replaced sequences
  back to be freed, catches panics and goes silent), `find_soundfont`
  (`GSCORE_SOUNDFONT`, `~/.local/share/soundfonts`, well-known Arch/Fedora/Debian files,
  any `.sf2` in the system folders). Feature `test-support`: a sine-wave SoundFont built
  in code, so tests need no third-party file.
- `gscore-cli play`: the built-in synthesizer by default (`--soundfont FILE` or the one
  found), `--port` for MIDI; without a SoundFont, the first MIDI port as before.
- App: the toolbar's "MIDI out" became "Output": the synthesizer (with the SoundFont found
  at start), "Choose SoundFont…" (dialog in the background) and the MIDI ports; "Loading
  sounds…" while the SoundFont loads; a failed synthesizer is reported and started afresh
  on the next play. App snapshots regenerated (toolbar text only).
- Tests: transport commands, SoundFont reading and refusal, note onset within one block
  and pitch (440/880 Hz), end, pause/resume, stop, seek, tempo scale, interleaving for
  mono/stereo/5.1 and integer formats with clipping, a model song through the synth,
  SoundFont discovery order, a broken SoundFont reported without blocking (player and
  app), and end to end on the default audio device (skipped without one). Tests: 328 (+15).
- Open (docs/gaps.md #11): no bundled SoundFont (licence and size to decide), the default
  device only, the chosen SoundFont is not remembered between runs, no WAV export.

## 2026-11-02 — Playback from the cursor (pulled forward from M12)

- Owner request: select a note and start playback there.
- `gscore-play`: `Sequence::tick_of(song, BeatPosition)`: the tick where a beat is first
  played (its measure's first pass plus the beats before it in its voice).
- App: Play starts at the cursor's beat. After a pause it resumes, unless the cursor moved
  meanwhile; then it starts at the cursor. After Stop or the end, Play starts at the
  cursor again. `Playback::position`, `GuitarScoreApp::document_mut`.
- `gscore-cli play --from MEASURE` (1-based; refused outside the song).
- Tests: `tick_of` with repeats, mixed lengths and positions past the last beat; the app
  plays from measure 3, resumes after a pause, starts at measure 1 after the cursor moved
  and at measure 2 after Stop; `play --from` errors and one measure on the device
  (skipped without one). Tests: 333 (+5).
- Still open (M12): from the first note of a selection, and the cursor following playback.

## 2026-11-02 — M10: performance

- Measured first (`gscore-cli bench`, release, the owner's largest files): an edit and the
  layout after it took 3.9 ms on `DrDr.gp` (172 measures, 16 tracks; 2.7 ms of it the
  whole-song copy every edit made) and 3.3 ms on `master.gp` (415 measures). The M8 note of
  "15 ms" included SVG output in a debug build. In a debug build (`cargo gui` until now)
  it was 21-31 ms, two frames: the slowdown the owner saw.
- `gscore-model`: edits are transactional per region. Note and beat edits (and their
  undo) copy and check only the measure they touch (`Song::edit_measure`); inserting and
  removing a measure check everything first and then change the song in place
  (`Song::edit_structure`), building and checking the new measures before inserting them.
  `Song::measure_stamp`: a number per measure that changes with the measure and is never
  reused. `Change::size_estimate`.
- `gscore-layout`: `LayoutCache` keeps each measure's layout by stamp (plus whether it shows
  its time signature) and lays out only new or changed measures; a moved measure reuses
  its layout with the new index. Track settings or options start it afresh. The app keeps
  one per track.
- `gscore-edit`: the undo history is capped at about 64 MiB as well as 10,000 entries
  (oldest first; the newest entry always stays).
- `gscore-cli bench FILE... [--check]` and `cargo xtask bench` (files from the arguments,
  `GSCORE_BENCH_FILES` or `testdata/basic`): opening, layout, sequencing, a note edit, a
  measure insert and edit-plus-layout, checked against the budgets now in AGENTS.md
  section 11 (16 ms key to layout, 2 s to open). `cargo gui` builds in release.
- After (release): edit-plus-layout 0.32 ms on `DrDr.gp`, 0.85 ms on `master.gp`, 0.59 ms
  on `war.gp`; a note edit or a measure insert takes 1-9 µs whatever the song's length.
  Debug build: 2.4 ms and 8.8 ms. What still grows with the song: line breaking and
  drawing all measures (fine far beyond the budget), opening, and the playback sequence
  (built on Play: 3.7 ms on `master.gp`).
- Tests: the cache gives exactly a fresh layout after 40 mixed edits and their undo on
  three files, with and without notation; one note edit lays out one measure, an inserted
  measure only itself; stamps change only for edited measures and move with inserted and
  removed ones; failed edits change neither song nor stamps; the memory cap. Tests: 338
  (+5).

## 2026-11-02 — `gscore-gp` under the workspace standards

- Owner request: apply the project's code rules to the crate copied from guitarpro.
- Removed the crate-level `allow` of every lint. The crate now passes the workspace lints
  (no `unwrap`/`expect`/`panic`, no indexing or slicing) plus `arithmetic_side_effects`
  and the lossy-cast lints, in library and test code; every file is under 400 lines
  (`gpif_import` 1,633 lines, `gpif_export` 840 and five more split into modules).
  About 760 findings, fixed file by file. Deliberate truncations (Guitar Pro tolerates
  junk in metadata fields) go through named helpers, so the bytes read and written stay
  the same. Details: docs/guitarpro.md change 29.
- Oracle: `info`, `dump` and round trips to `.gp`, `.gpx` and `.gp5` of all 78 owner and
  test files, compared after every step; identical except one GP5 round trip that now gets
  further (a correct track-name field; it then fails on the known 1/256 duration,
  docs/gaps.md #2).
- Crashes fixed, each with a test that panicked on the old code: over-long track and
  chord names in GP3-5 writing, repeat count 127 and 8-string notes in GP5 writing, a
  copyright without a line break, `Note::real_value` past the tuning, `PitchClass` display
  of B#/Cb, BCFS block numbers that overflowed. The library no longer prints. A song
  without tracks now writes to GP3-5 as an empty song (it panicked, contained by
  `gscore-io`). Opening times unchanged (`cargo xtask bench`). Tests: 346 (+8).
- Open (docs/gaps.md #1): rustdoc on the inherited public model fields.

## 2026-11-02 — M13: track and song grid (bottom panel)

- `gscore-model`: track edits with undo, one module (`edit/tracks.rs`):
  `insert_track` (rest-filled measures matching every header, tuning and instrument from
  the track before), `delete_track` (never the last one), `duplicate_track` (notes and
  sound, a unique name), `move_track` (final index), `rename_track`, `set_track_color`,
  `set_track_volume`, `set_track_pan`, `set_track_mute`, `set_track_solo`. A mixer value
  on a channel shared by several tracks clones the channel for that track first, so one
  slider does not move another track; undo restores the channel table too. Track edits
  leave measure stamps alone: a layout is per track, and the app's per-track cache is keyed
  by track index, so moving or renumbering clears the affected entry. Read-only
  additions: `TrackView::color/is_muted/is_soloed`, `MeasureView::has_notes`,
  `Song::section/any_soloed`, `TrackColor` and `Section`, and `settings_differences` (what a
  save may not keep: color, mute, solo, volume, pan; the command tests use it, the save
  path still compares only the musical content, docs/gaps.md #11).
- `gscore-play`: muted tracks are skipped, and while any track is soloed only soloed
  tracks play (an explicitly requested missing track is still an error).
- `gscore-edit`: ten commands (`insert_track`, `delete_track`, `duplicate_track`,
  `move_track`, `rename_track`, `set_track_color`, `set_track_volume`, `set_track_pan`,
  `set_track_mute`, `set_track_solo`), parsed from command lines, in `COMMANDS`, with
  `changes_tracks`/`changes_mix` for the app; `TabEditor::execute_command` runs one at the
  cursor. `Editor` merges consecutive volume or pan commands on the same track into one
  undo step, so dragging a slider is one undo. The shared apply-then-undo helper now
  compares `settings_differences` too.
- App (bottom panel, `app/grid/`): a track column (color, three-letter instrument tag,
  clipped name, M/S buttons, volume slider, pan knob; all keyboard/assistive-technology
  reachable) and a measure matrix (13 px per measure, measure numbers, section markers
  with their color, repeat signs with counts, alternative-ending numbers, a colored
  square where a track has notes, the playing measure and the edit cursor highlighted).
  Click moves the cursor to that track and measure; drag selects a measure range;
  Shift-click extends it. Right-click a track: add below, delete, duplicate, rename
  (window), move up/down. Every action goes through `gscore-edit`. The panel can be
  hidden from View › Track grid. The grid data (squares, sections, repeats) is cached per
  editor revision.
- Playback: `Playback::mix_changed` rebuilds the sequence and, while playing or paused,
  reloads it at the same tick, so volume, pan, mute and solo are heard at once.
- Tests: model track edits and undo, shared channels, sections and note squares; both
  command tests; `panic_hunt` (new commands run through it); parser cases; mute/solo in
  the sequencer; app tests for mute/solo/volume, a grid click moving the cursor and
  selecting a measure, and the row menu (duplicate, rename, delete). App snapshots
  regenerated (the grid is now in the frame; owner review needed). Tests: 365 in the merged tree (19 new; the merged base had 346).
- Still open (docs/gaps.md #11, #12): GP7 mixer persistence, the M15 track panel, and the
  selected measure range is not used by copy/paste or looping yet.
- Merge notes for the other agents:
  - M10 (merged before this entry was committed): track edits are in
    `gscore-model/src/edit/tracks.rs`. Adding, deleting, duplicating or moving a track uses
    the new `Song::edit_tracks`, which renews every measure stamp (a measure's layout
    includes all tracks); renaming, color and the mixer values use
    `Song::edit_track_settings` and leave stamps alone. `Document::forget_layouts` empties
    the per-track layout map after a structural track command so no entry stays under an
    old track index.
  - M12: the bottom panel is `app/grid/mod.rs` + `app/grid/matrix.rs`, shown with
    `Panel::bottom("track_grid")`, toggled by `ViewState::show_track_grid` and shown before
    the status bar. The M12 panel framework should take this over (visibility and size
    remembered, View menu) instead of adding a second bottom-panel mechanism.

## 2026-11-02 — M13 review fixes

- **Mixer settings survive a save.** The GP7 writer now stores what the GPIF format
  actually uses: `<PlaybackState>Solo|Mute|Default</PlaybackState>` for mute and solo,
  the RSE channel-strip `<Parameters>` (index 11 is the balance, index 12 the volume, as
  alphaTab's GPIF importer reads them) for volume and pan, and `<MidiConnection>` for the
  track's port and channels. The program is written from `midi_program_gpif` or, for
  songs from GP3-5, from the channel table. The reader reads all of these (preferring
  `<GeneralMidi>` for channels, as GP6 does), fills the channel table for GP7 files, and
  maps the RSE floats back to 0-127. `gscore-session` now counts
  `settings_differences` (color, mute, solo, program, volume, pan) as save losses, so
  anything a format cannot keep asks first instead of disappearing. Test: change volume,
  pan, mute, color and then solo on `steel_guitar_dadgad.gp`, save, reopen and compare
  every value; the existing three-file save test compares settings too.
- **Undo of a mixer change no longer breaks the channel.** When a shared channel was
  cloned for one track, the undo data recorded the clone as the previous channel and then
  removed it, leaving the track on a missing index. It now records the channel the track
  came from. The test uses a real 64-channel table (volume 90, pan 30) so the old bug is
  caught (it fell back to 104).
- **Live volume and pan.** `Player::send_event` sends one MIDI message through both
  backends (`gscore-audio`'s transport handles `Command::Send`), and
  `Playback::send_track_mix` sends CC 7/10 to the track's channel from the playing
  sequence. A slider no longer rebuilds the sequence or stops the player, so sounding
  notes continue; seeking re-sends the current mixer values because the sequence's setup
  holds the values from when it was built. Mute and solo still rebuild (they change which
  tracks play). The synth app test moves the volume slider while playing, checks that the
  position keeps advancing, and undoes it through the Edit menu without stopping.
- **Undo is live too.** `TabEditor::last_command` (set by edits, undo and redo) and
  `EditEffect` in the app choose between a direct mixer message, a sequence rebuild
  (mute/solo) and a full stop; Ctrl+Z of a volume change no longer stops playback.
- **Undo merging only within one drag.** `Editor::execute_with_merge` merges a volume or
  pan change only when the caller says the drag is still in progress; the grid passes that
  for a slider's `dragged` frames and not for a press or click, so two separate drags are
  two undo steps.
- **Stamp noise gone:** track edits no longer renew every measure stamp. A layout is per
  track, and the app's per-track layout map is keyed by track index (and dropped for
  structural track commands and their undo/redo), so only the affected track is laid out
  again. The model test checks that a rename and a move leave the stamps unchanged.
- **The grid paints only what is visible:** the measure and row ranges come from the
  scroll area's clip rect, so a long song no longer repaints every measure each frame.
- **Splits:** `gscore-model/src/edit/tracks/` (`mod.rs` 367 lines, `new_track.rs` 86) and
  `gscore-app/src/app/grid/` (`mod.rs` 193, `rows.rs` 298, `matrix.rs` 314, `mixer.rs`
  73); every file is under 400 lines.
- Tests: 369 (+4; save settings round trip, live transport message, sequence channel
  lookup and separate slider drags) plus the extended synth app test.
- Still open: the track panel (M15), and using the selected measure range (M14/M18).

## 2026-11-02 — M11: recovery journal and crash recovery

- `gscore-edit`: `Command::to_line` (the exact inverse of `parse_line`; exhaustive, so new
  commands must add themselves), `Command::position`/`measure`. `TabEditor` records what
  changes the song (commands, `undo`, `redo`; `take_applied`) and `replay`s it, which
  rebuilds the song and the same undo history. Feature `test-support`:
  `testing::panic_next_command` makes the next command panic (panic-injection test).
- `gscore-session`: `Journal` keeps one recovery folder per open document (`lock`,
  `meta` with the source path, `base` = the file's bytes as opened, `journal` with one
  checksummed line per entry, `snapshot.gp` + `snapshot` every 200 entries or 60 s,
  written to temporary files and renamed). A writer thread does all file work and syncs
  each batch; recording never blocks (a full queue buffers). `written()` counts entries on
  disk. A lock held while open tells a crash (lock released) from another running window.
  `find_sessions` lists crashed documents with edits since their last save (removing
  folders with nothing to recover), `recover` replays the journal on the base (or on the
  snapshot if that recovers more, for example with a damaged base) and stops at a damaged
  line, `discard` removes a folder. `open_with_bytes`, `default_recovery_dir`
  (`~/.local/state/guitarscore/recovery` on Linux).
- App: `GuitarScoreApp::with_options(.., recovery_dir)` (`main` passes the default);
  opening starts the document's journal in the background; every frame hands the edits to
  it; a save marks it (only if nothing changed while saving); closing a document or the
  app normally removes its folder. At the start, crashed documents are offered one by one
  (Restore, Discard, Later); a restored one opens unsaved, with its undo history, under a
  new journal that already holds the replayed edits before the old folder goes. A panic in
  a command is caught: nothing changes (the model edits a copy), a recovery snapshot is
  written, a notice says so, and editing goes on.
- Tests: commands round-trip through text; random keyboard sessions replay to the same
  song and undo history; journal lines with damage; recovery with its undo history,
  saved/closed sessions, a damaged journal end and a lost base (snapshot fallback); a test
  process killed with SIGKILL mid-edit is recovered up to everything it reported on disk
  (`written()`); in the app, restore with undo, Later and Discard, a normal exit, and an
  injected command panic. The process and app recovery tests passed 8 runs in a row.
  Tests: 357 (+11).
- Open (docs/gaps.md #7): view state is not restored; a crash within milliseconds of an
  edit loses that edit; snapshots clone the song on the UI thread; layout panics are
  contained but not injection-tested.

## 2026-11-02 — CI: install system dependencies (ALSA, software Vulkan)

Landed:
- `.github/workflows/ci.yml`: an "Install system dependencies" step
  (`apt-get install libasound2-dev pkg-config libvulkan1 mesa-vulkan-drivers`) before the Rust
  steps. `cpal`/`midir` pull in `alsa-sys`, which fails with "system library `alsa` was not
  found" on a bare Ubuntu runner. The snapshot test in `gscore-app` renders through wgpu, and a
  runner without a GPU has no adapter ("Failed to create render state … No adapter found");
  lavapipe (`mesa-vulkan-drivers`) is a CPU Vulkan adapter, which `egui_kittest` prefers.
- README: the Linux build note now names the Vulkan driver needed for headless snapshot tests.
- Open: the committed snapshots were generated on the owner's GPU; if lavapipe's output differs
  beyond kittest's default threshold (0.6, 0 failed pixels), regenerate them on the CI renderer.

## 2026-11-02 — M13 final: merge with M11, live mix without stale values

- Merged main (M11): the ten track commands have `Command::to_line`; track names are
  percent-encoded after `%%`. The journal records `Recorded` entries; a slider change merged
  into the undo step before is journaled as `merge <command>`, so replay rebuilds the same
  undo history. `Editor::execute_with_merge` reports whether it merged.
- Playback: a volume or pan change still goes to the player at once, and the sequence is
  then rebuilt and swapped in at the current position (`Scheduler::replace`,
  `Command::Replace`, `Player::replace`): no stop, no silence. Playing from the start,
  seeking and the borrowed channels of bends and slides now use the new values (before,
  the sequence's tick-0 setup restored the old mix). `resync_mix` is gone.
- Tests: track commands and odd names round-trip through text; replace keeps position in
  the scheduler and plays on without silence in the transport. Tests: 382.
- Open: an app-level test that checks the rebuilt sequence after a slider change; files
  over 400 lines (`gpif_import/mod.rs`, `gpif_export/mod.rs`, `model/edit/mod.rs`,
  `tab_editor.rs`, `sequence.rs`) and a line-count check in `cargo xtask layers`.

## 2026-11-02 — M15: the track panel

Landed:
- `gscore-model`: `Song::set_track_program`, `set_track_drums`, `set_track_capo`,
  `set_track_strings` (with `check_track_strings`, the same check commands use for
  `enabled`), `set_track_string_pitch`. Each is transactional and returns a `Change` that
  restores the old program, drum flag and instrument type/name, capo, string list, tuning
  label and the string's old pitch. Shrinking a track is refused while notes sit on the
  strings that would go; growing it adds lower strings in fourths (6 -> 7 gives the
  usual B standard). An explicit tuning clears the file's tuning name. `Pitch::parse`
  reads note names (`E2`, `C#4`, `Bb3`) or MIDI numbers; `program_name` gives the 128
  General MIDI names. `differences` now compares the capo.
- `gscore-gp`: GP6/GP7 `CapoFret` is read (track properties, else staff properties) and
  written (`docs/guitarpro.md` change 26). A GP7 file's capo used to be dropped on import.
- `gscore-edit`: `set_track_program`, `set_track_drums`, `set_track_capo`,
  `set_track_strings`, `set_track_string_pitch`, each with metadata, parsing, `to_line`
  (so the recovery journal replays them) and an `enabled` that matches `apply`. A comment
  now starts at a `#` that begins a word, so `pitch=C#4` keeps its sharp. New
  `rebuilds_playback` (mute, solo, drum kit, capo, strings, tuning) and
  `changes_track_display` (name, color) tell the app what playback must do. Consecutive
  color changes on one track merge into one undo step, like a slider drag.
- `gscore-app`: the right track panel (shown by default; View -> Track panel; a track
  row's "Edit track…" opens it): name, instrument kind, General MIDI program (the
  SoundFont preset the built-in synth plays), drum kit, color, string count, capo and each
  string's open pitch. Changes go through the commands above. A program change is sent to
  the player at once and the sequence is swapped in at the current position; mute, solo,
  the drum kit, capo and tuning rebuild in place, so they are heard without a stop; a
  rename or color change leaves playback alone. `gscore-layout` keys the measure cache on
  the capo too.
- Tests: 395 (+13): model edits and their undo/refusals, pitch parsing, capo reading and
  writing, command round trips through text, the new commands in `every_track_command`
  and in the random property sequences, shrink refusal, save/reload of the new settings,
  and the panel driven through its accessibility tree (program, drums, capo, string count,
  rename, View toggle, row menu). The two page snapshots were regenerated because the
  panel is now visible.
- Open: the program picker lists all 128 names without a search field; the "SoundFont
  preset" is the General MIDI program (a per-track SF2 bank beyond it is not stored); the
  panel has no volume/pan (the song grid does); the instrument type/name cannot be edited.

## 2026-11-02 — M12: app shell, settings, playback follow

Landed:
- `gscore-session`: `settings.json` in the platform config directory (XDG, macOS
  Application Support, Windows APPDATA), read and written with `serde_json` (1 MiB cap,
  temporary file and rename, a broken file falls back to the defaults with a problem for
  the user). `Journal::start_with_options`/`RecoveryOptions` set the snapshot interval and
  entry count (zero seconds: snapshots only by edit count). `open_with_progress` reads in
  64 KiB chunks, reports bytes read and checks a cancel flag between chunks and after the
  import (`OpenError::Cancelled`).
- `gscore-model`: `SongInfo`/`SongTextField`, `Song::set_text` and `set_tempo` with limits
  (4096 characters per field, 64 KiB of notes, 1-1000 BPM) and exact undo.
- `gscore-edit`: `set_song_text` and `set_tempo` commands (fields by id, text
  percent-encoded, `to_line` round trips). One Apply in the settings window is one undo
  step: the app breaks the merge before the batch and the batch commands continue each
  other. `break_merge` now consumes its barrier even when the next command does not ask to
  merge (found by the batch test). `changes_display_only` covers the song text and
  `rebuilds_playback` the tempo.
- `gscore-play`: `Sequence::beat_at` (the inverse of `tick_of`) maps a played tick back to
  a beat of a track and voice, repeats included.
- App: a settings window with Song, Audio and MIDI, and Editor tabs. The Song tab edits a
  `SongInfo` draft and applies the changed fields as commands (undoable); Audio offers the
  synthesizer/SoundFont and MIDI ports; Editor offers the theme, a keymap file (choose or
  reload, with the problem shown) and the autosave interval. Settings persist the theme,
  zoom, panel visibility and sizes, window rectangle, recent files, keymap path, autosave
  interval and audio output, and are restored at startup (the window size and position go
  to `eframe`); they are saved on exit and after changes. File > Open Recent, drag and
  drop to open, and `ctrl+=`/`ctrl++`/`ctrl+-`/`ctrl+0` zoom actions. Opening shows a
  progress bar (read share, then "Importing…") with Cancel (the result is dropped); a
  loading SoundFont shows an animated bar with Cancel. The cursor now follows the beat
  being played on the shown track (repeats included), the page scrolls with it, and
  pausing or stopping leaves it on the last played beat. Spacing, rounding and text sizes
  are set once for both themes; the toolbar uses our own painter-drawn icons with key
  hints (no third-party icon assets).
- Tests: 416 (+21): settings JSON round trip, missing/broken/oversized files; journal
  snapshot options; open progress and cancellation; song text/tempo commands and their
  merging; the settings window editing the title with undo; recent files; zoom keys;
  settings restoring the view; the sequence's tick-to-beat mapping; snapshots regenerated
  for the new toolbar and style.
- Open: the left panel is M14's edit palette. The autosave interval applies to documents
  opened afterwards. A Cancel during the import itself is honoured when the import ends
  (the importer has no cancellation hook). Clicking during playback does not seek. The
  settings window has no Page layout tab yet (M17). Settings are saved on exit and after
  choices, not after every frame.

## 2026-11-02 — M14: edit palette, selection, effects

Landed:
- `gscore-model`: rests (`set_beat_rest`), ties (`set_note_tie`), dead notes
  (`set_note_dead`), a measure's tempo, time signature, key signature and section marker,
  and a `Change::batch` that undoes several changes in reverse. Time/key signatures change
  the header and every track's copy; a tempo on measure 1 also sets the song's initial
  tempo. Header edits touch every measure's layout stamp. Voice 2 can be created by
  inserting its first beat (`InsertBeat` fills a missing voice up to voice 2).
  Full effect editing: `BendPoint`/`BendShape`/`WhammyShape` with all points,
  `NoteEffect` and `BeatEffect` setters on a note or beat, and `bend_shape()`/
  `whammy_shape()` accessors; each edit stores the previous effect for exact undo. The
  model tests cover every variant, invalid shapes and one-effect-at-a-time toggling.
- `gscore-edit`: commands `set_beat_rest`, `set_tie`, `set_dead_note`,
  `set_measure_tempo`, `set_time_signature`, `set_key_signature`, `set_section`,
  `clear_section`, `set_note_effect`, `set_beat_effect` and `batch` (an edit made of
  edits). Effect values have a command-line form (`effect=bend value=up:0:0,6:4,12:0`,
  `effect=accent value=heavy`), and a batch is a list of percent-encoded command lines,
  bounded to 32 nesting levels. `Selection` (beats in one measure/voice, or measures
  across every track) with copy, cut and paste: copying reads beats, durations, ties,
  notes, all note/beat effects, time signature, tempo and section; pasting generates
  commands and runs them as one batch, so it is one undo step and one journal line. The
  Copy/Cut/Paste actions are wired, and `TabEditor::set_entry_value`/`undo_depth` are
  exposed.
- App: the left edit palette (durations whole to 64th, dots, tuplets, rest, tie, dead,
  voice 1/2, measure tempo/time/key/section, and the effects matrix with bend and whammy
  point editors and a curve preview). Dragging on the page selects a beat range and paints
  it; grid measure selections are mirrored into the editor for copy/paste; playback can
  start at the selection's first note. The palette applies one command or one batch per
  click, so undo works as expected.
- Tests: 458 (+42): every new model edit and effect, command round trips and adversarial
  effects, batch undo, voice creation, selection copy/cut/paste (including copied effects),
  the palette (length, effect, tempo, voice), grid selection sharing, and the regenerated
  page snapshots with the left panel.
- Open: grace notes, chord diagrams and lyrics are not editable; measure paste only inside
  the existing song (insert measures first); a beat selection stays in one measure/voice
  (measure ranges cover tracks); the bend/whammy editors are point rows with a preview, not
  freehand dragging; playback still follows a bend's highest point, not its full shape;
  `dump` and the save comparison still summarize bends. See `docs/gaps.md` #4, #14.

## 2026-11-02 — M12/M14 review

Fixed after reviewing M12 and M14:
- The initial tempo (settings window, `set_tempo`) now also changes measure 1's tempo when
  it has one. GP6 and later files store the initial tempo there, and playback takes the
  measure's, so changing the song tempo had no audible effect until the file was saved and
  opened again. Undo restores both.
- Undoing a time signature gives every measure a new layout stamp, like the edit does (a
  cache keyed on stamps could show the undone signature on measure 1; the app was saved by
  dropping all layouts).
- Measure paste: tempo, time signature and section are set once per measure instead of
  once per track; voice 2 is pasted (created when the target lacks it) and target voices
  the copy lacks are cleared, so a pasted measure matches the copy; placeholder beats are
  no longer turned into rests; pasted notes carry only the effects they have (shorter
  journal lines). The paste code moved to `selection/paste.rs`.
- The edit palette re-reads its tempo, signature, key and section fields after every edit
  and undo and when another song opens (it read them only when the cursor changed measure,
  so undo or opening a file left stale values).
- Tests: model tempo/measure-1 and signature-undo stamps; a measure paste with voice 2 on
  two tracks; the palette's tempo field following undo/redo.
- Still open from the review: several files are above the 400-line soft limit
  (`gscore-model` `effects.rs` 826 and `edit/mod.rs` 773, `gscore-edit` `command.rs` 888,
  `parse.rs` 698, `info.rs` 662, the app's `edit_effects.rs` 699 and `edit_palette.rs`
  534); M14 lacks grace-note editing and M12 the Page layout tab (both in `docs/gaps.md`).

## 2026-11-02 — Selection UX

Owner feedback on the M14 selection, done:
- `Selection::Beats` is a range between two `BeatSpot`s (measure and beat) in one track
  and voice, so it crosses bar lines. `Selection::positions` lists the selected beats
  (placeholders left out); cut, clear, the palette and the page all use it.
- New actions `select_left`/`select_right` (Shift+Left/Right: the cursor moves one beat,
  never adding beats, and the selection follows it from where it started) and
  `select_beat` (Shift+Up/Down: the cursor's whole beat). Plain cursor moves end the
  selection. With a selection, `delete_beat` (Delete) and `clear_note` clear every selected
  note as one undo step and keep the beats; without one they work as before.
- Copying beats keeps them grouped by measure; pasting puts each group into the next
  measure from the cursor's (refused, unchanged, when the song is too short).
- Page: the selection is one solid band per system line (from the first selected column
  to the last; measure selections cover whole measures) instead of a box per column with
  gaps. A drag selects across bar and system lines in the voice it started in
  (`Places::hit_in_voice`), and the cursor follows it. While playing, only the moving
  column band is drawn, not the box around the string. A selection other than a measure
  range clears the grid's measure highlight.
- Tests: keyboard selection across bar lines, whole-beat selection and clearing, a bar-line
  copy/paste and cut (`test_selection.rs`); the band merging (`page.rs`); Shift+arrows in
  the app ending a grid selection.
- Open: the selection is not drawn in the song grid unless it is a measure range; a beat
  selection cannot span tracks.

## 2026-11-02 — Track panel and playback UX

Owner feedback, done:
- Clicking a beat on the page while playing continues playback from there
  (`Playback::jump_to`; the player silences sounding notes when it seeks).
- Track panel: strings are tuned by note name (a list from C0 to B7 plus semitone − and +
  buttons; no MIDI numbers shown); 16 preset colors as swatches next to the custom
  picker; a "Show" choice of notes and tabs, tabs only, or notes only; a top bar to add a
  track after the shown one, move it up or down, and mute or solo all tracks (one undo
  step, toggling back when all are already muted or soloed); a master row at the bottom
  with a volume slider (double-click: 100 %) and a pan knob.
- With a selection, the selection band is one darker shade everywhere; the cursor's own
  column band is not drawn inside it (it made one column darker than the rest).
- Model: `TrackStaves` (`TrackView::staves`, `Song::set_track_staves`, which lays out
  every measure again) and `MasterMix` (`Song::master_mix`, `set_master_volume`,
  `set_master_pan`, `played_sound` = a track's sound with the master applied, used by the
  sequence and the live mixer). Both are compared by `settings_differences`.
  `MASTER_UNITY` (97) is the neutral master: every GP7 test file stores Guitar Pro's
  untouched 0.76, so a plain linear reading would have played every file a quarter
  quieter than before.
- Commands `set_track_staves`, `set_master_volume`, `set_master_pan` (master drags merge
  into one undo step; the app sends every track's sound at once).
- `gscore-gp`: master volume/pan (`I01_VolumeAndPan`) and GP7's `PartConfiguration`
  (per-track staves) are read and written (`docs/guitarpro.md` 31, 32).
- Layout: per-track staves; "notes only" draws the notation staff alone (no tab staff,
  fret numbers or tab ornaments) and puts the cursor rows on it. The default output is
  unchanged (golden SVGs).
- Tests: `test_track_panel.rs` (presets, staves, note-name tuning, the bar, the master
  row, jumping while playing when a MIDI Through port exists), `test_staves.rs`, model
  `mix_tests`, `part_configuration` and `display_tests` in `gscore-gp`, command and parse
  cases. The page snapshots were regenerated for the new panel.
- Open: the master row has no number readout (its tooltip shows the percentage); the
  track panel's top bar has no "delete track" (the grid's row menu has it); dragging on
  the page during playback selects without moving playback.

Corrected the same day (owner): the action bar and the master row belong to the song grid,
not the track panel. The grid's header cell now holds add, move up/down, and mute/solo all
(above the rows' own M/S buttons), and a Master row under the last track holds the master
volume and pan (lined up with the rows' sliders and knobs). Inside a selection the cursor's
box is hidden as well. A click or measure selection in the grid during playback jumps there
too. Tests: `a_click_in_the_song_grid_while_playing_jumps_there`; snapshots regenerated.

## 2026-11-03 — M16 app design (first pass, with owner feedback)

Owner: the score looks good, the app around it looks amateur; adapt ideas from MuseScore 4
(not its code or icons). M16 is now "App design"; bundled SoundFonts moved to M19 and the
release to M20 (`AGENTS.md`).

Landed:
- Design tokens (`app/style.rs`): one soft-charcoal palette for all chrome (white text and
  icons), whatever the score theme; the light/dark choice now only changes the page
  ("Light score"/"Dark score"). The page floats on a canvas with a shadow.
- The window has no system frame: our own title bar with the menus, the song title
  centered and minimize/maximize/close (double click maximizes; dragging moves; resize
  edges replace the frame's).
- Toolbar in groups: file, edit, zoom; transport with position ("3 / 12") and tempo; the
  three panel toggles (palette, song grid, track panel); settings. The output picker and
  the track picker are gone from it (settings window and song grid have them).
- Panels have no title bars; their content sits in collapsible titled sections.
- Edit palette: tiles with music-font glyphs (Bravura) or drawn signs for note values,
  dots, tuplets, rest, tie and every technique; a chosen tile is lit, a second click
  turns it off. Every tile is an action with a description and its keys in the tooltip;
  keys run the same code as a click. Defaults: ctrl+1..7 note values, "." dot, r rest,
  l tie, x dead, g ghost, h hammer, s/S shift/legato slide, b bend, v/V vibrato/wide,
  a/A accent/heavy, p palm mute, i let ring, n natural harmonic, d/u pick down/up, D/U
  brush down/up, t tapping, < fade in, w whammy (all rebindable in the keymap file).
- Track panel: the first line is the track's color, its instrument icon and its name
  (click to rename in place; Enter renames, Escape cancels); sections Instrument,
  Appearance and Tuning as property sheets.
- Song grid: square cells without gaps, rows in line with the track column, the master
  row as the last line; instrument icons (guitar, bass, drums, keyboard, voice, other;
  from the drum flag, the General MIDI program, then the GP7 instrument name) replace the
  "STR"/"DRM" tags; the panel opens tall enough for eight tracks and the master row.
- Tests: palette keys work like the tiles; the in-place rename (Enter and Escape); the
  regenerated page snapshots.
- Open: the bass icon is close to the guitar's; the bend and whammy editors are still
  point rows; the settings window is not restyled yet; a key table for the new actions
  is not in `AGENTS.md` (the owner's spec); on Wayland the compositor decides whether an
  undecorated window can be moved by our title bar (it uses the standard request).

Owner feedback round (same day):
- Fixed: a second click on a technique tile did not remove it when the cursor's string
  had no note (the tile's state came from the cursor's note only, while the click acts on
  every note of the selected beats). A technique now counts as on when every target note
  (or beat) has it, so the tile is lit and a click takes it off all of them. Test
  `a_second_click_on_a_technique_removes_it_even_off_the_cursor_string`.
- Toolbar: file, undo/redo and the panel toggles on the left; the player centered on the
  bar (its width from the frame before); zoom and the settings on the right.
- Song grid height: the panel is as tall as its tracks (up to eight) and the master row,
  follows when tracks are added or removed, and is never taller than all its rows. Once
  the user drags its edge, that height is kept and saved (new settings key
  `grid_height_chosen`; the old `grid_height` held any height and is ignored).

Player bar check (owner): next to the position, the player shows how full the bar is in
the cursor's track and voice, in beats of its time signature: "4 / 4" with a green check
mark when exactly full, "5.5 / 4" in red with a red cross when too long, and "3 / 4"
muted without an icon while not full yet (at most two decimals, so a triplet gives
"3.33"). During playback it shows the measure being played. Placeholders count as zero,
like the measure issues. Tests: `bar_fill` unit tests (full, short, 4.5 and 5.5 over,
rounding) and the app reporting full and overfull bars to screen readers.


## 2026-11-03 — M18 playback (first slice): time, metronome, count-in, live track switching

Landed:
- Time readout in the player: "00:00 / 03:45" (elapsed / total. `gscore-play` gained a
  lightweight `Timeline` (played measures plus the tempo map, no events), cached per
  revision in `Document`. With a measure selection in the song grid the total is the
  selected bars and the position is measured from the selection's first measure, clamped
  to its length; without a selection it is the whole song. Tempo changes on a beat are
  honoured, as in the sequencer.
- Metronome and count-in icons in the transport. The metronome adds a click track on the
  General MIDI percussion channel (high wood block accented on the first beat of each
  measure, low wood block on the other beats); toggling it while playing swaps the
  sequence in place, without a stop. The count-in plays one bar of clicks at the start
  measure's tempo and time signature, waits for it to finish, then starts the song; it is
  cancelled by Stop and turning the toggle off starts the song at once. Both buttons are
  plain toggles for now (not saved in the settings).
- Changing tracks from the song grid during playback no longer interrupts it: clicking a
  track row (or a cell in the measure that is already playing) only switches the shown
  track and keeps the beat; only clicking another measure seeks.
- Reopen the last file at startup (on by default, "Startup" in the Editor settings tab):
  the open file is remembered in the settings; a file given on the command line wins, and
  a missing last file simply starts with the blank screen. Recent files, zoom, panels and
  window state were already remembered.
- Tests: timeline totals and measure ranges, click generation and count-in length, the
  time formatter and selection-aware position, reopening the last file (present, missing
  and disabled), a track switch during synthesizer playback, and a count-in running into
  the song with the metronome toggled live. Plain `cargo test --workspace`, clippy and
  `cargo xtask layers` pass.
- The page snapshots were regenerated for the new transport controls; the owner should
  review `crates/gscore-app/tests/snapshots/steel_guitar_dadgad_{light,dark}.png`.
- Open: the metronome uses the SoundFont's/peripheral synth's drum kit (no separate click
  sound choice); count-in is one bar only; playback does not yet loop a selection; the
  time readout and transport toggles are not in the keymap.

Owner follow-up (same day):
- Player navigation: `|◀` top of the score, `◀◀` one bar back, `▶▶` one bar forward
  and `▶|` end of the score. They move the cursor; while playing or paused they also
  seek the player, so the music carries on. Jumping outside an active loop turns the loop
  off (an explicit jump is a way out of it). Tests cover the bar and end jumps.
- The count-in got its own icon (a metronome followed by a play sign); the former
  circular arrow now means "loop selection", enabled when measures are selected. Loop
  boundaries are polled from the UI (every 16 ms while looping, instead of the usual
  50 ms), so a loop restarts within a frame of its end; a selection change outside the
  loop is followed on the next frame, and losing the selection turns looping off.
  Test `looping_the_selected_measure_restarts_at_its_beginning`.
- The count-in now builds the song's sequence before the clicks: `Playback::position`
  reports where the song will start while the count-in runs, and `current_measure` and
  the time readout use it. Starting at measure 3 with a count-in shows 00:04 / total
  during the clicks instead of 00:00, then continues from that tick. The pending sequence
  is kept in step with mixer, metronome and output changes.
- Tests: navigation, loop wrap, count-in readout at the start measure; page snapshots
  regenerated again for the four extra transport icons (owner review still needed).

## 2026-11-03 — App shell follow-up: settings, shortcuts, history, new song

- Time readout bug fixed: a leftover measure selection clamped the clock to the selection
  end while playback continued, so the seconds froze. While playing, the selection time is
  now only shown while playback is inside it; outside it the whole song's time takes over.
- Settings moved from Edit to the bottom of the File menu. The window opens centered at
  780×580 and has the app's own title bar: same chrome color and 34 px height, with a close
  button; it can be dragged and resized.
- Settings tabs are Song, Editor, Audio I/O (last). The Audio I/O tab offers every
  SoundFont found in the usual places in a dropdown when the synthesizer is selected
  (`gscore-audio`'s new `available_soundfonts`), plus the file chooser.
- Keymap editing no longer points at a text file. Settings → Editor → "Edit shortcuts…"
  opens a table of every action with its keys; a row can be changed (the new key replaces
  the action's old keys), reverted with an icon when it differs from the built-in keymap,
  or reset for everything with "Reset all shortcuts". Bindings are stored in the settings
  JSON (`keymap_bindings`) and loaded in `main`; a broken stored binding falls back to the
  loaded keymap with a notice. `enter_fret` keeps its fixed digit keys.
- New `gscore-edit` APIs: `Keymap::bindings/from_bindings/bind/unbind/revert_action`,
  `Editor::undo_commands/redo_commands/redo_depth` and the corresponding `TabEditor`
  labels, so tables can be built without exposing the history internals.
- History window (Edit → History…): every undoable change as a row, with "Undo" and, for
  undone rows, "Redo"; clicking one steps to that point. This is the same history that is
  written to the recovery journal.
- New song: File → New and a page-with-plus icon at the start of the toolbar create a
  one-track, one-measure 4/4 score (`Song::new_score`, model test). Save asks for a real
  `.gp` path because the placeholder is not writable in place.
- File icons: the open folder was redrawn, save is now a floppy disk, and the new-song
  icon was added.
- Tests: new song, Audio I/O tab, shortcut change/revert, history rows, keymap APIs,
  soundfont discovery, and the time-readout selection fallback. Snapshots regenerated for
  the new toolbar icons; settings window visually checked with
  `cargo run -p gscore-app --example snapshot -- <file> out.png --settings`.
- Open: the history window shows the in-memory undo history (bounded by memory), not the
  whole on-disk journal; switching a shortcut to one key replaces alternate defaults for
  that action; available SoundFonts are rescanned while the Audio I/O tab is open.

## 2026-11-03 — History details, shortcut capture, palette and mixer polish

- History: a clock icon next to Settings opens the same window. Rows are now newest first,
  with a UTC time (`HH:MM:SS`), the command label, its arguments (`track 2, measure 3,
  string 5, fret 7`, decoded text parameters, ...) and done/undone state. `gscore-edit`
  assigns stable ids to history entries (`Editor::undo_history/redo_history`,
  `Command::details`); the app timestamps ids with `SystemTime`, so undo/redo and history
  trimming keep their times without a clock in the core crates.
- Audio I/O: the SoundFont file chooser sits next to the dropdown and the full path is
  shown directly underneath. SoundFont controls are hidden while MIDI is selected, the
  MIDI port controls are hidden while the synthesizer is selected, and the input section
  now has an "Input" heading.
- Shortcuts: clicking Change captures the next key combination (esc cancels). A used
  combination names the action that owns it and offers "Replace it" or "Cancel"; no
  hand-written key text is needed. Tests cover a fresh binding and a replaced conflict.
- Edit palette: the Measure section moved below Techniques and Strokes and beat. A section
  title is typed in place and applied with Enter (empty Enter clears it, Escape reverts);
  the Set/Clear buttons are gone.
- Song grid: the track column and mixer band are wider, the volume slider no longer
  reaches the pan knob, and hovering/dragging the volume shows its level in decibels
  (0 = −∞ dB; 127 = 0.0 dB).
- Tests: command details, history timestamps, in-place section, shortcut capture/conflict,
  volume-to-dB; page snapshots regenerated for the history icon and wider grid.

## 2026-11-03 — New-track popup, mixer view, manager layout polish

- New track (the grid header or a track's "Add track below") now opens a small centered
  popup: name (prefilled with a free "Track N"), instrument symbol (guitar, bass, drums,
  keyboard, voice, other) and a preset/custom color. Create writes insert, rename, color,
  drums and program as one batch, so one undo removes the whole track. Enter creates,
  Escape cancels, and the new track is shown. Tests cover name, color and program.
- Instrument symbols are now authoritative for the string family (programs 40-51 show the
  generic symbol), so the "Other" choice can stick even when the copied preset is a guitar.
- Mixer view: the first icon in the grid header switches the bottom panel between the
  measure grid and DAW-style strips. Each track has a vertical fader, an activity meter, a
  pan knob, a greyed-out reverb knob (playback does not apply reverb yet), and underneath
  the fader its name, instrument preset and track-color bar. A master strip follows the
  tracks. The mode and its panel height are remembered.
- The meters are an estimate from recent note velocities scaled by the track's volume and
  decaying over a quarter note, not measured audio; this works for MIDI output too (the
  tooltip says so). The faders send the same live volume/pan commands as the grid.
- Mixer band geometry is now shared by track rows and the master row: wider track column,
  a dedicated decibel column, more space before the pan knob, and a dB readout that is
  always visible and highlighted while dragged (master included).
- Tests: new-track popup contents and results, mixer toggle and strip controls; snapshots
  regenerated for the wider grid and the mixer icon.

## 2026-11-03 — Mixer follow-up: mute/solo, dB scale, editable levels, activity LEDs

- Mixer strips now have M and S buttons (same commands as the grid) and a fixed bottom
  block: editable dB readout, mute/solo, name, preset and track-color bar. The block is
  pinned to the strip bottom with absolute rectangles, so names and colors no longer
  overlap the fader area.
- The fader and meter share exactly the same top and bottom. A decibel scale sits between
  them: 0, -6, -12 and -24 dB, positioned at the volume values that produce those levels
  (the fader travel is linear in MIDI volume, so the labels bunch towards the bottom).
- Clicking the dB readout turns it into a text field; Enter or focus loss applies the value
  (0 dB = 127, -60 dB or -inf = 0), Escape cancels. Invalid text leaves the level alone.
  The same works for the master strip.
- Grid rows (and the master row) now have a small green activity bar to the right of the
  pan knob: the compact form of the mixer meter, fed by the same estimated note activity.
- Tests: mixer mute/solo, typing -12 dB (maps to MIDI volume 32), activity indicators in
  the grid. Page snapshots regenerated for the wider grid; the mixer and new-track popup
  were rendered offscreen with the snapshot example.

## 2026-11-03 — Mixer fader layout, L/R meters, master mute/solo, activity LEDs

- The fader is now drawn by hand instead of egui's slider: a thin rail, a white rectangular
  thumb and a logarithmic travel from 0 dB (top) to -60 dB/silence (bottom). Its height is
  exactly the meter's height, and pointer dragging and Up/Down/Page/Home/End work.
- Two scales flank the strip: gain (0 to -54 dB, then -∞) left of the fader, meter level
  (0 to -60 dBFS) right of the L/R meter bars. Both use the same vertical positions.
- The meter is two segments (left and right), balanced by the pan knob and filled in dBFS,
  so the scale is accurate.
- Mute/solo-all moved out of the mixer header and into the master strip; per-track M/S are
  on every track strip. Pan and reverb knobs are centered in the column.
- Strips are wider and taller, with more padding around the fader, scales and bottom block.
  Names/presets/color bars stay pinned to the bottom.
- Grid rows and the master row now show a small green activity bar to the right of the pan
  knob, using the same estimated note-activity level as the mixer meters. Tests cover the
  mixer controls, M/S-all, and the decibel entry.

## 2026-11-03 — Mixer fader taper and +12 dB headroom

- The fader now uses a real logarithmic taper: fader position is the cube root of the MIDI
  volume, so 0 dB (at MIDI volume 32) sits about a third of the way down and the top is
  +12 dB. The gain labels are therefore *not* equally spaced; the meter stays on its
  equal-distance dBFS scale (0 to -60), matching the reference.
- The fader thumb, gain scale, L/R meter and meter scale all share the same top and bottom;
  the fader is drawn by hand (rail, white thumb, focus ring), with pointer dragging and
  keyboard control.
- The dB readout and grid volume labels now use the same +12 dB headroom mapping
  (`+12.0 dB` at volume 127, `0.0 dB` at 32, `−∞` at 0). Typing a dB value uses the same
  mapping; tests cover the new headroom.
- Mute/solo-all moved out of the grid header into the master row too, so the header is only
  view/add/move; the master strip in the mixer has the same pair.

## 2026-11-03 — Review of the app shell follow-up (commit cfc71a1)

Bugs fixed:
- Stored shortcuts froze the whole keymap: every settings save wrote all bindings, so a
  built-in shortcut added in a later version would never reach that user, and a keymap
  file was replaced wholesale. The settings now keep only the actions that differ from the
  built-in keymap (`shortcut_overrides`: action id plus all of its keys, empty when it lost
  them), applied over the built-in keymap or the keymap file (`Keymap::overrides` and
  `Keymap::apply_overrides` replace `Keymap::from_bindings`). An unusable stored entry is
  skipped and reported; the others still apply. The one-day-old `keymap_bindings` key is
  no longer read.
- `Keymap::bind(enter_fret, digit)` dropped the other nine digit keys. `enter_fret` keeps
  its fixed digits; `bind` refuses it (the shortcut editor never offered it).
- The Audio I/O tab listed the SoundFont folders on the UI thread on every frame
  (AGENTS.md "Never block the UI thread"). The search now runs on a background thread when
  the tab is first shown and on "Search again"; the dropdown says "Searching…" meanwhile.
- `available_soundfonts_in` moved the first file it happened to find to the top when no
  SoundFont was configured, so the list was not sorted (test extended).
- Keys reached the score while the new-track popup was open and its name field had lost
  focus (Escape/Enter or a shortcut could edit the song behind the popup).

## 2026-11-03 — Fader numbers that match what is heard

The previous mapping (0 dB at MIDI volume 32, +12 dB at 127) was not what playback does.
rustysynth, like the General MIDI recommendation, plays channel volume `v` at an amplitude
of `(v / 127)²`, and the master multiplies every track's volume by `master / 97`.

- Track gain is now `40·log10(v / 127)`: 127 is 0 dB (the top), 64 is −11.9 dB, 32 is
  −23.9 dB. Master gain is `40·log10(master / 97)`: 0 dB at the default, +4.7 dB at the
  top (until a track hits the MIDI maximum). The grid rows, the master row (which used the
  track mapping before), the strips' readouts, tooltips and typed dB values all use it;
  typing accepts `−` and a trailing `dB`, `-inf` is silence, and values above the top clamp.
- Fader travel is `(v / 127)^(2/3)`, i.e. `10^(dB / 60)` below the top: −6 dB at 79%,
  −12 dB at 63%, −24 dB at 40%, −60 dB at 10%. The gain scale is computed per fader (the
  master's 0 dB sits below its top) and thins out labels that would overlap.
- The meters used the volume linearly, and the master meter applied the master a second
  time (the sequence already plays the mixed volumes). The level estimate now uses the
  channel gain `(v / 127)²` of the played volume, and the master shows the loudest track.
- Tests: dB mapping both ways for tracks and master, fader taper round trip, typing −12 dB
  gives volume 64; page snapshots regenerated for the new readouts.

## 2026-11-03 — Master row: mute/solo in the tracks' column

- The master row placed mute/solo-all at fixed offsets from the left while the track rows
  place theirs from the right-hand mixer controls, so the two columns only met at one
  panel width. Both now use `mixer::mute_rect`/`solo_rect`. Test
  `the_master_rows_mute_and_solo_line_up_with_the_tracks`; snapshots regenerated.

## 2026-11-03 — Icons redrawn for their real size

The toolbar draws icons in a 14×10 box (10 px high). Rendered at that size, several from the
app-shell follow-up did not read: Open looked like a paper clip, Save was stretched to the
box's width (a battery rather than a floppy), New song's plus stuck out of the page, Grid's
cells became four dots, the metronome's weight was a blob and Count-in squeezed it to 7 px,
and the arc arrows of Loop and Revert ended in smeared strokes.

- Open is an open folder (back with tab, front flap tilted forward); Save a square floppy
  with a cut corner, shutter and label; New song a page with a folded corner and a plus on
  it; Grid a 2×2 table; the metronome has a thin pendulum with a small round weight, full
  height in Count-in too.
- Loop is two arrows chasing round a circle (repeat), Revert one arrow turning back; both
  use a new `paint_arc_arrow` with a filled head. Undo/redo shafts are level.
- Checked with an offscreen icon sheet at 2× (large and real size); page snapshots
  regenerated.

## 2026-11-03 — Toolbar: the player no longer covers the panel toggles

- The player was centered on the whole bar regardless of the side groups; with the new-song
  button the left group reached under it at 1000 px and the track-panel toggle was hidden.
  It stays centered where there is room and otherwise moves clear of both groups (the left
  one wins if even that does not fit). Test `the_player_never_covers_the_toolbar_groups`.

## 2026-11-03 — History window: full-width table, resizable columns, go to a point

- The table spans the window. Time, Change and State can be resized by dragging the
  handle at their right edge in the header (resize cursor, highlighted while dragged);
  Details takes the rest (at least 80 px) and shows its full text on hover when cut off.
  Built from plain egui (no `egui_extras`, which AGENTS.md says to ask about first).
- Undo/Redo buttons are replaced by a leading clock icon, "Go to this point in time": it
  brings the song to the state right after that change, undoing or redoing as many steps
  as needed. The current point is highlighted and marked, and a last "Start" row goes back
  to the song as opened.
- Rows are ordered by the undo/redo stacks (undone changes first, furthest first), not by
  sorting timestamps, and drawn with `show_rows`, so a long history only draws the visible
  rows. The snapshot example's `--history` now makes three edits and undoes one.
- Tests: column geometry, go to an earlier change, back to the undone one, and the start.

## 2026-11-03 — Player time readout: accuracy and cost

Checked against playback. The total and the playing position were right (repeats, tempo
changes on bars and beats, exact microsecond integration). Fixed:
- While stopped the clock said 00:00 while the measure readout showed the cursor's measure
  and Play starts at the cursor: with the cursor in bar 3 it read "00:00 · 3 / 9". The clock
  now shows the cursor's beat while stopped (`Timeline::tick_of`, sharing `beat_tick` with
  `Sequence::tick_of`, so both agree on where a beat lies). Test
  `while_stopped_the_clock_shows_where_play_starts`.
- The tempo readout only knew tempo changes on a measure; changes on a beat (which playback
  honours) were ignored. It is now the timeline's tempo at the shown tick
  (`TempoMap::bpm_at`, a binary search), which also replaces a per-frame walk back over the
  measures.
- Position and total come from the one cached timeline instead of mixing the sequence's
  tempo map (identical, but no reason for two sources or for cloning the sequence).

Cost per frame: the timeline is cached per revision; the readout does an Arc clone, two or
three `micros_at` walks over the tempo points, one `measure_range` scan of the played
measures and a binary search for the tempo, all negligible next to drawing.

## 2026-11-03 — Reverb and chorus sends per track

rustysynth has reverb and chorus built in and reads the per-channel sends (MIDI controllers
91 and 93); GP3-5 files keep both per channel. Only our side was missing.

- Model: `TrackSound::reverb/chorus` from the track's channel, `Song::set_track_reverb` and
  `set_track_chorus` (on the track's own channel, like volume and pan, so undo restores the
  shared channel). The save comparison checks both, so a format that drops them is reported.
- Commands `set_track_reverb` / `set_track_chorus` (parse, round trip, panic hunt, edit and
  undo tests); consecutive changes on one track merge into one undo step, like pan.
- Playback: `gscore_play::sound_events` is now the one builder of a channel's setup
  (program, volume, pan, reverb, chorus) for the sequence and for live changes in the app,
  which had its own copy. A knob is heard while it turns.
- GP7 has no per-track send (its reverb is an RSE effect), so the values are written as the
  track properties `GuitarScoreReverb` / `GuitarScoreChorus` (`<Number>`), only when
  non-zero: an untouched song is written exactly as before. Round-trip test in `gscore-gp`;
  `gscore-session`'s save test checks no loss is reported.
- Mixer strips: Pan, Reverb and Chorus knobs with captions (sends fill an arc up to their
  value; double-click sets 0). The master strip keeps only its pan.
- Sound change to note: every channel used to play with rustysynth's default reverb send of
  40, because no send was ever set. Now the file's value is sent, which is 0 (dry) for most
  songs, exactly as Guitar Pro stores and plays them.
- Test fix: `a_pitch_bend_reaches_the_port` stopped reading as soon as the player stopped,
  so a bend still queued in the MIDI port was missed under load; it now drains for 0.5 s.

## 2026-11-03 — Entered notes are heard

- Typing a fret plays that note at once with the track's sound (program, volume, pan,
  reverb, chorus, master mix; drums on the percussion channel), for 450 ms, at the note's
  sounding pitch (tuning, capo, harmonics) and velocity. It uses its own channel (15), and
  only while playback is stopped (entering a note stops playback anyway), so the song is
  never disturbed. Muted tracks stay silent.
- Only typed frets sound: undo, redo and paste that set frets do not.
- The first entered note starts the output if nothing is connected. While the
  synthesizer is still loading its SoundFont that one note is skipped rather than queued
  (it would sound late, cut short by its own note-off); the next ones are heard.
- Settings → Editor → "Play notes as they are entered" (on by default, remembered).
- Window title bars' close button now has an accessible label ("Close Settings", ...).
- Test `an_entered_fret_is_heard_and_undo_is_not` (real synthesizer with the test sine
  SoundFont; skipped without an audio device): pitch, note-off, silent undo, setting off.

## 2026-11-03 — Drum notation, dynamics, and marks in the score

Owner's request: drum notation, dynamics in the edit palette and the score, section names
(and the like) directly in the score.

- **Drum notation** (`gscore-layout/src/notation/drums.rs`): drum tracks were refused by
  layout. They are now written on a five-line percussion staff with the percussion clef,
  always (whatever the notation/tab switches say; a tab staff cannot show drum sounds).
  Each General MIDI drum sound has its usual place and notehead: bass drums in the bottom
  spaces, snare in the third space, toms rising from the floor tom to above the staff,
  closed/open hi-hat (cross/circled cross) and cymbals above, pedal hi-hat below; unknown
  sounds get a diamond on the middle line so they stay visible. Stems point up, no
  accidentals. Entry is unchanged: the "fret" is the MIDI note number. The layout cache
  keys on the drum flag, so toggling "Drum kit" lays the track out again. New glyphs:
  `unpitchedPercussionClef1`, `noteheadCircleX`, `noteheadDiamondBlack`,
  `noteheadTriangleUpBlack`, `repeatDot`, `dynamicPPP` ... `dynamicFFF`.
- **Dynamics**: model `Dynamic` (ppp to fff, Guitar Pro's velocities 15 ... 127),
  `BeatView::dynamic`, `NoteView::dynamic`, `Song::dynamic_at` (the beat's own, else the
  last earlier beat with notes in that voice, across barlines) and `Song::set_dynamic`
  (every note of the beat; undo restores each velocity). New notes take the dynamic in
  effect instead of always forte, as in Guitar Pro. The default is mezzo-forte: GP7 files
  store MF on every beat and Guitar Pro neither prints it nor starts new notes elsewhere.
  Command `set_dynamic` (parse, round trip, apply/undo, panic hunt); it rebuilds playback.
  Copy/paste carries a beat's dynamic. The save comparison now checks note dynamics.
  Palette: a "Dynamics" section with eight glyph tiles (the selected beats' mark
  highlighted) and the dynamic in effect at the cursor.
- **Marks in the score** (`gscore-layout/src/marks.rs`), decided in one pass over the
  track so measure layouts stay cacheable: framed bold section names, tempo changes
  (measure tempos and GP3-5 beat tempos, only where the tempo actually changes), repeat
  signs (thick-thin lines with dots on both staves, extra room so notes keep their
  distance), "x3" over an end repeat played more than twice, alternative-ending brackets
  ("1.", "2." with hooks), and dynamics under the notation staff (clear of low notes and
  the tab) or under the tab and its P.M./let ring rows. Rows above a system (endings,
  tempo, section) are only added where a system needs them, and push it down.
- Tests: drum map, drum layout (clef, heads, stems up, no fret numbers, cache), marks
  (sections, tempo, unchanged tempo not repeated, dynamics where they change incl. cache,
  repeats and endings), model dynamics, command and paste. Goldens unchanged (the test
  files are all mf). App snapshots updated: the palette's new Dynamics section (owner to
  review).
- Caught by `panic_hunt` before it shipped: `dynamic_at` walked back from the requested
  measure, so `set_fret measure=18446744073709551615` would loop practically forever. It
  now answers at once for positions outside the song (regression assertion in
  `new_notes_take_the_dynamic_in_effect`).
- Open: drum tab, custom drum maps and a feet voice; hairpins; D.C./D.S./coda; a
  dynamics key binding; the `fff` tile wraps to a second row at the default panel width.

## 2026-11-03 — Bend and whammy editors, score menus, window state, README

- **Bend and whammy editors** (`gscore-app/src/app/shape_editor.rs`): a graph with
  draggable square handles (snapped to position 0-12 and quarter tones, kept between their
  neighbours; click adds, right-click/double-click removes), value labels as bends are
  written (+½, +full) or in whole steps for the whammy (±6 steps, taller graph), built-in
  presets (bend: Bend, Bend/Release, Bend/Release/Bend, Pre-Bend, Pre-Bend/Release,
  Release; whammy: Dip, Dive, Release up, Inverted Dip, Return, Release down), user presets
  saved in the settings (`bend_presets`/`whammy_presets`, sanitized on load), a range menu,
  OK/Remove/Cancel. The bend and whammy tiles (now side by side) and their keys open the
  editor on the note's shape or a default (full-step bend, two-step dip); the windows are
  drawn outside the palette so they show with the section folded or the panel hidden.
- **Score drawing:** bends are a quarter ellipse entering a solid arrowhead at a right
  angle (new `DrawOp::Polygon`, in SVG and on screen); the whammy follows its real shape
  (`ColumnMarks::whammy` is now the `WhammyShape`) with the depth written above each
  turning point, no arrow. Goldens: `ornaments.svg` changed at both (owner to review).
- **Menus** (`page_menu.rs`): the score's right-click menu has Cut/Copy/Paste and Note
  (bend/whammy editors, techniques, slides and harmonics ticked when on, tie, rest, delete
  note, durations, beats), Bar (insert before/after, delete, select) and Track (edit,
  rename, add above/below, duplicate, move, delete) submenus; a right-click moves the
  cursor first unless it is inside the selection. The menu bar has the same Track, Bar
  and Note menus; Edit gained Cut/Copy/Paste. Grid actions run through one
  `apply_grid_action`. The page has the accessibility label "Score page".
- **Window:** the window state was never saved on Wayland (no position, so nothing was
  stored). `WindowState` now has an optional position and a `fullscreen` flag (old files
  load) and keeps the normal size while maximized; the app always opens maximized, or full
  screen if it was closed so. New action `toggle_fullscreen` (F11, View menu).
- **README** rewritten for newcomers: pitch, screenshots (`docs/media/`, rendered from the
  app offscreen), feature tour, honest status, keyboard table; build details kept below.
- Tests: shape editor (snap, move, insert, labels, presets valid, range), whammy turning
  points and labels, window state (Wayland, maximized, fullscreen, old settings), F11
  binding, UI tests for both editors and the menus. App snapshots updated (palette tiles,
  menu bar).
- Open: the whammy editor's "Return" shape is a guess (dive, hold, return); the
  full-screen switch and startup maximizing are not covered by headless tests; the
  screenshots need re-rendering when the UI changes.

## 2026-11-03 — Milestone M21 planned: pages, multi-track scores and print

- The owner made pages, multi-track systems and printing their own milestone (they were
  two bullets in M17): M21 in `AGENTS.md` section 9, next after M17, before the rest of
  M18 and before M19/M20. Scope: page setup (read from the GP file's `PageSetup`, which
  nothing used), pagination with headers/footers and breaks, multi-track systems with
  aligned columns, editing across staves, a page view, PDF/PNG/SVG export and print.
- Open for the owner (section 11): the PDF writer dependency (`pdf-writer` + `subsetter`
  recommended) and how printing works per platform. Needed: multi-track test files made
  with Guitar Pro, with their PDFs.
- Decided by the owner the same day: `pdf-writer` + `subsetter` for PDF, and PDF export
  before native printing. A multi-track test song from Guitar Pro is coming.

## 2026-11-03 — M21, first slice: pages and the page view

- **Layout** (`gscore-layout`): `LayoutOptions::page_height` (`on_pages()` sets A4; `None`
  keeps the endless page). Pages are stacked top to bottom with `PAGE_GAP` between them; a
  system that would run into the bottom margin (plus room for the page number and what
  hangs below the staff) starts the next page, and a page always takes at least one
  system. `Layout::pages` lists the page rectangles (one for the endless page), which the
  PDF export will cut by. "Page n/N" at the bottom of each page. The measure cache ignores
  the page height, so switching views lays out no measure again (tested). Goldens
  unchanged (they use the endless page).
- **App:** View › Page view / Endless view (remembered in the settings as `page_view`;
  pages by default, like Guitar Pro). Each page is its own sheet with a shadow on the
  canvas; the status bar shows "Page 2 of 5" for the cursor's page. Cursor, clicks,
  selection and scrolling work unchanged across pages (same coordinate space).
- Tests: `test_pages.rs` (pages stack with the gap, systems inside their page above the
  margin, no empty page, page numbers, endless page unchanged, cache kept on switching,
  odd page heights, a page too small for a system); a UI test for the View menu switch
  and the setting. App snapshots updated (the page is now a full A4 sheet).
- Bench (`cargo xtask bench`, release): within budget on the test files (edit and
  layout 0.023 ms on `ornaments.gp`); the owner's large files were not at hand. Paging adds
  one comparison per system.
- Open in M21: page setup from the GP file and a Page layout settings tab, running
  header and copyright footer, vertical justification, manual system/page breaks and bars
  per system, pages side by side when zoomed out, PDF export, multi-track systems (waiting
  for the owner's test song).

## 2026-11-03 — M21: page setup, breaks, header and footer, PDF export, pages side by side

- **Page setup in the file** (`gscore-gp`): GP7 keeps it in `Content/BinaryStylesheet`, a
  binary list of named values (u32 count; per entry a name, a type byte and the value:
  bool, i32, f32, text, color). New codec `io/gpx/stylesheet.rs` reads all 591 entries of
  Guitar Pro's file and writes them back byte for byte; the page size, orientation,
  margins, scale and the page-number, copyright and running-header switches go into
  `PageSetup` and back. A file without a style sheet gets one only when its page setup is
  not the default (only the page keys; not yet tried in Guitar Pro itself, owner to check).
  GP3-5 keep their own page setup (the running-header bit is masked off).
- **Line breaks**: GP7's per-track `<SystemsLayout>` (bars per line, "3 3 3 3") becomes a
  line break at the first bar of each line, and is written back from the breaks, so a
  Guitar Pro file is broken where Guitar Pro breaks it (the ornaments golden now matches
  Guitar Pro's PDF line for line). Page breaks (`LineBreak::Page`, a GuitarScore addition)
  and the bars-per-line limit are GuitarScore attributes on `<Track>`
  (`gsPageBreaks`, `gsBarsPerSystem`); GP5 keeps page breaks as line breaks.
- **Model and commands**: `PageSetup` (mm, percent scale; checked: paper 70-1000 mm,
  margins leave 40 mm, scale 50-200 %; odd file values are moved into range), `PaperSize`,
  `SystemBreak`; `set_page_setup`, `set_system_break` (alias `set_line_break`),
  `set_bars_per_system` (alias `set_bars_per_line`), all undoable, journaled, in the CLI,
  the property test and the panic hunt. The save check compares page setup, breaks and
  bars per line, so a format that loses them is reported before writing.
- **Layout**: options from the page setup (`with_page_setup`: paper, four margins, staff
  space by scale); line breaking honours forced breaks and the bar limit; placement in two
  passes: systems onto pages (page breaks, footer room, running-header room), then lines
  spread down every page that ended full (at most 6 spaces more per gap), then drawing.
  Page number at the bottom right as Guitar Pro does, copyright at the bottom of page 1,
  "title by artist" atop later pages.
- **PDF** (`gscore-layout/src/pdf`, `pdf-writer` 0.15 + `subsetter` 0.2 without default
  features, approved by the owner): one PDF page per layout page, vector drawing from the
  same `DrawOp`s, Bravura (CFF, CIDFontType0 via FontFile3/OpenType) and Noto Sans
  (TrueType, CIDFontType2) embedded as subsets with ToUnicode maps, title and producer in
  the document information. A small OpenType reader (cmap 4/12, hmtx, head, hhea) gives
  glyphs and advances, so no further dependency was needed. `pdfinfo`/`pdffonts`/
  `pdftotext` accept the output (A4, fonts embedded and subset, text extracted); the
  ornaments PDF is 88 KB and matches Guitar Pro's PDF of the file in its line breaks.
- **App**: Settings › Page layout (paper A4/Letter/A5/A3/custom, orientation, margins,
  score size, header and footer switches; one undo step; "Guitar Pro defaults"); Bar menu
  "Line break / Page break before this bar" (ticked when set); Track › Bars per line; File
  › Export PDF… (background job, written next to the target and moved into place); pages
  side by side when they fit the view's width (`canvas::Arrangement` maps layout and shown
  coordinates for painting, the cursor, the selection and clicks). `gscore-cli render`
  writes PDF for a `.pdf` output.
- Tests: style sheet codec (round trip, truncation, unknown types, clamping), GP7 page
  setup and breaks read and saved (`page_tests.rs`), model setters and undo, parse
  round trips and errors, line breaking with breaks and limits, PDF structure and errors,
  CLI render, the export job, the arrangement mapping, a UI test for the menus and the
  Page layout tab. Golden `ornaments.svg` changed (Guitar Pro's line breaks; owner to
  review); app snapshots updated (the song's margins, page number at the right).
- Bench: within budget on the test files (edit and layout 0.019 ms on `ornaments.gp`).
- Open in M21: multi-track systems and editing across staves (waiting for the owner's
  test song), native printing, PDF of several tracks; the minimal style sheet written for
  files that had none is untested in Guitar Pro.

## 2026-11-03 — Playback over empty bars, page turns while playing, page setup preview

- **The cursor follows over empty bars** (`Sequence::beat_at`): Guitar Pro stores a bar
  that was never filled without any voice, and `beat_at` gave up on a measure without the
  cursor's voice, so the cursor (and the view) stayed on the bar before until notes came
  again. It now lands on the bar's first beat; the page already had a place for it.
- **Page turns while playing** (`page.rs`, `cursor_scroll`): the view used to scroll the
  minimum to keep the playing beat visible, a line at a time. While playing it now stays
  put until the beat leaves the view and then puts that line at the top (with room for the
  bar numbers and section names above), so a whole screen of what follows shows; a jump
  back for a repeat does the same. Editing still scrolls just enough. Tested as a pure
  function; checked in the app with screenshots during fast playback.
- **Page setup preview**: Settings › Page layout shows the first page as the draft would
  print it (and "Page 1 of N"), before Apply; it is laid out again only when the draft, the
  track or the song changes, with its own layout cache. For it, the measure cache key now
  leaves out everything that only places measures on pages (paper, margins, staff size,
  header and footer): changing the page setup re-breaks lines without laying out a single
  measure again, in the preview and in the score.

## 2026-11-03 — Silent audition when no output can be started

- A typed fret no longer reports every output failure. The first entered note still starts
  the output, but if that fails (CI without ALSA MIDI, a machine with no MIDI port) the
  note is simply entered silently; only pressing Play reports the problem. Before, each
  typed note put "playback: MIDI support could not be initialized" in the notice bar,
  which also failed two editing tests on CI.
- Test `a_note_that_cannot_be_heard_does_not_interrupt_editing` (bogus MIDI port):
  the fret is entered and no notice appears.

## 2026-11-03 — Staff section in the edit palette; repeat and double-barline editing

- **Edit palette**: a new Staff section at the very top, with nine icon tiles that open
  small dialogs: tempo (a note with `=`), time signature (4/4), key signature, section
  marker, start repeat, end repeat (both open the repeats and endings dialog), double
  barline, line/page break, and bars per line. The old inline Measure property sheet is
  gone; tempo, time, key and section moved into the dialogs. The repeat and barline icons
  are drawn to fit the tile; the note and time-signature icons use their own faces. A tile
  lights up when the bar has the feature (section, repeat start/end, double bar, break,
  fixed bars). OK is one command per dialog, so one undo step.
- **Model and commands**: `Song::set_repeat` and `Song::set_double_bar` (both in the
  song-wide measure header), `Song::double_bar`, and the commands
  `set_repeat measure=.. start=.. end=.. endings=..` and
  `set_double_bar measure=.. double_bar=..` with info, parse round trip, undo, panic hunt
  and property coverage. `set_repeat` rebuilds playback; both drop the layout cache.
- **Layout**: `MeasureMarks` carries `double_bar`; a double barline is drawn as two thin
  lines. A final barline and an end repeat still win over it.
- Tests: model setters and undo, parser round trip, a UI test for the repeat, barline and
  break dialogs (and their undo), a new snapshot of the Repeats dialog; the two app
  snapshots changed for the new panel (owner to review).
- Open: D.C./D.S./coda, fermatas and free time; per-track double barlines (Guitar Pro's
  measure headers keep one per score, as repeats do).

## 2026-11-03 — Staff icons: endings, swing feel and fermatas

- **Icons**: the Staff section now has 12 tiles. Tempo is a note with `=`, the time
  signature is a stacked 4/4, start and end repeat are separate icons, alternative endings
  have their own bracket icon, and new tiles cover swing feel and fermatas. The repeat
  dialogs no longer contain the endings: start repeat, end repeat and alternative endings
  each open their own small dialog.
- **Feel**: `Feel` (straight, eighth/sixteenth triplet, dotted eighth/sixteenth) per
  measure, `Song::feel`/`set_feel` and `set_feel`/`set_triplet_feel`. GP3-5 keep the
  standard eighth/sixteenth triplet feel; GP7/GPX keep all five in the new `gsFeel`
  attribute on `<MasterBar>`. Playback and `.mid` export swing eighth/sixteenth notes
  (two-thirds/one-third) or dotted pairs (three-quarters/one-quarter) through a piecewise
  time warp per measure; an incomplete final pair stays straight so the bar end does not
  move. The cursor and the time readout use the same warp.
- **Fermata**: `Fermata` (very short to very long), `Song::fermata`/`set_fermata` and
  `set_fermata`; one per measure for now. Drawn above the staff (`Glyph::Fermata` and a
  new marks row). GP7/GPX keep it; GP3-5 do not, and the save check reports it.
- Tests: model setters and undo, GPIF feel round trip, a layout test for the fermata glyph
  and row, CLI round trip, the UI test for all new dialogs, snapshots for the feel and
  fermata dialogs. App snapshots updated.
- Open: fermata playback, fermatas below the staff, several fermatas per measure, and
  dotted feel in GP3-5.

## 2026-11-03 — Review of the staff editing commit; palette icons and staff dialogs

Review of fa7447c ("Improve staff editing"), found and fixed:

- **GP7 swing was lost on import and export.** The commit claimed GPIF has no triplet-feel
  field and wrote a private `gsFeel` attribute instead. GPIF has `<TripletFeel>` on
  `<MasterBar>`; real Guitar Pro files with swing opened straight, and files saved here
  carried swing only for GuitarScore. `gsFeel` is gone; `TripletFeel` (gscore-gp) and
  `Feel` gained the dotted and Scottish feels Guitar Pro has (seven in all), with
  Scottish (short-long, 1:3) timing in playback. GP3/4 wrote "swing" for any feel; now
  only for eighth-note swing.
- **The feel was not shown on the score.** The measure where the feel changes now gets
  Guitar Pro's mark above the staff ("♫ = ⌐3¬♩♪", dotted and Scottish forms, "♫ = ♫" back
  to straight), after a tempo mark at the same bar. `set_feel` now drops the layout
  cache. Test `a_feel_is_written_where_it_changes`.
- The Section tile showed a segno (the D.S. sign); it is now a framed "A" (rehearsal mark).
- A fermata was written with offset `0`; GPIF offsets are fractions, now `0/1`.

Palette and dialogs (owner: the icons looked cheap next to Guitar Pro's, the dialogs
lifeless):

- **Tiles**: no fill at rest, only on hover or when chosen, as a printed palette. Faces
  are Bravura glyphs where the font has the sign (repeat signs, double barline, fermata,
  time-signature digits, key-signature sharps, the swing triplet, tuplet numbers 3, 5:4,
  7:4 in the music font's italic digits), engraved filled ties and slurs, and bold or
  bold-italic lettering for text marks (P.M., let ring, tap/slap/pop, harmonics). Faces
  moved to `tile_faces.rs`.
- **Dialogs**: choices are rows with the engraved sign, the name and one line on what it
  does, chosen with a click (fermata, feel, repeats, barline, break, bars per line). The
  key signature is a circle of fifths (majors outside, relative minors inside, the key
  signature engraved in the middle, enharmonic spelling for keys with 5-7 accidentals).
  The tempo has a one-color metronome that swings at the tempo when clicked, the tempo
  name, a slider, Italian presets and tap tempo. The time signature shows engraved digits
  with steppers and presets; the section shows its framed mark in its color; endings are
  bracket tiles.
- Snapshot example: `--scale S` and `--click LABEL` (repeatable) to render a dialog.
- Tests: snapshots for the key and tempo dialogs, all app snapshots updated (owner to
  review); unit tests for the circle, tap tempo, tempo names, Scottish timing.
- Owner's idea, done the same day (below): an effect or dynamic chosen plays its beat.

## 2026-11-03 — Signatures and feel hold; effects heard in playback and on a click

- **Time and key signatures and the feel hold up to the next change** (owner: a swing set
  at a bar must go on for the following bars). Setting one at a bar changes that bar and
  the following ones that carried the same value, up to the next bar with a value of its
  own, as one undoable batch (`edit/headers.rs` `reach`, tests in `edit/reach_tests.rs`).
- **Effects in playback**: vibrato (slight and wide, a pitch wave on a borrowed channel,
  added to a bend or slide), ghost notes softer, accents and heavy accents louder,
  staccato half length, palm mute short and a little softer, let ring held to the next
  note on its string (at most a whole note more), tremolo picking as 32nd notes, brush
  strokes spread over a 32nd note (down from the low string). The sequence builder was
  split: `build.rs` collects notes and emits events, `effects.rs` holds the effects.
- **Hearing an edit** (owner: clicking vibrato should play the note with vibrato):
  `Sequence::beat_excerpt` is one beat alone with all its effects, from tick 0. The app
  plays it after a palette effect, dynamic or dead note, and after an effect shortcut
  (not undo, redo or paste), when "Play notes as they are entered" is on and the song is
  stopped. The excerpt borrows the player; Play, Stop or a mix change ends it and loads
  the song again (`playback/preview.rs`).
- Tests: `test_effects.rs` (vibrato bends both ways on a borrowed channel, ghost and
  accent velocities, palm mute and let ring lengths, brush spread, the excerpt), unit
  tests in `effects.rs`, and an app test that the vibrato tile plays its beat and Play
  then plays the song.
- Open: trills, the whammy bar, fade-in and grace notes in playback; a palm-muted note
  keeps the track's sound (no muted-guitar program).

## 2026-11-03 — Question dialogs, slide and history icons

- **Unsaved changes, recovery and "Save as Guitar Pro 7"** (owner: lifeless, no close
  button): one modal look in `alert.rs`: the app's title bar with its close button
  (closing is Cancel, or Later for recovery; Escape too, Enter takes the main answer), a
  large icon (an amber warning triangle; a circular arrow round a clock for recovery), a
  headline, a line of explanation and the answers right-aligned, the main one in the
  accent color.
- **Icons**: shift and legato slides join two notes, so they show a dot at each end (the
  legato one with a slur); the history button is a counter-clockwise arrow round clock
  hands.

## 2026-11-03 — Effects really heard (owner: mf silent, no vibrato, staccato not short)

- **mf was silent**: choosing the dynamic a beat already has (mezzo-forte is the default)
  made no command, so nothing played. The palette now plays the beat anyway.
- **Vibrato too subtle**: rendered offline through rustysynth, the slight vibrato swung
  only ±25 cents (3 % pitch spread), hard to hear on a plucked note. Now ±50 cents slight,
  ±100 cents wide. Test `a_vibrato_is_heard_as_a_changing_pitch` (gscore-audio) measures
  the pitch of the rendered sound.
- **Staccato rang on**: rustysynth has no release-time control, so a shortened note kept
  ringing for the SoundFont's release. Staccato (now at most an eighth note) and palm-muted
  notes play on a borrowed channel whose expression (CC 11) drops to zero at the note's
  end, so they stop dead; the next note on that channel resets it. Damped notes without a
  free channel only lose the cut (not counted as degraded pitch bends).

## 2026-11-03 — The first click is heard; Play after a heard beat; rests; hints

- **Vibrato and accent "not played"** (owner, with FluidR3_GM): rendering the owner's
  song and SoundFont offline showed both working (vibrato 215-227 Hz, accent +4 dB). The
  real cause: the synthesizer starts on first use and FluidR3 takes seconds to load; a
  palette click meanwhile was dropped, so the second click (which turns the effect off)
  was the first one heard. A beat chosen while the SoundFont loads now plays once it is
  ready (`Playback::poll_preview`).
- **Play right after a heard beat** sometimes paused instead: the player still reported
  the excerpt as playing. Ending an excerpt now always counts as a fresh start.
  `Playback::preview_sequence` exposes the excerpt for tests.
- **Rest tile** (owner: the rest did not take the chosen length): a beat without notes (a
  rest, such as a new bar's whole rest, or an empty beat) becomes a rest of the palette's
  length; beats with notes keep theirs; where the cursor has no beat (an empty GP7 bar) a
  rest of that length is inserted. Test
  `the_rest_tile_enters_a_rest_of_the_chosen_length_in_an_empty_bar`.
- The "Put the cursor on a note" lines under Dynamics and Techniques are gone; the greyed
  tiles say it in their tooltip.

## 2026-11-03 — Bend, whammy and harmonic previews; the harmonic editor

- **Bend and whammy curves are played** (`gscore-play` `curve.rs`): bends follow every
  drawn point (they played only their highest point before) and the whammy bar bends
  every note of its beat (it was silent). The test helper's bends now look like Guitar
  Pro's (up by the middle and held; a release starts bent).
- **Listen** in the bend, whammy and harmonic editors plays the draft: the commands run on
  a copy of the song (no undo step, nothing journaled) and the beat is played from it
  (`listen_to`). A beat chosen while the SoundFont loads now waits as a built excerpt, so
  a draft is heard as drafted.
- **Harmonic editor** (owner: artificial and pinch harmonics could not be entered; one
  tile and a window instead of five): one Harmonic tile opens it. Kinds as segments;
  touched harmonics (artificial, tapped, pinch) take a touch point in frets above the note
  (+12, +7, +5, +19, +24, +4 or any), stored as the node's fret; a natural harmonic warns
  when its fret is no node. A picture after the classic diagram of a string's partials:
  the vibrating length (from the nut, or from the fretted note) with 1-6 loops, the chosen
  partial shaded, a fret ruler and the touch point. The model works out what it sounds
  ("Sounds A3: the 3rd partial, an octave and a fifth up"). The harmonic keys still toggle
  their kind. Files: `harmonic_editor.rs`, `harmonic_diagram.rs`; the editor windows' glue
  moved to `edit_effects/editors.rs`.
- Found on the way: egui panics on a character no bundled font has ("▶"); the Listen
  button paints its triangle.
- Tests: curve unit tests, `a_drawn_bend_is_followed_point_by_point`,
  `a_whammy_dive_bends_every_note_of_the_beat`, the harmonic tile test, and Listen in the
  bend editor plays a bend without an undo step.

## 2026-11-03 — Four voices

- Owner: up to four voices, four buttons without the "Voice" label, tooltips "First
  voice" ... The palette shows 1-4 (tooltips name the voice; 3 and 4 note that GP3-5 keep
  two). The model creates a voice the measure lacks (up to the 4th, with empty voices
  before it) at its first beat, and undo removes all it created; paste handles four
  voices. GP7 files already read and write four voices; GP3-5 keep two (the save check
  reports more).
- Tests: `a_fourth_voice_is_created_with_the_ones_before_it_and_undone`,
  `notes_go_into_the_third_and_fourth_voice`.
- Open: standard notation draws only the first voice (voices 2-4 show in the tab and play).

## 2026-11-03 — Listen on tied beats and while paused

- Owner: Listen (bend, harmonic) was silent. Reproduced with the owner's song, FluidR3
  and the built-in synth: after playing and pausing, the cursor sat on a beat of tied
  notes; the excerpt took them as the continuation of the chord before and had nothing to
  strike. An excerpt now strikes the tied notes at its beat anew (in the song a tie still
  continues the note). Test `an_excerpt_of_a_tied_beat_strikes_its_notes`.
- A paused song no longer blocks hearing a beat (Play then starts at the cursor); a
  playing song or count-in still does. Listen says in the status bar when it cannot play
  and why (song playing, no note, output problem, SoundFont loading); palette clicks stay
  quiet about it. `Playback::preview_beat` returns `Heard`; `Playback::output_state` for
  tests.

## 2026-11-03 — A bend no longer drops at its end

- Owner: the default full-step bend fell back at the very end, which the graph does not
  show. Cause: a pitch reset right after the note-off pulled the still-ringing release
  back down; a heard beat also ended at the note-off, where the player centers every bend.
  Now no reset follows a note (a borrowed channel is set again when next borrowed), a
  borrowed channel stays reserved for a quarter note after its note for the release, and
  an excerpt runs on a whole note for the release, then fades its bent channels and
  silences them. Rendered with FluidR3: 225 Hz up to 248 Hz, held through the release,
  then quiet.
- Found on the way: a channel an excerpt borrowed can be a track's own channel in the
  song's sequence, and a damped note left its expression at 0. Every track setup now
  sends full expression. Test `a_bent_note_keeps_its_pitch_through_its_release`.

## 2026-11-03 — The synthesizer loads when a song opens

- The first note of a session waited seconds for FluidR3 to load because the built-in
  synthesizer started on the first audition or play. `Playback::preload_synth` starts it
  when a document becomes the open one (`replace_document`, so opening, a new song and a
  recovered session alike); MIDI output is untouched, and a synthesizer that failed before
  is retried on the next open. The toolbar's "Loading sounds…" now shows during that load.
- `Playback::is_connected` reports whether an output is connected (test and diagnostics).
  Test `opening_a_song_starts_loading_the_synthesizer`.

## 2026-11-03 — Songs open in tabs instead of replacing each other

- Owner: opening a song silently closed the current one; no way to have several songs open.
  The app now keeps one tab per open document. Opening a file, a new song, or a recovered
  session opens a new tab (the current one is left untouched); File › New opens a small
  "New song" dialog (title and tempo, prefilled "New Song" / 120 BPM so it needs no
  typing), which creates the score in a new tab.
- A tab shows a dot (•) before its title while it has unsaved changes, and its close
  button asks Save / Discard changes / Cancel first; closing a clean tab just closes it.
  Closing the last tab leaves an empty workspace. Closing the window asks about each
  changed tab in turn before it closes.
- Under the hood: `GuitarScoreApp` holds `tabs: Vec<Tab>` plus `active`; `Tab` groups a
  `Document` with the per-document view state (shown track, measure selection, track
  panel, edit palette, mixer, rename, new-track dialog, history stamps). Theme, zoom,
  panel visibility and page view stay shared on `ViewState` (owner's choice). Playback
  follows the active tab (`song_changed` + `preload_synth` on switch). `ViewState` lost
  its `track`/`selection` fields; tests read them through `shown_track()`/`selection()`.
- Public API: `tab_count()`, `tab_document(index)`, `active_tab()`, `shown_track()`,
  `set_shown_track()`, `selection()`, `set_selection()`. Tests
  `opening_another_file_opens_it_in_a_new_tab`, `the_new_song_dialog_creates_a_one_measure_score_in_a_new_tab`,
  `closing_a_changed_tab_asks_before_closing_it`. App snapshots regenerated (tab strip).
- Follow-up (owner): the tab strip now sits below the player bar and between the side
  panels (created after them, so it spans only the central column). Each tab is a single
  rounded widget with its close icon inside on the right (hover shows a circle); the
  accesskit labels are "Tab N: title" and "Close tab N".

## 2026-11-03 — Reopen all the tabs from last time, not just one

- Owner: the app remembered only the last open file; it should remember every tab that was
  open. The settings now store `open_files` (in tab order) and `active_file`, computed from
  the live tabs whenever `settings()` is taken (open, save, exit). At startup they are
  reopened one after another through the single-open task queue, each file skipped if it is
  gone, and the tab that was active is selected again when the queue drains. A file given on
  the command line still opens on its own (no restore).
- `Document` gained a `reopenable` flag: opened files and recovered-with-a-source are
  reopenable; a brand-new song and a recovered-without-a-file song are not (they are not
  backed by a real path until saved). `Document::unsaved` builds the former, and
  `mark_saved` makes a document reopenable.
- `Settings.last_file` is still written (and read as a fallback) so older settings, or an
  older build, still reopen something. The Settings label is now "Reopen the tabs from last
  time at startup". Tests `every_open_tab_is_restored_at_startup`,
  `a_missing_tab_is_skipped_at_startup`, `closing_and_reopening_remembers_every_open_tab`.
- Also made the shared test helper `settle` step-based instead of `Harness::run`, which
  used to trip its step limit while the "Opening…" bar repainted (a latent flake).

## 2026-11-03 — The opening progress lives in the status bar

- Owner: the "Opening…" bar was a top panel, so the whole page jumped when it appeared and
  disappeared. It now shows inside the always-present bottom status bar (`status_bar` in
  `bars.rs`): label, a slim progress bar (or a small spinner before the size is known),
  "Importing…", and a Cancel link, with the active document's counts/page/cursor still on
  the right. The status bar requests a repaint while it shows progress. Removed the
  `Panel::top("opening")` block from `mod.rs`; the settled status bar is unchanged, so the
  app snapshots still match.

## 2026-11-03 — Busy spinner in the status bar, one tab per file, nicer close hover, recent files when empty

- The toolbar's busy spinner moved to the status bar, next to the file name (and next to
  "No file open"), so a save/dialog/export no longer shifts the toolbar groups. The
  "Opening…" progress already lives there.
- Opening a file that is already open now switches to its tab instead of adding a duplicate
  (`GuitarScoreApp::open` -> `find_open_tab`, comparing canonical paths; an in-flight open
  of the same path is ignored via `Tasks::opening_path`). Choosing it from Open Recent still
  moves it to the top of the recent files. Startup restore skips duplicates too.
- A tab's close icon now gets the same red hover background as the window close button
  (white X on `style::CLOSE_RED`, shared with `title_bar`), not a circle.
- The empty workspace (`page.rs::empty_workspace`) suggests the recent files as links (name
  plus parent folder, full path on hover; clicking opens it), with an "Open a song" heading
  and a short hint. Tests `the_same_file_is_never_open_in_two_tabs`,
  `the_empty_workspace_suggests_recent_files`.

## 2026-11-03 — Review of the tabs branch: stable tab ids, window close, one open at a time

- Data loss fixed: when a Save started from the window-close question failed (or its
  Save-As dialog was cancelled), `closing_window` stayed set, so the *next* close request
  skipped the unsaved-changes check, closed the window, and `on_exit` deleted the recovery
  journals. Every way a save ends without success now goes through `abandon_save`, which
  also calls off the pending window close; a close request while a question or save is up
  just keeps the window open. Test
  `a_save_that_fails_while_closing_the_window_asks_again_on_the_next_close` (fails before).
- Tabs now have a stable `TabId`; `Then::CloseTab` and `saving_tab` hold it instead of an
  index. Before, closing another tab while a save (or its Save-As dialog) was pending
  shifted the indices: the save could mark the wrong tab saved, or the follow-up "close"
  could remove a different, unsaved tab without asking.
- Only one save runs at a time (its result does not say which tab it was for); a second
  one says so in the status bar instead of mixing the two up. A dirty tab's close question
  no longer replaces a question that is already up.
- Opens form a real queue: a file chosen while another opens waits in `pending_opens`
  (deduplicated) instead of racing it. The queue advances when the running open ends in
  any way, including Cancel (before, a cancelled open during startup restore stalled the
  rest of the restore until some later open finished). A panicking open now reports as
  that open's failure, so the queue cannot stall on it. Test
  `a_file_chosen_while_another_opens_waits_its_turn`.
- Note: `cargo xtask ci` runs in the directory baked into the xtask binary
  (`CARGO_MANIFEST_DIR`); a binary built in another worktree tests that worktree. Touch
  `xtask/src/main.rs` if the failure paths point elsewhere.

## 2026-11-03 — Section titles out of their frames and with box glyphs

- Root cause: Guitar Pro (GP6/GP7/GPX) writes a section's text as
  `<Text>\n<![CDATA[Chorus]]>\n</Text>`; the GPIF reader kept the newlines. The score's text
  layout then had an empty first line, so the title was painted one line below its frame,
  and the section dialog showed the line breaks as boxes before and after the name. The
  reader (`src/io/gpif_import/mod.rs`) now trims section text and letter and falls back to
  the letter when the text is blank. Tests: `gscore-gp`'s `section_tests`; the owner's
  `Ballad.gp` renders clean titles (checked with `gscore-cli render`).

## 2026-11-03 — A calmer default section color, track colors in the section dialog

- The default section color was Guitar Pro's pure red (`0xFF0000`), which jars in the song
  grid's dark ruler. GP6/GP7 files store no marker color, so the value is ours to choose:
  `gscore-gp`'s `Marker::DEFAULT_COLOR` is now a light grey (`0xB0B0B0`), and
  `gscore-model`'s `Section::DEFAULT_COLOR` points at it, so the importer, the app and the
  CLI (`set_section` without `color`) agree. GP3-5 files keep their stored color. The grey
  is a palette entry ("Light grey", next to "Grey"), so the dialog outlines it.
- The section dialog no longer has a bare color button: it offers the same preset swatches
  as the track panel's Appearance section, plus the custom picker. The swatch grid moved to
  `app/swatches.rs`; the track panel and the new-track dialog use it too (the new-track
  dialog had its own copy).
- `TrackColor::from_parts` replaces three copies of the `r*0x10000 + g*0x100 + b` formula.
  Tests: `gscore-model`'s `colors_assemble_from_their_parts`, `gscore-gp`'s `section_tests`
  (marker color); the section dialog and the grid were checked with the offscreen snapshot.
  The seven `gscore-app` snapshots were regenerated: the only change is the added "Light
  grey" swatch in the track panel (the dialogs' background).

## 2026-11-03 — Key signatures in the score

- Key signatures were never drawn, and accidentals assumed C major, so every score looked
  like C major. The layout now reads the key per measure (`Song::key_signature`) and:
  - draws the sharps/flats after the clef at the start of every system, and at a key change
    in the middle of a system (with naturals cancelling the accidentals the old key had and
    the new one does not). The notation staff carries them; a tab-only score shows them on
    the tab staff, centered on a five-line frame;
  - spells notes in the key: pitch classes the signature alters use its flats/sharps, other
    pitch classes keep sharps (as Guitar Pro does for accidentals);
  - starts each measure's accidental state from the key signature, so an F sharp in G major
    or a B flat in F major needs no accidental, and a natural cancels the signature's alter.
- The key (and the key before a change) is part of the per-measure layout cache key, so a
  key edit or its undo lays out exactly the affected measures again; `test_cache` compares a
  key change against a fresh layout. Line breaking now takes a system-start width per
  measure, since the clef is followed by a different number of accidentals on each system.
- Files: `notation/pitch.rs` (key alters, spelling, key-signature positions per clef),
  `notation/draw.rs` (`KeySigLayout`, start widths), `measure/*` (the signature and its
  room), `lib.rs`/`system.rs` (per-system prefixes), `glyph.rs` (`accidentalFlat`). Tests:
  `crates/gscore-layout/tests/test_keys.rs` (sharp and flat keys, suppression, cancellation,
  tab-only, drums) and `test_cache`. Rendered `Ballad.gp` (B minor, tab only) and
  `note_lengths.gp` with an E-flat key: the signature appears on each system, and a natural
  is written on the E that the key would flatten.

## 2026-11-03 — Second voice in notation, ties across barlines

- Notation now writes every voice, not only the first. A column holds one item per voice
  (`Vec<Option<NotationItem>>`), all voices share the measure's accidental state, and each
  voice is drawn with its own stems, beams and ties. With two or more voices the first
  points its stems up and the rest down; a notehead of a lower voice that would sit exactly
  on the first voice's position moves right by 0.6 staff spaces. The layout gives the
  column the extra room and the second voice is offset only when it collides. Dynamics
  below the staff still use the lowest head of any voice.
- Ties across barlines: after a system is placed, adjacent measures draw the tie from the
  last notehead with the destination's staff position to the tied head in the next
  measure's first column, on the notation staff and on the tab (where the continuation has
  no fret number). At a system break the tie becomes a short outgoing fragment at the end
  of the line and a short incoming one at the start of the next; nothing is drawn across
  systems. Ties work per voice and the tab matches the note on the same string.
- Playback already played all voices and held ties across barlines; tests now cover it.
  The tie bookkeeping (`on_string`) is keyed by (voice, string) instead of string alone, so
  a tie in one voice can never extend another voice's note on the same string.
- Files: `notation/{mod,draw,beams,pitch}.rs`, `measure/{mod,draw}.rs`, `lib.rs`,
  `gscore-play/src/build.rs`. Tests: `crates/gscore-layout/tests/test_voices.rs` (second
  voice, collision offset, tie across a barline on both staves, fragments at a system
  break, a tie in the second voice) and three new `gscore-play` tests (cross-barline tie,
  every voice plays, no cross-voice tie). Rendered a copy of `steel_guitar_dadgad` with
  voice 2 (stems up/down, both in the tab) and a tied note across a barline (curves on both
  staves). All app snapshots stay unchanged (the test files have one voice and no
  cross-barline ties).

## 2026-11-03 — Grace notes

- GP6/GP7 store a grace note as a whole beat marked `<GraceNotes>`; the model kept it as an
  ordinary beat, so its 32nd/16th length made the measure longer than its time signature
  and the grace notes were drawn as normal notes. `Beat` now carries a `grace` flag (set by
  the GPIF reader, used by the writer instead of the "all notes have a grace effect"
  guess), and `BeatView::is_grace` exposes it.
- The model counts a grace beat as zero length (`VoiceView::total_duration`), so measures
  are no longer reported overfull. Layout and playback skip it when advancing time:
  - Layout draws the grace notes just left of the next beat's main note, as small heads
    with a stem, flag and the acciaccatura slash on the notation staff, and as small
    slashed fret numbers on the tab (`measure/mod.rs`, `measure/draw.rs`, with the grace
    group's width reserved in the column). GP3-5 grace effects attached to a normal note
    are drawn the same way.
  - Playback plays each grace note as a 32nd just before its beat (at the beat when it is
    the first sound of the song, where no time is available), and does not lengthen the
    song (`gscore-play/src/build.rs`; the beat clock and cursor lookups skip grace beats).
  - A grace beat at the end of a measure gets a column of its own before the barline.
- Editing grace notes (creating or removing them) is not implemented yet; they are read,
  drawn and played. The cursor skips grace beats (and clamping after an edit does too), and
  the note/duration/copy commands treat them as ornaments, so editing cannot turn one into
  a normal note by accident.
- Tests: `crates/gscore-layout/tests/test_graces.rs` (no time, small notation head and tab
  number, GP5 attached grace) and two `gscore-play` tests (grace beat and attached grace
  sound with their beat). Checked on the owner's `farewell.gp` (track 8, measure 102: a
  small slashed `0` before the main `0`) and a synthetic both-staves song (small slashed
  head before the main note).
- `Beat` gained a field: the GPIF round trip of `farewell.gp` and `hangar18.gp` keeps its
  grace beats (`<GraceNotes>` count survives).

## 2026-11-04 — Review of the last four commits; readable and audible grace notes

- Owner: grace notes were almost unreadable and could not be heard.
- Drawing (`gscore-layout/src/measure/graces.rs`, new; the grace code moved out of
  `measure/{columns,draw}.rs`):
  - Graces were placed from the main note's *center* (`x - grace_width`), so they ran into
    its notehead, fret number and accidental; the column reserved `max(grace, accidental)`
    instead of the sum. Columns now keep the main note's own left extent (`main_left`) and
    the graces end left of it, with room for their flag and slash.
  - Notation: cue size 0.65; their own accidentals (against the key signature,
    `Clef::place_grace`); ledger lines; stems always up, one eighth flag (the GP5 grace
    duration unit was misread as a note value and gave four flags); the acciaccatura slash
    crosses stem and flag instead of the head.
  - Tab: fret numbers at 0.78 of the normal size instead of 0.6, and no slash (it covered
    the digit: measure 102 of `farewell.gp` read like "Ø10").
- Playback (`gscore-play/src/graces.rs`, new; `effects::one_note_per_string`):
  - Cause: a grace note lasted a 32nd (44 ms at 170 BPM) while the previous note on its
    string rang on over it at full level until the beat. Offline with FluidR3 on
    `farewell.gp` measure 101 the previous note was 2.2x the grace note's level in that
    window; now the grace note is 3.4x what is left of the previous note.
  - A grace note sounds about 75 ms (between a 32nd and a 16th), a note still ringing on
    the same string stops when the next note there starts, appoggiaturas (on-beat graces)
    take the beat and delay the main note on their string, and at the very start of the
    song the graces play first instead of together with the main note.
  - The beat excerpt (palette clicks, notes as entered) plays a beat's grace notes too, and
    a brush stroke no longer spreads the grace notes as if they were chord notes.
- Key signatures (review of f361b20): chromatic notes are now spelled with flats in flat
  keys (A flat, not G sharp, in B flat major), as Guitar Pro writes them.
- Section titles (db73d8e) and the modelling plan (7dd4242): reviewed, no changes.
- Tests: `test_graces` (graces clear of the main accidental, own sharp, readable unslashed
  tab number, ledger lines), `test_sequence` (a grace note cuts the ringing note, the
  excerpt plays the grace first), unit tests for grace placement and lengths, one note per
  string, and flat spelling. `testing::song_with_graces` builds such songs.

## 2026-11-04 — Entering grace notes

- Owner: grace notes must be enterable, with a dialog for the speeds. (Ghost notes were
  already enterable: `g` and the palette.)
- One form for editing: the model keeps a grace note attached to its main note
  (`NoteEffect::Grace(Option<Grace>)`; `Grace { fret, speed, on_beat, dead }`, speeds
  fast/normal/slow = 64th/32nd/16th, `gscore-model/src/grace.rs`). `NoteView::grace`
  returns the same type, so layout, playback and the dialog read what commands write.
- Files (`gscore-gp/src/model/grace.rs`, docs/guitarpro.md #37): reading a GP6/GP7 file
  folds each grace beat into its main note where nothing is lost; writing `.gp`/`.gpx`
  turns attached grace notes back into grace beats, and the GP3-5 writer folds (or leaves
  out) grace beats. Grace beats that do not fold (a grace note on a string the main beat
  does not play, an eighth-note grace, one with its own vibrato) stay as they are: drawn
  and played, not edited. The save check now compares grace notes and grace beats, so a
  lost grace note is reported. `farewell.gp` still saves without a reported loss.
- Command text: `set_note_effect ... effect=grace value=5:normal:before` (`:on` for on
  the beat, `:dead` appended for a dead one, `none` to remove), journaled like every other
  effect.
- App (`grace_editor.rs`): the "Grace note" palette tile (a slashed grace note, next to
  the harmonic tile), `Shift+G` and Note › Grace note… open the dialog: the fret with quick
  choices one or two frets below/above the main note, a dead-note switch, the three speeds
  as engraved 64th/32nd/16th notes with what each does, and before/on the beat as
  engraved acciaccatura/appoggiatura signs. Listen plays the beat with the draft; OK applies
  it to every selected note, Remove takes it off. The harmonic editor's button row and
  Escape/Enter handling are shared with it.
- Playback: a grace note now lasts its written value, at least 40/60/90 ms for
  fast/normal/slow, so the speeds stay apart and audible at any tempo.
- Tests: fold/unfold round trip and the cases that must not fold (`gscore-gp`), set/
  change/remove with undo (`gscore-model`), the command text round trip and bad values
  (`gscore-edit`), an extreme grace in `panic_hunt`, grace lengths per speed
  (`gscore-play`), the dialog via `Shift+G`, OK and Remove via the palette tile
  (`test_editing`). Checked with `gscore-cli edit` on `note_lengths.gp`: three grace notes
  (slow before, fast on the beat, dead) save as `.gp`, reload, and render.

## 2026-11-04 — Tuplet tiles and the tuplet dialog

- Owner: tiles for triplets and sextuplets like Guitar Pro's, and a dialog for every kind
  of tuplet.
- Palette: the "3", "5:4", "7:4" tiles became Guitar Pro style bracket tiles: a triplet
  (`⌐3¬`), a sextuplet (`⌐6¬`) and `⌐n:m¬` with a corner triangle that opens the dialog
  (it shows the chosen tuplet, such as `⌐5:4¬`, when one without a tile of its own is
  chosen). The bracket tiles are half as wide again as the others (`tiles::wide_tile`); the
  bracket is drawn by `tile_faces::tuplet_bracket` with Bravura's tuplet digits.
- Dialog (`tuplet_dialog.rs`): the chosen tuplet as a large engraved bracket, steppers for
  "n in the time of m" (n 2-32, m 1-32), what it means in a line, and the common tuplets as
  engraved signs with an explanation (triplet, sextuplet, quintuplet, septuplet, nonuplet,
  decuplet, and the duplet and quadruplet of 6/8). OK sets the palette's tuplet and the
  selected beats' length, Remove takes the tuplet off; no Listen (a length has nothing new
  to hear). The harmonic editor's button row takes an optional Listen for it.
- Actions: `tuplet_six` (sextuplet) and `tuplet_other` (opens the dialog); `tuplet_five`
  and `tuplet_seven` keep working from the keymap without tiles. No default keys.
- Checked: 9:4, 2:3, 10:8 and 6:4 beats save as `.gp` without a reported loss (GP3-5's
  fixed tuplet list does not apply to GPIF). Test: `test_editing` (sextuplet tile, dialog
  with a common tuplet, steppers to 9:4, Remove).

## 2026-11-03 — Release workflow and build version (M20 groundwork)

- New `.github/workflows/release.yml`: on a published GitHub Release it builds `guitarscore`
  and `gscore-cli` in release mode for Linux (x86_64, tarball), Windows (x86_64, zip) and
  macOS (universal Apple Silicon + Intel via `lipo`, tarball), and attaches the archives to
  the release with `softprops/action-gh-release`. Each archive carries the binaries plus
  `LICENSE`, `README.md`, `THIRD_PARTY.md` and the two OFL font licenses.
- New `gscore-version` crate (layer 0, dependency-free): its `build.rs` bakes the version in
  at compile time — the `GSCORE_VERSION` variable when the release workflow sets it to the
  tag, otherwise `<crate version>-dev+<short git sha>`. `gscore-cli --version` now prints it,
  and the app shows it (and the build SHA) in a new Help → About GuitarScore window.
- The binaries are self-contained (fonts compiled in). Nothing is signed/notarized yet, so
  macOS shows Gatekeeper and Windows SmartScreen — that is a later M20 task.
- Open: run the workflow once on a real tag and confirm each archive opens on its platform
  (the bundled SoundFont and the AppImage follow in the next entry).

## 2026-11-03 — Bundled SoundFont and a Linux AppImage

- The built-in synth now finds a SoundFont shipped next to the executable: `gscore-audio`'s
  `find_soundfont`/`available_soundfonts` check `<exe dir>/FluidR3_GM.sf2` and
  `<exe dir>/soundfonts/*.sf2` (after the user's own folders, before system fonts). A
  source build has none, so nothing changes there. Tests cover the order (the user's own
  folder still wins over the bundled default).
- Release archives now bundle FluidR3 GM (MIT, ~141 MB): a `soundfont` job apt-installs
  `fluid-soundfont-gm` once and shares the `.sf2` with the Linux, Windows and macOS jobs, so
  a downloaded build has sound out of the box. It is not committed to git (over GitHub's
  100 MB file limit); `assets/soundfonts/FluidR3-LICENSE.txt` records the license and source.
- Linux gains a single-file AppImage (`GuitarScore-<tag>-x86_64.AppImage`), built with
  linuxdeploy + appimagetool from `packaging/guitarscore.desktop` and a new icon
  (`packaging/guitarscore.svg`/`.png`); the GPU/driver libraries are left to the host.
- Open: the AppImage and the bundled-SoundFont archives still need a real-tag run and a
  handful of distros (Ubuntu, Fedora, Arch) to confirm they start and make sound.

## 2026-11-04 — Releases build on pre-releases; a new app icon (M20)

- The release workflow runs when a **pre-release** is published on GitHub
  (`release: prereleased`), not on every published release. The owner checks the
  attached assets on the pre-release, then unticks "pre-release" to make it the full
  release, which builds nothing again. `overwrite_files` lets a re-run replace the assets.
- App icon: the old pick-shaped icon was too close to Guitar Pro's. The real icon is
  postponed (owner); until then a placeholder: a white G (Noto Sans Bold) with an eighth
  note (Bravura) in front of it on a blue disc that fades from light in the middle to
  deep blue at the rim, both glyphs as outlines, so the SVG needs no fonts. The app now
  sets it as the window icon (it showed eframe's default), uses the app id `guitarscore`
  to match the desktop file (and its `StartupWMClass`) so docks on Wayland find the icon,
  and shows it in Help › About. `gscore_app::APP_ICON` holds the PNG for both. Tests
  `test_icon.rs` and `help_about_shows_the_version` (Help › About had no test).
- The baked-in build SHA went stale: `gscore-version`'s build script reran only when
  `GSCORE_VERSION` changed, so a local build after a new commit still showed the old SHA
  in Help › About. It now also reruns when git's HEAD, the current branch or the packed
  refs change (paths from `git rev-parse --git-path`, so worktrees work too).
- Tests: the app snapshots were stale since Help › About added a "Help" menu to the title
  bar (the only difference; regenerated). `opening_a_song_starts_loading_the_synthesizer`
  failed about one run in three: `settle` called `Harness::run`, which panics when the
  toolbar repaints for more than 4 frames while the song and the SoundFont load (the player
  is measured again as its contents change). `settle` now steps until nothing is busy and
  `try_run` settles (15 of 15 runs green).
- Open: the real app icon (postponed); a first pre-release run on GitHub (it was not run
  from here); Windows `.exe` and macOS `.app` icons (both packages are plain archives so
  far); the rest of M20 (macOS file-dialog fix, name check, user guide, public test
  files, signing).

## 2026-11-04 — Beat effects and notation articulations drawn and played

- Owner: the "Strokes and beat" effects were neither drawn nor played, and some symbols
  were missing on the notation staff.
- Drawn: pick strokes (down and up bow signs), tapping, slapping and popping (`T`, `S`,
  `P`, as Guitar Pro writes them) and fade-ins (an opening hairpin) in a row of their own
  above the top staff, reserved like the fermata row (`marks::Rows`); with notation it sits
  above the notation staff and rises over a high note in its column. Brush strokes get a
  vertical arrow beside the chord on the tab and on the notation staff, pointing the way the
  palette tile does.
- Notation staff (it showed no ornaments before): accents, heavy accents and staccato
  opposite the stem (staccato in a space), ghost notes in parentheses, natural harmonics
  as diamond heads, tremolo picking as strokes through the stem, slurs for hammer-ons,
  pull-offs, legato slides and beat-level legato, lines for shift and legato slides, and
  short lines for slides into and out of a note (`notation/marks.rs`). With notation
  shown, accents, staccato and tremolo are written there only, as in Guitar Pro; tab-only
  keeps them on the tab. A dynamic below a low note clears its accent.
- A notation-only staff now also shows what lived only on the tab: trill, "PH", vibrato
  and the whammy line in a band above it, P.M. and let ring below it (the dynamics move
  below those rows). Bends are still drawn on the tab only.
- New glyphs (`articAccent*`, `articStaccato*`, `articMarcato*`, `stringsDownBow`,
  `stringsUpBow`, `noteheadDiamondWhite`, `noteheadParenthesis*`), and a test that every
  glyph's code point is the named glyph in the bundled Bravura (font advance against the
  metadata's).
- Played (`gscore-play/src/strokes.rs`): a pick stroke strikes a chord string by string
  within a 128th note in its direction, an upstroke at 90%; tapping 85%, slapping 115%,
  popping 125%; a fade-in swells its notes' expression from 0 to full over the note on a
  borrowed channel; a note reached by a hammer-on, pull-off or beat-level legato is not
  picked (80%). The beat preview (Listen) plays them too. The crate doc listed the whammy
  bar as unplayed; it has been played for a while.
- Golden: `ornaments.svg` regenerated (the notation staff gains the slurs, slides, accents,
  staccato, ghost parentheses, legato slur and harmonic diamond; it now matches Guitar
  Pro's export of that file much more closely). Tests: `gscore-layout/tests/test_effects.rs`
  (tab, notation and notation-only), new playback tests in `gscore-play/tests/test_effects.rs`.
- Open: trills are still not played; artificial, pinch, tapped and semi harmonics keep a
  normal head on the notation staff (their text stays on the tab); bends on a notation-only
  staff; a dynamic under a chord with a down-stem beam can touch the beam (older).

## 2026-11-04 — Advanced ornaments: drawn, played and in the palette

- Owner: everything in `testdata/basic/advanced_ornaments.gp` must be read, written, drawn,
  played and editable from the palette, with Guitar Pro's `advanced_ornaments#1.png` as the
  reference.
- Brush and arpeggio arrows: a roll down is drawn as an arrow pointing **up**, as Guitar Pro
  does; the brush arrow is on the tab only (the notation keeps it only when the tab is
  hidden), and the two brush palette tiles were swapped to match.
- Layout (`gscore-layout`): Guitar Pro's beat text in a new row above the staff; `(7)` for
  a tie carried into a measure's first column; tuplet brackets with `n:m` (a number over a
  beam when the group is exactly one beam group, otherwise a bracket, including rests);
  a large cross instead of the rest/notes of a dead slapped beat; `P.S.` and a scrape
  wiggle for pick slides; a circled T and a small arc for left-hand taps; fingering signs
  left of the heads; circled string numbers above or below the staff; wavy arpeggio arrows
  on both staves; `rasg.`; `H`/`P` and slurs for grace-note hammers; `tr` on both staves
  and the trilled fret as `2 (4)` on the tab; tremolo picking with one to three strokes
  by speed (under the tab number and through the notation stem); fade-in, fade-out and
  volume-swell hairpins. The model's `Grace` gained `hammer`; `NoteMarks`, `ColumnMarks`,
  `ChordMarks`, `Head` and `Rows` carry the new state. New modules `ornaments/symbols.rs`,
  `measure/grace_marks.rs` and `notation/tuplets.rs` keep the files from growing further.
- Playback (`gscore-play`): arpeggios spread over an eighth; rasgueados strike three times;
  a dead slapped beat plays short muted hits on every string; a pick slide is a quick run of
  muted notes to the next note; trills alternate with the trilled fret at 16th/32nd/64th
  speed; tremolo picking uses its speed (8th/16th/32nd) instead of always 32nds; fade-out
  and swell ramp expression; left-hand taps and grace-note hammers are played as legato
  (softer). The crate doc no longer lists trills as unplayed.
- Palette (`gscore-app`): tiles for left-hand tap, string number, the two pick slides, both
  arpeggio directions, dead slapped, `rasg.`, fade out and volume swell. The trill and
  tremolo tiles show a corner triangle; a click on the sign toggles the default, a click on
  the corner opens a small engraved dialog (`edit_effects/dialogs.rs`) for the trill fret
  and speed or the tremolo speed. New actions and menu entries; no default keys.
- Tests: `advanced_ornaments` is in the layout testdata and golden lists
  (`tests/golden/advanced_ornaments.svg`, owner review pending); GPIF import tests for the
  new beat and note fields, pick-slide flags, trill pitch conversion and grace hammers;
  `.gp` round trip without losses; playback tests for every new effect and the whole file;
  a palette test clicking the new tiles. `note_lengths.svg` and `ornaments.svg` were
  regenerated (tuplet brackets instead of per-beam numbers, beat text, trill/tremolo
  changes; owner review pending).
- Open: `edit_effects.rs` (835 lines) and `notation/draw.rs` (655) are past the ~400-line
  guideline and should be split in a later cleanup; the fade hairpins sit above the
  notation staff with the other beat effects (Guitar Pro prints them above the tab); the
  beat effects' row placement is otherwise unchanged from the previous entry.

## 2026-11-04 — Tie beat, beat text, fingerings and the last ornament fixes

- A dead slapped beat no longer draws a rest behind its large cross on the notation staff:
  the rest suppression moved before the beat-kind match in `notation::beat_item`.
- Fades moved from "Strokes and beat" to **Dynamics**: fade in, fade out and volume swell
  are dynamics markings; the dynamics section now ends with a "Fades" row. Every tile that
  opens a dialog (bend, whammy, harmonic, grace, trill, tremolo, fingering, beat text) now
  shows the corner triangle, and the whole tile opens the dialog (the tiny corner alone was
  too hard to hit).
- New **Tie beat** tile: `set_beat_tie` in the model ties every note of a beat to the note
  on its string in the previous beat (also across a barline), so a chord continues as one;
  `Command::SetBeatTie`, `gscore-cli` support, undo and `panic_hunt` coverage. The tile is
  highlighted when the whole beat is already tied.
- New **Beat text** tile and dialog: Guitar Pro's beat text, separate from a section title.
  `Song::set_beat_text` + `ChangeKind::BeatText`, `Command::SetBeatText`, the
  `set_beat_text` CLI command, an undoable app dialog, and a text row already drawn above
  the staff.
- **Fingering** tiles and dialogs: one for the left hand (0-4, t) and one for the right
  hand (p i m a c), opening a small dialog that picks the finger by clicking its engraved
  sign; Remove clears it. Both apply to the selected notes through the existing
  `NoteEffect::Finger`.
- Pick slides now follow the pitch change between the two notes on the **notation** staff
  (a wavy, slightly slanted scrape) instead of a horizontal line; the tab keeps its
  string-line wiggle. New `ornaments::symbols::draw_slanted_wiggle`.
- Fixed a `panic_hunt` hang: `previous_beat` searched every index down from a hostile
  `beat=18446744073709551615`, now it stops at the voice's beat count.
- Tests: model tests for whole-beat ties and beat text (with undo), command round-trip
  cases, a palette test for the tie-beat, beat-text and left-hand-fingering tiles, and the
  regenerated layout/app snapshots. `cargo xtask ci` is green.

## 2026-11-04 — Review of the advanced-ornament commits

- Grace notes: the command language lost a grace note's hammer-on (`grace_text` never
  wrote it, `parse_grace` always read `false`), so a journal replay or a pasted command
  dropped it. The form is now `fret:speed:timing[:dead][:hammer]`.
- Rasgueado playback: the extra strikes were copies of the whole note shifted later, so
  each strike overlapped the next on the same key and the last one rang past the beat into
  the next note (strings are cut to one note before the rasgueados are added). Each strike
  now ends where the next begins and the last keeps the note's end; a note too short for
  three strikes is struck once.
- Palette: the pick slide down and up tiles had the same face; the wiggle now slants with
  the direction. The beat text dialog read its text before the frame's input. Removed a
  duplicated `Fx::on_note` wrapper.
- Open (older layout design): the rows above the staff (beat text, fermata, sections) are
  at a fixed height, so a high note's ornament can touch them (bar 7 of
  `advanced_ornaments`: "Left hand tap" and the circled T).
## 2026-11-04 — Browser (wasm) build and AudioWorklet player

Landed:
- `cargo xtask web [--smoke]`: builds `gscore-app` for `wasm32-unknown-unknown` with
  WebGL2 (`--no-default-features --features web`), runs wasm-bindgen, optionally wasm-opt,
  and writes the static site to `dist/web` (`index.html`, `guitarscore.js`,
  `guitarscore_bg.wasm`, `assets/`, `manifest.json`, `sw.js`). Sizes: 13.5 MiB raw,
  5.4 MiB gzip, 3.0 MiB brotli with `wasm-opt -Oz`; 16.1 MiB raw without it.
- Platform split in `gscore-app`: desktop entry in `main.rs`, browser entry in `web.rs`
  (eframe `WebRunner`, glow). `rfd` dialogs are async in the browser; open reads checked
  bytes and imports with `gscore_session::open_bytes`, save uses the new
  `gscore_session::save_bytes` plus the browser's download handle. Drag and drop reads
  `DroppedFile::bytes_async`. `web-time` replaces `Instant`; threads are avoided (imports
  run on the UI thread after the bytes arrive).
- `gscore-session/src/web.rs`: settings and shortcuts in `localStorage`, with size caps and
  the same fallback-with-problem behaviour as files; the recovery journal is still
  file-based and is not written in the browser (gap).
- `gscore-audio`: `cpal`/`midir` are `cfg`-gated to native; `web_player` creates the
  AudioWorklet and sends commands. New `gscore-worklet` crate (L4, cdylib) wraps
  `SynthEngine` with wasm-bindgen; `gscore-play::wire` is a checked binary sequence format
  (round-trip and damaged-message tests). The main thread compiles the worklet wasm and
  sends the `WebAssembly.Module`; the worklet defines UTF-8 globals, uses `initSync`, and
  renders stereo blocks. A site-level `assets/soundfonts/default.sf2` is optional.
- `cargo xtask layers` now also compiles `gscore-audio` and `gscore-worklet` for wasm.
- Smoke test: `--smoke` serves the site and drives headless Chromium over a dependency-free
  DevTools client (`xtask/src/cdp.rs`), checking the app marker and the worklet port, with
  screenshots. `worklet-check.html` is built into the site for that check.
- Docs: `docs/web.md`.

Open: browser recovery journal/autosave; SoundFont picker in the browser; PDF export in
the browser; multi-threaded import (web worker) if opening large files becomes noticeable;
`xtask web --smoke` needs Chromium and Python 3.

## 2026-11-05 — Review of the browser build

- Saving in the browser failed or lied: Save on an opened `.gp` (which saves in place)
  and "Save anyway" after a loss warning had no save handle and always failed ("try Save
  again"), and rfd's web save showed its own "click here to download" overlay whose
  Cancel still reported success, so a song that was never downloaded was marked saved.
  The browser save now starts the download itself (Blob + link, URL revoked later); Save
  and Save As both download under the song's name.
- With no recovery journal in the browser, closing or reloading the tab lost unsaved
  edits silently: the page now asks first (`beforeunload`, set from the editors' dirty
  state every frame).
- Playback: a Play, seek or tempo change sent while the SoundFont was still loading was
  dropped by the worklet; it now waits and takes effect when the font is loaded. A new
  SoundFont keeps the loaded song. The player reports loading until the worklet answers
  `loaded`, and a site without `assets/soundfonts/default.sf2` says so on Play instead of
  playing silence. A dropped `WebPlayer` (a failed one is restarted on Play) closes its
  `AudioContext`; browsers allow only a few.
- Browser chrome: no minimize/maximize/close buttons or window dragging in the title bar
  and no File > Quit (the browser owns the tab); the start no longer shows a red notice
  that only said how to open a file.
- Not done: `cargo xtask web` writes the smoke screenshots and `worklet-check.html` into
  the site directory (they would be deployed with it); clippy for the wasm target fails on
  older `drop_non_drop` lints in `gscore-session` (CI does not lint wasm).

## 2026-11-05 — M21: multi-track scores and editing across staves

- Owner's test file `testdata/basic/multi_staff.gp` (six tracks: four guitars, bass,
  drums) with Guitar Pro's pages as the reference.
- **Layout** (`gscore-layout`): `layout_tracks` (and `LayoutCache::layout_tracks`) stacks
  every shown track's staves in each system. Each measure is still laid out per track and
  cached per track; `measure/align.rs` then puts one measure of every track on one time
  grid (a gap is the larger of what its length asks and what any track's notes there
  need; a column spanning other tracks' beats keeps its own room) and lines up the key
  and time signatures and the first beat. The cache keeps the alignment per measure and
  reuses it while the tracks' layouts are the same (by identity). The top track carries
  measure numbers, sections, tempo, endings and repeat counts; every track keeps its
  barlines, repeat signs, dynamics, fermatas, beat text and beat effects. Short names
  ("el.g…", cut to fit) stand in the left margin as Guitar Pro writes them, one line
  joins all staves, the header lists each track's name and tuning, and the gap between
  tracks grows for notes above or below a staff (drum cymbals under a bass tab).
  `layout_track` is the one-track case of the same code (`score/`), so every single-track
  golden is unchanged. `Layout::tracks` holds each track's places; `Layout::hit` finds the
  track and beat under a point.
- **Multitrack line breaks**: GP7 keeps them apart from each track's (`<Score>`'s
  `ScoreSystemsLayout`, "4 4 2"); read, written back, compared on save, and used when
  several tracks are shown (a page break on any shown track applies; a track's bar limit
  does not). New command `set_score_line_break measure=N on=true|false` with undo,
  journal, CLI and property test; in the app, Bar › Line break and the palette's break
  dialog set it while several tracks are shown.
- **Model**: track edits (add, delete, duplicate, move, and their undo) now renew every
  measure stamp, since the cache keeps measures per track index (the old test that pinned
  the opposite was changed); `TrackView::short_name`; `Song::score_line_breaks`.
- **App**: View › One track / All tracks / Tracks shown (a tick per track; the cursor's
  track always shown), per tab; action `toggle_multitrack` (no default key). Clicks,
  right-clicks and drags work on any staff and move the cursor and the track picker there;
  Up past the top string and Down past the bottom one go to the track shown above or
  below, at the same time in the bar. PDF export writes the shown tracks.
- **CLI**: `render --track N` repeats for several tracks; `--all-tracks`.
- **Bench** (`cargo xtask bench`, release) gains an all-tracks case; on the owner's files:
  `master.gp` (415 bars, 6 tracks) edit and layout 3.8 ms with every track shown (0.6 ms
  one track), `battery.gp` (215 bars, 9 tracks) 2.6 ms; within the 16 ms budget.
- Tests: layout (`test_multitrack.rs`: places per track, stacking, aligned measures and
  beats, multitrack breaks, hits per track, marks once, cache reuse, errors), goldens
  `multi_staff.svg` and `multi_staff.all.svg` (owner review pending), model (multitrack
  breaks with undo, stamps on track edits), command round trip, CLI render and round
  trip, app (View menu, Up/Down across tracks, Bar menu break with undo, snapshot
  `multi_staff_all_tracks.png`).
- Differences from Guitar Pro's pages: our default spacing is wider, so Guitar Pro's
  "4 4 2" bars per line become 3 per line where 4 do not fit (5 pages instead of 3);
  later systems keep horizontal short names (Guitar Pro rotates them); no bracket or brace.
- Open: hiding empty staves, native printing, the bars-per-line limit and the Settings
  page preview for several tracks, which tracks are shown is not remembered between runs,
  inserting or deleting a bar does not move the multitrack breaks after it.

## 2026-11-05 — Hiding empty staves; score zoom and interface zoom apart

- Owner: hiding empty staves next; the other open M21 points go to the backlog (AGENTS.md
  M21). Ctrl+- shrank the whole interface while Ctrl++ zoomed only the score.
- **Empty staves** (`LayoutOptions::empty_staves`: `Shown`, `Hidden`,
  `HiddenButPrimary`): with several tracks, a track without a note in a system's bars is
  left out of that system; the first system shows every track (as Guitar Pro); the
  topmost drawn track takes the system's marks and measure numbers. A hidden track keeps
  an empty place per system (system numbers stay in step; nothing hits it; ties into a
  hidden bar are skipped). View › Hide empty staves (remembered) keeps the cursor's track
  in the app, so it stays editable; PDF export and `render --hide-empty-staves` hide every
  empty staff. On `multi_staff.gp` the empty guitars drop out of every line after the
  first.
- **Zoom**: the cause was egui's built-in keyboard zoom of the whole interface, plus our
  key reader passing egui's name of the minus key (U+2212 `−`), so `ctrl+-` never matched
  the keymap and egui shrank everything, while `ctrl++` matched and zoomed the score. Now
  egui's keyboard zoom is off; `ctrl+=`/`ctrl++`, `ctrl+-` and `ctrl+0` zoom only the
  score; Ctrl+Shift with +, - and 0 make the interface larger, smaller or normal (read
  from the key event with its position, since Shift changes the character: `+` is
  Shift+= on QWERTY, Shift+0 is `=` on QWERTZ), also in the View menu; the interface size
  is remembered in the settings.
- Tests: layout (systems with tracks left out, marks on the topmost drawn track, hits,
  the primary track kept), app (View menu option, Ctrl vs Ctrl+Shift zoom and the
  settings), key reader (Ctrl+minus), the zoom key table for QWERTY and QWERTZ.

## 2026-11-05 — Browser build plays out of the box (GeneralUser GS 2.0.3)

Landed:
- Font choice: rendered 13 General MIDI programs (piano, nylon/steel/clean/overdriven/
  distortion guitars, acoustic/finger/slap basses, strings, choir, pad, kick) through
  `SynthEngine` with GeneralUser GS 2.0.3 and FluidR3 (`gscore-audio/examples/
  render_soundfont.rs`, writes WAVs and prints RMS/peak). Every program is audible with
  sane levels; GeneralUser is about 2–7 dB below FluidR3 and has no silent or extreme
  preset, so rustysynth's modulator support is adequate for this font. The owner should
  still listen to `/tmp` WAVs of both before a public release.
- Pinned font: GeneralUser GS 2.0.3 from commit `9704918` (2026-02-23), 32 319 396 bytes,
  SHA-256 `9575028c…88cfe`, GeneralUser GS License v2.0. The `.sf2` is not committed.
  `cargo xtask web` downloads it once into `target/web-soundfont/`, verifies size and
  SHA-256 every build (in-tree SHA-256 with reference tests), and writes the file, license
  and `soundfont.json` into the site. `--soundfont PATH` (size + RIFF header checked) and
  `--no-soundfont` (silent, says so) are the alternatives.
- Browser playback loads it automatically: the manifest is fetched, the body read as a
  stream (real progress in the toolbar) and the bytes transferred to the worklet, not
  copied on the UI thread. The browser's `Output` default is the synthesizer; web.rs
  preloads it when the editor starts; the Audio tab names the bundled font and hides MIDI
  and folder-search controls.
- Service worker: SoundFonts live in `guitarscore-soundfonts`, which survives app-cache
  changes and is pruned to the manifest's current file; the manifest is network-first,
  the font cache-first, and partial (206) responses are never cached.
- Smoke test: the app page waits for `data-guitarscore-sound=loaded`; the worklet page
  loads the font, plays middle C through an analyser and requires non-silent samples.
  Results: font ready in ~0.6 s from page start (local server, 30.8 MB) and worklet RMS
  ~0.0073 — well above silence. Screenshot: `dist/web/smoke-worklet.png`.
- Site size: wasm 13.56 MiB raw / 5.39 gzip / 2.99 brotli, JavaScript 90 KiB, SoundFont
  30.82 MiB raw / 27.94 gzip (most hosts do not compress `.sf2`).
- Credits and docs: THIRD_PARTY row, license beside FluidR3's, About window names the font,
  `docs/web.md`, README build options, gaps.

Open: choosing another SoundFont in the browser; the worklet parses 30 MB on the audio
thread at startup (measured under 1 s; acceptable, but a worker-side parse would be
needed for a much larger font); owner listening check.

## 2026-11-05 — Review of the browser SoundFont

- Reviewed the cheaper model's `web-soundfont` work (rebased onto main as
  `web-soundfont-review`; only the devlog conflicted). It follows the handoff: pinned and
  hashed download at build time, automatic load with progress, transfer to the worklet,
  its own service-worker cache, an audible-note smoke test, credits.
- Fixed: the downloaded font was copied chunk by chunk into the app's wasm memory and
  then back out for the worklet, which grew the memory by 30 MB for good (wasm memory
  never shrinks); the chunks are now joined as a JavaScript array and transferred.
  `--soundfont PATH` copied GeneralUser GS's license next to someone else's font; it no
  longer does (the build says to add the font's own license). The About window gave any
  font GeneralUser's license, and the toolbar named the bundled font even when the site's
  manifest names another; both use the manifest's name now. The smoke-test marker said
  `loaded` when the audio output never started. The Audio tab was one function with the
  whole desktop code re-indented in a `cfg` block; now a desktop and a browser function,
  the desktop one unchanged from main.
- Also fixed an older race in the desktop synthesizer: it cleared "loading" before it
  stored the error of a broken SoundFont, so for a moment a failed font looked loaded
  (`the_player_reports_a_bad_soundfont_without_blocking` failed now and then).
- Checked: `cargo xtask ci`; `cargo xtask web --smoke` (font loaded 0.6 s after page start
  locally, worklet RMS 0.0074). The owner's listening check of GeneralUser GS is still open.

## 2026-11-05 — Browser build in an nginx image

- `Dockerfile` (nginx:1.29-alpine) copies `dist/web` and `docker/nginx.conf`; it builds
  nothing (owner: build in GitHub Actions, copy the artifacts). The release workflow's new
  `web` job runs `cargo xtask web` and pushes `ghcr.io/<owner>/<repo>-web:<tag>` and
  `:latest`. Checked locally: image 176 MB, wasm served as `application/wasm` with gzip,
  the SoundFont cached for a year, everything else revalidated. The workflow job itself
  is untested until the next pre-release.

## 2026-11-05 — File › Export: MIDI, GP7/GP6/GP5/GP4/GP3 and PDF together

- The single "Export PDF…" entry became a File › Export submenu: PDF (the printed score),
  MIDI, Guitar Pro 7 (`.gp`), Guitar Pro 6 (`.gpx`), 5 (`.gp5`), 4 (`.gp4`) and 3
  (`.gp3`). The app had no MIDI export at all; the CLI's `midi` subcommand already did.
- MIDI export builds the same `Sequence` playback uses (all tracks, tempo changes,
  repeats, techniques) and writes a format-1 SMF in the background; the status bar says
  how many tracks and measures went in.
- Score export goes through the new `gscore_session::export`: serialize with
  `gscore_io`, read the bytes back in memory to list what the target format drops, then
  write next to the target and move it into place (no half file). Older GP versions warn
  in the menu and the status bar reports how many details they could not keep; files that
  cannot be read back at all are an error, not a silent conversion.
- Tests: `gscore-session::export` writes and replaces a GP3/GP4 file, reports no losses
  for an empty song and never leaves a temporary file; `ExportFormat` keeps the menu's
  extensions unique and maps the score entries to their `FileFormat`.

## 2026-11-05 — Dialog indicator on every dialog tile; beat text in Staff

- The small corner triangle that the technique tiles use for "this opens a dialog" now
  marks every tile that opens one: all twelve Staff tiles (tempo, time and key signature,
  section, repeats, endings, swing, fermata, barline, break, bars per line) and beat text.
  `tiles::dialog_tile` draws it; faces themselves are unchanged, so dialog choice rows
  (which reuse `paint_face`) stay clean.
- Beat text moved from Note input to the Staff section: it is a bar-level mark that opens
  a dialog, like its neighbours. `t` opens it (as in Guitar Pro); tapping moved to
  `shift+t` (`T`), since `t` used to be tapping. The dialog focuses its text field in the
  first frame, so typing continues straight away and Enter still applies. Escape cancels.
- The score keys are now also held back while a palette effect editor is open (bend,
  whammy, harmonic, grace, trill, tremolo, beat text, fingering), not only while a staff
  dialog is: with the text field unfocused, `r` or `x` used to edit the score behind the
  dialog.
- Tests: the app snapshots of the staff dialogs and the page updated (the corner marks and
  the moved tile), the keymap still parses (`t` and `T` are distinct bindings).

## 2026-11-05 — A chosen instrument symbol per track

- The track panel's Instrument section gained **Symbol**: a menu of fifteen symbols
  (electric and acoustic guitar, bass, drums, piano, organ, synthesizer, voice, violin,
  trumpet, saxophone, flute, banjo, ukulele, other), each drawn with the painter like the
  toolbar icons. The icon shows next to the menu, in the panel header, in the song grid
  and in the mixer; the symbol is only the picture, the preset still decides the sound.
- Until now the icon was *derived* from the General MIDI program and the GP7 instrument
  name, which is still the fallback when no symbol is chosen (`instrument::derived`).
- Unusually for the model, the symbol is stored separately from `instrument_type`: that
  type also decides whether a track has a tuning and tablature, so choosing a violin icon
  on a guitar track must not turn its tab into "other". `Track` gains `symbol`, written as
  the GPIF property `GuitarScoreSymbol` and read back (`docs/guitarpro.md` change 38);
  GP3-5 export drops it, like other GuitarScore-only data.
- `Command::SetTrackSymbol` (`set_track_symbol track=… symbol=…`) with apply-then-undo
  coverage, a `panic_hunt` value and `settings_differences` so a test notices the change.
- Tests: symbol ids are unique and round-trip; a chosen symbol wins over the derived one;
  GPIF write/read keeps it and leaves the instrument type alone; the app snapshots updated.

## 2026-11-05 — Tuning in its own dialog

- The track panel's inline string list (one note menu per string, plus count and capo) did
  not read as a tuning and made the panel long. It is now a **Tuning…** button under the
  preset (disabled for tracks that are not string instruments), showing the current
  tuning's name.
- The dialog shows the strings as a headstock (string 1, the highest, at the top; lower
  strings drawn thicker), one note menu and − / + per string, a **Common tuning** menu
  with sixteen presets across 4 to 8 strings (E/Eb/D standard, Drop D, double drop D,
  DADGAD, Open C/D/G, B standard, Drop A, the 4/5-string bass tunings), the string count,
  the capo, and Reset to standard. OK turns the whole draft into one `Command::Batch`, so
  it is one undo step. A string count that would remove strings carrying notes is refused
  with the model's message and the dialog stays open.
- Tests: the panel test drives the capo and string count through the dialog; a new
  snapshot (`tuning_dialog`) covers the layout; preset/default tuning lengths are unit
  tested.

## 2026-11-05 — A visible rename button; rename dialog in the common layout

- Clicking the track's name already edited it in place, but nothing said so. The track
  panel header now has a pencil button ("Rename track") next to the name; it opens the
  same rename window the grid's context menu uses. The in-place edit stays.
- The rename window used egui's default frame, title bar and button row, unlike every
  other dialog. It now uses `style::window_frame` with the custom title bar, a hint line,
  a full-width field focused at once, and Cancel / accent Rename buttons, with Escape and
  Enter. A new pencil icon (`Icon::Rename`) is drawn like the other toolbar icons.
- Tests: the panel test renames through the button; a snapshot (`rename_track_dialog`)
  covers the window; the string-tuning test drives the new tuning dialog.

## 2026-11-05 — A tuner with a sine reference tone

- A tuning-fork button left of Settings opens the **Tuner**: a drawn headstock and tuning
  fork, the chosen note's name and frequency (A4 = 440 Hz by default), a picker over C1 to
  C8, the shown track's open strings as quick choices, and Play/Stop (Space toggles too).
- The tone is a new `gscore_audio::TunerPlayer`: a 12 ms faded sine at 0.25 of full scale
  on the default device. Like the synthesizer it opens the device on its own thread, its
  stream lives in a worker thread, and the audio callback only reads two atomics (the
  frequency and whether it should sound), so it never allocates, locks or blocks. A device
  failure is reported in the window; editing goes on. Closing the window fades it out and
  detaches the worker (never waiting for a device on the UI thread). The browser build has
  a stub that says the tuner is not available there yet.
- Tests: the note-to-frequency mapping (A4, the octave, the low E) and the audible-range
  guard; a new `tuner_dialog` snapshot covers the window and the picture.

## 2026-11-05 — Section titles in the grid's master row

- Section titles used to sit at the bottom of the ruler, where repeat signs (`|:`, `:|`),
  alternative-ending labels and the repeat count (`×2`) are drawn too, so a marker could
  clip into them. They now go into the master row below all tracks, clipped at the next
  section's marker and free to run into the room after the last measure. The ruler keeps
  only the colored start line, which still lines up with the measure.
- Tests: a new `grid_section_title` snapshot adds a "Chorus" section and shows it in the
  master row under its measure, clear of the number.

## 2026-11-05 — Dynamics palette without the status line

- The Dynamics section no longer prints "In effect here: mf (mezzo-forte)" or the "Fades"
  caption. The highlighted dynamic tile already says what the selection has, and the
  hairpin tiles below read as fades on their own. `DynamicsSnapshot` no longer computes the
  cursor's dynamic; its test drops that assertion. Snapshots updated.

## 2026-11-05 — Tuner: the Stop button and semitone buttons

- Stop did nothing: it cleared the pending note instead of telling the player to stop. The
  window now passes a `ToneRequest` (play or stop) and acts on both.
- Small − / + buttons beside the note move it a semitone, clamped to the picker's C1–C8.
- Tests: the semitone step and its clamping at both ends.

## 2026-11-05 — Export menu width

- The Export submenu no longer carries a long explanation under the formats: it stretched
  the menu to that line's width. The entries say enough, and the older formats still
  explain their losses in a tooltip.

## 2026-11-05 — Larger toolbar icons

- The toolbar's icon buttons draw their glyphs larger (a 3.5 px shrink instead of 6) while
  keeping the same layout size. Making the buttons themselves wider pushed the zoom group
  under the player at a 1000 px window; the panel's icon buttons keep the compact size.
- Snapshots updated.

## 2026-11-05 — Instrument symbols as a grid

- The track panel's Symbol dropdown became a grid of the fifteen symbols: one click
  chooses, like the palette tiles. The selected one is filled with the accent color.
- Tests: the panel test chooses Banjo and undoes it; snapshots updated.

## 2026-11-05 — Brushes and arpeggios in a dialog

- The brush and arpeggio tiles (and keys) open a dialog instead of toggling directly: the
  direction (with the engraved sign), **How quick** (1/4 to 1/64) and whether the spread
  **starts on the beat** or **ends on it** (its last string lands on the beat). OK sets the
  stroke on every selected beat; Remove clears it; one undo step.
- Model: `Stroke { direction, speed, on_beat }`; `BeatEffect::BrushStroke` and `Arpeggio`
  carry it. `BeatView::stroke(kind)` reads it; the direction-only accessors stay for the
  layout and the dump. The brush's existing GP3-5 speed field is used; the arpeggio's speed
  and both timings are GuitarScore additions (`GuitarScoreBrush`/`GuitarScoreArpeggio`
  GPIF properties), so GP7 keeps them and GP3-5 keep the brush speed only.
- Playback: the spread length is `4/ speed` quarter notes (1/32 for a brush, 1/8 for an
  arpeggio by default) instead of two fixed constants, and "ends on the beat" moves the
  roll earlier so the last string lands exactly on the beat.
- Tests: model round trip of a stroke's speed/timing; playback onset tests for a 16th
  spread and for an arpeggio ending on the beat; GP7 round trip of both strokes and GP5 of
  the brush speed; an app test sets 1/16 and "ends on the beat" through the dialog, with a
  new `brush_dialog` snapshot.

## 2026-11-05 — Every dialog closes with Escape

- The windows had grown different habits: About, History, Settings and New song ignored
  Escape. They share `style::dialog_body` now: the standard inner frame and margins, and
  "Escape closes me". Shortcuts (which has its own filter field and key capture) closes on
  Escape too, after an active capture has been cancelled by the first press.
- Shared modal behaviour: the score's keys are held back while About, History, New song,
  the rename window or the tuning dialog is open, not only for the other dialogs, so a
  stray key no longer edits the song behind them.
- Tests: one test opens About, Settings, History and New song and closes each with Escape.

## 2026-11-05 — The header is the track's property strip

- The first line of the track panel is now interactive: the color bar opens a **color
  picker** (the presets and the custom picker, moved out of the Appearance section), the
  instrument icon opens the **symbol picker** (the grid, instead of a row of fifteen
  symbols in the panel), and the name still edits in place (with the pencil button as the
  second way). The Appearance section keeps only the staves.
- Every tile that opens a dialog now uses `tiles::dialog_tile`, so the corner mark also
  appears on brushes and arpeggios (and stays on the bend, whammy, harmonic, grace, trill,
  tremolo and fingering tiles).
- Tests: the preset-color test opens the color picker first; new `symbol_picker` and
  `color_picker` snapshots; snapshots updated.

## 2026-11-05 — Review: MIDI export against Guitar Pro, stroke timing, tuner picture

- MIDI export is now checked against Guitar Pro's own `.mid` export for every reference
  in `testdata/basic` (`.gp` and `.gpx`): same notes (within a tick) and programs. Two
  differences are on purpose and allowed for: we play pick slides, trills and tremolo
  picking as notes where Guitar Pro bends one, and we strum brushes and pick strokes where
  its export strikes the chord at once; we also send no program change on the drum
  channel.
- The check found a playback bug: Guitar Pro 7 stores a drum note as an articulation
  number, and 44 of them (half-open hi-hat 92, ride edge 93, chokes, cowbell and conga
  variants, ...) are not General MIDI sounds. They played as the wrong sound or nothing;
  `gscore-play/src/drums.rs` maps them as Guitar Pro's drum kit does, for main notes and
  flams. GP6 drum notes are not read at all yet (`docs/gaps.md`).
- MIDI export wrote only the tracks that were audible: a muted track, or every track but
  the soloed one, was missing from the file. It now writes every track.
- A brush or arpeggio that ends on the beat did not land on it when its notes were shorter
  than the spread (the onsets were clamped to the note length), nor on the first beat of
  the song (no room before it: the spread was cut at tick 0 and the strings landed after
  the beat). Starting earlier only lengthens a note, so nothing is clamped now, and with no
  room before the beat the spread shrinks to what there is.
- The tuner shows the tone as a sine wave instead of the headstock and tuning fork:
  denser for higher notes, in the accent color and moving while it sounds, gray at rest,
  fading out at both ends. The open-string buttons show which one is chosen; Space no
  longer toggles the tone a second time when a button has focus; Play opens the audio
  device again after it failed instead of keeping the broken one.

## 2026-11-05 — Instrument symbols redrawn

- All fifteen symbols are redrawn as flat one-color silhouettes in one shape language:
  the guitars, bass, ukulele and banjo lie on the same diagonal (head up to the right,
  scaled to the box's diagonal), each with one distinguishing feature: horns and a solid
  body (electric), an outline with a soundhole (acoustic), a longer neck and four pegs
  (bass), a small outline (ukulele), a round rim with a bridge (banjo). Drums are a
  snare with crossed sticks; piano a keyboard; organ pipes with tapered feet; synth a
  panel with knobs over keys; trumpet, saxophone, flute, a microphone, a violin with its
  bow, and two beamed eighths for "other".
- A `Pen` in `instrument.rs` draws in the symbol's own turned coordinates, so the shapes
  are written once and stay crisp from 18 px up. The new `instrument_symbols` snapshot
  shows every symbol at 18, 24 and 64 px on light and dark.
- Derived symbols: GM programs 24 and 25 (nylon and steel string) and instrument names
  with "acoustic", "nylon" or "steel" now get the acoustic guitar; 26-31 stay electric.

## 2026-11-05 — Dynamics clear the notes they span; CI snapshot tolerance

- A dynamic under low notes ran into them: it only avoided the lowest head of its own
  column, while a mark like `fff` spans the neighbouring notes and their accidentals,
  and where the fixed gap above the tab ran out it was pulled back up into the notes.
  Now each column knows its lowest ink (`MeasureLayout::column_depth`: notehead, accent
  or staccato below it, accidental, a stem pointing down), a dynamic goes below the
  deepest column its glyph spans, and the gap between notation and tab grows in a system
  that needs it (for its dynamics, or for notes hanging lower than the gap allows) instead
  of being the same everywhere. Other systems keep the usual gap.
- Test: alternating `f`/`fff` under low eighths with accidentals; no dynamic's box
  touches any other glyph, and every one stays above the tab. Golden SVGs: the
  eight-string example gets one space more above its tab, three dynamics of
  `multi_staff` move a fraction of a space lower.
- CI: GitHub's software renderer failed every app snapshot by 1-3 antialiased pixels;
  `kittest.toml` now allows 10 (a real change fails hundreds).

## 2026-11-05 — A long track name no longer widens the track panel

- After renaming a track to a long name the track panel grew: the name label was
  shortened to all the width left, and the rename button after it stuck out by its own
  width, which an egui side panel answers by growing (987 to 1019 px in the test). The
  rename button now takes its place at the header's right edge first and the name is
  shortened to what remains; the in-place name field is sized to the room left, its
  margin included.
- Test: the track panel test checks that the full-width tuning button keeps its right
  edge while a name is edited and after a long name is entered.

## 2026-11-05 — Staff test file, slash notation, five views, metronome volume

- **Tuplets no longer change note values.** The dot and tuplet tiles changed every
  selected beat to the palette's value, so taking a triplet off a previously eighth note
  left a quarter. They now keep each beat's written value and change only its dots or
  tuplet; a tab-only staff repeats the tuplet numbers (3, 5, 6, 7) it used to hide.
- **Legato slides** draw only their slur and "sl."; the shift-slide's straight line no
  longer crosses it. Golden SVGs updated.
- **Slash notation** is a third per-track staff (GP7 `PartConfiguration` bit 2, saved and
  loaded): rhythm slashes with stems, beams, rests and tuplets, combinable with notation
  and tab. The track panel has three icon switches and always leaves one on.
- **Five score views** (View menu, saved in settings, old `page_view` read as either page
  vertical or screen vertical): page vertical, page horizontal, page grid, screen vertical
  and screen horizontal (the score is laid out on one line and scrolls right).
- **Metronome volume** is a knob in the mixer view only; it scales the click velocities
  (0 is silent) and is saved in the settings. The grid view has no knob.
- **The edit palette** stays on the cursor and selection from when playback started,
  instead of following the playing cursor.
- **`testdata/basic/staff.gp`** now drives tests for clef changes (treble, bass, alto,
  tenor), free time (parenthesized signature and "Free time"), simile marks (simple and
  both halves of a double), octave signs (8va/8vb/15ma/15mb), chord diagrams (name and
  shape), repeats, alternative endings, key changes and time signatures. Clefs, ottava and
  simile marks are written back; chord diagrams are read and drawn but not yet written.
- Tests: `cargo test --workspace`, clippy `-D warnings`, `cargo xtask layers`; the app
  snapshots were regenerated for the three staff icons.

## 2026-11-05 — Clef and free-time editing; slash and playback-view fixes

- The Staff section of the edit palette gained **Clef** and **Free time** tiles. The clef
  dialog chooses treble, bass, alto or tenor for one track's bar; free time turns the
  metronome off for the bar and writes its signature in parentheses. Both are ordinary
  commands (`set_clef`, `set_free_time`) with undo, parse lines, panic-hunt coverage and a
  UI test.
- Slash notation alone now draws a start line as tall as its five-line staff (it borrowed
  the tab staff's six-string height), and a slash staff's clef is included in the prefix
  even beside a tab staff, so a repeat sign on the first bar no longer runs into it. The
  rhythm slashes are thicker and their stems leave the slash at its end.
- Playing in a side-by-side page view now scrolls to the whole page; the screen views
  scroll just enough to show the playing column. Only the printed vertical page view still
  turns over line by line, so the page or screen no longer slides partly out of view when
  the music wraps.

## 2026-11-05 — Bar-repetition tiles; free time as a switch

- The Staff palette has **Bar repeat** and **Double bar repeat** icons. The first toggles a
  one-bar (simple) simile mark; the second sets the two halves of a two-bar repetition on
  the current and next bar as one undoable batch, and clears the pair again when clicked.
  Backed by `set_simile_mark` (parse, panic-hunt and model/UI tests).
- Free time is now a one-click switch tile with a dotted-line icon instead of a dialog; the
  clef tile's glyph was shrunk to fit its tile.

## 2026-11-05 — Palette icons, bar repeats, slash staff and mixer review

- **Palette icons fit their tiles**: a test paints every tile face (staff, note input,
  effects, dynamics, tuplets, staff switches) and fails when ink reaches the tile's edge;
  a snapshot (`palette_faces`) shows them all. Music glyphs are centered on their ink, so
  the slanted *f*, *ff* and *fff* sit in the middle like the other dynamics.
- **Staff palette**: clef, key, time signature and tempo come first; beat text sits next to
  the section tile. The double barline, the bar repeats and free time are on/off switches
  without the dialog corner (the barline dialog is gone); free time shows a dashed barline.
- **Bar repeats** use Bravura's signs as Guitar Pro prints them: a one-bar sign in the middle
  of its bar, a two-bar sign once on the barline between its pair, the bars' own notes
  hidden. They **play** the bars they repeat (repeats of repeats followed), matching Guitar
  Pro's MIDI export of `staff.gp` (now part of the MIDI reference test). The tiles are only
  enabled where there are bars to repeat, and setting or clearing one never leaves half a
  pair behind.
- **Free time** bars end with a dashed barline, as in Guitar Pro.
- **Key changes** inside a line keep a small gap after the barline (the first flat touched
  an end-repeat's thick line).
- **Slash notation** is drawn as Guitar Pro does: the top staff, one line, no clef or key,
  stems up from the slash's upper end (down from the lower end in voice 2), hollow slashes
  for halves and wholes, a whole rest hanging from the line; one line joins it to the
  staves below at each system start. With slash notation alone, the time signature sits on
  its line. GP7's `PartConfiguration` bit 2 is read as the slash staff again: Guitar Pro's
  print of `staff.gp` (bit set, with notation and tab) shows all three staves.
- **Track panel**: the three staff switches are palette-style tiles (treble clef, TAB clef,
  rhythm slash) without the "Show" label; a chosen tile that cannot be switched off stays
  legible.
- **Rest tile**: a second click on rests it just made brings their notes back (undo)
  instead of doubling the rest's length.
- **Mixer**: the metronome strip has a fader and level field like the others and a
  metronome symbol; the master strip shows a small waveform. New `mixer_view` snapshot.
- **Side-by-side page views while playing**: a page taller than the window no longer leaves
  the playing line out of view after turning to the next page. A page that fits is still
  shown whole; a taller one is scrolled to with the playing line at the top, as the
  vertical view turns over.
- **Vertical screen view while playing** turns over like the printed page view: when the
  playing line leaves the view, it goes to the top instead of the view creeping down a
  line at a time.
