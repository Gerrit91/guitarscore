# Architecture Overview

GuitarScore is a Cargo workspace of small crates in layers. A crate may depend only on
crates in lower layers; `cargo xtask layers` enforces this with the table in
`xtask/src/layers.rs` (the directory tree and the rules are in AGENTS.md section 5).

| Layer | Crate | Job |
|---|---|---|
| 0 | `gscore-version` | the build version baked in at compile time |
| 0 | `gscore-gp` | Guitar Pro song data, reading and writing GP3-5, GPX and GP7 (derived from `guitarpro`, MIT; changes in `docs/guitarpro.md`) |
| 1 | `gscore-model` | the document model: wraps the Guitar Pro song with views, positions, checked edits, issues and invariants |
| 2 | `gscore-io` | bytes to model and back: format detection, size limits, `catch_unwind` around the reader |
| 2 | `gscore-layout` | model to positioned draw operations: pages, systems, notation and tab; SVG and PDF output |
| 2 | `gscore-play` | the sequencer: model to timed MIDI events, the metronome, SMF export, the `Player` trait |
| 3 | `gscore-edit` | cursor, selection, clipboard, commands with undo and redo, the keymap |
| 3 | `gscore-audio` | `Player` backends: MIDI port (`midir`), SoundFont synthesizer (`rustysynth` + `cpal`), the browser worklet; the tuner |
| 4 | `gscore-session` | opening, safe save and export, the recovery journal, autosave, settings files |
| 4 | `gscore-worklet` | the synthesizer compiled into the browser's AudioWorklet |
| 5 | `gscore-cli` | the headless tool |
| 5 | `gscore-app` | the egui/eframe application, desktop and browser |

Dependency restrictions: only `gscore-app` may use egui, eframe, winit or rfd; only
`gscore-audio` may use `cpal`, `midir` or `rustysynth`; only `gscore-model` and `gscore-io`
may use `gscore-gp`. The crates up to `gscore-edit`, plus `gscore-audio` and
`gscore-worklet`, build for `wasm32-unknown-unknown` (checked by `cargo xtask layers`).

## Model (`gscore-model`)

`Song` wraps `gscore_gp::Song` and is the only way to the Guitar Pro types. Reading goes
through views (`TrackView`, `MeasureView`, `BeatView`, `NoteView`) addressed by typed
positions (`TrackIndex`, `MeasureIndex`, `VoiceIndex`, `BeatIndex`, `BeatPosition`).
Writing goes through checked methods (in `edit/`) that validate first and return a
`Change` that can be reverted, so every command is transactional. Each measure carries a
stamp that changes when it is edited; layout caches by it. `issues` lists problems such as
unfilled measures; `compare` finds differences between two songs (used to verify a save).

## Layout (`gscore-layout`)

Layout is a pure function of the song, the settings and the font metrics:
`layout_track(song, track, options, fonts)` and `layout_tracks(...)` (several tracks in one
score) return a `Layout { width, height, pages, ops: Vec<DrawOp>, places }`. Coordinates
are in staff spaces (y down); colors are `ColorToken`s that a `Theme` (light, dark) maps to
RGB. Glyphs are referred to by SMuFL name (`Glyph`); engraving values come from the bundled
Bravura metadata (`MusicFont`).

1. `header`: title block, credits, tuning, tempo mark.
2. `measure`: each measure on its own, for every staff the track shows (standard notation,
   tab, slash notation, a percussion staff for drums). Beats of all voices are merged into
   columns by start time and spaced by length; `notation` spells pitches, places heads,
   stems, beams, ties and accidentals; `ornaments` and `marks` draw techniques, dynamics,
   repeats, sections and tempo. A `MeasureLayout` does not depend on its position, so
   `LayoutCache` keeps it per measure stamp: an edit lays out only what changed.
3. `measure::align`: with several tracks, one time grid per bar, so beats line up.
4. `system` and `score`: line breaking (automatic, bars per line, or the file's breaks),
   justification, the staves of each track stacked in a system.
5. `pages`: systems flow onto pages with the song's page setup (paper, margins, score
   size, header and footer), or onto one endless surface for the screen views.

`svg::to_svg` and `pdf::to_pdf` (fonts embedded as subsets) write the operations; the app
paints the same operations with egui. `Places` records where each system, measure and
beat column is, so the app draws the cursor and turns clicks into positions without
looking into the draw operations.

## Editing (`gscore-edit`)

- `Command`: every edit, with an id, parameters, `apply` and its undo (the model's
  `Change`). Commands write themselves as text lines, which the recovery journal stores and
  `gscore-cli edit` reads (`parse`); `info` lists them for `gscore-cli commands`.
