# Handoff: a bundled SoundFont for the browser build (GeneralUser GS)

Branch `web-soundfont` (off `main` at 480da2c, in the main folder, no worktree). Owner's
goal: the browser build must **play out of the box**: a small General MIDI SoundFont
(GeneralUser GS) ships with the site and loads automatically, with no file to choose.
Read `docs/web.md` first: it describes the browser build and its audio path.

## Where things stand

- `cargo xtask web` (`xtask/src/web.rs`) builds the site into `dist/web`. `copy_shell`
  copies `web/assets/` into `dist/web/assets/`; nothing provides a SoundFont today.
- `WebPlayer` (`crates/gscore-audio/src/web_player.rs`) fetches the fixed URL
  `assets/soundfonts/default.sf2` after creating the AudioWorklet. It reports
  `is_loading()` until the worklet answers `loaded`. A 404 becomes the error "no
  SoundFont on this site", which the app shows when you press Play.
- The worklet (`web/worklet.js` + `crates/gscore-worklet`) keeps a Play, seek or tempo
  change made while the SoundFont loads, and applies it once the font is loaded.
- The app starts the player late. `Playback::connect` creates `WebPlayer` on the first
  Play or entered note. `Playback::preload_synth` (called when a song opens) does
  nothing in the browser, because the browser's `Output` is the default `Midi(None)`. The
  toolbar therefore says "MIDI (automatic)", and the 30 MB download starts only when
  the user first presses Play.
- Desktop releases already bundle FluidR3 GM (`.github/workflows/release.yml`, ~140 MB,
  MIT). The desktop is **out of scope** here: do not change its SoundFont.

## Facts about GeneralUser GS (checked 2026-11-05)

- Repo: https://github.com/mrbumpy409/GeneralUser-GS. Latest version 2.0.3
  (2026-02-22), file `GeneralUser-GS.sf2` in the repo root, about 30.8 MB. There are
  **no tags or releases**, so pin a **commit hash** (look it up on GitHub; I could not
  see one).
- License: "GeneralUser GS License v2.0" in `documentation/LICENSE.txt` (not SPDX). It
  allows use in software projects and redistribution free of charge. Attribution is not
  required, but ship the license text anyway. The author also asks: *"please do not link
  directly to my download files … provide your own local copy instead."* So the site
  serves its own copy, and visitors never download from GitHub. Downloading once at
  build time is fine.
- The author's caveat: it needs a standards-compliant synth (full SoundFont 2.01 with
  modulators). FluidSynth is "Excellent"; TinySoundFont ("no modulator support") and
  MuseScore 4 are poor. **Our synth is `rustysynth`** (a port of MeltySynth), and its
  modulator support is unknown. That is the first risk to check.

## Steps (each step: tests, `cargo xtask ci`, a commit)

### 1. Listen first, and stop if it sounds wrong

Before writing code, play GeneralUser GS through our engine on the desktop. Download
the `.sf2`, then run `GSCORE_SOUNDFONT=/path/GeneralUser-GS.sf2 cargo gui FILE` (or choose
it in Settings › Audio). Compare it with FluidR3 on `testdata/basic/*.gp` and a few
multi-track files: clean, overdriven and distorted guitar, bass, strings, pads, piano,
drums, plus bends and slides.

You cannot hear the output, so also render both SoundFonts to WAV with `SynthEngine`,
using a small test or example, and compare levels and silence. A preset that is near
silent or badly filtered is the typical modulator failure.

