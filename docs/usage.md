# Using GuitarScore

The keyboard, sound setup and the command-line tool in detail. The [README](../README.md)
has the overview.

## Keyboard

| Key | Does |
|---|---|
| Arrows | Move between beats and strings (click a note to jump there) |
| `0`–`9` | Type a fret; two digits typed quickly make one number (`1` `2` is fret 12) |
| `+` / `-` | Shorter / longer note |
| Insert / Delete | Add a beat; Delete removes the note under the cursor, or the whole beat when that string has none; with Ctrl: a bar |
| Right at the end of a bar | Fill it with rests, then start a new bar |
| Shift+arrows | Select beats |
| `b` / `w` | Bend / whammy bar editor |
| `p`, `h`, `v`, `x`, `g` … | Palm mute, hammer-on, vibrato, dead note, ghost note …; the palette's tooltips show every key |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Ctrl+C / Ctrl+X / Ctrl+V | Copy / cut / paste |
| Space | Play / pause |
| Ctrl+S | Save |
| Ctrl+Shift+S | Save As |
| Ctrl+N / Ctrl+O | New song / open a file |
| Ctrl+, | Settings |
| Ctrl+Q | Quit (asks about unsaved changes) |
| Shift+H | History |
| Shift+L | Lyrics |
| F11 | Full screen |
| F3 | Mixer in the bottom panel (again for the measure grid) |

## Transposing

The toolbar's transposition (the tuning fork, the number and the small −/+ pair beside the
tuner) moves what the song *sounds* like: 0 to 12 semitones either way, by ear, while the
fret numbers, the tablature and the score stay exactly as they are (as Guitar Pro's track
transpose does). Click the number to type one; the − and + buttons step a semitone; the
tuning fork switches the transposition off and on again, so a part can be compared with the
written pitch (it starts off, and it is only clickable once there is something to switch).
It is kept in the song's notes, so it comes back with the file; Guitar Pro ignores it.

**Tools › Transpose…** opens the window for more: transpose the whole song or only the shown
track, and either the sound (as the toolbar does) or, with **Change the fingerings**, the
notes themselves — they move on the fretboard and the tablature shows the new pitches. A
note that would leave the strings (below the lowest open string, above the highest fret)
stops that edit unchanged, with the reason in the window.

**Tools** also holds the tuner (the tuning fork's old place in the toolbar is now the
sound-wave button): a reference tone for tuning by ear. The tuning dialog (Track panel ›
Tuning) has a small play button beside every string that sounds its open note, and the same
button stops it again.

Change any key in Settings → Editor. The keymap can also live in a text file
(`~/.config/guitarscore/keymap.txt` on Linux): `gscore-cli keymap` prints the built-in one
and every action id, and `gscore-cli keymap --check <file>` checks a file. A broken keymap
is reported and the built-in one is used.

## Sound

The built-in synthesizer plays on the default audio device. It needs a General MIDI
SoundFont (`.sf2`; `.sf3` files are not supported) and finds one in this order: the
`GSCORE_SOUNDFONT` variable, `~/.local/share/soundfonts/`, the SoundFont shipped next to
the binary (release archives include FluidR3 GM, so a downloaded build has sound out of the
box), the well-known files in `/usr/share/soundfonts/` and `/usr/share/sounds/sf2/`, then
any `.sf2` there. In the app,
the toolbar's "Output" menu switches between the synthesizer, "Choose SoundFont…" and the
MIDI ports. Play starts at the cursor's note; after a pause it resumes, unless the cursor
was moved meanwhile.

### Modelled guitars and basses

Guitar and bass tracks (General MIDI programs 24 to 39) are played by modelled strings
instead of the SoundFont (Settings → Audio I/O, "Modelled guitars and basses (PM
synthesis)", on by default). Every other instrument, and the drums, stay on the SoundFont.

