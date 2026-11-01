# Known Gaps

Ranked known shortfalls, most important first. Updated 2026-11-01 (M1).

1. **`guitarpro` should be vendored long term** (owner decision, 2026-11-01; not during
   scaffolding). It is young and single-maintainer, and pulls `zip` 9 with default features,
   including `zstd-sys` (C code). `gscore-io` passes the wasm check today only because clang
   is available to compile that C code for wasm.
2. **Import reads only GP5.** `read_gp5` is the only parser tried; GP3/4, GPX and GP7+
   need format detection (M2). Everything the parser reads is kept (the model wraps it).
3. **The model is read-only.** No edit methods yet; they arrive with the command layer (M5).
   Only the initial tempo is exposed; tempo changes, repeats, effects and lyrics are kept in
   the file but have no accessors yet.
4. **Saving is not implemented** (returns an "unsupported" error). M6.
5. **No recovery journal or autosave yet** (crash reports exist since M0). M5.
6. **No fuzz/corrupt-file tests** for the importer beyond three garbage inputs; no
   `testdata/` files. M2.
7. Layout, playback, editing, session and app crates are placeholders.
