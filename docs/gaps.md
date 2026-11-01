# Known Gaps

Ranked known shortfalls, most important first. Updated 2026-11-01 (M1).

1. **Vendored `guitarpro` is not held to the workspace rules yet.** About 32k lines with
   `unwrap`, indexing and `as` casts; all lints are allowed for it (`vendor/guitarpro`).
   It is only called behind `catch_unwind`, which does not exist on wasm. Bring it under
   the lints module by module, starting with the parsers of untrusted input.
2. **Instrument data and tuning labels are read but not written.** GP7 `<InstrumentSet>`
   (type, name) and the tuning `<Label>` are read into the track, but the GPIF exporter
   writes neither. Must be handled before saving (M6).
   Drum and piano notes are still mapped onto tab strings (several drum notes on string 1,
   negative piano frets); reported as issues, kept unchanged.
3. **GP6 (`.gpx`) tuning order unverified.** The fix assumes GP6 lists pitches lowest
   first, like GP7; no GP6 file was available to check.
4. **The model is read-only.** No edit methods yet; they arrive with the command layer (M5).
   Only the initial tempo is exposed; tempo changes, repeats, effects and lyrics are kept in
   the file but have no accessors yet.
5. **Saving is not implemented** (returns an "unsupported" error). M6.
6. **No recovery journal or autosave yet** (crash reports exist since M0). M5.
7. **Little test data.** One owner file (`testdata/basic/steel_guitar_dadgad.gp`, GP7) with
   import and `info` reference tests, plus truncation/byte-flip robustness over it. No GP3-5
   or GPX files yet, and no real fuzzer (cargo-fuzz) yet. M2.
8. Layout, playback, editing, session and app crates are placeholders.
