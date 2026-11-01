# GuitarScore

A cross-platform desktop editor for guitar tablature (Linux first, also Windows and macOS),
with a Guitar Pro-like workflow: fast, keyboard-only note entry, a clean modern UI and solid
playback. It opens and saves Guitar Pro files. Written in Rust.

GuitarScore must never crash and never lose your work; that outranks features.

## Status

Early development. There is no editor window yet; what works today is reading Guitar Pro
files on the command line:

- **Import:** Guitar Pro 3, 4, 5 (`.gp3`–`.gp5`), 6 (`.gpx`) and 7+ (`.gp`), detected from
  the file content. Malformed files give an error, never a crash.
- **`gscore-cli info`:** song information, tracks, instruments and tunings (with names such
  as "Drop D" or "DADGAD"), and problems such as unfilled measures.
- **`gscore-cli dump`:** the tablature as ASCII tab.

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
```

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

Exit codes: 0 success, 1 the command failed (e.g. unreadable file), 2 wrong usage.

## Layout

A Cargo workspace of small crates. A crate may only depend on crates in lower layers;
`cargo xtask layers` enforces this.

| Layer | Crate | Purpose |
|---|---|---|
| 0 | `gscore-gp` | Guitar Pro song data and reading/writing of all GP formats |
| 1 | `gscore-model` | The document model the app works with: views, exact durations, positions, checks |
| 2 | `gscore-io` | Safe entry point for files: format detection, size limits, crash containment |
| 2 | `gscore-layout`, `gscore-play` | Tablature layout, playback sequencing (planned) |
| 3 | `gscore-edit`, `gscore-audio` | Editing commands, audio output (planned) |
| 4 | `gscore-session` | Crash reports today; recovery journal and autosave planned |
| 5 | `gscore-cli`, `gscore-app` | Command-line tool; desktop app (planned) |

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
