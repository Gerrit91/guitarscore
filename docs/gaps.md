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
   - The built-in tones: one per guitar and bass program, under fixed names (2026-11-13).
     The sounds are few so far: all eight bass tones are the owner's clean bass (Pick,
     Slap, Fretless and the rest sound like Clean Finger), and Warm Jazz and Clean Single
     Coil are the same clean tone, until the owner dials in their own.
   - Guitar Pro's RSE sounds can be mapped to built-in tones by name
     (`cargo xtask tones rse`), but no mapping ships yet: a track starts on its program's
     tone whatever RSE sound the file names.
   - No impulse responses are bundled. The Cab block has three computed cabinets (Rock,
     Acoustic, Classical); a room or a cabinet from a WAV file is stored by its path on
     this machine and skipped on another.
   - A bass ignores a tone the song carries and always plays its program's tone.
   - The strings benchmark (`cargo xtask bench`, 16 tracks × 6 strings ringing) is over its
     250 ms budget for a second of audio; CI does not run it.
   - The browser build has its own list of gaps below ("Browser build").
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

Collected 2026-11-13 for later work; `docs/web.md` describes how the build is put
together. Everything here was found in the code (`cfg(target_arch = "wasm32")`) or while
testing the built site in headless Chromium; what could only be judged by ear is marked.

### Sound

16. **The playing position comes about ten times a second.** The desktop player carries
    the position on between audio callbacks (`gscore-audio/src/clock.rs`,
    `Player::display_position`); the browser player has no such clock and returns what the
    worklet last reported (`web/worklet.js`, every 32 blocks). The cursor glides all the
    same (owner, 2026-11-13); whether it is as even as on the desktop at fast tempos is
    not checked.
17. **The mixer's meters are estimates, and there is no CPU bar.** `Playback::meters()`
    and `audio_load()` are `None` in the browser, so the strips show levels estimated from
    the notes being played (as for MIDI output): a track's from its notes and fader, a
    bus's from the loudest send into it. They do not follow the real signal: a chain's
    gain, a drive or a compressor does not show. A drum track's part meters and the
    gain-reduction needles stay empty, and the load bar is missing. The engine measures
    all of it in the worklet (`SynthEngine::meters`); it is not posted to the page.
18. **A chain is prepared on the audio thread.** The worklet has no other thread, so
    `SynthEngine::apply_sound` builds a chain (resampling and transforming its impulse
    responses) inside a message handler between audio blocks. Choosing a tone or changing
    a block while playing can click or drop out; a long room is the worst case. A second
    worker that prepares chains and hands them over would fix it.
19. **No impulse responses from files.** A cabinet, room, bus or master response is chosen
    through a native file dialog; in the browser the buttons report "not available in the
    browser build yet" (`tasks.rs`, `pick_impulse` and its siblings). A tone that names a
    WAV by its path on another machine plays without that block. The built-in cabinets
    (Rock, Acoustic, Classical) work. The wire already carries a response as samples, so
    what is missing is reading a file the user picks (`<input type=file>`) and keeping it
    (IndexedDB).
20. **Tones and the mixer are only checked by measurement.** That the string models,
    track chains, buses, master chain, drum parts and comp sends reach the worklet is
    covered by a round-trip test and one measured case (a driven tone on one track). Not
    checked: a song with many tracks and drum parts, the buses by ear, whether the audio
    thread keeps up with several amp chains (the desktop strings benchmark is already over
    budget, and wasm is slower).
21. **Live mixer moves made before the SoundFont has loaded are lost.** The worklet
    keeps the last sound message per track, bus and part, the song, the position and the
    tempo, and gives them to the engine once the SoundFont arrives. A MIDI event sent
    live in that time (a fader or pan moved) is dropped.
22. **No SoundFont picker.** The site's bundled GeneralUser GS is the only one ("choosing
    a SoundFont is not available in the browser build yet"); `.sf3` is not supported
    anywhere. The 31 MB download is cached by the service worker after the first visit.
23. **No MIDI out.** A browser has no MIDI ports here (Web MIDI is not used): the "MIDI
    out" choice is hidden and a track saved with it falls back to the SoundFont.
24. **No sample-rate choice.** The AudioContext's rate is used; the setting has no effect.
25. **Audio starts only after a user gesture** (the browser's rule): the first Play
    resumes the context. Not a defect, but a count-in or an auditioned note before any
    click is silent.

### Files

26. **No recovery journal.** Settings and shortcuts live in `localStorage`, but a crash
    or a closed tab loses unsaved edits; the page only asks before it is left. Needs a
    storage-backed journal (IndexedDB).
27. **No export.** PDF, MIDI, the older Guitar Pro formats and audio (WAV) report
    "exporting ... is not available in the browser build yet" (`tasks.rs`). Saving as
    Guitar Pro 7 works, as a download.
28. **Saving is a download.** There is no file handle to write back to: every save
    downloads a new file (the File System Access API is not used), and the `.bak` copy and
    the read-back check of the desktop save do not apply.
29. **Opening runs on the UI thread** after the bytes arrive (no wasm threads): fine for
    the tested files, a very large one freezes the page while it imports. A file that was
    open cannot be reopened from the recent list ("in the browser, choose the file
    again").

### App

30. **Any panic kills the page.** The wasm build aborts on panic; the hook only logs it
    ("GuitarScore panicked: ..." in the console) and the page shows "RuntimeError:
    unreachable". The desktop's containment (`catch_unwind` around commands, layout and
    drawing) does not exist there. Found this way on 2026-11-13: `SystemTime::now()` in
    the edit history crashed the page on every edit; a test now keeps the `std` clocks out
    of the app. Other `std` calls that panic or fail on wasm (threads, `std::fs`) are
    gated by hand, not by a check: `cargo xtask layers` scans only the core crates.
31. **An old build stays cached.** The service worker (`web/sw.js`) kept serving the
    previous wasm after a rebuild during testing; a normal reload was not enough. Needs a
    cache name tied to the build, or a check for a new version on load.
32. **No automated browser test of playing or editing.** The smoke test
    (`docs/web.md`) waits for the page and the SoundFont; nothing clicks Play, types a
    note or chooses a tone. The checks of 2026-11-13 were done by hand with headless
    Chromium over the DevTools protocol (the messages posted to the worklet were recorded
    and replayed into the built worklet in Node); that could become a test.
33. **Not looked at in the browser:** the Sound chain window, the string sound tabs, the
    mixer's bus windows, the tuner (`tuner_web.rs` exists), drag and drop of a file,
    keyboard shortcuts the browser takes for itself (Ctrl+S, Ctrl+O, Ctrl+N, F11), touch
    input and small screens.