- `Editor`: applies commands with undo and redo, memory-capped history.
- `Cursor` and `Selection`: where the user is (track, measure, voice, beat, string) and
  what is selected (beats in one voice, or whole measures); the clipboard copies and pastes
  either.
- `ActionId` and `Keymap`: what a key asks for, by typed character or key name; the
  built-in keymap is a text file, the user's overrides live in the settings.
- `TabEditor`: owns the `Editor`, cursor and selection, turns actions into commands, and
  hands playback and saving back to the app as requests.

## Playback (`gscore-play`, `gscore-audio`)

`gscore-play` turns a song into a `Sequence`: timed MIDI events per track, with the tempo
map and the played order of measures (repeats, endings). `timeline` and `order` walk the
song; `build` collects the sounding notes with their techniques (`sounding`, `effects`,
`strokes`, `graces`, `swing`, `pitch` for bends and slides on borrowed channels) and emits
the events; `click` adds the metronome and the count-in (joined to the song by `lead_in`);
`smf` writes a MIDI file; `wire` encodes a sequence for the browser worklet. The
`Scheduler` walks a sequence in time for any backend.

`gscore-audio` plays a sequence behind the `Player` trait: `MidiOutPlayer` to a MIDI port,
`SynthPlayer` on a thread of its own with `SynthEngine` (a SoundFont synthesizer plus the
metronome's own `click` voice; `render` never allocates or blocks), `WebPlayer` through the
AudioWorklet (`gscore-worklet` runs the same engine there). `TunerPlayer` plays the tuner's
reference tone. Milestone M22 adds a modelled guitar engine next to the SoundFont one
(`docs/guitar-modelling.md`).

Inside `SynthEngine` every track is a strip of the `mixer` (M22, 2026-11-07): its source
(the MIDI channels it plays on, each rendered apart by our fork of RustySynth in
`vendor/rustysynth`, or its modelled strings from `guitars`), its chain (`chains`, the
blocks of `gscore-guitar`), then fader and pan, then a post-fader send to the reverb bus;
the bus and the strips sum into the master. The song's volume, pan and reverb send (MIDI
CC 7, 10, 91) move the strips; the synthesizer stays at full volume and centred. Tracks
sharing a MIDI channel are one strip (the first owns it). `meters` publishes every strip's,
the bus's and the master's peak for the interface (atomics, no locks).

## Session (`gscore-session`)

`open` reads a file with a size cap and imports it; `save` is the safe save (temporary
file, atomic rename, `.bak`, read back and compare); `export` writes other formats and lists
what they drop; `recovery` keeps the journal of commands and periodic snapshots on a thread
of its own and finds sessions to restore after a crash; `config` and `web` keep settings
and the keymap (files on the desktop, `localStorage` in the browser); `crash_report` writes
panic reports locally.

## The app (`gscore-app`)

A thin eframe/egui shell (wgpu renderer) that holds state and delegates; the same crate
builds for the desktop and the browser.

- `state`, `settings`: view state and settings as plain serializable data.
- `document`: one open song: its `TabEditor`, the saved revision, the layout cache and
  the recovery journal; layout runs inside `catch_unwind`.
- `tasks`: file dialogs, opening, saving and exporting on background threads, reported
  back through a channel polled once per frame.
- `playback`: the app's side of the player (output choice, metronome, count-in, loop,
  previews of a single beat).
- `keys`: egui's key and text events to keymap combinations.
- `canvas`: paints `DrawOp`s, culling what is off screen.
- `app/`: the window. `tab`, `tab_bar` (one tab per open song), `title_bar`, `toolbar`,
  `bars` (menus and status bar), `page` and `score_view` (the five score views, cursor,
  clicks, scrolling while playing), `edit_palette`, `edit_staff`, `edit_effects`,
  `edit_dynamics` and `tiles` (the left palette), `track_panel` (right panel), `grid`
  (song grid and mixer), `settings_window`, and the dialogs (tuning, tuner, bend and
  whammy editors, grace notes, tuplets, history, shortcuts, new song and track).
  `editing` turns keys into actions before any widget sees them; drawing is wrapped in
  `catch_unwind`, so a bug shows a notice instead of closing the window.

## The command-line tool (`gscore-cli`)

`info`, `dump` (ASCII tab), `render` (SVG or PDF), `midi`, `play`, `ports`, `convert`,
`edit` and `commands`, `bench`, `keymap`, `legend` and `gui`; `roundtrip-check` is hidden.
It shares opening, saving and the command parser with the app, so the whole pipeline can
be tested without a window.
