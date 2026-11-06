# AGENTS.md: guide for AI agents and contributors

## 🚨 AGENT PROTOCOL (Read First)
To avoid execution errors and "weird behavior," all agents must follow these three rules:
1. **TOOLING:** Follow the strict tool-use protocol in `docs/tool-guide.md`. **Never** mix Top-Level tools (read, write, shell, websearch) and Execute-Only tools (codegraph, rust-analyzer).
2. **STABILITY:** Adhere to the stability rules in Section 2 of this document. No panics, no `unwrap()`, and verify all changes with the required tests.
3. **DIRECT ACTION:** You are an agent with full write access. **Do not provide code for the user to copy-paste.** You must implement changes directly using the `write` and `edit` tools. Saying "I will provide the code for you to add" is considered a task failure.

---

## 1. What this project is

**GuitarScore** is a cross-platform (Linux first, also Windows and macOS) desktop editor for guitar tablature, with a workflow and feel similar to Guitar Pro: fast, **keyboard-only note entry**, a clean modern UI, and solid playback. It opens and saves Guitar Pro files. The command-line name is `guitarscore`.

Not goals: pixel-copying Guitar Pro, full engraving parity with MuseScore, or a DAW.

### Legal / IP rules
- Similar *workflows and shortcuts* are fine. **Never copy Guitar Pro's assets, icons, logos, name, or UI artwork.**
- Do not use code or data from proprietary software. File format knowledge (via the MIT-licensed `guitarpro` crate, now integrated as `gscore-gp`, and public documentation) is fine for import/export.
- **Do not copy code from GPL-licensed projects** (MuseScore, TuxGuitar, and others) unless the owner explicitly decides on a compatible repository license. Studying their *behavior and output* as a reference is fine.
- Test and demo scores must be created by the owner or be clearly permissively licensed. Never commit copyrighted tabs.
- Every third-party asset (font, icon, soundfont, sample) keeps its license file next to it and gets a row in `THIRD_PARTY.md` (path, title, author, source, license) in the same change. Check licenses before bundling; do not guess.

**GuitarScore must never crash, never panic, and never lose the user's work.** A user who forgets to save must still get everything back after any failure. This outranks features, speed, and elegance. **A known crash bug blocks new feature work:** fix it before building on top.

A malformed file, a bad command parameter, a corrupt settings file, an odd keystroke, or a full disk must produce an error the user can act on, never a panic.

