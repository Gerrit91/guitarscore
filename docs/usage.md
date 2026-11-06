# Using GuitarScore

The keyboard, sound setup and the command-line tool in detail. The [README](../README.md)
has the overview.

## Keyboard

| Key | Does |
|---|---|
| Arrows | Move between beats and strings (click a note to jump there) |
| `0`–`9` | Type a fret; two digits typed quickly make one number (`1` `2` is fret 12) |
| `+` / `-` | Shorter / longer note |
| Insert / Delete | Add / remove a beat; with Ctrl: a bar |
| Right at the end of a bar | Fill it with rests, then start a new bar |
| Shift+arrows | Select beats |
| `b` / `w` | Bend / whammy bar editor |
| `p`, `h`, `v`, `x`, `g` … | Palm mute, hammer-on, vibrato, dead note, ghost note …; the palette's tooltips show every key |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Ctrl+C / Ctrl+X / Ctrl+V | Copy / cut / paste |
| Space | Play / pause |
| Ctrl+S | Save |
| F11 | Full screen |

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
it repeats the selected bars.

The speed dropdown in the transport slows playback to 50-90 % or speeds it to 110-120 % of
the written tempo. The gauge button left of it is the speed trainer: with Loop on, it
raises the speed by a chosen step every few passes, up to 100, 110 or 120 %.
Previous and next bar during a count-in count in again at the new bar.

File › Export › Audio (WAV) renders the song with the chosen SoundFont (16-bit stereo,
44.1 kHz); `gscore-cli wav song.gp -o song.wav --soundfont FluidR3_GM.sf2` does the same
without the window.

The Staff tiles hold Beat text and Lyrics (Shift+L): five lines of syllables, written
under the notation staff (above the tab when both are shown). Spaces separate syllables,
Tab skips a beat, a `+` puts two words on one beat (`in+the`) and `[text]` is hidden.
What is typed shows in the score at once, as one undo step per line; Cancel takes it back.
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
