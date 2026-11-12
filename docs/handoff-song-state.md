# Handoff: the song carries our sound, and a tone library for PM synthesis

Owner's decisions of 2026-11-07. Two parts, independent of each other: (A) everything
GuitarScore adds to a song's sound travels inside the song file, (B) a library of named
tones for tracks played by PM synthesis.

## A. Our state in the song's Notices

### What exists today

`crates/gscore-app/src/app/song_state.rs` (commit `1ae8267`): a one-line block in the
song's free-text notes, `[GuitarScore 1]{json}[/GuitarScore]`, with each track's Comp send
and the tone it plays when that is not the built-in one. Taken out of the notes when a
song opens (the user never sees it in our program), written back on save
(`Song::with_notes`, `app/files.rs`). Guitar Pro shows the notes ("Notices") and keeps
them when it saves the file again.

### Why the Notices, not an invisible track

Guitar Pro has no truly hidden track: an extra track shows in its track list, takes a
MIDI channel, and other programs may play or drop it; lyrics also have limits per track.
The Notices are a plain text field every format has (GP3-5 as lines, GPX/GP7 as XML).

### The new form of the block

1. **Compact JSON → deflate → base64.** Base64 is plain ASCII, which matters more than
   the size: GP3-5 store text in a single-byte code page, so non-ASCII characters (a tone
   named with an umlaut) can be mangled on the way through another program. Deflate makes
   it about 3-5 times smaller; it needs a small pure-Rust crate (`miniz_oxide`, MIT or
   Apache-2.0, builds for wasm). New dependency: register it in `THIRD_PARTY.md`.
2. **Lines of at most 200 characters.** A GP3-5 notice line holds at most 255 bytes
   (`read_int_byte_size_string` / `write_int_byte_size_string`, `crates/gscore-gp/src/io/song.rs`). Every line:
   `::GS1:<part>/<count>:<crc32 of the whole payload>:<base64 chunk>`
   (`GS1` is the format version). On reading, collect the lines with `::GS`, check that
   every part from 1 to count is there once and that the CRC matches; otherwise ignore
   the whole block (never half-apply it). Use the same line form for GP6/GP7 too (one
   code path).
3. **Migration:** still read the current `[GuitarScore 1]{json}[/GuitarScore]` block; on
   save, write only the new form and drop the old one.
4. **The user's own notes stay untouched**, in their order; our lines go after them.
   Hidden in our program as now.

### Matching tracks

Entries are keyed by track number today. If someone deletes or reorders tracks in Guitar
Pro and keeps the notes, the settings land on the wrong tracks. Every track entry also
stores the track's **name** and **instrument** (GM program, drum or not). On reading:
match by number, check name and instrument; if they do not fit, look for the one track
with that name and instrument; if there is none (or several), drop the entry instead of
applying it to the wrong track. Same for an instrument changed in Guitar Pro after we
saved (a tone for a guitar must not land on a piano).

### What the block must hold: everything that sounds different, beyond the GP format

Rule: everything that changes the sound, has no field in the Guitar Pro format and is not
a built-in default. Store **full values** for anything the user changed (the whole tone,
the whole chain), not differences to the defaults: otherwise a song would change its
sound whenever a later version changes a built-in tone.

Per track:

- What plays it: MIDI synth, PM synthesis, or MIDI out (the port name is this machine's:
  store it as a hint; another machine falls back to the MIDI synth if it has no such
  port; channel too).
- For PM synthesis: the kind of string and the string sound (every knob, the pickup).
- The chain: every block with every knob. Impulse responses **by file path for now**
  (owner, 2026-11-07: until our own licence-free IRs ship bundled, before the release;
  then by name and checksum of a bundled IR, see below).
- The Comp send (0-127). Volume, pan, reverb send, chorus, mute and solo already have GP
  fields: not in the block.
- For a drum track: the chain of each part (kick, snare, hi-hat, toms, cymbals,
  percussion) and **each part's level** (the mixing levels inside the drum track). Today
  the part chains are kit-wide in the settings (`Playback::drum_parts`,
  `settings.rs` `DrumPartTone`), not per track: they must become per track (a track
  without its own uses the kit's), so they can travel.

Song-wide:

- The reverb bus (kind, all knobs, level, mute) and the compressor bus (its chain, level,
  mute). Today both are machine settings (`settings.rs` `ReverbBus`, `CompressorBus`):
  a song's own values override them; a song without uses the machine's.
- The master volume and pan stay in the GP format.

Tones dialed in on this machine: a track plays its song's tone, else its preset's local
tone (`preset_tones` in the settings, per GM program), else the built-in one. **On save,
write the tone each track actually plays whenever it is not built-in**, including a
preset's local tone: otherwise the song sounds different for anyone else. (The three
layers stay as they are in the program; only the file gets the resolved tone.)