### Rules for code
1. **Non-test code never panics.** No `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`, or `unimplemented!`. Return the crate's error type and propagate with `?`; use `ok_or(..)?`, `let .. else { return Err(..) }`, `if let`, or `unwrap_or*` only where the fallback is truly correct (never one that silently corrupts a document). **Unfinished or unsupported features return an "unsupported" error.** The only exception is a provably infallible literal: `#[allow(clippy::expect_used)]` plus `.expect("why it cannot fail")`.
2. **Enforced by tooling.** The workspace `Cargo.toml` defines `[workspace.lints]` (denying `clippy::unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `unreachable`, `indexing_slicing`; enabling `arithmetic_side_effects` in parsing, layout, and sequencing code). **Every crate must contain `[lints] workspace = true`.** `clippy.toml` allows these in tests only (verify the option names for the pinned toolchain). CI and the definition of done run clippy with `-D warnings`.
3. **No `unsafe`.** `unsafe_code = "forbid"` workspace-wide. Any exception needs a written design note, an isolated crate, and the owner's approval.
4. **Input-derived numbers are hostile.** This covers file contents, command parameters, selections, and arithmetic on them. Use `get()` instead of `[i]` or `[a..b]`; slice strings only at char boundaries; use `checked_*` and `saturating_*` for lengths, offsets, durations, and counts; guard division by zero and NaN/inf-to-integer casts; cap allocations that are sized by input.
5. **Bound all recursion and loops** over input-derived structures with depth limits or seen-sets (files can be deep, cyclic, or enormous).
6. **Do not cascade failures.** Handle lock poisoning (`lock().unwrap_or_else(PoisonError::into_inner)`) and treat thread joins as `Result`s.
7. **Third-party code can panic too.** Every call into the Guitar Pro reader in `gscore-gp` (derived from the `guitarpro` crate and not yet held to these lints) and into any other parser of untrusted data happens inside `gscore-io` behind `std::panic::catch_unwind`, converting panics into errors. Fix such bugs in `gscore-gp` with a regression test, and record the change in `docs/guitarpro.md`.
8. **Commands are transactional.** A command either fully applies or leaves the model unchanged (validate before mutating, or work on a copy).
9. **Never block the UI thread** with file I/O, parsing, or other long work. Use background threads with progress and cancellation. Use bounded queues; cap undo history by memory.
10. **Prove every fix.** Each crash fix comes with a small synthetic regression test that panicked before the fix.

### Containment as a last resort
- Keep `panic = "unwind"` in all profiles (never `abort`).
- Install a panic hook that records diagnostics (message, location, backtrace) to a log file.
- Wrap in `catch_unwind`: command execution (roll back to the pre-command state), importers and exporters, each layout pass, and the per-frame UI update where practical. After a caught panic, write an emergency recovery snapshot and show a friendly notice instead of exiting. This is a safety net, not a licence to panic.
- The audio engine runs on its own thread(s) with the same containment. If audio fails, playback is disabled with a message, and **editing continues**.

### Never lose work
- **Recovery journal:** every applied command is appended to a journal in the recovery directory, plus periodic full snapshots (by command count and elapsed time) written off the UI thread. Writes are atomic or checksummed so a half-written file is detected and ignored.
- **Crash recovery on startup:** if a session did not exit cleanly, offer to restore it from the journal and snapshot.
- **Autosave never overwrites the user's file.** It writes only to the recovery directory. Explicit saves use the safe-save procedure (section 5).
- On window close, SIGTERM, and system shutdown (best effort), write a recovery snapshot and prompt about unsaved changes.
- Crash reports are written locally. Nothing is sent anywhere without explicit consent.

### Stability tests (required)
- **Fuzz and property tests for every importer:** arbitrary and mutated bytes must never panic or hang. Keep deliberately corrupted files in `testdata/corrupt/`.
- **`panic_hunt` test for commands:** iterate over *every* registered command, apply it to varied songs (empty, one measure, huge, unusual tunings) with adversarial parameters (negative, huge, NaN, empty, wrong type). It must return `Ok` or `Err`, never panic, and model invariants must hold. Every new command also comes with a graceful-failure test.
- **Property tests:** random command sequences keep model invariants; undoing everything restores the original song.
- **Recovery test:** kill the process mid-edit, restart, and verify the session is restored.
- **Panic-injection test:** a test-only hook that makes a command or layout pass panic, to verify rollback and the recovery path.
- Every dependency is a potential crash source: prefer well-maintained crates and review how they handle untrusted input.

## 3. Working agreements for agents

- **Keep the tree building at every step.** Sessions can end abruptly; a green tree plus the devlog is how the next session picks up.
- **Devlog:** after each task, append a terse entry to `docs/devlog.md` (what landed, numbers, what is still open). Keep `docs/gaps.md` (ranked known shortfalls) and `docs/parity.md` (checklist of Guitar Pro features: live, partial, missing) up to date when your work changes them, with the date.
- **Pick work in this order:** crashes and data-loss risks, then the current milestone, then `docs/gaps.md`, then parity items that are cheap because the building blocks already exist.
- **Verify UI changes visually.** Render offscreen to PNG with the snapshot example (no window, no focus stealing) and look at the image. Compare tablature layout against the MuseScore reference (section 5).
- **Everything automatable is a command or CLI subcommand.** `gscore-cli` provides headless `info`, `dump` (ASCII tab), `render` (SVG/PDF), `midi`, `play`, `convert`, `edit`, `bench` and `roundtrip-check`, so agents and tests can exercise the whole pipeline without the GUI. A JSON control channel for driving the running app may follow later; design the command layer so it is possible.
- **Project tasks live in `xtask`** (`cargo xtask ci | layers | bench | web`), not in ad-hoc shell scripts.
- **Parallel agents:** use separate target dirs (`CARGO_TARGET_DIR=target/agent-<name>`), put new commands in a new module, keep every `Cargo.toml` valid at all times (write manifests atomically: temp file, then `mv`), make small surgical edits to shared files, and re-read before editing them.
- Ask the owner before irreversible or architectural decisions; write a short design note for anything that affects several crates.
- **Branches:** work on a feature branch in the main checkout, never commit on `main`
  (check the branch right before each commit; the owner may have merged meanwhile).
  Commit only when the checks in section 6 pass. The owner pushes, opens pull requests
  and merges; agents do not.

## 4. Code quality

Write the best idiomatic Rust you can. Use generics, traits, enums, and iterators wherever they earn their place: removing real duplication, enabling tests, or making invalid states unrepresentable. Do not add abstraction speculatively; wait for two or three real uses.

Avoid the "big ball of mud":
- **Small, focused crates and modules**, each with one clear job and a short public API. Keep everything else private (`pub(crate)` or less).
- **Small traits, defined where they are consumed.** No giant "manager" or "service" traits.
- **Composition over hierarchy.** Plain structs, enums, and functions. No god object: the app struct holds state and delegates.
- **Explicit over magic.** No global mutable state, few macros, typed errors in library crates (for example via `thiserror`).
- **Layering is enforced by `cargo xtask layers`** (section 5). No cycles.
- **Soft limit of about 400 lines per file.** Split by responsibility, not by kind of type.
- **Prefer pure functions** for layout, conversion, and sequencing, so they are trivially testable.
- **No instrument assumptions.** Do not hard-code 6 strings, standard tuning, 4/4, a single voice, or ASCII text. Support 4 to 8 strings (bass, extended range), arbitrary tunings, capo, time signature and tempo changes, repeats, multiple voices, and Unicode titles and lyrics. Test with several configurations.
- Public items have rustdoc comments; each module starts with a short comment on its responsibility.
- **At the end of each milestone, review the architecture for creeping coupling** and propose refactors before moving on.

**Performance is a feature.** Layout and rendering must stay incremental: cache layout per measure and per document revision, never relayout or rescan the whole score per frame, and keep key-press-to-screen latency within one frame on large scores. Benchmark heavy paths (layout, import, sequencing) in release mode with a large test score, and record before/after numbers in the devlog.

Communication: the owner is new to Rust and reviews the code. After each task, summarize what changed, which files to read first, and any non-obvious design decisions or Rust idioms (briefly). The code itself should not be dumbed down.

## 5. Architecture

Cargo workspace. Layers are listed from lowest to highest; **a crate may depend only on lower layers**, and `cargo xtask layers` (reads `cargo tree`, uses the table in `xtask/src/layers.rs`) fails the build otherwise. A new crate must be registered there.

```
guitarscore/
  Cargo.toml                 # workspace, [workspace.lints]
  rust-toolchain.toml        # pinned toolchain (with rustfmt, clippy, the wasm target)
  clippy.toml, kittest.toml  # lint options for tests; snapshot tolerance
  AGENTS.md, CLAUDE.md, THIRD_PARTY.md, LICENSE, README.md
  docs/                      # architecture, development, usage, web, parity, gaps, devlog,
                             # guitarpro (the vendored reader's changes), guitar-modelling
  testdata/                  # owner's own test songs with Guitar Pro's PDF and MIDI export;
                             # testdata/corrupt/ for broken files
  corpus/                    # gitignored: the owner's larger private collection
  assets/                    # fonts/ (Bravura, Noto Sans), soundfonts/ (licenses only)
  packaging/                 # desktop file and app icon (AppImage)
  web/, docker/, Dockerfile  # the browser build's page, worklet and service worker; nginx
  xtask/                     # project tasks: layers, ci, bench, web
  crates/
    gscore-version/           # L0: the build version baked in at compile time
    gscore-gp/                # L0: Guitar Pro song data + reading/writing GP3-5, GPX, GP7+ (derived from guitarpro, MIT)
    gscore-model/             # L1: document model (wraps gscore-gp's song), positions, queries, edits, invariants
    gscore-io/                # L2: GP files <-> model: format detection, limits, catch_unwind (bytes in, bytes out; no file system)
    gscore-layout/            # L2: model -> positioned draw ops (pages, systems, notation, tab), SVG and PDF. No GUI imports.
    gscore-play/              # L2: sequencer, MIDI events, channel allocator, metronome, SMF export, Player trait. No device crates.
    gscore-edit/              # L3: cursor, selection, clipboard, commands, undo/redo, keymap
    gscore-audio/             # L3: output backends (midir, cpal, rustysynth) implementing Player; the tuner
    gscore-session/           # L4: open, safe save and export, recovery journal, autosave, settings files
    gscore-worklet/           # L4: the browser's AudioWorklet synthesizer (wasm only)
    gscore-cli/               # L5: headless tool (info, dump, render, midi, play, convert, edit, bench, ...)
    gscore-app/               # L5: egui/eframe application (desktop and browser); the `guitarscore` binary
```

Dependency restrictions enforced by the layers check: only `gscore-app` may use egui, eframe, winit, or rfd; only `gscore-audio` may use `cpal`, `midir`, or `rustysynth`; nothing below `gscore-app` may use a GUI crate; only `gscore-model` and `gscore-io` may use `gscore-gp` (and only `gscore-io` may use `gscore_model::interop`).

### Key design decisions

1. **Document model.** The `guitarpro` crate (MIT) reads and writes GP3-5, GPX, GP7+, MusicXML and MuseScore files and has its own score model. Since the app saves back to GP files, converting to a separate model risks silently dropping any field we do not map. **Default plan: use the crate's model directly**, wrapped by `gscore-model` with helper methods (queries, position types such as track/measure/voice/beat indices, invariant checks). Introduce our own data structures only where the editor demonstrably needs them. **Decision gate at M2:** evaluate the crate against the owner's real files, including corrupted ones. Because the crate is young and single-maintainer, the plan allows **vendoring it into the workspace** (MIT), in particular to fix panics. Never scatter direct uses of the crate's types across the code base; go through `gscore-model`.
   **Decided (2026-11-01):** `gscore-model` wraps the crate's song model. The crate was vendored to fix data-loss bugs and then integrated as our own crate `gscore-gp` (unused parts removed); origin, license and every change are in `docs/guitarpro.md`.
2. **No custom score file format.** The score is saved as a GP file (see "Saving"). User settings and UI state (theme, keymap, recent files, window layout) live in separate serializable config files, never inside score files.
3. **Layout is headless.** `gscore-layout` turns the model plus viewport settings into a `Vec<DrawOp>` (lines, glyphs, text, rects). Colors are **semantic tokens** (for example `Staff`, `NoteText`, `Cursor`, `Selection`), never hard-coded RGB. The GUI maps tokens to a theme. This keeps layout testable without a window and makes theming a day-one feature.
4. **Theming from day one.** Light and dark palettes defined once. Icons are monochrome SVG recolored by the theme. (Black icons on a dark theme are unusable; do not ship that.)
5. **The UI is thin and data-driven.** UI state is plain serializable data, so it can be saved, restored after a crash, and inspected by tests. All actions go through commands; UI code never mutates the model directly.
6. **Every edit is a command.** Commands have an id, label, `enabled`, parameters, `apply`, and `undo`. They power undo/redo, the recovery journal, the keymap, the CLI, and the `panic_hunt` test.
7. **Keymap is data.** Keys map to *command ids* via a config file (names in section 7). **A binding means a typed character, not a physical key position.** Letters, digits, and symbols are bound by the character the user's layout produces (for example `"c"`, `"+"`, `"5"`); non-character keys (arrows, Enter, Tab, Backspace, Escape, F-keys) are bound by name; modifiers combine with either. Rules:
   - Use the logical key or text events the GUI toolkit provides; verify how egui reports them (it may offer both logical and physical key information).
   - Test the keymap on QWERTY, QWERTZ, and AZERTY layouts, including characters that need Shift or AltGr (on some layouts `+`, `-`, or even digits need Shift; fret entry must still work, and the numpad is an alternative).
   - Dead keys and IME composition must never be interpreted as commands; in note-entry mode ignore composition events.
   - Do not bind keys with AltGr, and avoid browser-reserved combinations (new tab, close tab, and similar) in case a web build happens.
   - The keymap loader reports conflicts and unknown command ids as errors shown to the user, never as panics, and falls back to the default keymap.
8. **Playback behind a trait.** `gscore-play` defines the `Player` trait (load a sequence, play, pause, seek, stop, tempo scale, live events, replace); `gscore-audio` implements it for a MIDI port, the native synthesizer and the browser's worklet. See section 8.
9. **Audio thread rules** (internal synth). The audio callback must not allocate, wait on locks, do I/O, or panic. Send events through a bounded lock-free queue or channel.
10. **MIDI channel allocation.** `ChannelAllocator` in `gscore-play`, not in the model. Default: one channel per track; drums on channel 10 (index 9, General MIDI). Notes needing independent pitch bend or slide borrow a free channel temporarily (copy program, volume, pan; reset bend afterward; release). If channels run out, degrade gracefully instead of failing. Verify the internal synth's channel support (expected 16) before designing around more.

### Portability (the browser build)
A browser (WebAssembly) build exists since 2026-11-04 (`docs/web.md`). These rules keep it working:
- **The core crates must compile for `wasm32-unknown-unknown`:** `gscore-gp`, `gscore-model`, `gscore-io`, `gscore-layout`, `gscore-play`, and `gscore-edit`; so must `gscore-audio` and `gscore-worklet` (the browser's synthesizer). `cargo xtask layers` runs `cargo check --target wasm32-unknown-unknown` on them.
- In those crates, **no direct use of `std::fs`, `std::thread`, `std::time::Instant`, or `std::time::SystemTime`** (the time types panic on that target as far as we know; verify). Pass such capabilities in through small traits (for example `Clock`, `Storage`, `BackgroundRunner`) implemented in `gscore-session` or `gscore-app`.
- **`gscore-io` works on bytes, not paths:** bytes in, `Song` out, and `Song` in, bytes out. File system work (safe-save, backups, recovery journal files) belongs to `gscore-session`.
- **Do not rely on `catch_unwind` in core logic.** On `wasm32-unknown-unknown` a panic aborts the instance by default, so the never-panic lints and rules in section 2 are the primary defense; containment is only a native safety net.
- The browser build opens with a file picker and saves with a download, keeps settings in browser storage, plays through the internal synth in an AudioWorklet (no MIDI output), and has a smaller feature set than the desktop app (`docs/gaps.md`).

### Notation fonts (SMuFL) and visual quality
- Use **SMuFL-compliant fonts** (for example Bravura, or Leland as used by MuseScore). Check each font's license (typically SIL OFL) before bundling.
- Refer to glyphs **by SMuFL name** (via the SMuFL `glyphnames.json`), not by raw code point. Draw ops carry the name; the renderer resolves it.
- Use the font's **metadata JSON** (bounding boxes, anchors, engraving defaults such as staff line and stem thickness) in layout instead of magic numbers.
- **Fret numbers:** MuseScore draws them by default with an ordinary text style ("Tablature fret number") and offers bundled tab-specific fonts as an alternative. Follow the same approach: a normal OFL text font by default, configurable later. Use SMuFL tab glyphs (such as tab clefs) where they exist. Verify against the SMuFL specification.
- **MuseScore as the visual benchmark.** Its tablature and scores look great, and GuitarScore should reach comparable polish: spacing, line thickness, fret number placement over staff lines, beam and stem proportions, margins. The owner exports the same song from MuseScore; layout work is compared visually against it. Do not copy MuseScore's code (GPL).
- Font loading uses the GUI's custom-font mechanism and stays inside `gscore-app`.

### Saving
- Primary save format: **GP7+ (`.gp`) first**. Add `.gp5`, `.gp4`, `gp3` and `.gpx` writing once round-trip tests pass for it. Re-check the crate's current status before deciding.
- **Preserve unknown data.** Fields we do not edit (for example sound-engine data) must survive a load-save cycle unchanged where the crate allows it. Where it cannot, report exactly what would be lost.
- **Safe save:** write to a temporary file and rename atomically; keep a `.bak` of the original the first time a file is overwritten; after writing, **re-read the saved file and compare it with the in-memory song**, and tell the user if anything differs.
- Saving into an older format than the one the file was opened in can lose features. Warn before doing so, and default to "Save As" for files opened from a different format.
- No custom score format unless a concrete need appears; if it does, write a short design note first and get the owner's approval.

## 6. Testing and definition of done

A task is done only when `cargo xtask ci` passes. It runs:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask layers        # dependency graph, lints, std usage, wasm checks
```

Also, when relevant:
- commands added or changed: `cargo test -p gscore-edit --test panic_hunt` (part of the
  workspace tests, worth running on its own while working)
- importers, exporters, `gscore-gp`, or `gscore-model` touched: check the owner's files in
  `corpus/` by hand (`gscore-cli info`, `render`, `midi`); automated corpus tests
  (`cargo xtask corpus`) are not built yet
- hot paths touched: `cargo xtask bench` and record the numbers in the devlog
- the browser build touched: `cargo xtask web --smoke`

Required kinds of tests (in addition to the stability tests in section 2):
- **Model:** unit tests for invariants (beat durations fill measures, valid fret and string ranges).
- **Commands:** apply then undo must equal the original song. Write one shared helper and use it in every command's tests.
- **Import:** parse each file in `testdata/` and compare against expected summaries (track count, measure count, note counts, tempo). The owner compares against Guitar Pro manually and records the expected values.
- **Round trip:** load, save to GP, load again; the musical content must be equal, and unsupported features must be reported, not silently dropped.
- **Layout:** render `DrawOp`s to **SVG** and compare against golden files in `crates/gscore-layout/tests/golden/` (`GSCORE_UPDATE_GOLDEN=1` rewrites them). Update goldens only with the owner's approval of the visual change.
- **App:** snapshot tests with `egui_kittest` in `crates/gscore-app/tests/snapshots/` (`UPDATE_SNAPSHOTS=1` rewrites them; `kittest.toml` sets the tolerance); look at a changed snapshot before accepting it.
- **Playback:** convert a small song to MIDI events and assert timings and notes; the `.mid` files Guitar Pro exported in `testdata/` are the reference.

## 7. Workflow spec

This is the heart of the project. List the actions you do constantly in Guitar Pro. Each becomes a named command in `gscore-edit`. **Do not guess keys.** Leave the key column empty until the owner fills it in.

| Command id | What it does | Default key | Notes |
|---|---|---|---|
| `cursor_left` / `cursor_right` | move cursor to previous/next beat | Arrow Left / Arrow Right | |
| `cursor_up_string` / `cursor_down_string` | move cursor between strings | Arrow Up / Arrow Down | |
| `enter_fret` | type a fret number on the current string  | [0-9] | For multi-digit numbers there is a small time window ~0.5s in which a second digit can be provided |
| `set_duration` | set note/beat duration | + / - | Plus makes the note shorter, minus makes it longer |
| `insert_beat` / `delete_beat` | add/remove a beat | insert / delete | Using `cursor_right` also adds a bit if the last note in the bar is reached and the bar is still not complete |
| `insert_measure` / `delete_measure` | add/remove a bar | ctrl+insert / ctrl+delete | Also works over multiple bar selections |
| `copy` / `cut` / `paste` | on selection (beats or bars) | ctrl+c, ctrl+x, ctrl+v | |
| `undo` / `redo` | there history should not be limited and ideally persisted | ctrl+z / ctrl+y | |
| `play_pause` / `stop` | Starts / stops the playback | space | |
| `clear_note` | remove the note on the cursor's string | | unbound |
| `previous_measure` / `next_measure` | cursor to the first beat of the previous/next bar | | unbound |
| `save` | save the file | ctrl+s | |
| `edit_beat_text` | text written above the current beat | t | the dialog's text field takes the keyboard at once (Guitar Pro's shortcut) |

