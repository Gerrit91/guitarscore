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
