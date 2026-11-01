# Known Gaps

Ranked known shortfalls, most important first. Updated 2026-11-01 (M1).

1. **`gscore-gp` is not held to the workspace rules yet.** About 10k lines derived from
   guitarpro still use `unwrap`, indexing and `as` casts; a crate-level `allow` exempts them.
   Several files exceed 400 lines (`io/gpif_import.rs` 1,040, `io/gpif_export.rs` 720).
   Bring it under the lints module by module, starting with the parsers of untrusted input.
   Until then the reader runs behind `catch_unwind` in gscore-io, which does not exist on
   wasm.
2. **Instrument data and tuning labels are read but not written.** GP7 `<InstrumentSet>`
   (type, name) and the tuning `<Label>` are read into the track, but the GPIF exporter
   writes neither. Must be handled before saving (M6).
   Drum and piano notes are still mapped onto tab strings (several drum notes on string 1,
   negative piano frets); reported as issues, kept unchanged.
3. **GP6 instrument not read.** GP6 stores only an instrument id (`<Instrument
   ref="s-gtr6"/>`), no type or name; such tracks count as string instruments unless they
   are percussion. (The GP6 tuning order, unverified before, is now covered by the GPX
   test file.)
4. **The model is read-only.** No edit methods yet; they arrive with the command layer (M5).
   Only the initial tempo is exposed; tempo changes, repeats, effects and lyrics are kept in
   the file but have no accessors yet.
5. **Saving is not implemented** (returns an "unsupported" error). M6.
6. **No recovery journal or autosave yet** (crash reports exist since M0). M5.
7. **Little test data.** One song (`testdata/basic/steel_guitar_dadgad`) as GP7, GPX and
   GP5, with import and `info` reference tests and truncation/byte-flip robustness. No
   GP3/GP4 files, no effects, multiple tracks or voices yet, and no real fuzzer
   (cargo-fuzz). M2.
8. Layout, playback, editing, session and app crates are placeholders.
