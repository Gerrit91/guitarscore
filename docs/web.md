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
| `index.html` | the page: a canvas for eframe, a loading screen, `#status` for errors |
| `guitarscore.js` | wasm-bindgen's glue |
| `guitarscore_bg.wasm` | the whole editor (about 13.5 MiB; 5.4 MiB gzip, 3.0 MiB brotli) |
| `assets/icon.png` | app icon for the page and the manifest |
| `manifest.json` | installable app metadata |
| `sw.js` | service worker: cached app shell, offline start |
| `worklet.js`, `worklet-polyfills.js`, `worklet_synth.js`, `worklet_synth_bg.wasm` | the AudioWorklet synthesizer |

The build needs `wasm-bindgen` (`cargo install wasm-bindgen-cli --version 0.2.130`, or a
release binary) and benefits from `wasm-opt` (binaryen): without it the module is not
size-optimised. `--debug` builds without optimisations, `--out DIR` writes elsewhere.

## How the platform split works

The app crate is the same on both platforms. Native-only pieces are behind
`cfg(target_arch = "wasm32")`:

- `gscore-app/src/main.rs` is the desktop entry; `gscore-app/src/web.rs` is a
  `#[wasm_bindgen(start)]` function that finds `#guitarscore` and starts eframe's
  `WebRunner` with WebGL2 (`glow`), so it runs anywhere with WebGL, not only WebGPU.
- Files: the browser picks a file with `rfd`'s async dialog, reads its bytes with a
  checked `File.arrayBuffer` (not rfd's panicking `read`), and imports them through
  `gscore_session::open_bytes`. Saving uses `gscore_session::save_bytes` (the same
  read-back and loss check as a file save) and writes to the browser's save handle, which
  downloads the file.
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

A SoundFont is optional. If the site has `assets/soundfonts/default.sf2`, it is fetched and
loaded; without one the engine is silent and editing works. Bundling a licensed General MIDI
SoundFont is M19; choosing one from the browser is not wired up yet.

## Smoke test

`cargo xtask web --smoke` serves `dist/web` and drives Chromium through the DevTools
protocol (a small dependency-free client in `xtask/src/cdp.rs`, because `--dump-dom` does
not wait for wasm and AudioWorklet loading). It checks:

- the app page sets `data-guitarscore-ready` and saves `smoke.png`;
- `worklet-check.html` loads the AudioWorklet module, instantiates the synth wasm and gets
  an answer on its port, and saves `smoke-worklet.png`.

It needs Chromium and Python 3 (`http.server`), not a display.
