# GuitarScore

A cross-platform desktop editor for guitar tablature (Linux first, also Windows and macOS),
with a Guitar Pro-like workflow: fast, keyboard-only note entry, a clean modern UI and solid
playback. It opens and saves Guitar Pro files. Written in Rust.

GuitarScore must never crash and never lose your work; that outranks features.

## Status

Early development. The desktop app shows a Guitar Pro file's tablature and edits it from
the keyboard; reading, saving and editing also work on the command line:

- **`guitarscore [FILE]`:** the desktop app. Opens a file (File › Open… or the command
  line) in the background and shows one track's tablature as a page; track picker, zoom,
  light and dark theme, MIDI playback. Editing at a cursor (see "Keyboard" below), undo and
  redo, and saving as Guitar Pro 7 (`.gp`); it asks before closing or opening another file
  with unsaved changes.

- **Import:** Guitar Pro 3, 4, 5 (`.gp3`–`.gp5`), 6 (`.gpx`) and 7+ (`.gp`), detected from
  the file content. Malformed files give an error, never a crash.
- **`gscore-cli info`:** song information, tracks, instruments and tunings (with names such
  as "Drop D" or "DADGAD"), and problems such as unfilled measures.
- **`gscore-cli dump`:** the tablature as ASCII tab.
- **`gscore-cli convert`:** saves a file as Guitar Pro 7+ (`.gp`). The song is checked
  before anything is written, the previous file is kept as `.bak`, and the new file
  replaces it atomically.
- **`gscore-cli edit`:** headless editing with undo/redo: set frets, change beat lengths,
  insert/delete beats and measures, from `-c` commands or a script. `gscore-cli commands`
  lists them. Every successful edit still round-trips through the GP7 writer.

### Keyboard

Click a note or use the arrows to move the cursor. Type a fret number (two digits typed
quickly make one number, `1` `2` is fret 12); `+` and `-` make the note shorter and longer;
Insert/Delete add and remove a beat, Ctrl+Insert/Ctrl+Delete a measure; Right at the end
of a measure adds rests until it is full, then a new measure. Ctrl+Z/Ctrl+Y undo and redo,
Space plays, Ctrl+S saves.

Keys are bound to the **typed character**, so `+` works on any keyboard layout. The keymap
is a text file, `~/.config/guitarscore/keymap.txt` on Linux; `gscore-cli keymap` prints the
built-in one (a file to start from) and every action id, and `gscore-cli keymap --check
<file>` checks one. A broken keymap file is reported and the built-in keymap is used.

Milestones, progress and known gaps: `AGENTS.md` (section 9), `docs/devlog.md`,
`docs/gaps.md`.

## Building

