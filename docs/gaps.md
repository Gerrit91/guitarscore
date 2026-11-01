# Known Gaps

Ranked known shortfalls, most important first. Updated 2026-11-01 (dump effect symbols).

1. **`gscore-gp` is not held to the workspace rules yet.** About 10k lines derived from
   guitarpro still use `unwrap`, indexing and `as` casts; a crate-level `allow` exempts them.
   Several files exceed 400 lines (`io/gpif_import.rs` 1,040, `io/gpif_export.rs` 720).
   Bring it under the lints module by module, starting with the parsers of untrusted input.
   Until then the reader runs behind `catch_unwind` in gscore-io, which does not exist on
   wasm.
2. **GP3-5 writing is not usable for real songs yet** (`gscore-cli roundtrip-check --to
   gp5` on the 26 band files): 18 write durations GP5 cannot express (read back as an
   invalid note value), 4 produce files that run out of data when read, 3 fail on
   artificial harmonics ("MissingState harmonic fret"). Guitar Pro's own GP5 export
   drops the second dot of a double-dotted note (the 7/16 note in `note_lengths`
   becomes 3/8, leaving measure 9 short), so the writer should drop it too and warn
   instead of failing. Per AGENTS.md section 5 (Saving), GP7 is the primary save format;
   GP3-5 and GPX writing are only offered once their round trips pass. GP7 -> GP5 also
   loses what GP5 cannot store (instrument name), which saving must warn about (M3).
   Related: our GPIF XML is about 4x larger than Guitar Pro's (7 MB vs 1.6 MB for
   `DrDr.gp`), which exceeds the GPX container limit (~4 MB, now a clear error).
   Drum and piano notes are mapped onto tab strings on import (several drum notes on
   string 1, negative piano frets); reported as issues, kept unchanged.
3. **GP6 instrument not read.** GP6 stores only an instrument id (`<Instrument
   ref="s-gtr6"/>`), no type or name; such tracks count as string instruments unless they
   are percussion. (The GP6 tuning order, unverified before, is now covered by the GPX
   test file.)
4. **`dump` does not show every effect.** It writes the usual ASCII-tab symbols: ties (`L`
   on the origin), dead notes (`x`), hammer-ons/pull-offs (`h`/`p`, chosen by the next
   note's pitch), bends (`b`/`r`, followed by the target fret: `15b17`), slides (`/`/`\`,
   direction from the next pitch, else `S`/`s`), vibrato (`~`), natural harmonics (`*`),
   pinch and artificial harmonics with their node (`7[19]`), tapped harmonics
   (`7(19)`), ghost notes (`(fret)`), accents (`>`), trills (`tr`), tremolo picking
   (`TP`), tapping (`t`) and staccato (`.`). Palm mutes and let ring go on rows under the
   staff (`PM----|`, `LR----|`). `gscore-cli legend` lists every symbol from the same
   enums the dump uses. Concepts the table lacks get letters: slap `T`, pop `P`, fade-in
   `<`, whammy `w`, brush `v`/`^`, pick stroke `V`/`n`. Grace notes, fingerings and other
   note ornaments are not drawn; wide vibrato shares `~` with slight vibrato and heavy
   accent shares `>` with accent because the model stores one flag. Hammer/pull and slide
   direction look at the next note on the same string, so a target in the next measure
   falls back to `h`/`S`/`s`; ties are matched within a measure. Drum, voice and keyboard
   tracks get no tablature. Verified against Guitar Pro's layout (not its legend) for
   `steel_guitar_dadgad`, `note_lengths` and `ornaments` (goldens in `crates/gscore-cli`).
   The GP6/GP7 whammy bar is read (`<Whammy>` and GP6's `WhammyBar*` beat properties), so
   the dive bomb in `ornaments` survives; it used to be dropped. GP5 stores no harmonic
   fret, so its pinch harmonics show `[?]`.
5. **The model is read-only.** No edit methods yet; they arrive with the command layer (M4).
   Only the initial tempo is exposed; tempo changes, repeats, effects and lyrics are kept in
   the file but have no accessors yet.
6. **Saving to files is not implemented** (only `gscore_io::export_to_bytes`). M3.
7. **No recovery journal or autosave yet** (crash reports exist since M0). M7.
8. **Little test data.** Three songs (`steel_guitar_dadgad`, `note_lengths`, `ornaments`)
   as GP7, GPX and GP5, with import, `info` and `dump` reference tests and
   truncation/byte-flip robustness — still only one track and one voice per song. No
   GP3/GP4 files, and no real fuzzer (cargo-fuzz; deferred to "Later" by the owner).
9. Layout, playback, editing and app crates are empty until their milestones.
