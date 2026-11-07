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

### The track panel

- With PM synthesis: a tone picker (grouped, searchable list with the description on
  hover) where the GM preset picker is; the GM program stays in the file for MIDI
  playback and for other programs. With the MIDI synth: the GM picker as today.
- Choosing a tone gives the track that tone (it then travels in the song, part A).
- The sound chain window's tone row and the track panel show the same choice.

### Reading Guitar Pro's choice

A GP7 file records the RSE sound chosen for a track (`<Sounds>`: its path such as
"Stringed/Electric Guitars/Clean Guitar", its bank patch, pickups, effect chain with amp
and pedal names). Map that to our nearest built-in tone (a table by RSE path and amp
names), so a song made with an RSE preset starts with a fitting PM tone instead of a
generic one. Needs the `<Sounds>` round trip of part A first. Show the RSE name next to
our tone ("RSE: …"), so the user sees what the author chose.

### Tests

- The picker lists built-in and saved tones; choosing one gives only that track the tone
  and it survives save and reopen.
- The RSE mapping: a few GP7 files with different RSE sounds start with the expected
  tones (use repository test data only; the owner's band files in ~/svn/wyw are local
  test material, never committed, and files in ~/svn/bigfive must never be used).
