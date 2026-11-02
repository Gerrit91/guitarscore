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
- **Everything automatable is a command or CLI subcommand.** `gscore-cli` provides headless `info`, `dump` (ASCII tab), `render` (SVG/PNG), `convert`, and `roundtrip-check`, so agents and tests can exercise the whole pipeline without the GUI. A JSON control channel for driving the running app may follow later; design the command layer so it is possible.
- **Project tasks live in `xtask`** (`cargo xtask layers | corpus | ci | ...`), not in ad-hoc shell scripts.
- **Parallel agents:** use separate target dirs (`CARGO_TARGET_DIR=target/agent-<name>`), put new commands in a new module, keep every `Cargo.toml` valid at all times (write manifests atomically: temp file, then `mv`), make small surgical edits to shared files, and re-read before editing them.
- Ask the owner before irreversible or architectural decisions; write a short design note for anything that affects several crates.

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
  rust-toolchain.toml        # pin a stable toolchain
  clippy.toml
  CLAUDE.md
  THIRD_PARTY.md
  docs/                      # architecture.md, development.md, parity.md, gaps.md, devlog.md
  testdata/                  # owner's own test tabs; testdata/corrupt/ for broken files
  corpus/                    # gitignored: owner's larger private collection of GP files (opt-in tests)
  assets/fonts/              # SMuFL fonts + metadata, text font
  xtask/                     # project tasks: layers, corpus, ci
  crates/
    gscore-gp/                # L0: Guitar Pro song data + reading/writing GP3-5, GPX, GP7+ (derived from guitarpro, MIT)
    gscore-model/             # L1: document model (wraps gscore-gp's song), positions, queries, invariants
    gscore-io/                # L2: GP files <-> model: format detection, limits, catch_unwind (bytes in, bytes out; no file system)
    gscore-layout/            # L2: model -> positioned draw ops. No GUI imports.
    gscore-play/              # L2: sequencer, MIDI events, channel allocator, Player trait. No device crates.
    gscore-edit/              # L3: cursor, selection, commands, undo/redo, keymap
    gscore-audio/             # L3: output backends (midir, cpal, rustysynth) implementing Player
    gscore-session/           # L4: open document, recovery journal, autosave, safe-save orchestration
    gscore-cli/               # L5: headless tool (info, dump, render, convert, roundtrip-check)
    gscore-app/               # L5: egui/eframe application shell (thin); produces the `guitarscore` binary
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
8. **Playback behind a trait.** `gscore-play` defines the `Player` trait (load song, play, pause, seek, stop, tempo scale); `gscore-audio` implements it. See section 8 for backend order.
9. **Audio thread rules** (internal synth). The audio callback must not allocate, wait on locks, do I/O, or panic. Send events through a bounded lock-free queue or channel.
10. **MIDI channel allocation.** `ChannelAllocator` in `gscore-play`, not in the model. Default: one channel per track; drums on channel 10 (index 9, General MIDI). Notes needing independent pitch bend or slide borrow a free channel temporarily (copy program, volume, pan; reset bend afterward; release). If channels run out, degrade gracefully instead of failing. Verify the internal synth's channel support (expected 16) before designing around more.

### Portability (groundwork for a later web build)
A browser (WebAssembly) build is not a current goal, but the groundwork is cheap now and painful to retrofit later:
- **The core crates must compile for `wasm32-unknown-unknown`:** `gscore-gp`, `gscore-model`, `gscore-io`, `gscore-layout`, `gscore-play`, and `gscore-edit`. `cargo xtask layers` also runs `cargo check --target wasm32-unknown-unknown` on them.
- In those crates, **no direct use of `std::fs`, `std::thread`, `std::time::Instant`, or `std::time::SystemTime`** (the time types panic on that target as far as we know; verify). Pass such capabilities in through small traits (for example `Clock`, `Storage`, `BackgroundRunner`) implemented in `gscore-session` or `gscore-app`.
- **`gscore-io` works on bytes, not paths:** bytes in, `Song` out, and `Song` in, bytes out. File system work (safe-save, backups, recovery journal files) belongs to `gscore-session`.
- **Do not rely on `catch_unwind` in core logic.** On `wasm32-unknown-unknown` a panic aborts the instance by default, so the never-panic lints and rules in section 2 are the primary defense; containment is only a native safety net.
- If a web build happens later, expect: a file picker for open and a download for save, browser storage for the recovery journal and settings, the internal synth (not MIDI output) for sound, and a smaller feature set than the desktop app.

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

