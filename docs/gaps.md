# Known Gaps

Ranked known shortfalls, most important first. Updated 2026-11-01 (M1).

1. **GP7+ tunings are wrong (guitarpro bug, data loss once saving exists).** The GPIF
   importer builds tracks with `..Default::default()`, which already holds E-standard
   6-string tuning, so the staff's `Tuning` property is never read: every GP7 track reports
   6 strings in E standard (seen on all 238 tracks of the owner's files). When the code path
   does read a tuning (GP6 track properties), it also numbers the strings lowest-first,
   reversed. Bass and drum tracks therefore look like guitars. Fix in guitarpro (vendor or
   upstream) or work around in gscore-io; owner decision pending.
2. **Drum and piano tracks are mapped onto tab strings.** GP7 drum notes land on string 1
   (several per beat), piano notes get negative frets, and `percussion_track` is not set.
   Reported as issues, kept unchanged. Probably partly the same importer limitation.
3. **`guitarpro` should be vendored long term** (owner decision, 2026-11-01; not during
   scaffolding). It is young and single-maintainer, and pulls `zip` 9 with default features,
   including `zstd-sys` (C code). `gscore-io` passes the wasm check today only because clang
   is available to compile that C code for wasm.
4. **The model is read-only.** No edit methods yet; they arrive with the command layer (M5).
   Only the initial tempo is exposed; tempo changes, repeats, effects and lyrics are kept in
   the file but have no accessors yet.
5. **Saving is not implemented** (returns an "unsupported" error). M6.
6. **No recovery journal or autosave yet** (crash reports exist since M0). M5.
7. **No fuzz/corrupt-file tests** for the importer beyond a few garbage inputs; no
   `testdata/` files yet (owner will provide self-made files). M2.
8. Layout, playback, editing, session and app crates are placeholders.