- **Tone.** A tone is the whole sound of a track: the kind of string (nylon, steel,
  electric, bass), the strings' own knobs and the effect chain after them. Each guitar and
  bass program starts on a built-in tone. The track panel on the right lists the built-in
  tones and the ones you saved; clicking one gives it to the track.
- **Sound chain.** The track panel's "Sound chain" button (and the "Sound chain" bar
  under the knobs of every mixer strip) opens the chain of the track: "Played by" chooses the strings or the SoundFont, the strings'
  tabs set how they are plucked and ring, and up to five blocks follow: Drive, Amp, Cab,
  Room, Reverb, Comp, EQ, Chorus and Delay. The Amp plays direct; put a Cab block after it
  for its speaker (a built-in cabinet, or an impulse response from a WAV file). "Save
  tone" and "Save as…" keep the sound under a name for other tracks and songs.
- **Input trim and meters.** Under the chain's board, the "Input trim" knob sets how hard
  the first block is fed (−24 to +24 dB, starting at −6 dB so the strings do not pin it)
  and the small meter beside it shows that level before the chain (after the trim), left
  and right. The meter beside "Chain output" shows what leaves the chain; turn the
  "Chain output" knob to set it. Turn the trim up to drive an amp or a compressor harder,
  down to clean them up; the track's own level does not change. The trim is saved with the
  song.
