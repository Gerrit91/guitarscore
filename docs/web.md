# The browser build

GuitarScore runs in a browser as a static site: no server-side code, no database, and the
song files stay on the user's disk. The build is produced by:

```bash
cargo xtask web            # dist/web/
cargo xtask web --smoke    # also start it in headless Chromium and save screenshots
```

`dist/web` is what a static host serves:

| File | What it is |
|---|---|
| `index.html` | the page: a canvas for eframe, a loading screen (editor, then SoundFont download), `#status` for errors |
| `guitarscore.js` | wasm-bindgen's glue |
| `guitarscore_bg.wasm` | the whole editor (about 13.5 MiB; 5.4 MiB gzip, 3.0 MiB brotli) |
| `assets/icon.png` | app icon for the page and the manifest |
| `assets/soundfonts/GeneralUser-GS-2.0.3-<hash>.sf3` | the bundled General MIDI SoundFont, samples compressed (5.6 MiB; 30.8 MiB as SF2) |
| `assets/soundfonts/soundfont.json` | which SoundFont to load, its size and SHA-256 |
| `assets/soundfonts/GeneralUser-GS-LICENSE.txt` | the font's license |
| `manifest.json` | installable app metadata |
| `sw.js` | service worker: cached app shell, offline start |
| `worklet.js`, `worklet-polyfills.js`, `worklet_synth.js`, `worklet_synth_bg.wasm` | the AudioWorklet synthesizer |

The build needs `ffmpeg` with libvorbis (to compress the SoundFont), `wasm-bindgen` (`cargo install wasm-bindgen-cli --version 0.2.130`, or a
release binary) and benefits from `wasm-opt` (binaryen): without it the module is not
size-optimised. `--debug` builds without optimisations, `--out DIR` writes elsewhere.

