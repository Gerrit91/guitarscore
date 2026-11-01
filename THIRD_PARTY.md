# Third-Party Dependencies

This file documents all third-party assets used in the GuitarScore project.

## Fonts

| Path | Title | Author | Source | License |
|------|-------|--------|--------|---------|
| `assets/fonts/bravura/` (`Bravura.otf`, `bravura_metadata.json`) | Bravura 1.482, SMuFL music font and its metadata | Steinberg Media Technologies | https://github.com/steinbergmedia/bravura, commit `37b194378b710cc40e406ab6c4b07608bb9548ae` (`redist/`) | SIL OFL 1.1 (`assets/fonts/bravura/OFL.txt`) |
| `assets/fonts/noto-sans/` (`NotoSans-Regular.ttf`, `NotoSans-Bold.ttf`) | Noto Sans, text font for fret numbers and titles | The Noto Project Authors | https://github.com/notofonts/notofonts.github.io, commit `e2ec4a11da38a23b1ecebfcb233833a153ecb841` (`fonts/NotoSans/googlefonts/ttf/`) | SIL OFL 1.1 (`assets/fonts/noto-sans/OFL.txt`) |

## Sound Libraries

| Path | Title | Author | Source | License |

## Derived Code

| Path | Title | Author | Source | License |
|------|-------|--------|--------|---------|
| `crates/gscore-gp/` | Code derived from guitarpro 0.4.4 (Guitar Pro reader/writer), see `docs/guitarpro.md` | slundi | https://github.com/slundi/guitarpro, commit `b62be7c5e4bedb0c6eb283ac40aa8371adab5c2a` | MIT (`crates/gscore-gp/LICENSE-guitarpro`) |

## Tools and Libraries

| Name | Version | License | Description |
|------|---------|---------|-------------|
| clap (with `derive`) | 4.6.7 | MIT OR Apache-2.0 | Argument parsing and help for `gscore-cli` (only that crate) |
| serde_json | 1.0 | MIT OR Apache-2.0 | Reading the SMuFL font metadata in `gscore-layout` |
| eframe, egui (wgpu renderer; `wayland`, `x11`, `accesskit`) | 0.37.0 | MIT OR Apache-2.0 | Desktop app window, widgets and rendering (only `gscore-app`) |
| rfd | 0.17.2 | MIT | Native file dialogs; XDG desktop portal on Linux (only `gscore-app`) |
| egui_kittest (dev) | 0.37.0 | MIT OR Apache-2.0 | Headless UI tests and offscreen snapshots of the app |
| image (dev, `png`) | 0.25.10 | MIT OR Apache-2.0 | Saving the snapshot example's PNG (already used by egui_kittest) |

> Add new entries when adding third-party assets or dependencies to the project.