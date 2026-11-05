# Known Gaps

Ranked known shortfalls, most important first. Updated 2026-11-02 (M13: track and song grid).

1. **`gscore-gp` public items are mostly undocumented.** Since 2026-11-02 the crate
   follows the workspace rules (no-panic lints, checked arithmetic, no lossy casts, files
   under 400 lines; docs/guitarpro.md change 29). Left: rustdoc on the many public model
   fields inherited from guitarpro, and dead code (fields parsed but unused are marked).
   `gscore-io` keeps `catch_unwind` around it as a second line of defense (not on wasm).
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
   test file.) GP6 drum notes are not read either: GP6 writes a drum note as a drum
   element and variation instead of a MIDI number, so `multi_staff.gpx`'s drums import
   as key 0 and play nothing (its `.gp` twin plays like Guitar Pro's MIDI export).
4. **Complex bends are simplified** (later, with owner test data). The GPIF reader and
   writer keep the three-point shape (origin, middle, destination), so bend-and-release
   survives import and saving since M3 (214 notes in the owner's files were dropped
   before). Since M14 the model exposes the full shape (`bend_shape()`), but `dump`,
   layout and the save comparison still use `Up`/`Release` and the highest point, so a
   bend-and-release is written as `15b17` without the release and the save verification
   cannot tell the two apart. Playback also uses the highest point (see #11). GP5 bends
   with more points are written as first, furthest and last point.
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
   script; the app edits at a cursor (#10). Since M14 there is selection with copy/cut/
   paste, a left edit palette (durations, ties, rests, tempo, time/key signatures, sections
   and the effect matrix) and model/command support for every effect, grace notes included (since 2026-11-04;
   GP6/GP7 grace beats that cannot be attached to a main note on their string stay
   read-only). Still missing: chord diagrams and lyrics, and accessors for the
   data the model does not expose. Edits copy
   and check only the measure they touch (M10), and the app lays out only changed measures
   again; line breaking and drawing still run over the whole track (0.85 ms for 415
   measures in release; `cargo xtask bench`). Undo history is in memory only, capped at
   10,000 entries and about 64 MiB; the recovery journal restores it after a crash (M11),
   but a normal close does not keep it.
   Only the initial tempo is exposed.
6. **Saving is GP7 only.** `gscore_session::save` and `gscore-cli convert` refuse other
   formats until their round trips pass (#2). Whether Guitar Pro itself opens our GP7 files
   is not verified yet (the owner can check). Effects the model has no accessor for
   (fingerings, lyrics, mix tables, ...) are not part of the save verification.
7. **Recovery covers the editor's commands (M11).** Every edit, undo and redo is journaled
   (checksummed lines, periodic snapshots, written on a thread of its own) and replayed
   after a crash, with the undo history. Edits are on disk within milliseconds; a crash in
   that window loses the last edit. Not journaled: the cursor and shown track, and the
   playback position; the view, window, panel, recent-file, keymap and audio settings are
   saved separately in `settings.json` (M12). Edits that do not go through `gscore-edit`
   would not be journaled (none; every command adds itself to `Command::to_line`, which the
   compiler enforces).
   A snapshot clones the song on the UI thread (about 2.5 ms on `DrDr.gp`, every 200 edits
   or 60 s). SIGTERM and system shutdown rely on the journal, not on a final snapshot.
   Only commands are wrapped in `catch_unwind` and panic-tested; layout and drawing are
   contained since M6, without an injection test.
8. **Little test data.** Three songs (`steel_guitar_dadgad`, `note_lengths`, `ornaments`)
   as GP7, GPX and GP5, with import, `info` and `dump` reference tests and
   truncation/byte-flip robustness — still only one track and one voice per song. No
   GP3/GP4 files, and no real fuzzer (cargo-fuzz; deferred to "Later" by the owner).
9. **Layout is a first version (M5).** `gscore-layout` draws one track as standard
   notation above tablature (as Guitar Pro does), on one endless page, with the tab's
   ornaments, line breaking and justification. Notation: clef (treble, bass for basses),
   key signatures at the start of every system and at changes (naturals cancel the old key;
   on the tab staff when notation is hidden), time signatures, noteheads (cross for dead
   notes), ledger lines, sharps, flats and naturals per measure (notes the key signature
   alters are spelled with its sharps or flats and need no accidental; other pitch classes
   use sharps), stems, flags, beams per beat (partial beams included), dots, rests, ties
   within a measure and across barlines (split into fragments at a line break) and tuplet
   numbers. Up to four voices are written: with two or more the first points its stems up and
   the rest down, and a notehead that would sit on the first voice's position moves right.
   Missing in notation: tuplet groups spanning several beats (each beat's beam gets its own
   number), articulations and ornaments on the notation staff (they stay on the
   tab), harmonic diamonds (harmonics are written at their sounding pitch). Grace notes are
   drawn as cue-sized heads (own accidentals and ledger lines, slashed eighth flag) and
   smaller fret numbers before their note (GP6/GP7 grace beats and GP3-5 attached
   graces) and take no time. Since 2026-11-03
   section names, tempo changes, repeat signs (with "x3" counts), alternative endings and
   dynamics are drawn, and drum tracks get a percussion staff. Missing everywhere:
   D.C./D.S./coda and other directions, hairpins, lyrics, chord diagrams and native
   printing (M21, AGENTS.md section 9; pages, page setup, breaks and PDF export are live
   since 2026-11-03, multi-track systems since 2026-11-05), rhythm
   under the tab
   (shown in the notation instead). Ornaments that connect notes only look within one
   measure. Voice and piano tracks get no notation (no tuning). Text widths are estimated for non-digits; no PNG output (`rsvg-convert` works).
10. **The app edits one voice at a cursor (M8, first version).** It opens files in the
   background, shows one track's page, edits voice 1 from the keyboard (keymap file) or
   menus with undo/redo, saves as `.gp` (Save As for files in other formats; a lossy save
   asks), and asks about unsaved changes before closing or opening another file. It
   contains panics in layout, drawing and background jobs. Since M12 it also remembers the
   view, window and panels between runs, lists recent files, opens dropped files, zooms with
   `ctrl+=`/`ctrl+-`/`ctrl+0`, has a settings window, and the cursor follows playback and
   scrolls with it. Since M14 it has a left edit palette, beat and measure selection with
   copy/cut/paste, voice 2 entry, effect editing, tempo/time/key signatures and section
   markers. Missing: a multi-page view, chord diagrams and lyrics, and
   a beat selection that crosses tracks. Undo back to the saved state still
   counts as "changed". `clear_note`,
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
   Simplifications: although M14's model exposes a bend's full points, playback still uses
   only its highest point, so a bend that releases again plays as a bend up and hold, and a
   release bend starts bent and returns to center;
   slides are pitch bends (clamped to two octaves); an "out" slide moves a whole step; a
   slide takes precedence over a bend on the same note. Grace notes are short notes just
   before their beat (at the beat when it starts the song) and take no time. Not played yet:
   trills, whammy bar, fermatas, D.C./D.S./coda. Swing and dotted
   feel play as a per-measure time warp. Mute and solo are
   played (M13): a muted track is skipped, and while any track is soloed only soloed
   tracks play. Volume and pan changes go straight to the player as MIDI CC messages while
   it plays; mute and solo rebuild the sequence at the same tick. Track volume, pan, mute
   and solo are saved in GP7 files (RSE channel-strip parameters and `<PlaybackState>`,
   M13 review). Since M12 the cursor follows the beat being played on the shown track (across
   repeats) and the page scrolls with it; pausing or stopping leaves the cursor on the last
   played beat. Clicking a beat on the page during playback continues playback from there
   (its measure's first pass); so does a click or a measure selection in the song grid.
   Inside a selection the cursor's box and column band are not drawn. Pause/resume does not re-trigger notes that were
   sounding, and stopping `gscore-cli play --port` with Ctrl+C can leave notes hanging in
   the external synth. The built-in synthesizer needs an installed `.sf2` SoundFont (release
   archives bundle FluidR3 GM, MIT, next to the binary; a source build has none, and `.sf3`
   is not supported), always uses the default audio device, and the
   app remembers the chosen SoundFont and output in `settings.json`. GP7 stores keyboard and vocal notes by pitch, not
   string and fret; the reader turns them into invalid frets, so those tracks are silent
   (the owner's corpus: 8 of 59 files leave 18-116 notes out).

12. **The track panel is basic (M15).** The right panel edits a track's name, instrument
   (General MIDI program, drum kit), color (16 presets or any color), shown staves (notes
   and tabs, tabs only, notes only), string count, capo and each string's open note (by
   name, with semitone steps), through commands with undo; sound changes are live. The song grid's
   header adds a track after the shown one, moves it up or down, and mutes or solos all
   tracks (one undo step); its last row is the master volume and pan. The master volume
   scales every track (97, Guitar Pro's untouched 0.76, leaves them as they are; a boost
   stops at the MIDI maximum) and the master pan shifts every track's pan; both are
   saved in GP7 files. "Notes only" draws no tablature ornaments (bends, slides, palm mute
   and the like live on the tab staff), and the cursor's string rows are spread over the
   notation staff. Limits: the 128-program list
   has no search field; the "SoundFont preset" is the General MIDI program (a per-track
   SF2 bank beyond the program is not stored); per-track volume and pan live in the song
   grid, not the panel; the GP7 instrument type/name is displayed but not editable. Shrinking a track
   is refused while notes sit on the strings that would go (no silent data loss). The
   the measure range selected in the grid is used by copy/cut/paste (M14) and will drive
   looping (M18). The grid rebuilds its note squares once per song revision, and a
   structural track command empties the per-track layout map; the page itself reuses the
   M10 per-measure layout cache.

13. **Settings and the app shell (M12).** Theme, zoom, panel visibility and sizes, the
   window rectangle, recent files, keymap path, autosave interval and audio output are
   stored in `settings.json` and restored at startup. Limits: the autosave interval applies
   to documents opened afterwards (an open journal keeps its options); cancelling an open
   during the import itself takes effect when the import ends, because the importer has no
   cancellation hook (the result is then dropped); the settings window has no Page layout tab (page size, fonts and breaks are M17); there
   is no settings file versioning yet beyond serde defaults for new fields; on Wayland the
   compositor may ignore a requested window position.

14. **The M14 palette and selection are a first version.** Beat selections stay in one
   track and voice but may cross bar and system lines (Shift+Left/Right, or dragging on
   the page); Shift+Up/Down selects the cursor's whole beat, and Delete (or `clear_note`)
   with a selection clears every note in it, keeping the beats. A beat selection is pasted
   bar by bar: the part from each copied measure goes into the next measure from the
   cursor's, so a paste near the end of the song needs measures inserted first. Measure
   selections cover every track (copy/paste carries notes,
   durations, ties, effects, time signature, tempo and section marker). Pasting measures
   needs room inside the song (insert empty measures first) and replaces both voices of
   each target measure (creating voice 2 when the copy has one); a pasted measure keeps
   the target's tempo change when the copied measure had none (there is no command to
   remove a tempo change). Pasting beats appends beats at the end of a short voice.
   Placeholder beats (Guitar Pro's fillers in unused voices) are not copied. Chord diagrams and lyrics are not copied or editable (the
   model has no setters); grace notes attached to a note are copied with it. Bend and whammy shapes are edited as point rows
   (position 0-12, value in quarter tones) with a preview, not by dragging a curve; the
   full shape is stored, but playback and `dump` still use the highest point. A batch is one
   undo step and one usually long journal line (its nested commands are percent-encoded;
   pasted notes list only the effects they have). If a part of a batch fails, the parts
   before it are reverted; a failing revert there is ignored (it has not been seen).

## Browser build (2026-11-04)

- **No recovery journal in the browser.** Settings and shortcuts live in `localStorage`,
  but a crash loses unsaved edits: the journal is still file-based. The browser needs a
  storage-backed journal and autosave (IndexedDB for the base song and snapshots) before
  the web build is as safe as the desktop.
- **No PDF export in the browser.** The dialog and download path are not wired up.
- **No SoundFont picker in the browser.** The site ships GeneralUser GS 2.0.3 and loads it
  automatically (2026-11-05); a browser user cannot choose another SoundFont yet.
- **Opening imports on the UI thread** after the bytes arrive (no wasm threads). Fine for
  the tested files; a web worker is the escape hatch for very large scores.

39. **Brush/arpeggio timing is GuitarScore-only in GP7.** GP3-5 store a brush's speed but
    have no place for "ends on the beat"; GP3-5/GP6 store no arpeggio speed or timing
    either. GP7 keeps all of it in `GuitarScoreBrush`/`GuitarScoreArpeggio` properties
    (Guitar Pro itself shows the direction and plays its own defaults). An export to an
    older format therefore reports the lost timing; arpeggio speed and timing are dropped
    by GP3-5/GP6.