### Impulse responses (owner, 2026-11-07)

For now the block stores IR file paths as they are. Before the first release the owner
makes licence-free IRs of our own, bundled with the program; then a chain refers to a
bundled IR by its name and a checksum (so a renamed or edited file is noticed), and only
a user's own IR keeps a path (it plays only on that machine; elsewhere the block is
skipped with a notice). The owner's test IRs (Voxengo, ~/Downloads/IMreverbs; Bureå
Church) may not be redistributed: never bundle them.

### Our state follows the tracks while editing (owner: important)

The open tab keeps four things per track **by track position**
(`crates/gscore-app/src/app/tab.rs`): what plays it (`track_engines`), its MIDI-out route
(`midi_outs`), its Comp send (`comp_sends`) and its song tone (`track_tones`). Inserting,
deleting, duplicating or moving a track in the program (`gscore-edit` `Command::InsertTrack`,
`DeleteTrack`, `DuplicateTrack`, `MoveTrack`, and their undo) does not touch these maps,
so the settings stay at the old positions and land on the wrong tracks: move the drum
track up and the guitar below it gets the drums' Comp send.

- Fix it at the root: keep our per-track state **with the track in the document**, not
  beside it. Give the song model's track a field of our own (in `gscore-model`, not
  written to the Guitar Pro data itself; the Notices block of this document is how it is
  saved), so every track command and its undo carry it along like the track's name or
  color: a moved track keeps its tone, a duplicated one gets a copy, a deleted one takes
  its state with it and undo brings it back.
- The tab's four maps then go; playback and the mixer read the state from the document.
- The Notices block is written from and read into that field.
- Tests: move, insert, duplicate and delete a track with a tone, an engine choice, a MIDI
  out route and a Comp send on it and on its neighbours; each stays with its track, and
  undo/redo of each command restores them.

### Sound changes are edits: undo and "modified" (owner: important)

Changing what plays a track, its tone or chain (a block added, moved or removed, a knob),
its Comp send, a drum part's chain or level, or a bus's settings does not mark the song as
modified and cannot be undone today: the changes go to the tab or the settings, not
through the editor. Now that they travel with the song, they are edits of the song.

