# GuitarScore

**A fast, free guitar tablature editor.** Write tabs at the speed you think, hear them as
you type, and open the Guitar Pro files you already have. Native, written in Rust, and
built never to lose your work.

![GuitarScore editing a tab with notation, techniques and the edit palette](docs/media/editor.png)

## Why GuitarScore

- **Keyboard first.** Type a fret, press `+` for a shorter note, arrow on. Every action
  has a key, and every key can be rebound.
- **Your Guitar Pro files.** Opens Guitar Pro 3 to 7+, saves Guitar Pro 7, and exports
  older Guitar Pro versions, MIDI and PDF.
- **Never lose work.** Every edit goes to a recovery journal first; after a crash the next
  start offers your document back, undo history included.
- **Hear it.** A built-in synthesizer plays every track with its techniques, or send it
  to any MIDI synthesizer. A tuner is included.
- **The whole score.** Notation and tab, all techniques with their own editors, several
  tracks on one page, real pages or one endless page, light and dark.
- **Native and light.** One binary, GPU-rendered, no account, no cloud. Also runs in the
  browser. MIT licensed.

![The bend editor: a graph with draggable points and presets](docs/media/bend-editor.png)

## Status

Early and in active development: it edits real songs today, but it is not yet a complete
Guitar Pro replacement. [`docs/parity.md`](docs/parity.md) tracks every Guitar Pro
feature, [`docs/gaps.md`](docs/gaps.md) the known shortfalls. Bug reports with a file
that misbehaves are especially welcome.

## Getting started

Builds for Linux, Windows and macOS are attached to
[GitHub Releases](https://github.com/Gerrit91/guitarscore/releases). To build from
source, install Rust with [rustup](https://rustup.rs) (on Linux also the ALSA development
files: `alsa-lib` or `libasound2-dev`), then:

```bash
git clone https://github.com/Gerrit91/guitarscore
cd guitarscore
cargo gui testdata/basic/ornaments.gp   # build and start the app with a sample song
```

For sound, the app needs a General MIDI SoundFont (release builds include one), for
example `pacman -S soundfont-fluid`. Keys, sound and the `gscore-cli` command-line tool
are described in [`docs/usage.md`](docs/usage.md); the browser build in
[`docs/web.md`](docs/web.md).

## Contributing

GuitarScore is a Cargo workspace of small, layered crates, from the Guitar Pro reader
(`gscore-gp`) up to the app (`gscore-app`). `cargo xtask ci` is the definition of done.
[`docs/architecture.md`](docs/architecture.md) describes the design,
[`docs/development.md`](docs/development.md) the tooling, and [`AGENTS.md`](AGENTS.md)
the rules for contributors, human or AI.

## License

MIT, see [`LICENSE`](LICENSE); third-party code and assets are listed in
[`THIRD_PARTY.md`](THIRD_PARTY.md). Reading and writing Guitar Pro files is based on the
[`guitarpro`](https://github.com/slundi/guitarpro) crate by **slundi** (MIT); its modified
copy is the `gscore-gp` crate, and [`docs/guitarpro.md`](docs/guitarpro.md) lists the
changes. Many thanks for making the Guitar Pro formats accessible.