- **With the song.** A tone you change on a track is saved with the song (inside the
  Guitar Pro 7 file, in a block of the song's notes that Guitar Pro leaves alone). A track
  you never touched keeps following its program's built-in tone. A bass always plays its
  program's tone.
- **Guitar Pro's sounds.** A Guitar Pro 7 file names the RSE sound of each track. Where the
  tone library maps that name to a built-in tone, the track starts on it.

### Mixer

The grid's "Mixer view" shows one strip per track: fader and pan after the track's chain,
sends into the reverb, compressor and chorus buses, mute and solo, and a level meter
measured in the engine. Each drum track can be split into its kit parts with a chain and
a level of their own; the "Room" knob on every part sends it into one short room the whole
kit shares (as a room's overheads glue a kit together), the drum strip's own "Room" knob
sends the whole kit, and the Sound chain window's "Kit room…" button opens that room's
size, decay, damping and tone. The kit's own strip is the drum bus. The buses have their
own windows; the master strip has a chain too. There is no limiter yet: a mix that is too
loud clips.

```bash
gscore-cli play testdata/basic/note_lengths.gp                        # built-in synthesizer
gscore-cli play testdata/basic/note_lengths.gp --soundfont my.sf2    # with this SoundFont
gscore-cli play testdata/basic/note_lengths.gp --from 5               # from measure 5
gscore-cli midi testdata/basic/note_lengths.gp -o note_lengths.mid   # export a MIDI file
```

To use an external synthesizer instead, start it and pick its MIDI port (`--port` on the
command line). Without any SoundFont, `play` uses the first MIDI port. With FluidSynth:

```bash
fluidsynth -a pipewire -m alsa_seq -is /usr/share/soundfonts/FluidR3_GM.sf2
gscore-cli ports                                          # list MIDI outputs
gscore-cli play testdata/basic/note_lengths.gp --port FLUID
```

Bends and slides use a borrowed MIDI channel per note, so a bend does not move the rest of
the chord. What playback does not do yet is in `docs/gaps.md`.

The metronome and the count-in are switched in the toolbar; Settings › Metronome sets the
click's volume and sound, the count-in's length (1 to 4 bars) and whether a loop is counted
in again each time it starts over. The built-in synthesizer plays its own click; a MIDI
output gets General MIDI's metronome notes. Play with a selection plays it; with Loop on,
it repeats the selected bars. Selecting more than one beat turns Loop on; click Loop to
play on past the selection instead.

The speed dropdown in the transport slows playback to 50-90 % or speeds it to 110-120 % of
the written tempo. The gauge button left of it is the speed trainer: with Loop on, it
raises the speed by a chosen step every few passes, up to 100, 110 or 120 %.
Previous and next bar during a count-in count in again at the new bar.

File › Export › Audio (WAV) renders the song with the chosen SoundFont (16-bit stereo,
44.1 kHz); `gscore-cli wav song.gp -o song.wav --soundfont FluidR3_GM.sf2` does the same
without the window. File › Export › Stems (WAV) writes one file per track (through its
chain, fader and pan) and one per mixer bus (what the tracks send through it) into a
folder, for mixing elsewhere; `gscore-cli wav song.gp --stems DIR` writes the tracks'
files from the command line.

The Staff tiles hold Beat text and Lyrics (Shift+L): five lines of syllables, written
under the notation staff (above the tab when both are shown). Spaces separate syllables,
Tab skips a beat, a `+` puts two words on one beat (`in+the`) and `[text]` is hidden.
What is typed shows in the score at once, as one undo step per line; Cancel takes it back.
The L.H. and R.H. tiles open a small fingering window that stays open and always
shows the cursor note's finger: move through the notes with the arrows and press a
finger's key (`0`-`4` and `t` for the left hand, `p i m a c` for the right; Backspace
removes it) or click it. While the window is open those keys set fingers, not frets.
The technique matrix holds the chord diagram (`Ch`): a dialog sets the name, base fret
and a fret or `x` per string. Both are drawn in the score and can be edited or removed
again.

## Command line

```bash
cargo run -p gscore-cli -- --help
cargo run -p gscore-cli -- info testdata/basic/steel_guitar_dadgad.gp
```

```
Format:       Guitar Pro 7+ (.gp)
Title:        Steel Guitar DADGAD
Subtitle:     Test Material
Artist:       GuitarScore
...
Tracks:       1
  track 1: Steel Guitar: Steel Guitar, 6 strings, tuned DADGAD (D2 A2 D3 G3 A3 D4)
```

```bash
cargo run -p gscore-cli -- dump testdata/basic/steel_guitar_dadgad.gp
```

```
track 1: Steel Guitar (DADGAD)

D |-----------------0----------||
A |--------------0-------------||
G |-----------0----------------||
D |--------0-------------------||
A |-----0----------------------||
D |--0-------------------------||
```

- **`info`:** song information, tracks, instruments and tunings, and problems such as
  unfilled measures.
- **`dump`:** the tablature as ASCII tab.
- **`convert`:** saves a file as Guitar Pro 7+ (`.gp`). The song is checked before
  anything is written, the previous file is kept as `.bak`, and the new file replaces it
  atomically.
- **`edit`:** headless editing with every command the app uses (notes, durations,
  effects, bars, tracks, signatures, page setup, ...), from `-c` commands or a script.
  `gscore-cli commands` lists them. Every successful edit still round-trips through the
  GP7 writer.
- **`render`:** a track's score as SVG (`--track N`, repeated for several tracks in one
  score, or `--all-tracks`, with `--hide-empty-staves`; `--theme dark`), using the bundled
  Bravura and Noto Sans fonts by name (see `docs/development.md` for viewing it), or as a
  PDF on pages with the song's page setup and the fonts embedded when the output ends in
  `.pdf`.

```bash
cargo run -p gscore-cli -- commands
cargo run -p gscore-cli -- edit testdata/basic/steel_guitar_dadgad.gp -o edited.gp \
  -c "set_fret track=1 measure=1 beat=1 string=6 fret=7" \
  -c "set_duration measure=1 beat=1 duration=1/8"
cargo run -p gscore-cli -- edit song.gp -o edited.gp --script edits.txt
cargo run -p gscore-cli -- render testdata/basic/note_lengths.gp -o note_lengths.svg
cargo run -p gscore-cli -- render testdata/basic/ornaments.gp -o ornaments.pdf
```

A script has one command per line; blank lines and `#` comments are ignored. Positions are
1-based. The same parser drives scripts and `-c`, so a script is a regression test.

Exit codes: 0 success, 1 the command failed (e.g. unreadable file), 2 wrong usage.
