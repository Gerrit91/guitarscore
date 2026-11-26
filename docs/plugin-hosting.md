# Plugin hosting (M24): design note

Status: step 1 started 2026-11-26 (branch `m24-plugins`). Owner decisions are dated.

## What and why

Plugins as a track's sound source (an instrument fed the track's notes) and as blocks of
a sound chain (track, drum part, bus, master), beside SoundFont, modelled strings and MIDI
out. Desktop builds only: the browser has no plugins.

Format: **CLAP** first (C ABI, MIT, simpler threading, good for running in another
process). VST3 later. Its SDK has been MIT-licensed since VST 3.8 (2025, check it again
before the VST3 step). Not VST2: its SDK cannot be licensed.

## The unsafe exception (owner's approval, 2026-11-26)

AGENTS.md section 2, rule 3 asks for a design note, an isolated crate and the owner's
approval. This section is the note.

- **Why it is needed:** a plugin is native code in a dynamic library. Opening the library
  runs its initializers and gives us function pointers that we have to trust to follow the
  CLAP ABI. Rust cannot check either, so opening a library is `unsafe` and cannot be
  written in safe Rust.
- **How small it is:** the host uses clack (`clack-host`, `clack-extensions`, MIT/Apache-2.0,
  crates.io), safe wrappers around the CLAP API with the `unsafe` and its proofs inside
  clack. GuitarScore's own `unsafe` is a single block, `PluginEntry::load` in
  `crates/gscore-plugin/src/library.rs`, with its `SAFETY` comment. The test plugin
  (`crates/gscore-plugin/examples/test_plugin.rs`, built by `cargo test`, never shipped)
  allows it too, because a CLAP library must export its unmangled `clap_entry` symbol;
  clack's macro writes it.
- **Isolated crate:** `gscore-plugin` (layer 2, no workspace dependencies). It is the only
  crate that may use `clack-*` and `libloading`, and the only one that does not take
  `[workspace.lints]` as they are: a `forbid` cannot be allowed locally, so it copies the
  table with `unsafe_code = "deny"`, plus `clippy::undocumented_unsafe_blocks = "deny"`.
- **Enforced:** `cargo xtask layers` (`xtask/src/unsafe_check.rs`) fails if the copied
  lints differ from the workspace's in any other entry, or if any file other than the two
  above allows `unsafe_code`. Every other crate keeps `forbid`.
- **What it does not protect against:** a plugin that crashes (a segfault) or hangs.
  Rust cannot catch that inside the process. The separate host process (step 3) is what
  keeps such a plugin away from the app and the user's work.

## Structure

- `gscore-plugin`: finding libraries (`default_dirs`, `bundles_in`: CLAP's folders,
  `CLAP_PATH` first), opening them (`Library`, `PluginInfo`), an effect's main-thread side
  (`Effect`: parameters, value texts, settings as an opaque blob, latency, requests) and
  its audio side (`Processor`: stereo in place, split into blocks, parameter changes as
  events, all buffers made at activation, nothing allocated while processing).
- CLAP's threads: an `Effect` stays on the thread that made it (that thread is the
  plugin's "main thread"); the `Processor` goes to the audio thread and comes back to
  `Effect::deactivate`. In the app, one plugin thread owns every `Effect`, and later the host
  process's main thread does.
- `gscore-guitar` stays pure and wasm-clean. The chain gets a small trait for a block whose
  processing happens elsewhere. `gscore-audio` implements it on native builds with a
  `Processor` (step 2), and later with the host process's client.
- Saving: the plugin's settings blob (and its id and library) go with the tone or the
  song's GuitarScore state, never into the GP file. A song opened where the plugin is
  missing plays without that block (bypassed) and says which plugin is missing.

## Plugin editors (owner, 2026-11-26: they must work; replaces "no editor windows")

Users expect a plugin's own window, and some plugins can only be set up there (loading an
amp model, an impulse response, a sample library).

- The editor is a **floating window owned by the plugin host process**, not embedded in
  the app's window. On Linux that window is X11 through XWayland (CLAP's `x11` window API;
  VST3 has only X11 on Linux). The app itself stays a native Wayland window. On Windows and
  macOS it is the same model with a native window.
- An editor that crashes takes only the host process down, which is the case the watchdog
  already handles.
- **The egui parameter view stays** and is always there: styled like the built-in blocks,
  with search and favourites for plugins with many parameters. It is also the fallback when
  a plugin has no editor or there is no X server (XWayland turned off).
- Known limits: across Wayland and X11 the editor cannot be a child of our window (it does
  not stay above it or move with it; "bring to front" from the block helps). HiDPI goes
  through `gui.set_scale`, which some plugins ignore. Keyboard focus between the two
  windows needs testing on GNOME and KDE.
- Changes made in the editor come back as parameter events and the "settings changed"
  request: the egui view and the saved blob follow, and one gesture is one undo step.

## Never crash: the host process (step 3)

- A helper binary runs the plugins. The app talks to it over a pipe (commands) and shared
  memory (audio). The audio thread never waits on it: a plugin's output is used one block
  later, and that latency is made up like a plugin's own latency. A block that is not
  ready in time is silence, and repeated misses bypass the plugin.
- If the process dies or hangs (watchdog), its plugins are bypassed, the user gets a
  message, and they can be restarted. Editing continues.
- Scanning runs in that process too, in the background. A library that crashes while
  being scanned goes on a blocklist (kept in the settings, and the user can clear it).

## Steps

1. A CLAP effect, headless: open, list, start, process, parameters, settings, latency;
   `gscore-cli plugins [--check]`; tests with the test plugin. *Done 2026-11-26.*
2. The plugin block in a sound chain (in process, "experimental" in Settings): the chain
   trait, the plugin thread, choosing a plugin, the egui parameter view, saving the blob
   with the tone, latency made up, WAV export and `gscore-cli wav` rendering offline.
3. The host process, the watchdog and background scanning with the blocklist. Plugins
   leave "experimental" only after this step.
4. Plugin editors in the host process (X11 through XWayland on Linux).
5. A CLAP instrument as a track's sound (notes, bends and slides as CLAP note events and
   expressions; tempo and transport).
6. VST3.

## Test plugins

- `examples/test_plugin.rs`: our gain effect, used by every automated test.
- By hand: Dragonfly Reverb 3.2.10 (four reverbs, GPL-3) and LSP Plugins 1.2.37 (198
  plugins in one library, LGPL-3/GPL-3), both in the owner's Downloads folder. They are
  only loaded, never bundled or committed. `GSCORE_TEST_CLAP=<file>[#id] cargo test -p
  gscore-plugin` runs the real-plugin test against one of them. On 2026-11-26,
  `gscore-cli plugins --check` ran all 4 Dragonfly reverbs and 179 LSP effects without a
  failure.