A task is done only when all of these pass:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask layers
```

Also, when relevant:
- commands added or changed: `cargo test -p gscore-edit --test panic_hunt`
- importers, exporters, `gscore-gp`, or `gscore-model` touched: `cargo test --workspace --features corpus` (opt-in tests over `corpus/` and `testdata/`; plain `cargo test` skips them, and with the feature on a missing corpus fails with a clear message)
- hot paths touched: run the benchmarks and record numbers in the devlog

Required kinds of tests (in addition to the stability tests in section 2):
- **Model:** unit tests for invariants (beat durations fill measures, valid fret and string ranges).
- **Commands:** apply then undo must equal the original song. Write one shared helper and use it in every command's tests.
- **Import:** parse each file in `testdata/` and compare against expected summaries (track count, measure count, note counts, tempo). The owner compares against Guitar Pro manually and records the expected values.
- **Round trip:** load, save to GP, load again; the musical content must be equal, and unsupported features must be reported, not silently dropped.
- **Layout:** render `DrawOp`s to **SVG** and compare against golden files in `crates/gscore-layout/tests/golden/`. Update goldens only with the owner's approval of the visual change.
- **Playback:** convert a small song to MIDI events and assert timings and notes. Support exporting a `.mid` file so results can be checked in any other player.

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
| `clear_note` | remove the note on the cursor's string | | added in M8, unbound |
| `previous_measure` / `next_measure` | cursor to the first beat of the previous/next bar | | added in M8, unbound |
| `save` | save the file | ctrl+s (proposed) | added in M8; the key is a proposal |

## 8. Sound plan

Strictly in this order; each stage plugs in behind the `Player` trait:

1. **MIDI output** (`midir`): send events to any MIDI port, so the user can route to an external synth (for example FluidSynth). Plus `.mid` export. This is the first playback milestone.
2. **Internal synth:** `cpal` for audio output plus `rustysynth` (pure-Rust SoundFont synthesizer) for sound without external setup. Verify the current state of both crates before starting.
3. **Real sound libraries** (only if stages 1 and 2 go well): SFZ-based sampled instruments with technique-to-articulation mapping, possibly plugin hosting (CLAP, LV2) in a separate process, which also isolates plugin crashes. Write a design note first.

## 9. Milestones (one at a time; stop and summarize after each)

Headless editing and saving come before the UI (owner decision, 2026-11-01): the core
functionality is built and tested through the CLI first, and the UI later drives the same
command layer.

- **M0:** workspace, toolchain pin, `[workspace.lints]`, `clippy.toml`, `xtask layers` (including the wasm check on the core crates), the commands in section 6, an empty devlog.
- **M1:** `gscore-model` wrapper with tests.
- **M2:** `gscore-io` import via `gscore-gp` (with `catch_unwind` containment), `gscore-cli` `info`, `dump` (ASCII tab) and `roundtrip-check`, corrupt-file and damaged-input tests. (The decision gate on using the crate's model directly and on vendoring it was settled early; see section 5, decision 1. Fuzzing with `cargo-fuzz` is deferred to "Later".)
- **M3:** save to GP with safe-save (temporary file and atomic rename, `.bak`, re-read and compare) and round-trip verification, warnings before saving to an older format, `gscore-cli convert`.
- **M4:** headless editing: checked editing methods on the model, the command registry in `gscore-edit` (id, label, `enabled`, parameters, `apply`, `undo`; transactional) with the shared apply-then-undo test helper, `panic_hunt` and property tests, and the CLI on top of it: `gscore-cli commands` (lists command ids and parameters) and `gscore-cli edit <in> -o <out>` with `-c "<command> key=value ..."` or `--script <file>` (positions 1-based). Script files double as regression tests.
- **M5:** `gscore-layout` headless layout for one track's tab staff, SVG golden tests, SMuFL font loading and metadata, comparison against MuseScore output.
- **M6:** `gscore-app` read-only egui view drawing the `DrawOp`s, with light and dark theme, plus the offscreen snapshot example.
- **M7:** MIDI output playback and `.mid` export (moved before interactive editing by the owner, 2026-11-01: sound first, for a first usable version).
- **M8:** interactive editing in the app: cursor, keyboard entry and undo/redo through the M4 commands, driven by the keymap file (tested on QWERTY, QWERTZ, and AZERTY layouts), **and `gscore-session` with the recovery journal and crash recovery**.
- **M9:** internal synth playback (`rustysynth` + `cpal`, SoundFont found or chosen; done 2026-11-02).

Planned (owner's list of 2026-11-02, extended). Order: speed and safety first, because every
UI feature below triggers edits on the whole song; then the window's structure, then the
panels that fill it.

- **M10: Performance.** Long songs must stay instant to edit. (Done 2026-11-02: see the
  devlog; `cargo xtask bench` checks the budgets.)
  - Per-region edits: copy and validate only the touched measures instead of the whole song
    (now about 3.6 ms per edit on `DrDr.gp`).
  - Layout cache per measure (and per system): an edit lays out only what changed and
    re-breaks lines from there (now about 15 ms for 236 measures).
  - Performance budgets (section 11 open question): key-to-screen under one frame (16 ms)
    on a 500-measure, 16-track song in release; opening such a file under 2 s.
  - A benchmark command (`cargo xtask bench`) on the owner's largest files, run before and
    after each milestone; results in the devlog.
  - Undo history capped by memory, not only by entry count.
- **M11: Recovery.** The recovery journal and autosave deferred from M8 (contribution rule
  5: recovery before new features): journal of commands, recovery offer after a crash,
  autosave, persisted undo history.
- **M12: App shell and styling.** The frame the panels plug into.
  - Docked panels (bottom, left, right), each shown or hidden from the View menu and a
    context menu, sizes and visibility remembered between runs.
  - Progress bar while a song opens (and while a SoundFont loads), cancellable.
  - Settings window with tabs:
    - Song: title, artist, album, author, copyright, notes, tempo.
    - Page layout: music and text fonts, font sizes, staff spacing, page size and margins.
    - Audio and MIDI (I/O): output (synthesizer, MIDI port, audio device), SoundFont,
      later inputs (MIDI keyboard or guitar input for note entry).
    - Editor: keymap file, theme, autosave interval.
  - Settings stored in `~/.config/guitarscore/` (with `gscore-session`), like the keymap.
  - General styling: a consistent look (spacing, typography, icons, colors) in light and
    dark; icons bundled with their licence (`THIRD_PARTY.md`); toolbar tooltips with keys.
  - Recent files, drag and drop to open, window size and position remembered, zoom keys.
  - Playback and the cursor (important to the owner): select a note and start playback
    from it (done early, 2026-11-02: Play starts at the cursor's beat, at its measure's
    first pass; still to do: from the first note of a selection). During playback the cursor advances beat by beat to the note being
    played (across repeats and track switches), and the view scrolls with it; pausing or
    stopping leaves the cursor on the last played note, so play continues from there.
- **M13: Track and song grid (bottom panel).**
  - One row per track: track color, instrument symbol, name.
  - Right-click on a track: add, delete, duplicate, rename, move up/down, edit (opens the
    track panel). All of these as `gscore-edit` commands with undo.
  - Volume slider and pan knob per track, plus mute and solo, applied to playback live.
  - A colored matrix: one column per measure, a filled square where the track has notes;
    clicking a square moves the cursor to that track and measure.
  - Measure numbers, section markers and repeats above the matrix; the playing measure
    highlighted during playback; selecting a range of measures for copy/paste and looping.
- **M14: Edit palette (left panel).**
  - Note durations (whole to 64th), dots, triplets and other tuplets, rests, ties; the
    chosen duration stays selected for the next notes.
  - A matrix of effects and ornaments (bends, slides, hammer-on/pull-off, vibrato, palm
    mute, let ring, harmonics, dead and ghost notes, accents, staccato, tapping, whammy,
    grace notes, trills, tremolo picking, strokes); a click applies it to the selected
    note(s) or beat(s) and shows its state there.
  - Needs the model's effect setters and `gscore-edit` commands for every effect (each
    with apply-then-undo tests), plus bend and whammy editors (curve with points).
  - Tempo, time signature and key signature changes at the cursor; section markers.
  - Selection (beats, measures, across tracks) with copy, cut and paste; voice 2 entry.
- **M15: Track panel (right panel).** Track name, instrument (General MIDI program, drum
  kit), color, tuning and capo, string count, and sound (SoundFont preset); changes are
  commands with undo and are heard at once.
- **M16: Bundled SoundFonts.** Ship ready-to-use General MIDI SoundFonts so nobody has to
  install a synthesizer.
  - Choose by licence (MIT, CC0 or similar; flag anything else) and size (a small default
    in the repo or the release, a larger high-quality one as an optional download).
  - Licence file next to each and a row in `THIRD_PARTY.md`; `.sf2` only unless `.sf3`
    support is added.
  - The app picks the bundled one when none is installed; a quality check by ear for
    guitars, basses and drums.
- **M17: Layout polish.** Remaining layout issues and effects.
  - Whammy bar (dips, dives, curves with values).
  - Bends drawn properly: bend-and-release, pre-bend, curves with all points and values
    (needs the full bend shape in the model, docs/gaps.md #4).
  - Repeats and alternative endings, tempo changes, section markers, key signatures, ties
    across barlines, the second voice, grace notes, rhythm under the tab, chord diagrams,
    lyrics.
  - Pages (page breaks, headers, page numbers), multi-track systems, drum notation.
  - Print and export to PDF and PNG.
- **M18: Playback polish.**
  - Play from a selection and loop it, count-in, metronome, speed slider.
  - Remaining techniques: vibrato, let ring, palm mute, grace notes, trills, whammy,
    D.C./D.S./coda, swing.
  - Export the song as audio (WAV) with the internal synthesizer.
- **M19: Release.** Packages for Linux (AppImage or Flatpak), Windows and macOS (with the
  macOS file-dialog fix, docs/gaps.md #10), the name check from section 11, a user guide,
  a public test-file set.
- **Later:** fuzzing with `cargo-fuzz`, GP3/4/5 writing, JSON control channel, sound
  libraries (section 8, stage 3), a fretboard view, MusicXML import and export, a web build.

## 10. Dependency rules

- Ask before adding any dependency. State what it does, its license, its maintenance status, and how it handles untrusted input and failure.
- Allowed licenses by default: MIT, Apache-2.0, BSD, ISC. **Flag GPL, LGPL, and anything else** before use.
- Avoid C/C++ dependencies unless the owner agrees. System audio libraries needed by `cpal` (for example ALSA development headers on Linux) are acceptable; document them in the README.

## 11. Open questions

- The repository license is MIT
- ~~Which SMuFL font(s) to bundle first, and which text font for fret numbers.~~ **Decided (2026-11-01):** Bravura (with its metadata) as the music font, Noto Sans as the text font, both SIL OFL 1.1 in `assets/fonts/`.
- When (and whether) to ship a browser build. The groundwork rules in section 5 ("Portability") apply from the start either way; the web build itself is a later goal.
- ~~Performance budgets (latency, large-score layout time).~~ **Decided (M10, 2026-11-02):**
  from a key press to the new layout within 16 ms, opening a file within 2 s, in release
  builds; `cargo xtask bench` checks them on the owner's files.
- Name availability checks for GuitarScore (GitHub, crates.io, domain) before the first public release.
