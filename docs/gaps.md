# Known Gaps

Ranked known shortfalls, most important first. Updated 2026-11-01.

1. **wasm build of `gscore-io` unverified, probably broken.** `guitarpro` depends on `zip` 9
   with default features (pulls `zstd-sys`, C code). Options: disable zip's default features
   upstream or in a vendored copy, or exempt `gscore-io` from the wasm rule. Owner decision.
2. **Import maps almost nothing.** Only GP5 (`read_gp5`) is tried; tuning is hard-coded to
   6-string standard, time signature to 4/4, durations to 1.0, artist to "Unknown Artist".
   Violates "no instrument assumptions". M1/M2 work.
3. **Saving is not implemented** (returns an "unsupported" error). M6.
4. **No crash containment yet:** no panic hook/crash log, no recovery journal, no autosave.
5. **No fuzz/corrupt-file tests** for the importer beyond three garbage inputs; no
   `testdata/` files.
6. Layout, playback, editing, session and app crates are placeholders.