## 8. Sound plan

Each stage plugs in behind the `Player` trait:

1. **MIDI output** (`midir`) and `.mid` export. Done (M7).
2. **Internal synth:** `cpal` + `rustysynth` (SoundFont) for sound without external setup;
   also in the browser as an AudioWorklet. Done (M9, browser 2026-11-04). It stays for
   drums, other instruments and as the fallback.
3. ~~Sampled libraries (SFZ) or plugin hosting~~: set aside (owner, 2026-11-03) in favour
   of stage 4.
4. **Physically modelled guitars and basses:** a pure-Rust string model (digital
   waveguide) that plays every technique, with pickup, amp, cabinet and body stages.
   Design note: `docs/guitar-modelling.md`; milestone M22, the current one.

## 9. Milestones (one at a time; stop and summarize after each)

Headless editing and saving came before the UI (owner decision, 2026-11-01): features are
built and tested through the command layer and the CLI first, and the UI drives the same
commands. Details of every step are in `docs/devlog.md`; known shortfalls in
`docs/gaps.md`; feature status against Guitar Pro in `docs/parity.md`.

### Done

- **M0** (2026-11-01): workspace, toolchain pin, workspace lints, `xtask layers` with the
  wasm check, CI.
- **M1** (2026-11-01): `gscore-model` wraps the Guitar Pro song model.
- **M2** (2026-11-01): import of GP3-5, GPX and GP7 through `gscore-io` (with
  `catch_unwind`); `gscore-cli info`, `dump`, `roundtrip-check`; the `guitarpro` crate
  vendored and integrated as `gscore-gp`.
