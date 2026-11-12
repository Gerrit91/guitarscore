# Known Gaps

Known shortfalls of the current state, most important first. Rewritten 2026-11-06 after
the v0.1.0 release, sound section updated 2026-11-13 for v0.2.0; the history behind each item is in `docs/devlog.md`. Planned work is
in AGENTS.md section 9.

## Sound and playback

1. **Modelled guitars and basses are a first version.** Guitar and bass tracks play by
   default through physically modelled strings and an effect chain; every other
   instrument, the drums, and every track set to the MIDI synth still play through a
   SoundFont (`rustysynth`; release archives bundle FluidR3 GM, the browser GeneralUser GS
   2.0.3). This is M22 (`docs/guitar-modelling.md`); it is not yet judged comparable to
   Guitar Pro's RSE on every passage.
   - The built-in tone library is small: nine tones the owner dialed in, one per guitar
     program and one clean bass for all eight bass programs (no picked, slap or fretless
     bass yet). A program without a tone plays its plain strings, no effects.
   - Guitar Pro's RSE sounds can be mapped to built-in tones by name
     (`cargo xtask tones rse`), but no mapping ships yet: a track starts on its program's
     tone whatever RSE sound the file names.
   - No impulse responses are bundled. The Cab block has three computed cabinets (Rock,
     Acoustic, Classical); a room or a cabinet from a WAV file is stored by its path on
     this machine and skipped on another.
   - A bass ignores a tone the song carries and always plays its program's tone.
   - The strings benchmark (`cargo xtask bench`, 16 tracks × 6 strings ringing) is over its
     250 ms budget for a second of audio; CI does not run it.
   - The browser build runs no effect chains: it has no Sound chain window, and the
     built-in tones' chains and the buses are not sent to its synthesizer.
2. **Pitched tracks without strings are read as string and fret.** GP7 stores the real
   pitch of a voice, synth, piano or strings note (`Midi`, `ConcertPitch`) and only
   leftover string and fret values against a guitar tuning, often negative. We read the
   string and fret: notes below that tuning's lowest string show as "?" and are silent
   (8 of the owner's 59 files leave 18-116 notes out), and such tracks get no notation
   staff (no tuning), only a tab. Guitar Pro shows them as standard notation only.
3. **Playback controls.** Fermatas and D.C./D.S./coda are played now, and a speed
   dropdown (25-150 %, any value typed) with a speed trainer, WAV export and count-in navigation are in
   place. Audio export renders the modelled tracks, their chains and the buses as playback
   does. Still open: WAV has no progress or cancel. Previous/next bar during a count-in counts
   in again (fixed 2026-11-06).
4. **Player limits.** Pause and resume do not re-trigger notes that were sounding;
   stopping `gscore-cli play --port` with Ctrl+C can leave notes hanging in an external
   synth. The synthesizer always uses the default audio device; `.sf3` SoundFonts are not
   supported; a source build has no SoundFont of its own. External MIDI synths play their
   own sound for the metronome (GM keys 33/34), which a GM1-only synth may not have.
   A per-track MIDI-out route lives in the open tab, not in the song, and the hardware's
   audio never returns: its strip shows note activity, and its chain and bus sends do not
   apply (volume, pan, program and reverb/chorus CCs are sent to the device). The master
   chain has no limiter block yet, so an over-loud mix is still hard-clipped to -1..1; the
   kit's built-in part tones are empty until the owner designs them (2026-11-07). Every
   drum part's chain still runs every block, even in silence (the proposed skip is open).

## Files

5. **Second staves are not read.** A GP7 track with two staves (a piano's or synth's
   treble and bass) keeps its first staff only. The tracks after it read their own bars
   (fixed 2026-11-05; bars are listed per staff, not per track).
6. **Lyrics and fingerings** can be shown and edited: five lyric lines under the tab, per
   syllable on the beats, and left/right-hand fingerings in the notation. Chord diagrams
   can be created, shown in the score, edited and removed. Still open: lyric editing is
   per line for the whole song (not per syllable), and a chord diagram has no drag-and-drop
   grid yet (the dialog sets each string).
7. **Older formats lose detail.** File › Export writes GP6, GP5, GP4 and GP3 and reports
   what each drops, but GP3-5 cannot store every duration (a double-dotted 7/16, some
   tuplets), artificial-harmonic frets, GuitarScore's brush and arpeggio timing (kept in
   GP7 properties only), fermatas and more. Saving (as opposed to exporting) is GP7 only.
   Whether Guitar Pro opens every file we write is checked by hand, not by a test.
8. **GP6 instruments and drums.** GP6 stores only an instrument id, so its tracks count
   as string instruments unless they are percussion; GP6 drum notes (element and
   variation) are not read, so `multi_staff.gpx`'s drums are silent.
9. **Complex bends are simplified outside playback.** Playback and the bend editor use
   every point; `dump` and the save comparison still use the highest point, so they cannot
   tell a bend-and-release from a bend.
10. **`gscore-gp` public items are mostly undocumented** (inherited from `guitarpro`); the
    crate follows the workspace lints, and `gscore-io` keeps `catch_unwind` around it.

## Score and layout

11. **Missing in the score:** D.C./D.S./coda and other directions, lyrics, rhythm under
    the tab (shown in the notation instead), a bracket or brace for multi-track systems,
    native printing (print from the PDF). Beat text can collide with high notes in the
    notation ("Bend", "Slides" in `ornaments.gp`). Text widths are estimated for
    non-digits.

## Editing and the app

12. **Editing limits.** A beat selection stays in one track and voice (it may cross bars
    and systems); pasting measures needs room inside the song; chord diagrams and lyrics
    cannot be edited; the GP7 instrument name is shown but not editable; the program list
    has no search field and a per-track SoundFont bank beyond the program is not stored.
    Undo back to the saved state still counts as changed, and the undo history is kept
    across a crash (journal) but not across a normal close.
13. **Platforms.** On macOS the file dialog must run on the main thread (rfd); it runs on
    a background thread here, so Open… needs a change there. On Windows, egui turns
    Ctrl+Insert into Copy, so `insert_measure` needs another key. On Wayland the compositor
    may ignore a requested window position. The app snapshot tests need a GPU or software
    Vulkan. Releases are not signed (Gatekeeper, SmartScreen warnings).
14. **Recovery.** Edits reach the journal within milliseconds; a crash in that window loses
    the last edit. The cursor, shown track and playback position are not journaled; SIGTERM
    and shutdown rely on the journal, not a final snapshot. Only commands are panic-tested
    with injection; layout and drawing are contained without an injection test.

## Tests

15. **Little test data.** Six songs in `testdata/basic` (`steel_guitar_dadgad`,
    `note_lengths`, `ornaments`, `advanced_ornaments`, `staff`, `multi_staff`), most as
    GP7, GPX and GP5 with Guitar Pro's PDF and MIDI export; only `multi_staff` has several
    tracks; no GP3/GP4 files and no two-staff track (a test builds one); no fuzzer
    (`cargo-fuzz` is in the backlog).

## Browser build

16. **No recovery journal in the browser.** Settings and shortcuts live in `localStorage`,
    but a crash loses unsaved edits; it needs a storage-backed journal (IndexedDB).
17. **No PDF export and no SoundFont picker in the browser**; opening imports on the UI
    thread after the bytes arrive (no wasm threads), fine for the tested files.
