# Vendored `guitarpro`

- **Upstream:** https://github.com/slundi/guitarpro (crate `guitarpro/`)
- **Commit:** `b62be7c5e4bedb0c6eb283ac40aa8371adab5c2a` (version 0.4.4)
- **License:** MIT, see `LICENSE` (copied from the upstream repository root)
- **Vendored on:** 2026-11-01, owner decision: the GP7 tuning bug below loses data, and
  GuitarScore plans to own this code long term.

## What was copied

`guitarpro/src/` and `guitarpro/README.md`, plus the repository's `LICENSE` and
`rustfmt.toml`. Not copied: upstream's `src/tests/` and `src/test_audit.rs` (they need
test files from upstream's `test/` and `samples/` directories, whose licensing is unclear),
the CLI, web server, benches and docs. Removed `src/model/optimized/CLAUDE.md`
(upstream's instructions for AI agents, which would be picked up when working here). The 17 self-contained inline test modules are kept.

`Cargo.toml` is rewritten: same dependencies and versions, `publish = false`, and
`[lints]` that allow everything, because the code is not yet held to the workspace's
no-panic rules. It is only called from behind `catch_unwind` in `gscore-io`.

## Changes to upstream (search for "GuitarScore" in the sources)

Each change should be offered upstream.

1. `src/lib.rs`: removed `mod tests;` and `mod test_audit;` (files not vendored).
2. `src/io/gpif_import.rs`, GP6/GP7 track import: reset `track.strings` before reading the
   tuning. `..Default::default()` filled it with E-standard 6-string tuning, so the
   "no tuning yet" checks never fired and every GP7 track came out in E standard.
3. `src/io/gpif_import.rs`, `extract_tuning`: reverse the pitch list. GPIF lists pitches
   from the lowest string up; the legacy model's string 1 is the highest.
4. `src/io/gpif_export.rs`: write the pitch list lowest string first (mirror of 3).
5. `src/model/legacy/track.rs`: new field `Track::has_tuning` (default `true`). Set to
   `false` by the GPIF importer when the file has no tuning (GP7 vocal/piano tracks); the
   placeholder E-standard strings stay because code indexes `strings`. The GPIF exporter
   writes a `Tuning` property only when `has_tuning` is true.
   `src/convert/legacy/mod.rs` sets it to `true` (unchanged behaviour).
6. `src/io/gpif.rs`, `src/io/gpif_import.rs`, `src/model/legacy/track.rs`: read the GP7
   `<InstrumentSet>` (type and name) into new fields `Track::instrument_type` and
   `Track::instrument_name`; tracks of type `drumKit` are marked `percussion_track`.
   Not written back by the exporter yet. `src/convert/legacy/mod.rs` sets both to `None`.
7. `src/io/gpif.rs`, `src/io/gpif_import.rs`, `src/model/legacy/track.rs`: read the
   tuning `<Label>` into new field `Track::tuning_label` (non-empty only). Not written back
   yet. `src/convert/legacy/mod.rs` sets it to `None`.
8. `src/io/gpif_import.rs`, `src/io/gpif_export.rs`: score metadata uses the same fields as
   the GP3-5 reader: `Tabber` <-> `writer` (was: import wrote the composer into `writer`
   and the tabber into `transcriber`; export read `transcriber`), `Instructions` <->
   `instructions` (was `comments`). `WordsAndMusic` (new `Score::words_and_music`) fills
   empty `words`/`author`.
9. `src/io/gpif_import.rs` (`flip_note_strings`), `src/io/gpif_export.rs`: note string
   numbers are converted between GPIF order (string 1 = lowest) and the legacy order
   (string 1 = highest) on import and export. Upstream stored GPIF order unchanged, which
   only matched its equally reversed tunings (change 3). Notes outside the track's strings
   stay unchanged.
10. `src/io/gpif_import.rs`: regression tests `tuning_tests` for 2–9.
11. `src/io/gpx.rs`: memory limits for untrusted input (`MAX_UNPACKED_BYTES`, 128 MiB):
    bounded reads of `score.gpif` and embedded audio from GP7 zips (zip bombs); BCFZ
    rejects declared sizes above the limit (was: reserved up to 2 GB); BCFS clamps file
    sizes (a negative size became a huge `usize`). Failed allocations abort the process,
    which `catch_unwind` cannot contain. `read_gpx` also accepts uncompressed BCFS.
    Tests in `limit_tests`.
12. `src/io/gpx.rs` (`read_le_i32`) and `src/model/legacy/headers/io.rs` (directions):
    index panics on truncated GPX containers and out-of-range GP5 direction measure numbers,
    found by the truncation/mutation test in `crates/gscore-io/tests/test_testdata.rs`.
13. Removed the modules GuitarScore does not use (about 21k lines): `analysis/`,
    `convert/`, `model/{optimized,musicxml,mnx,mscz}/`, `io/mscz/`, and their re-exports in
    `lib.rs`. (Change 5's edit to `convert/legacy/mod.rs` went with them.)