- **M3** (2026-11-01): safe save as GP7 (atomic, `.bak`, read back and compare).
- **M4** (2026-11-01): commands with undo in `gscore-edit`, `panic_hunt`,
  `gscore-cli commands` and `edit`.
- **M5, M6** (2026-11-01): headless layout (tab and standard notation, SMuFL, SVG goldens)
  and the egui app with light and dark themes and the snapshot example.
- **M7** (2026-11-01): MIDI output and `.mid` export; bends and slides on borrowed channels.
- **M8** (2026-11-01): keyboard editing in the app through the keymap.
- **M9** (2026-11-02): internal SoundFont synthesizer.
- **M10** (2026-11-02): performance budgets (16 ms per edit, 2 s to open) and
  `cargo xtask bench`.
- **M11** (2026-11-02): recovery journal, crash recovery, autosave.
- **M12-M16** (2026-11-02 to 11-03): app shell and settings, track and song grid with the
  mixer, edit palette with selection and every effect, track panel, app design.
- **M17** (2026-11-03 to 11-05): layout polish: bends and whammy, repeats and endings,
  dynamics, grace notes, voices, drum notation, chord diagrams, clefs, free time, bar
  repeats, octave signs, slash notation.
- **M21** (2026-11-03 to 11-05): pages and page setup, breaks, multi-track systems with
  editing across staves, five score views, PDF export.