If presets sound clearly wrong, **stop and report to the owner** with the WAVs. The
alternatives are GeneralUser GS 1.471 (older, from the author's site), TimGM6mb
(~6 MB, GPL-2), or a slimmed FluidR3. The owner picks; do not decide this yourself.

### 2. Get the file at build time, pinned and checked (not in git)

Do not commit the 30 MB `.sf2`: it would bloat the repository forever, and Git LFS is
not set up. Extend `cargo xtask web`:

- Add a constant for the source: the repo's raw URL **at the pinned commit**
  (`https://github.com/mrbumpy409/GeneralUser-GS/raw/<commit>/GeneralUser-GS.sf2`),
  the expected byte size, and the SHA-256. Put the license URL at the same commit next
  to it.
- Download with `curl -fL --retry 3` through `std::process::Command`. xtask has no
  dependencies on purpose; the smoke test already shells out to `python3`. Cache the
  file in `target/web-soundfont/GeneralUser-GS-2.0.3.sf2`, so a rebuild works offline.
- Verify the size and SHA-256 every time (`sha256sum`, or a few lines of SHA-256 in
  xtask). A mismatch is an error that names both hashes. Never ship an unchecked file.
- Add options: `--soundfont PATH` to use a local file (verified only by size >1 MB and
  the `sfbk` RIFF header), and `--no-soundfont` for a build without sound (it must say
  so in its output).
- Write into the site:
  - `assets/soundfonts/GeneralUser-GS-2.0.3.sf2`. The name carries the version, so a
    new font gets a new URL; see step 4.
  - `assets/soundfonts/GeneralUser-GS-LICENSE.txt`, copied from the repo at the same
    commit. Also commit a copy of this license to `assets/soundfonts/` in our repo,
    beside `FluidR3-LICENSE.txt`.
  - `assets/soundfonts/soundfont.json`, a manifest:
    `{"url": "GeneralUser-GS-2.0.3.sf2", "name": "GeneralUser GS 2.0.3", "bytes": N, "sha256": "…"}`.
- Unit-test the pure parts: manifest writing, header check, hash comparison, option
  parsing.

### 3. Load it automatically, as soon as the app starts

- `WebPlayer`: fetch `assets/soundfonts/soundfont.json` first, then the URL it names.
  Keep `default.sf2` as a fallback for a site someone assembled by hand. A missing
  manifest and a missing file keep today's error.
- In the browser, the output is the built-in synth. Make `Output` default to the
  synthesizer on wasm (for example `Output::Synth(PathBuf::from("GeneralUser GS"))`,
  or a new variant if a path feels wrong; `connect` already ignores the path there).
  `output_label` and the Settings › Audio tab should then name the bundled font rather
  than MIDI. In the browser, hide the "Choose…" and MIDI-port parts of that tab:
  `pick_soundfont` returns an error, and the folder scan spawns a thread, which wasm
  does not have. Check it does not panic.
- Start loading when the editor starts, not on first Play: call
  `playback.preload_synth()` once from `crates/gscore-app/src/web.rs` after the app is
  created (or in `with_settings` behind `cfg(target_arch = "wasm32")`). An
  `AudioContext` created before a user gesture starts suspended. That is fine:
  `WebPlayer::play` already resumes it on Play. Chrome may log an autoplay warning;
  that is expected.
- Hand the bytes to the worklet **with a transfer list**
  (`port.post_message_with_transferable(&message, &js_sys::Array::of1(&buffer))`),
  not a structured clone. The clone copies 30 MB on the main thread. `Shared::post`
  needs a variant that also queues the transfer list.
- Progress: read the response body as a stream (`response.body()` →
  `ReadableStreamDefaultReader`) and count bytes against the manifest's `bytes`.
  Expose `WebPlayer::load_progress() -> Option<(u64, u64)>` and use it in the toolbar's
  "Loading sounds…" bar (`app/toolbar.rs`, today an endless animation) as a real
  fraction. Add the needed `web-sys` features to `gscore-audio/Cargo.toml`.
- Parsing 30 MB in the worklet blocks the audio thread for a moment. That is
  acceptable at startup, since nothing plays yet. Measure it in the smoke test and
  note it in the devlog; if it is over ~1 s, report it.

### 4. Cache it across visits and deploys

`web/sw.js` uses one cache named per build, and `activate` deletes every other cache.
Each deploy would therefore throw away the 30 MB font and download it again.

- Keep SoundFonts in their own cache, `guitarscore-soundfonts`. The `activate` handler
  must not delete it. Serve `assets/soundfonts/*.sf2` cache-first from it, and on
  `activate` drop entries whose URL is not the manifest's current one.
- Fetch the manifest network-first (fall back to the cache when offline), so a new
  font is noticed.
- Do not put the `.sf2` in `SHELL`: install would wait for 30 MB. The first fetch
  fills the cache.
- `cache.put` rejects partial (206) responses: only cache `status === 200`.

### 5. Credits and docs

- `THIRD_PARTY.md`: a row like FluidR3's (path, title, author S. Christian Collins,
  source URL with the pinned commit, license).
- About window (`app/about.rs`): in the browser, name the SoundFont and its license.
- `docs/web.md` (Audio section and the files table), `docs/gaps.md` (remove the
  "No SoundFont" gap), `web/assets/README.txt`, `README.md` (build options), and an
  entry in `docs/devlog.md`. Report the site's size: the `.sf2` raw and gzipped. Static
  hosts often do not compress `application/octet-stream`.

### 6. Tests

- Extend `cargo xtask web --smoke`:
  - The app page waits until the player has loaded. Add a page marker, such as
    `data-guitarscore-sound="loaded" | "error: …"` on `<body>`, set from Rust when
    `WebPlayer` sees `loaded` or an error. It fails on an error.
  - `web/worklet-check.html` loads the manifest's font, renders about one second of a
    played note (send a short sequence or a `midi` note-on), and checks the samples
    are not silent (RMS above a small threshold).
- Rust tests for the worklet's pending play/seek/tempo, if you touch it.
- Run `cargo xtask ci` and `cargo xtask web --smoke`. The web build needs
  `wasm-bindgen-cli` **0.2.130** exactly (`cargo install wasm-bindgen-cli --version
  0.2.130 --locked`; Arch's package is 0.2.128 and fails), plus `chromium` and
  `python3`.

## Rules

- Commit locally on this branch; the owner merges. **Never push, open PRs or use
  `gh`.** Writing a deploy workflow is a separate task, only if the owner asks.
- Keep files under ~400 lines (`web_player.rs` is ~390: put the fetch and progress code
  in a new `web_soundfont.rs`).
- Follow the house style: errors are messages for the user, no `unwrap` outside tests,
  checked arithmetic (`clippy::arithmetic_side_effects` is on in several crates).
- Delete this file in the last commit, and summarise in the devlog what was done and
  what is still open.