The site ships GeneralUser GS 2.0.3 (GeneralUser GS License v2.0, which allows modifying
it). The first build downloads it from the upstream repository at a pinned commit into
`target/web-soundfont/` and verifies its size and SHA-256 every time; later builds reuse the
cache. Then its samples are compressed to Ogg Vorbis (q6, ffmpeg's libvorbis, one process
per sample, in parallel; cached as well): an SF3, 5.6 MiB instead of 30.8. Every Vorbis
stream repeats the same header pages (3.4 MiB over 920 samples), so they are stored once
in a `vhdr` chunk; `crates/gscore-audio/src/sf3.rs` describes the layout. The encoding is
reproducible, and the file name carries a hash of the content. In the browser the worklet
decodes it back to an SF2 before the synthesizer reads it, about a second (one native
second; the page shows "Preparing sounds…" meanwhile). Measured against the original:
every sample has its exact length and loop points, and its level within 4 %.
`SoundBank` reads SF3 everywhere, so a desktop user may pick one too.
`--soundfont PATH` ships another `.sf2` or `.sf3` as it is (its size and RIFF header are checked; its
license is yours to add to the site, GeneralUser GS's is not copied for it), and
`--no-soundfont` builds a silent site and says so. Nothing from the upstream repository is
committed except the license text.

## How the platform split works

The app crate is the same on both platforms. Native-only pieces are behind
`cfg(target_arch = "wasm32")`:

- `gscore-app/src/main.rs` is the desktop entry; `gscore-app/src/web.rs` is a
  `#[wasm_bindgen(start)]` function that finds `#guitarscore` and starts eframe's
  `WebRunner` with WebGL2 (`glow`), so it runs anywhere with WebGL, not only WebGPU.
- Files: the browser picks a file with `rfd`'s async dialog, reads its bytes with a
  checked `File.arrayBuffer` (not rfd's panicking `read`), and imports them through
  `gscore_session::open_bytes`. Saving uses `gscore_session::save_bytes` (the same
  read-back and loss check as a file save) and starts a download under the song's name
  (Save and Save As alike; the browser decides where it goes). rfd's web save is not
  used: it shows its own "click here to download" overlay and reports success even when
  that is cancelled, which would mark an undownloaded song as saved.
- Leaving: with unsaved edits, closing or reloading the page asks first (`beforeunload`),
  because there is no recovery journal in the browser yet. The title bar has no window
  buttons and the File menu no Quit: the browser owns the tab.
- Storage: settings and shortcuts live in `localStorage` (`gscore-session/src/web.rs`),
  with the same "missing or broken falls back to defaults, and says why" behaviour as the
  files on the desktop. The settings window and keymap continue to work; a keymap file is
  not read in the browser.
- The recovery journal is still file-based, so the browser build starts without one. This
  is the main missing browser feature (see `docs/gaps.md`); saving is unaffected.
- Threads: a wasm instance has none, so opening imports on the UI thread after the bytes
  arrive (the one step that briefly blocks), and save/export run in `spawn_local` tasks.
  `web-time` supplies `Instant` (`performance.now()`), since `std::time::Instant` panics on
  wasm.
- `gscore-audio`'s device backends (`cpal`, `midir`) are `cfg`-gated out. The synth engine
  and all effects are the same pure-Rust code, compiled a second time into the worklet.

## Audio

The browser has no audio device API from wasm, so playback goes through an AudioWorklet:

1. `WebPlayer` (main thread) creates an `AudioContext` and loads `worklet.js`.
2. It fetches and compiles `worklet_synth_bg.wasm` **on the main thread** and sends the
   `WebAssembly.Module` to the worklet. An AudioWorkletGlobalScope has no `fetch`, no `URL`
   and no `TextDecoder`, so this is the only way; `worklet-polyfills.js` supplies the UTF-8
   globals wasm-bindgen's glue expects, and `worklet.js` uses `initSync`.
3. Songs cross as `gscore_play::encode_sequence` bytes (events, tempo map and end tick;
   checked decoding, capped by the message length).
4. The worklet renders stereo blocks with `SynthEngine`; the main thread sends play, pause,
   stop, seek, tempo and live mixer events through the node's port, and receives position
   and state back.

At startup the page fetches `assets/soundfonts/soundfont.json`, then the versioned `.sf2`
it names (falling back to `default.sf2` for a hand-assembled site). The response is read
as a stream, so the page's loading screen and the toolbar's "Loading sounds…" bar show
real bytes (the app writes `data-guitarscore-sound-bytes` on `<body>` for the page; the
screen's "Start without waiting" leaves the download to the toolbar), and the bytes are
transferred to the worklet rather than copied. The worklet parses the font on the audio
thread and answers `loaded`; a Play, seek or tempo change made meanwhile takes effect once
it is loaded. Measured in the smoke test: about 2.2 s from page start to a decoded, parsed font
(local server). A site without one says so on Play; editing works either way. The app names
the font in About; choosing another one from the browser is not wired up yet.

## Smoke test

`cargo xtask web --smoke` serves `dist/web` and drives Chromium through the DevTools
protocol (a small dependency-free client in `xtask/src/cdp.rs`, because `--dump-dom` does
not wait for wasm and AudioWorklet loading). It checks:

- the app page sets `data-guitarscore-ready` and saves `smoke.png`;
- a song dropped on the page (a synthetic drop event) and a typed fret leave a rescue copy
  of the edited song with the page (`smoke-edited.png`), and the crash screen a panic shows
  offers it for download (`smoke-crashed.png`), see "Crashes" below;
- a deploy: once the service worker controls the page, the built `sw.js` and `index.html`
  are changed as a new build's would be, and one visit later the page must run the new
  build (the files are put back afterwards);
- `worklet-check.html` loads the AudioWorklet module, instantiates the synth wasm, loads
  the bundled SoundFont, plays middle C through an analyser, requires non-silent samples,
  and saves `smoke-worklet.png`.

It needs Chromium and Python 3 (`http.server`), not a display.

## Crashes

A panic aborts the wasm instance (`panic = abort` on `wasm32-unknown-unknown`; nothing
unwinds, so the desktop's `catch_unwind` containment does not exist here). Two things keep
the user's work:

- **Rescue copies.** Every tab with unsaved edits hands the page a Guitar Pro 7 copy of its
  song (the bytes Save would download) once its edits have rested for a second
  (`crates/gscore-app/src/app/web_rescue.rs`). The copies live in the page's JavaScript,
  `globalThis.guitarscoreRescue`, outside the instance; a saved or closed tab takes its
  copy back. They are in memory only: a closed tab or a reload loses them (a
  storage-backed journal is still open, `docs/gaps.md`).
- **The crash screen.** The panic hook (`web.rs`) calls the page's `guitarscoreCrashed`,
  which hides the canvas and offers each copy for download, then Reload (asking first if a
  copy was not downloaded). An uncaught `WebAssembly.RuntimeError` after the editor started
  (an abort whose hook could not run) shows the same screen.

## Updates

`cargo xtask web` stamps a cache name unique to the build into `sw.js` (version and build
time). The worker fetches the app shell with `cache: "reload"`, so the browser's HTTP cache
cannot hand it the previous build's files. The visit after a deploy is still served by the
old worker; when the new one takes over, the page reloads itself once (`controllerchange`
in `index.html`; the unsaved-edits guard asks first), so the new build runs at that visit
rather than the one after.

## GitHub Pages

The release workflow deploys the same `dist/web` to GitHub Pages:
<https://gerrit91.github.io/guitarscore/>. Every path in the site is relative, so it runs
under the repository's subpath; Pages serves HTTPS (audio needs a secure context) and
`.wasm` as `application/wasm`. Pages sets a short cache lifetime on every file, the
SoundFont included, which the service worker's own SoundFont cache makes up for. Once per
repository: Settings > Pages > Source "GitHub Actions", and the `github-pages` environment
must allow the release tags (Settings > Environments > github-pages > deployment branches
and tags, add `v*`), since a release runs on its tag.

## Docker

The `Dockerfile` puts a built site into nginx; it builds nothing itself:

```bash
cargo xtask web
docker build -t guitarscore-web .
docker run --rm -p 8080:80 guitarscore-web     # http://localhost:8080
```

`docker/nginx.conf` serves `.wasm` as `application/wasm`, gzips the app, makes the
browser revalidate everything except the SoundFont (whose versioned name is cached for a
year). The release workflow builds the site and pushes the image to
`ghcr.io/<owner>/<repo>-web` with the release tag and `latest`. Audio needs a secure
context: serve it over HTTPS (or `localhost`).