- Pulled forward from M18-M20 and done: play from a selection and loop it, count-in (1-4
  bars, also before each loop pass) and metronome with its own click, every technique
  played (vibrato, let ring, palm mute, grace notes, trills, whammy, swing, strokes),
  export to MIDI and GP7/GP6/GP5/GP4/GP3, the browser build, release packages (Linux
  tarball and AppImage, Windows, macOS) with a bundled SoundFont.
- **Release v0.1.0** (2026-11-06).

### Current

- **M22: Modelled guitar sound** (owner, 2026-11-03, made urgent 2026-11-06: the band's
  songs sound bad on General MIDI; without playback comparable to Guitar Pro's RSE nobody
  will compose with the program. Reference for "good enough": AAS Strum, which is fully
  modelled). Design note: `docs/guitar-modelling.md`. Order decided by the owner
  (2026-11-06): **the playback sequence first**, then the engine, with **all sounds at
  full quality** (distorted and clean electric, bass, acoustic), none left rough.
  - Step 0, the playback sequence: per-note events with track, string, fret, pitch,
    start, end, and every technique as it changes over time (bend and whammy curves,
    slides, vibrato, damping, hammer-on and pull-off without a new pluck, harmonics,
    strokes). The MIDI and SoundFont path is derived from it, so both engines play the
    same notes. Takes over M18's "one MIDI channel per string".
  - The string engine behind the `Player` trait: digital waveguide per string with exact
    tuning, loss filter and dispersion; every technique played by the model. Bass on the
    same engine.
  - Electric guitar: pickup position, amp (clean to high gain), cabinet. Acoustic guitar:
    commuted-synthesis body, string coupling, two vibration directions, pick attack.
    Then noises and strum feel.
  - Per track: engine (SoundFont or modelled) and model, as commands with undo; drums
    and other instruments stay on the SoundFont synth.
  - Pure Rust, wasm-clean, `render` never allocates or blocks; impulse responses (if any)
    with licenses in `THIRD_PARTY.md`. Faust only for prototypes, its library licenses
    checked before any generated code is committed.
  - Tests: deterministic rendering, pitch within a few cents, decay and bend targets, no
    NaN, clipping or allocation in `render`, `panic_hunt` over parameters; a real-time
    budget for a 16-track song in `cargo xtask bench`.
  - **Done when:** the owner judges electric, acoustic and bass playback comparable to
    RSE for composing, on reference passages covering every technique.

