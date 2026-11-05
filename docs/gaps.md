# Known Gaps

Known shortfalls of the current state, most important first. Rewritten 2026-11-06 after
the v0.1.0 release; the history behind each item is in `docs/devlog.md`. Planned work is
in AGENTS.md section 9.

## Sound and playback

1. **Playback sounds like General MIDI.** Every instrument plays through a SoundFont
   (`rustysynth`; release archives bundle FluidR3 GM, the browser GeneralUser GS 2.0.3).
   Distorted guitars and basses are far from Guitar Pro's RSE, so the band's songs sound
   bad. This is M22 (modelled guitars and basses, `docs/guitar-modelling.md`).
2. **Pitched tracks without strings are read as string and fret.** GP7 stores the real
   pitch of a voice, synth, piano or strings note (`Midi`, `ConcertPitch`) and only
   leftover string and fret values against a guitar tuning, often negative. We read the
   string and fret: notes below that tuning's lowest string show as "?" and are silent
   (8 of the owner's 59 files leave 18-116 notes out), and such tracks get no notation
   staff (no tuning), only a tab. Guitar Pro shows them as standard notation only.
3. **Not played yet:** fermatas, D.C./D.S./coda and other directions. Missing controls:
   a speed slider and export to audio (WAV). Previous/next bar during a count-in stops
   playback instead of counting in again.
4. **Player limits.** Pause and resume do not re-trigger notes that were sounding;
   stopping `gscore-cli play --port` with Ctrl+C can leave notes hanging in an external
   synth. The synthesizer always uses the default audio device; `.sf3` SoundFonts are not
   supported; a source build has no SoundFont of its own. External MIDI synths play their
   own sound for the metronome (GM keys 33/34), which a GM1-only synth may not have.

## Files

5. **Second staves are not read.** A GP7 track with two staves (a piano's or synth's
   treble and bass) keeps its first staff only. The tracks after it read their own bars
   (fixed 2026-11-05; bars are listed per staff, not per track).
6. **Lyrics and fingerings** are kept in the file but cannot be shown or edited; chord
   diagrams are read and drawn but cannot be edited.
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
