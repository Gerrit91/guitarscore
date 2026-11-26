# Third-Party Dependencies

This file documents all third-party assets used in the GuitarScore project.

## Fonts

| Path | Title | Author | Source | License |
|------|-------|--------|--------|---------|
| `assets/fonts/bravura/` (`Bravura.otf`, `bravura_metadata.json`) | Bravura 1.482, SMuFL music font and its metadata | Steinberg Media Technologies | https://github.com/steinbergmedia/bravura, commit `37b194378b710cc40e406ab6c4b07608bb9548ae` (`redist/`) | SIL OFL 1.1 (`assets/fonts/bravura/OFL.txt`) |
| `assets/fonts/petaluma/` (`Petaluma.otf`, `petaluma_metadata.json`) | Petaluma, handwritten SMuFL music font and its metadata (desktop builds) | Steinberg Media Technologies | https://github.com/steinbergmedia/petaluma | SIL OFL 1.1 (`assets/fonts/petaluma/OFL.txt`) |
| `assets/fonts/leland/` (`Leland.otf`, `leland_metadata.json`) | Leland 1.0, MuseScore's engraved SMuFL music font and its metadata (desktop builds) | MuseScore Limited | https://github.com/MuseScore/lelandFont | SIL OFL 1.1 (`assets/fonts/leland/LICENSE.txt`) |
| `assets/fonts/noto-sans/` (`NotoSans-Regular.ttf`, `NotoSans-Bold.ttf`) | Noto Sans, text font for fret numbers and titles | The Noto Project Authors | https://github.com/notofonts/notofonts.github.io, commit `e2ec4a11da38a23b1ecebfcb233833a153ecb841` (`fonts/NotoSans/googlefonts/ttf/`) | SIL OFL 1.1 (`assets/fonts/noto-sans/OFL.txt`) |

## Sound Libraries

| Path | Title | Author | Source | License |
|------|-------|--------|--------|---------|
| `assets/soundfonts/FluidR3-LICENSE.txt` (the `.sf2` itself is bundled into release archives, not the repo) | FluidR3 GM SoundFont (`FluidR3_GM.sf2`) | Frank Wen, Toby Smithe | Debian/Ubuntu `fluid-soundfont-gm` 3.1 (originally MuseScore's fluid-soundfont) | MIT (`assets/soundfonts/FluidR3-LICENSE.txt`) |

| `assets/soundfonts/GeneralUser-GS-LICENSE.txt` (the `.sf2` is downloaded, checked, its samples compressed to Ogg Vorbis and the result copied into the browser build, not the repo) | GeneralUser GS 2.0.3 SoundFont (`GeneralUser-GS-2.0.3.sf2`) | S. Christian Collins | [mrbumpy409/GeneralUser-GS](https://github.com/mrbumpy409/GeneralUser-GS) at `9704918` | GeneralUser GS License v2.0 (`assets/soundfonts/GeneralUser-GS-LICENSE.txt`; not SPDX; allows use and redistribution) |
## Derived Code

| Path | Title | Author | Source | License |
|------|-------|--------|--------|---------|
| `vendor/rustysynth/` | RustySynth 1.3.8 (SoundFont synthesizer), forked: each MIDI channel rendered apart, no `unsafe`; see `vendor/rustysynth/FORK.md` | Nobuaki Tanaka | https://github.com/sinshu/rustysynth, crates.io release 1.3.8 | MIT (`vendor/rustysynth/LICENSE.txt`) |
| `crates/gscore-gp/` | Code derived from guitarpro 0.4.4 (Guitar Pro reader/writer), see `docs/guitarpro.md` | slundi | https://github.com/slundi/guitarpro, commit `b62be7c5e4bedb0c6eb283ac40aa8371adab5c2a` | MIT (`crates/gscore-gp/LICENSE-guitarpro`) |

## Tools and Libraries

| Name | Version | License | Description |
|------|---------|---------|-------------|
| clap (with `derive`) | 4.6.7 | MIT OR Apache-2.0 | Argument parsing and help for `gscore-cli` (only that crate) |
| serde_json | 1.0 | MIT OR Apache-2.0 | Reading the SMuFL font metadata in `gscore-layout` |
| eframe, egui (wgpu renderer; `wayland`, `x11`, `accesskit`) | 0.37.0 | MIT OR Apache-2.0 | Desktop app window, widgets and rendering (only `gscore-app`) |
| midir | 0.11.1 | MIT | MIDI output (ALSA sequencer via the system's libasound on Linux; only `gscore-audio`) |
| rfd | 0.17.2 | MIT | Native file dialogs; XDG desktop portal on Linux (only `gscore-app`) |
| egui_kittest (dev) | 0.37.0 | MIT OR Apache-2.0 | Headless UI tests and offscreen snapshots of the app |
| image (dev, `png`) | 0.25.10 | MIT OR Apache-2.0 | Saving the snapshot example's PNG (already used by egui_kittest) |
| pdf-writer | 0.15.0 | MIT OR Apache-2.0 | Writing PDF files for the PDF export (Typst project; only `gscore-layout`) |
| subsetter (without default features) | 0.2.6 | MIT OR Apache-2.0 | Embedding only the used glyphs of the fonts in PDFs (Typst project; only `gscore-layout`) |
| miniz_oxide (deflate) | 0.8.9 | MIT OR Apache-2.0 OR Zlib | Compressing GuitarScore's state block in a song's notes (pure Rust, works in the browser; only `gscore-app`) |
| crc32fast | 1.5.3 | MIT OR Apache-2.0 | CRC-32 of that block, so a damaged or edited line is noticed and never half-applied (only `gscore-app`) |
| cpal's `pulseaudio` feature: pulseaudio (with futures, mio, enum-primitive-derive) | 0.3.2 | MIT | Linux only: lists the real audio outputs (PulseAudio, or PipeWire's PulseAudio server) for Settings › Audio I/O and plays through the chosen one; a pure-Rust protocol client, no C library (only `gscore-audio`; owner approval 2026-11-26) |
| lewton (with ogg, byteorder, tinyvec) | 0.10.2 | MIT OR Apache-2.0 | Decoding the Ogg Vorbis samples of SF3 SoundFonts, the browser build's compressed font (pure Rust; only `gscore-audio`) |
| clack-host, clack-extensions (with clack-common, clap-sys, libloading) | 0.2.0 (libloading 0.8) | MIT OR Apache-2.0 (libloading: ISC) | Hosting CLAP plugins: safe wrappers around the CLAP C API, loading `.clap` libraries (desktop builds; only `gscore-plugin`, the one crate allowed `unsafe`; owner approval 2026-11-26, `docs/plugin-hosting.md`) |
| clack-plugin (dev) | 0.2.0 | MIT OR Apache-2.0 | Building GuitarScore's test plugin for the plugin tests (never shipped) |

> Add new entries when adding third-party assets or dependencies to the project.