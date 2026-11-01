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
5. `src/io/gpif_import.rs`: regression tests `tuning_tests` for 2–4.
