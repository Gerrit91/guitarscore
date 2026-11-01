# Known Gaps

Ranked known shortfalls, most important first. Updated 2026-11-01.

1. **`guitarpro` should be vendored long term** (owner decision, 2026-11-01; not during
   scaffolding). It is young and single-maintainer, and pulls `zip` 9 with default features,
   including `zstd-sys` (C code). `gscore-io` passes the wasm check today only because clang
   is available to compile that C code for wasm.
2. **Import maps almost nothing.** Only GP5 (`read_gp5`) is tried; tuning is hard-coded to
   6-string standard, time signature to 4/4, durations to 1.0, artist to "Unknown Artist".
   Violates "no instrument assumptions". M1/M2 work.
3. **Saving is not implemented** (returns an "unsupported" error). M6.
4. **No crash containment yet:** no panic hook/crash log, no recovery journal, no autosave.
5. **No fuzz/corrupt-file tests** for the importer beyond three garbage inputs; no
   `testdata/` files.
6. Layout, playback, editing, session and app crates are placeholders.
