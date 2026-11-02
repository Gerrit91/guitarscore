# Known Gaps

Ranked known shortfalls, most important first. Updated 2026-11-01 (M7: bends and slides).

1. **`gscore-gp` is not held to the workspace rules yet.** About 10k lines derived from
   guitarpro still use `unwrap`, indexing and `as` casts; a crate-level `allow` exempts them.
   The GP7 XML writer (`io/gpif_export.rs`) is the first module under the no-panic lints
   (M3). Several files exceed 400 lines (`io/gpif_import.rs` ~1,300, `io/gpif_export.rs`
   ~800). Continue module by module, next with the parsers of untrusted input. Until then
   the reader runs behind `catch_unwind` in gscore-io, which does not exist on wasm.
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
4. **Complex bends are simplified** (later, with owner test data). The GPIF reader and
   writer keep the three-point shape (origin, middle, destination), so bend-and-release
   survives import and saving since M3 (214 notes in the owner's files were dropped
   before). The model still reports only `Up`/`Release` with the highest point, so `dump`
   writes a bend-and-release as `15b17` without the release, and the save verification
   cannot tell the two apart. GP5 bends with more points are written as first, furthest and
   last point.
4b. **`dump` does not show every effect.** It writes the usual ASCII-tab symbols: ties (`L`
   on the origin), dead notes (`x`), hammer-ons/pull-offs (`h`/`p`, chosen by the next
   note's pitch), bends (`b`/`r`, followed by the target fret: `15b17`), slides (`/`/`\`,
   direction from the next pitch, else `S`/`s`), vibrato (`~`), natural harmonics (`*`),
   pinch and artificial harmonics with their node (`7[19]`), tapped harmonics
   (`7(19)`), ghost notes (`(fret)`), accents (`>`), trills (`tr`), tremolo picking
   (`TP`), tapping (`t`) and staccato (`.`). Palm mutes and let ring go on rows under the
   staff (`PM----|`, `LR----|`). `gscore-cli legend` lists every symbol from the same
   enums the dump uses. Concepts the table lacks get letters: slap `T`, pop `P`, fade-in
   `<`, whammy `w`, brush `v`/`^`, pick stroke `V`/`n`. Grace notes, fingerings and other
   note ornaments are not drawn. The model now distinguishes wide from slight vibrato and
   a heavy from a normal accent, but the dump still prints `~` and `>` for both; beat-level
   `VibratoWTremBar` is read and drawn by layout but the dump does not print it. Hammer/pull and slide
   direction look at the next note on the same string, so a target in the next measure
   falls back to `h`/`S`/`s`; ties are matched within a measure. Drum, voice and keyboard
   tracks get no tablature. Verified against Guitar Pro's layout (not its legend) for
   `steel_guitar_dadgad`, `note_lengths` and `ornaments` (goldens in `crates/gscore-cli`).
   The GP6/GP7 whammy bar is read (`<Whammy>` and GP6's `WhammyBar*` beat properties), so
   the dive bomb in `ornaments` survives; it used to be dropped. GP5 stores no harmonic
   fret, so its pinch harmonics show `[?]`.
5. **Editing is headless and narrow (M4).** The model has checked methods to set/clear a
   note, set a beat length, insert/delete a beat and insert/delete a measure; `gscore-edit`
   turns them into commands with undo/redo, and `gscore-cli edit` runs them from `-c` or a
   script; the app edits at a cursor (#10). Still missing: selection and copy/cut/paste;
   editing tempo, time
   signatures, effects, chord diagrams and lyrics; lyric/chord/effect accessors. Every model
   edit clones the whole wrapped song (including embedded audio) to stay transactional; this
   is correct but must become a per-region copy for large scores: a single-note edit takes
   about 3.6 ms in release on `DrDr.gp` (172 measures x 16 tracks), mostly the clone plus
   validating the whole song; it should copy and validate only the touched region. In the
   app the shown track is also laid out again after every edit (about 15 ms for 236
   measures in release, more in debug builds), so key-to-screen latency on big scores is
   over one frame until both are incremental. Undo history is in memory
   only and capped by entry count (10,000), not by bytes (persisting it and a memory cap
   belong with the recovery journal in M8). Only the initial tempo is exposed.
6. **Saving is GP7 only.** `gscore_session::save` and `gscore-cli convert` refuse other
   formats until their round trips pass (#2). Whether Guitar Pro itself opens our GP7 files
   is not verified yet (the owner can check). Effects the model has no accessor for (grace
   notes, fingerings, lyrics, mix tables, ...) are not part of the save verification.
7. **No recovery journal or autosave yet** (crash reports exist since M0). M8.
8. **Little test data.** Three songs (`steel_guitar_dadgad`, `note_lengths`, `ornaments`)
   as GP7, GPX and GP5, with import, `info` and `dump` reference tests and
   truncation/byte-flip robustness — still only one track and one voice per song. No
   GP3/GP4 files, and no real fuzzer (cargo-fuzz; deferred to "Later" by the owner).
9. **Layout is a first version (M5).** `gscore-layout` draws one track as standard
   notation above tablature (as Guitar Pro does), on one endless page, with the tab's
   ornaments, line breaking and justification. Notation: clef (treble, bass for basses),
   time signatures, noteheads (cross for dead notes), ledger lines, sharps and naturals per
   measure, stems, flags, beams per beat (partial beams included), dots, rests, ties within
   a measure and tuplet numbers. Missing in notation: key signatures (notes are spelled with
   sharps), the second voice, ties across barlines, tuplet groups spanning several beats
   (each beat's beam gets its own number), grace notes, articulations and ornaments on the
   notation staff (they stay on the tab), harmonic diamonds (harmonics are written at their
   sounding pitch). Missing everywhere: repeats and endings, tempo changes, section
   markers, lyrics, chord diagrams, page breaks, multi-track systems, rhythm under the tab
   (shown in the notation instead). Ornaments that connect notes only look within one
   measure. Voice, piano and drum tracks get no notation (no tuning); drum tracks are
   refused. Text widths are estimated for non-digits; no PNG output (`rsvg-convert` works).
10. **The app edits one voice at a cursor (M8, first version).** It opens files in the
   background, shows one track's page, edits voice 1 from the keyboard (keymap file) or
   menus with undo/redo, saves as `.gp` (Save As for files in other formats; a lossy save
   asks), and asks about unsaved changes before closing or opening another file. It
   contains panics in layout, drawing and background jobs. Missing: selection and
   copy/cut/paste, entering notes in voice 2, editing effects, tempo and time signatures,
   a duration toolbar, playback from the cursor and a playhead, remembering the view state
   and window between runs, recent files, drag and drop, keyboard shortcuts for zoom, a
   multi-page view. Undo back to the saved state still counts as "changed". `clear_note`,
   `previous_measure` and `next_measure` have no default key yet (owner's decision; they
   can be bound in the keymap file), and `ctrl+s` for save is a proposal. On Windows, egui
   turns Ctrl+Insert into Copy, so `insert_measure` needs another key there. On macOS the file dialog must run
   on the main thread (rfd); it runs on a background thread here, so Open… needs a change
   there. The snapshot test needs a GPU or software Vulkan.
11. **Playback is MIDI only and plays notes, bends and slides, not the other techniques
   (M7).** `gscore-play` sequences notes (all voices, exact timing), ties, dead notes,
   harmonics, instruments, volume, pan, tempo changes, repeats, alternative endings, bends
   and shift/legato/in/out slides; `gscore-audio` sends them to a MIDI port. A note that
   bends or slides borrows a free channel with the track's program, volume and pan and a
   6-semitone bend range (RPN 0,0; wider for a wide slide, up to 24), so the rest of the
   chord keeps its pitch; when every channel is in use the note plays without the pitch
   movement and `Sequence::degraded_pitch_bends` counts it.
   Simplifications: the model stores only a bend's highest point, so a bend that releases
   again plays as a bend up and hold, and a release bend starts bent and returns to center;
   slides are pitch bends (clamped to two octaves); an "out" slide moves a whole step; a
   slide takes precedence over a bend on the same note. Not played yet: vibrato, let ring,
   palm mute, grace notes, trills, whammy bar, D.C./D.S./coda, swing, mute/solo, GP7
   mixer volume and pan (GP7 tracks use defaults). The app shows the playing measure's
   number, not a playhead on the page. Pause/resume does not re-trigger notes that were
   sounding, and stopping `gscore-cli play --port` with Ctrl+C can leave notes hanging in
   the external synth. The built-in synthesizer needs an installed `.sf2` SoundFont (none
   is bundled; `.sf3` is not supported), always uses the default audio device, and the
   app does not remember a chosen SoundFont between runs. GP7 stores keyboard and vocal notes by pitch, not
   string and fret; the reader turns them into invalid frets, so those tracks are silent
   (the owner's corpus: 8 of 59 files leave 18-116 notes out).
