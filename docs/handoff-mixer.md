# Handoff: the mixer (M22), from step 4 on

Branch `m22-mixer` (off `m22-string-engine`; neither merged into `main` yet). Read
`docs/devlog.md` from "Mixer, step 1" on, `docs/architecture.md` (Playback) and
`vendor/rustysynth/FORK.md` first. The owner merges branches himself; commit on the branch,
never on `main`, never push.

## The owner's design (decided 2026-11-07)

- Every track is a strip: source (SoundFont channels or modelled strings) → chain →
  fader and pan (after the chain; the song's volume/pan automation moves the fader) →
  post-fader sends → master. Buses: the reverb bus and a compressor bus (parallel /
  "New York" compression), both into the master.
- Every track has its own **Comp send** knob next to its Reverb send.
- **Chorus** is a chain block (cheap, mounted per instrument), not a bus. The SoundFont's
  own chorus (CC 93) is gone since step 2 until that block exists.
- The compressor: "CA-76", like the 1176 FET compressors. As a chain block and as the
  compressor bus.
- Every drum track has its own channel and strip (done).
- Later: a track can be played by a hardware synthesizer (MIDI out per track, "Played by:
  MIDI out" with port and channel; MIDI held back by the audio output latency; strip shows
  MIDI activity instead of a meter).
- Meters: hold peaks briefly, fall slowly (done; owner happy so far).

## Done (commits on `m22-mixer`)

1. `ceb7b8c` RustySynth fork in `vendor/rustysynth` (each channel rendered apart, no
   `unsafe`).
2. `76b3a1b` Engine mixer: `gscore-audio::mixer` (strips, chains for SoundFont tracks too,
   CC 7/10/91 to strips, synth held at full volume and centred), `meters`.
3. `60562ba` Real meters in the mixer window (bar holds 0.3 s, falls 20 dB/1.7 s; peak
   mark 1.5 s), estimates for MIDI out and the browser.
4. `0667d13` Every drum track its own channel (`ChannelAllocator`, fork
   `set_percussion_channel`, `Sequence::general_midi` for MIDI out and MIDI files);
   browser wire format `GSQ4` carries the tracks (the browser mixer had no strips before).
5. The commit with this file (step 4, engine half):
   - `gscore-guitar::compressor`: the CA-76 DSP (`CompressorKnobs`: input_db, output_db,
     attack_ms 0.02-0.8, release_ms 50-1100, `CompRatio` 4/8/12/20/All, mix; program
     dependent release; FET color; stereo linked), `BlockKind::Compressor`,
     `BlockSettings::compressor`, `knob_ranges::{COMP_GAIN, ATTACK, RELEASE}`,
     `Chain::take_gain_reduction` (deepest reduction of its compressors since last read).
     Tests in the module and in `test_no_alloc`.
   - `gscore-audio`: buses generalized: `pub enum Bus { Reverb, Compressor }`,
     `Mixer::{set_bus, set_bus_settings, set_comp_send}`, commands `Bus`, `BusSettings`,
     `CompSend`; `SynthEngine::{set_bus, set_bus_settings, set_comp_send}` and
     `SynthPlayer::{set_bus, set_bus_settings, set_comp_send}` (the `set_reverb_bus*`
     wrappers stay). The compressor bus is stereo (post-fader sends), the reverb bus hears a
     mono sum as before. Meters: `Meters::{compressor, reduction(track), bus_reduction}`
     (gain-reduction needles, jump to the deepest, return with 0.3 s time constant).
   - The app is untouched except `chain_settings.rs` passing `CompressorKnobs::default()`.

## To do

### Step 4, the rest

1. **Tests for the engine half** (none yet for the bus): in
   `crates/gscore-audio/tests/test_mixer.rs`, a compressor bus fed by `set_comp_send`
   (louder tracks → `bus_reduction() < 0`, its meter moves, level 0 / mute silences it, no
   send → silent bus), and a compressor in a track's chain moves `meters.reduction(track)`.
   Also a test in `gscore-guitar/tests/` that a compressor in a chain reduces a chord's
   crest factor.
2. **WAV export** still only knows the reverb bus: `gscore-audio::wav::write_wav_with` takes
   `bus: Option<&ReverbBusChoice>`. Add the compressor bus and the comp sends (a struct for
   the buses would keep the argument list sane); `gscore-app/src/tasks.rs` `AudioExport`
   and `app/files.rs` `start_export`.
3. **App, the block:** `chain_settings.rs` `BlockChoice::Compressor` (label "Compressor",
   box title "Comp", detail the ratio), `BlockSetting` fields (input_db, output_db,
   attack_ms, release_ms, ratio as a serde enum like `AmpVoiceSetting`, mix; reuse
   `gain_db`/`mix` where it fits, old settings must still read: serde defaults),
   `knobs()` (replace the placeholder), `BlockSetting::of`. `playback/chains.rs`
   `chain_spec`: `BlockChoice::Compressor → BlockKind::Compressor`; `reverb_bus_parts`
   match. `app/sound_chain.rs`: `block_choices` (add it), `box_text`, `block_knobs` →
   a `compressor_knobs` in `app/amp_knobs.rs` style: ratio buttons in a column (4, 8, 12,
   20, All, selectable labels like the amp's voices), round knobs Input, Output, Attack
   (ms), Release (ms), Mix, and a **gain-reduction meter** (a horizontal bar or needle,
   0 to -20 dB) read from `playback.meters()` → `meters.reduction(track)`; the window must
   request a repaint while it moves (see `app/grid/mod.rs`, `meters.moving()`).
   Snapshot in `tests/test_app.rs` `the_track_dialogs_match_their_snapshots` (like
   `sound_chain_drive`).
4. **App, the compressor bus:** settings like `ReverbBus` in `settings.rs` (a
   `CompressorBus { level, muted, block }`, default a CA-76 at ratio All or 8, input
   driven hard, output made up, mix 1), playback like `reverb_bus` in
   `playback/chains.rs` (`send_reverb_bus`, `set_reverb_bus`, `reverb_bus_parts`,
   `bus_sent`/`bus_structure`; generalize rather than copy), a strip in the mixer
   (`app/grid/strips.rs` `reverb_strip`: meter from `meters.compressor()`, fader, mute,
   a window like `app/reverb_bus_window.rs` with the compressor's knobs and its
   gain-reduction from `meters.bus_reduction()`). The mixer strips' readings:
   `strips::Readings` gets a `compressor` reading.

### Step 5

1. **Comp send knob** per track in the mixer strip (next to Reverb; `strips::knobs`).
   **Open question for the owner: where is a track's comp send stored?** Guitar Pro has no
   field for it. Options: in the open tab by track index (like `Tab::track_engines`, lost
   on close), or in our own entry in the GP7 archive (planned for tones, see
   `docs/guitar-modelling.md` "Tones"). Ask before building it. Engine side is ready:
   `SynthPlayer::set_comp_send(song_track, value)`; it must be sent again after a synth
   restart (`Playback::connect`) like the tones.
2. **Chorus block** in `gscore-guitar` (stereo, cheap: two or three modulated delays,
   rate, depth, mix, maybe a width), `BlockKind::Chorus`, app block like the others. The
   GM Chorus knob (CC 93) does nothing meanwhile; decide with the owner whether it should
   drive the chorus block's mix when a track has one.

### Later

- MIDI out per track (see the design above). `MidiOutPlayer` already maps drum tracks to
  channel 10 (`Sequence::general_midi`).
- The strings benchmark (`cargo xtask bench`) is about 280 ms against its 250 ms budget on
  a busy machine since the pickup; see the devlog ("The electric guitar gets a pickup").

## Gotchas

- The engine renders in blocks of 64 (`synth::BLOCK`); nothing in `mixer`, `guitars`,
  `meters` or a chain may allocate or lock (there is a no-allocation test for chains;
  keep new blocks in it).
- Tones (chains) are per General MIDI preset, kept in the settings (`preset_tones`); a
  SoundFont track gets a chain only if its preset has its own tone (step 2 decision).
- The engine keys chains, meters and comp sends by the **song's** track index
  (`PerformanceTrack::track`), not the sequence position: a beat excerpt keeps it.
- Filters must flush denormals (`gscore-guitar::filters::flushed`): a long reverb after a
  drive or amp once took half a core per track.
- The owner runs a debug build of the app and may save `testdata/basic/multi_staff.gp`
  from it (he wants to redo an edit to it); if mixer snapshots change because of that,
  update them (`UPDATE_SNAPSHOTS=1`), do not revert his file.
- `cargo xtask ci` is the definition of done; snapshot tests need a GPU (wgpu).