Install Rust with [rustup](https://rustup.rs). `rust-toolchain.toml` pins the version and
installs `rustfmt`, `clippy` and the `wasm32-unknown-unknown` target. With a distribution's
Rust instead, install the wasm target yourself (Arch: `pacman -S rust-wasm`). Building the
Guitar Pro reader for wasm also needs `clang`.

```bash
cargo build
cargo xtask ci   # everything a change must pass: fmt, clippy, tests, layer and wasm checks
cargo gui testdata/basic/note_lengths.gp      # build and start the app (a cargo alias)
gscore-cli gui testdata/basic/note_lengths.gp # start the built app from the command line
```

The app renders with wgpu (Vulkan, Metal or DirectX 12). Its snapshot test needs a GPU or a
software Vulkan driver such as lavapipe.

### Sound

GuitarScore has a built-in synthesizer that plays on the default audio device. It needs a
General MIDI SoundFont (`.sf2`; `.sf3` files are not supported) and finds one in this order:
the `GSCORE_SOUNDFONT` variable, `~/.local/share/soundfonts/`, the well-known files in
`/usr/share/soundfonts/` and `/usr/share/sounds/sf2/`, then any `.sf2` there (Arch:
`pacman -S soundfont-fluid`). In the app, the toolbar's "Output" menu switches between the
synthesizer, "Choose SoundFont…" and the MIDI ports.

```bash
gscore-cli play testdata/basic/note_lengths.gp                        # built-in synthesizer
gscore-cli play testdata/basic/note_lengths.gp --soundfont my.sf2    # with this SoundFont
gscore-cli midi testdata/basic/note_lengths.gp -o note_lengths.mid   # export a MIDI file
```

To use an external synthesizer instead, start it and pick its MIDI port (`--port` on the
command line). Without any SoundFont, `play` uses the first MIDI port. With FluidSynth:

```bash
fluidsynth -a pipewire -m alsa_seq -is /usr/share/soundfonts/FluidR3_GM.sf2
gscore-cli ports                                          # list MIDI outputs
gscore-cli play testdata/basic/note_lengths.gp --port FLUID
```

Playback covers all tracks, tempo changes, repeats, ties, harmonics, and bends and slides.
Bends and slides use a borrowed MIDI channel per note, so a bend does not move the rest of
the chord. Remaining technique limits are in `docs/gaps.md` (#11).

On Linux, building needs the ALSA development files (`alsa-lib` on Arch, `libasound2-dev` on
Debian/Ubuntu).

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

```bash
cargo run -p gscore-cli -- commands
cargo run -p gscore-cli -- edit testdata/basic/steel_guitar_dadgad.gp -o edited.gp \
  -c "set_fret track=1 measure=1 beat=1 string=6 fret=7" \
  -c "set_duration measure=1 beat=1 duration=1/8"
cargo run -p gscore-cli -- edit song.gp -o edited.gp --script edits.txt
```

A script has one command per line; blank lines and `#` comments are ignored. Positions are
1-based. The same parser drives scripts and `-c`, so a script is a regression test.

```bash
cargo run -p gscore-cli -- render testdata/basic/note_lengths.gp -o note_lengths.svg
```

Draws a track's tablature as SVG (`--track N`, `--theme dark`). The image uses the bundled
Bravura and Noto Sans fonts by name; see `docs/development.md` for viewing it.

Exit codes: 0 success, 1 the command failed (e.g. unreadable file), 2 wrong usage.

## Layout

A Cargo workspace of small crates. A crate may only depend on crates in lower layers;
`cargo xtask layers` enforces this.

| Layer | Crate | Purpose |
|---|---|---|
| 0 | `gscore-gp` | Guitar Pro song data and reading/writing of all GP formats |
| 1 | `gscore-model` | The document model the app works with: views, exact durations, positions, checks |
| 2 | `gscore-io` | Safe entry point for files: format detection, size limits, crash containment |
| 2 | `gscore-layout` | Tablature layout: draw operations, SVG output, light and dark theme |
| 2 | `gscore-play` | Playback sequencing: MIDI events, channel borrowing for bends/slides, `.mid` writer |
| 3 | `gscore-edit`, `gscore-audio` | Editing commands with undo/redo; MIDI output backend (`midir`) |
| 4 | `gscore-session` | Crash reports and safe saving; recovery journal and autosave planned |
| 5 | `gscore-cli`, `gscore-app` | Command-line tool; desktop app (`guitarscore`) |

Test files made with Guitar Pro are in `testdata/` (see `testdata/basic/README.md`).
`docs/architecture.md` describes the design; `AGENTS.md` holds the rules for contributors,
human or AI.

## License

MIT, see `LICENSE`. Third-party code and assets are listed in `THIRD_PARTY.md`.

## Acknowledgements

Reading and writing Guitar Pro files is based on the
[`guitarpro`](https://github.com/slundi/guitarpro) crate by **slundi** (MIT license).
GuitarScore includes a modified copy of its code as the `gscore-gp` crate; the license is kept
next to it (`crates/gscore-gp/LICENSE-guitarpro`), and `docs/guitarpro.md` lists where it came
from and what was changed. Many thanks for making the Guitar Pro formats accessible.