### Still open from earlier milestones

- **M18 (playback):** a speed slider, export to audio (WAV), fermatas and
  D.C./D.S./coda in playback; previous/next bar during a count-in should count in again
  instead of stopping. *Done 2026-11-06:* speed 50-120 % and a speed trainer, WAV export
  (desktop and `gscore-cli wav`), fermatas, D.C./D.S./coda/fine, count-in navigation.
- **M19 (SoundFonts):** a quality check by ear of the bundled FluidR3 GM (desktop) and
  GeneralUser GS (browser); a smaller default for the source build.
- **M20 (release):** signing and notarizing (Windows, macOS), the macOS file-dialog fix
  (`docs/gaps.md`), the name check (section 11), a user guide, a public test-file set,
  the real app icon (placeholder for now).
- **M21 backlog** (owner, 2026-11-05): native printing, a bracket or brace, rotated names
  on later pages, the bars-per-line limit and the Settings preview for several tracks,
  remembering the shown tracks, moving multitrack breaks when bars are inserted or
  deleted.

### Backlog

- Pitched tracks without strings (voice, synth, piano, strings): notes keep their own
  pitch (GP7's `Midi`/`ConcertPitch`), standard notation only, no "?" for leftover
  negative frets (`Ballad.gp`).
- Second staves (a piano's or synth's bass staff); lyrics (read, shown, edited).
- Beat text colliding with high notes in the notation.
- Automated corpus tests (`cargo xtask corpus`), fuzzing with `cargo-fuzz`, a JSON control channel, a fretboard view, MusicXML import
  and export.

## 10. Dependency rules

- Ask before adding any dependency. State what it does, its license, its maintenance status, and how it handles untrusted input and failure.
- Allowed licenses by default: MIT, Apache-2.0, BSD, ISC. **Flag GPL, LGPL, and anything else** before use.
- Avoid C/C++ dependencies unless the owner agrees. System audio libraries needed by `cpal` (for example ALSA development headers on Linux) are acceptable; document them in the README.

## 11. Open questions and decisions

Open:
- Name availability for GuitarScore (GitHub, crates.io, domain) before wider publicity.

Decided:
- The repository license is MIT.
- Bravura (with its metadata) as the music font, Noto Sans as the text font, both SIL OFL
  1.1 in `assets/fonts/` (2026-11-01).
- Performance budgets: from a key press to the new layout within 16 ms, opening a file
  within 2 s, in release builds; `cargo xtask bench` checks them (M10, 2026-11-02).
- PDF writing with `pdf-writer` + `subsetter` (MIT/Apache-2.0, pure Rust); printing
  happens from a PDF viewer, native printing later (2026-11-03).
- A browser build exists (2026-11-04); the portability rules in section 5 keep it working.