- Make them editor commands with undo (`gscore-edit` `Command`, as the mixer's fader and
  pan already are, `Command::SetTrackVolume` and the like), so they show in the edit
  history, mark the document as modified (the title's dot, the question on closing) and
  are journaled for crash recovery (`gscore-session`'s journal, `Command::to_line`).
- A knob drag is one undo step: merge the steps of one drag as the mixer's faders do
  (`apply_command(command, merge)`).
- Settings that are not the song's stay settings without undo: a preset's local tone
  (`preset_tones`), the machine's default buses, the user's saved tones library.
- Tests: change a tone's knob, undo, redo; a drag is one step; a changed tone marks the
  document modified and asks before closing; recovery after a crash brings the change
  back.

### Prerequisite: keep Guitar Pro's own sound choice

The GP7 writer (`crates/gscore-gp/src/io/gpif_export/`) does not write a track's
`<Sounds>` (the RSE sound: its path, bank patch, pickups, effect chain); the reader keeps
only the MIDI part (`gpif/mod.rs` `SoundsWrapper`). Saving a file in GuitarScore therefore
loses the RSE choice a Guitar Pro user made. Keep `<Sounds>` through a round trip first
(read it whole, write it back); part B reads from it too.

### Tests

- Round trip of the block through every format we write (GP3, GP4, GP5, GPX, GP7),
  including a tone name with non-ASCII characters and a state longer than one line.
- A missing, doubled or edited line, a wrong CRC, a future version: the block is ignored,
  the user's notes are kept.
- The old `[GuitarScore 1]` block still reads.
- Tracks reordered or deleted after saving: entries go to the right track or are dropped.
- A song with a preset's local tone sounds the same in a fresh app with default settings
  (the app test in `test_track_panel.rs` from `1ae8267` is the model).

## B. A tone library for PM synthesis

Owner's wish: Guitar Pro's RSE offers many more than 128 sounds (many named after
guitarists). When a track is played by PM synthesis, the track panel should offer a
library of our own tones instead of only the 128 General MIDI programs.

### What we can and cannot use

RSE's presets are Arobas's: recorded sample sets plus Guitar Pro's amp and effect models,
part of Guitar Pro. We cannot use their sounds, and we must not name our tones after real
guitarists (trademarks and personality rights). Our tones are named by style and sound.

### The library

- *(Changed 2026-11-12: the made-up tones named below are gone. The library holds only
  tones the owner dialed in and imported with `cargo xtask tones import`; a program
  without one plays its plain strings. See the devlog.)*
- **Built-in tones**, designed by the owner, grouped by family, for example:
  Clean (Clean Single Coil, Glassy Neck, Warm Jazz, Funk Rhythm), Crunch (Texas Blues,
  British Crunch, Classic Rock Rhythm), Lead (80s Lead, Singing Sustain), High gain
  (Modern Metal Rhythm, Djent Tight), Acoustic (Steel Strummed, Steel Fingerpicked,
  Nylon Classical), Bass (Clean Finger, Pick, Slap, Driven). A tone is what
  `chain_settings::Tone` is today (kind of string, string sound, chain) plus a name, a
  family and a short description. Kept in the program (Rust or a bundled data file),
  versioned like everything else. Built-in tones may use bundled IRs only.
- **The user's saved tones** (today's `string_presets`, `ChainPreset`) appear in the same
  list, in their own group.
- Each GM program keeps its default built-in tone (owner's decision: "every preset ships
  with a default tone for PM synthesis"); the library is the choice on top of it.

### The track panel (owner, 2026-11-07)

The track panel's Sound section gets the **sound source** of the chain window's first box
(MIDI synth / PM synthesis / External MIDI): one setting shown in two places, so a change
in either shows in the other at once (today's choice: `app/track_engine.rs`,
`sound_chain.rs` `strings_editor`). PM synthesis is the default for string instruments.
Below the switch, depending on the source:

- **MIDI synth:** the General MIDI presets, no longer as a dropdown (hard to navigate) but
  as a **small scrollable table**: about 8 rows high, grouped by GM family (Piano,
  Chromatic percussion, Organ, Guitar, Bass, Strings, …), a filter field above it, the
  track's preset highlighted and scrolled into view; a click chooses. A **drum track**
  lists the General MIDI drum kits instead (Standard, Room, Power, Electronic, Jazz, Brush,
  Orchestra, …: the SoundFont's bank-128 presets).
- **PM synthesis:** the same table with **our tones** instead: grouped by family (Clean,
  Crunch, Lead, High gain, Acoustic, Bass), then the user's saved tones; the preset's
  built-in tone marked; the description on hover. A click gives the track that tone (it
  then travels in the song, part A) and plays the short Listen phrase as a preview (the
  chain window's Listen, `playback/demo.rs`), unless the song is playing. Only offered for
  string instruments that are not drums: elsewhere the PM choice is greyed out with a
  tooltip saying why.
- **External MIDI:** a dropdown for the MIDI output port and one for the channel
  (1-16), **and** the General MIDI table as for the MIDI synth (the hardware synthesizer
  gets the program change too).
- Switching the source keeps both choices: the General MIDI program stays the track's (it
  is what the file stores and what MIDI plays), the PM tone is chosen apart from it;
  switching back and forth loses neither.
- The sound chain window's tone row and the track panel show the same choice. The table
  is one widget used in both places (and maybe in the new-track dialog).

### Reading Guitar Pro's choice

*(Removed 2026-11-12 together with the made-up tones it mapped to; the `<Sounds>` round
trip stays. A mapping from RSE sound names to the owner's own tones is an open idea.)*

A GP7 file records the RSE sound chosen for a track (`<Sounds>`: its path such as
"Stringed/Electric Guitars/Clean Guitar", its bank patch, pickups, effect chain with amp
and pedal names). Map that to our nearest built-in tone (a table by RSE path and amp
names), so a song made with an RSE preset starts with a fitting PM tone instead of a
generic one. Needs the `<Sounds>` round trip of part A first. Show the RSE name next to
our tone ("RSE: …"), so the user sees what the author chose.

### Tests

- The picker lists built-in and saved tones; choosing one gives only that track the tone
  and it survives save and reopen.
- The source switch in the track panel and in the chain window stay in step; switching
  from PM to MIDI and back keeps the GM program and the PM tone; a drum track lists the
  kits and offers no PM; External MIDI shows port, channel and the GM table; the filter
  field narrows the table (app tests, a snapshot of each source).
- The RSE mapping: a few GP7 files with different RSE sounds start with the expected
  tones (use repository test data only; the owner's band files in ~/svn/wyw are local
  test material, never committed, and files in ~/svn/bigfive must never be used).
