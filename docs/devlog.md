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
