# Realistic guitar sound: physical modelling

Design note for M22 (owner, 2026-11-03). Status: the current milestone (2026-11-06); step 0
(the performance) is done, step 1 (the string engine) has a first version to listen to. Owner's decisions (2026-11-06): the playback sequence comes first (step 0), then
the engine; distorted and clean electric, bass and acoustic are all aimed at full quality,
none left rough.

## Why

Guitar Pro's RSE is a big part of why people compose in Guitar Pro: playback sounds close
enough to a real guitar to judge a song by ear. Our General MIDI SoundFont playback
(`gscore-audio`: `SynthEngine` on `rustysynth`) is far from that. Without comparable
sound, few people will use GuitarScore for composing. The owner's reference for "good
enough" is AAS Strum (Applied Acoustics Systems). It is fully physically modelled, not
sampled, and even its acoustic guitar is more than sufficient.

## Options considered

| Option | Quality | Cost | Notes |
|---|---|---|---|
| Better SoundFonts (GeneralUser GS, MuseScore General) | GM level | small | M19; still no techniques |
| Per-string MIDI channels, preset switching | GM level, more expressive | small | M18; the first step in any case |
| SFZ libraries via sfizz | good to very good | medium | C++ (BSD-2) FFI, no wasm, large downloads, few free guitar libraries |
| Plugin hosting (CLAP, VST3) | best | large | needs plugins the user owns; no wasm; section 8 stage 3 |
| **Physical modelling** | very good (Strum shows the ceiling) | medium to large | pure Rust, wasm, no assets, techniques come naturally |

Chosen: **physical modelling** as a second engine next to the SoundFont synth. The
SoundFont synth stays as the fallback and plays drums and all non-guitar instruments.

## What physical modelling is

Physical modelling records nothing. It simulates the vibrating string and computes every
sample while playing. For guitars the method is the **digital waveguide**: the two waves
running along a plucked string become a delay line in a feedback loop.

```
excitation ──► [ delay line ] ──► [ loss filter ] ──► [ dispersion ] ──┬──► output
  (pluck)          ▲                                                     │
                   └─────────────────────────────────────────────────────┘
```

- **Delay line:** a ring buffer whose length sets the pitch (110 Hz at 48 kHz is about
  436 samples).
- **Excitation:** a short burst written into the line. Its shape is pick, finger or thumb.
- **Loss filter:** a lowpass in the loop, so high harmonics fade first. Damping decides
  between a ringing note and a palm mute.
- **Dispersion:** allpass filters that model string stiffness (slightly sharp high
  harmonics; the metallic sound of wound strings).
- **Fractional delay:** exact tuning and smooth pitch changes (bends, slides, vibrato).

The simplest form is the Karplus-Strong algorithm (1983); everything since adds realism
around this loop.

## Mapping our techniques

The score already has every technique; the model plays each one by changing parameters.

| Technique | In the model |
|---|---|
| Bend, slide, vibrato, whammy | change the delay length while the note sounds |
| Hammer-on, pull-off, tapping | change the length without a new pluck (keeps the energy) |
| Palm mute | strong loss filter, short decay |
| Dead note | very strong damping plus a noise burst |
| Natural, artificial harmonic | damp at the node (12th fret: middle of the line) |
| Let ring vs. staccato | damp or not when the next note starts on that string |
| Pick position, pick vs. finger | position and shape of the excitation |
| Re-striking a sounding string | the new pluck adds to the vibrating string |
| Strokes (brush up and down) | strings start a few milliseconds apart, in stroke direction |

## What makes it sound real (the gap to Strum)

A basic string model sounds like "a string", not yet like a guitar. Known parts, all
described in the literature below:

1. **Body:** for acoustic guitars, commuted synthesis. A recorded body impulse response
   is used as the excitation (all parts are linear, so their order can be swapped).
   Does most of the work.
2. **Attack:** the pick's noise and how it leaves the string; the first 50 ms decide.
3. **String coupling:** all strings share the bridge, so unplayed strings resonate along.
4. **Two directions of vibration:** horizontal and vertical with slightly different
   decay, which gives the beating and the two-stage decay of a real note.
5. **Noises:** fret noise, string squeaks, release noise.
6. **Feel:** strum timing, up and down strokes, dynamics, humanizing.
7. **Electric:** pickup position (where the line is read), then amp and cabinet.
   Distortion hides small flaws; distorted electric is the easiest target.

## Architecture

- **Sequence first (done 2026-11-06).** `gscore-play` builds a `Performance`: per track
  the sound, the kind and the sounding pitch of each open string; per note the track,
  string, fret, key, velocity, start, end and beat onset, the attack (picked, legato,
  left-hand tap, tapped, slapped, popped, a pick slide's scrape), the stroke direction,
  palm mute, dead note, harmonic, let ring, a damped end, grace, a fade, and a
  `PitchShape` for bends, slides, vibrato and whammy (`cents_at(offset)` reads it at any
  tick). Hammer-ons, pull-offs, legato and a trill's alternations are notes with a legato
  attack on the same string: the engine changes the string's length without a new pluck.
  Tremolo picking, rasgueados and strokes are already spread into the strikes a player
  makes. The `midi` module derives the `Sequence` from it with the channel borrowing of
  decision 10 (the owner kept it; no channel per string), and the golden events in
  `crates/gscore-play/tests/golden/` prove the MIDI did not change. Every `Sequence`
  carries its `Performance` (also excerpts, count-in, lead-in, and the browser's wire
  form), so a `Player` gets both. Velocities still hold the MIDI path's technique
  shading (legato softer, slaps louder); an engine that models the attack itself may want
  the written velocity back, which would be a new field.
- **Engine.** A new engine behind the `Player` trait (section 8), in `gscore-audio` or a
  new crate (`gscore-model-synth` or similar, decided with the layers check). One voice
  per string per track; a track's voices share a body or amp stage.
- **Real-time rules.** `render` never allocates or blocks (like `SynthEngine::render`):
  delay lines are allocated when a track is set up, sized for the lowest pitch plus the
  deepest bend down. No panics; the `indexing_slicing` and `arithmetic_side_effects` lints
  apply. A broken parameter mutes the voice instead of failing playback.
- **Portability.** Pure Rust, no C/C++, builds for wasm32 (section 5).
- **Assets.** Body and cabinet impulse responses are small (a few KB to a few hundred
  KB). Each needs a license file and a `THIRD_PARTY.md` row; preferably recorded by the
  owner or CC0.
- **Track settings.** A track chooses its engine (SoundFont or modelled) and model
  (steel acoustic, nylon, electric with pickup, bass); stored in the document as commands
  with undo, written to GP files where Guitar Pro has a field for it, otherwise kept in
  our own settings without breaking the file.
- **CPU.** A string voice is cheap (one delay line and a few filters); convolution for
  bodies and cabinets is the main cost. Budget: a 16-track song plays in real time on the
  owner's machine with headroom; checked by `cargo xtask bench`.

## The string engine (step 1, first version 2026-11-06)

- **Crate `gscore-guitar`** (layer 2, no dependencies, wasm-clean). `StringVoice` is a
  single delay loop: a ring buffer read at a fractional delay (third-order Lagrange), a
  one-pole loss filter and up to four stiffness allpasses. The delay is one period minus
  the filters' phase delay at the fundamental, recomputed every block, so every model is
  in tune (tests: within 2 cents from B0 to E6 at 44.1, 48 and 96 kHz). The output is taken
  where the wave leaves the line, so a dead note's single trip is heard.
- **Pluck:** a pulse split at the pick position (zero mean, so its harmonics have the
  pick position's comb), its edges rounded by hardness and velocity, with deterministic
  pick noise. A pick on a sounding string leaves 35 % of the vibration and adds the new
  strike. A legato attack on a sounding string glides the delay to the new pitch in 2 ms
  with a light finger tap; on a silent string it is a soft pluck.
- **Damping** moves two 60 dB decay times, the fundamental's and the harmonics' at
  2 kHz (settling in 4 ms): ringing (the model's sustain, falling with pitch, and its
  brilliance), palm muted 0.18/0.03 s, released 0.3/0.06 s, damped 0.04/0.01 s, dead
  0.015/0.005 s. The loss filter is designed from both (a bisection, redone only when
  they or the pitch change), so the highs fade at the same rate on every string. Before
  (first version) a fixed filter let a low string's highs ring about four times longer
  than a high string's: the owner heard the low notes as "a synthesizer with slight
  distortion" (2026-11-06).
- **The strike** blends a sharp pulse (bright) with the string's triangular shape
  (round) by the model's softness, rounded above 400 Hz to 12 kHz by hardness and
  velocity. A resonant lowpass on each string's output stands in for the pickup or body
  (tone, resonance) until step 2 and 3 bring the real ones.
- **Knobs:** every `StringModel` value has a range (`gscore_guitar::ranges`); the app's
  String sound dialog (track panel › Sound) turns them per kind of string, heard at once
  (`Command::StringModel`), kept in the settings, used by WAV export.
- **Presets** (`GuitarModel`): nylon, steel (the owner's dialed-in sound), electric,
  bass; chosen by default from the track's General MIDI program (24 nylon, 25 steel, 26-31
  electric, 32-39 bass) through `PerformanceTrack::engine`, or per track in the String
  sound window.
- **Two directions of vibration and the bridge** (2026-11-06): each string is two loops,
  the second detuned by a few cents and decaying faster (beating, two-stage decay); the
  output blends the string's swing with the force on the bridge (its change), which
  carries much less of the fundamental, as a bridge or a magnetic pickup does; then a low
  cut. Without them the low notes sounded like a synthesizer bass (owner).
- **In the synth engine** (`gscore-audio::guitars`): 16 tracks x 8 string voices are made
  with the engine. Notes start at the block they are due (as MIDI events do), pitch
  shapes and fades are applied per block, tracks are mixed with their volume and pan. The
  SoundFont gets no note-ons or bends of a modelled track. 96 ringing strings take about
  57 ms per second of audio (budget 250 ms, `cargo xtask bench`).
- **Not yet:** pickup, amp and cabinet (step 2), body, coupling and two polarizations
  (step 3), noises and feel (step 4). Everything is one generic string model now: the
  electric preset has no amp, so distorted parts sound clean. Harmonics play the
  harmonic's pitch with a purer pluck rather than damping at a node.

## The effect chain (2026-11-06)

- **The board** (owner's design): per kind of string, six boxes in a fixed row joined by
  cables, like a pedal board; the order cannot change and boxes do not move. The strings
  come first and cannot be removed (their box opens the String sound dialog); the five
  places after them hold nothing or a block. Opened from the track panel's Sound section.
- **Blocks:** *Cabinet / body*, an impulse response from a WAV file played mono with
  mix, gain, low cut and high cut: what amp modelers call a cab block, and an acoustic
  guitar's body too. *Room*, a room's impulse response (stereo if recorded so): a
  convolution reverb with its length (the decay stretched 0.5× to 4×, below), mix, gain,
  cuts and width. *Reverb*, computed after Freeverb
  (public domain): size, damping, width, mix, and a low and high cut on its sound. Amp blocks come with step 2.
- **Convolution** (`gscore-guitar::convolver`): an own FFT and partitioned overlap-save in
  three stages (partitions of 128, 1024 and 8192 samples): 128 samples of lag, and long
  tails cheap. The two large stages are one block later and spread their frame (a
  resumable FFT, the multiply-add in slices every 16 samples) over that block; done all
  at once, a frame took 1.3-1.6 ms inside one 64-sample block (1.33 ms of audio) and long
  rooms stuttered (owner, Voxengo IRs). Measured per track, release: a 4 s stereo hall
  about 3 % of a core, worst block 0.13 ms; a 0.2 s body about 1 %; the computed reverb
  0.1 %. The dry signal is delayed by the lag so a mix does not comb-filter.
- **Impulse responses:** WAV, 16/24/32-bit PCM or 32/64-bit float, up to two channels,
  read defensively, capped at 6 s, scaled to unit energy, resampled linearly to the
  engine's rate. No responses ship with the program (licenses); the user loads their own.
- **Real time:** a new block or file prepares the chain on the synth's own thread (which
  knows the device rate), one instance per track of that kind (a 6 s stereo room needs
  about 9 MB per instance), and swaps it into the callback; the old one is freed on that
  thread. Knob values go straight to the callback, so tails are not cut while a knob
  turns. A tail counts as over after 0.5 s of silence. WAV export uses the same chains.
- **Longer (or shorter) rooms** (`ImpulseResponse::lengthened`, committed 2026-11-06): the
  decay time is measured from the energy decay curve (fitted -5 to -25 dB); the recording
  keeps its start under an envelope that turns its decay into the new one, and where it
  fades out a diffuse noise tail coloured by the late response carries it on, darker as it
  goes, down to -80 dB, at most 16 s. Deterministic; 30-400 ms to compute per room. The
  Room block's *Length* knob sets the factor (0.5-4). Because it changes the response
  itself, not a value the audio thread can take, the room is built again when the knob is
  let go (a click or an arrow key applies at once), not on every step of a drag.
- **Not yet:** the browser build has no chains (no file dialog, no message for them);
  chains are per kind of string, kept in the settings, not per track in the document.

## Faust

Faust (faust.grame.fr, GRAME) is a functional language for signal processing that
compiles to C++, Rust, WebAssembly and others (`faust -lang rust` gives plain Rust with
no runtime). Its `physmodels.lib` (mainly Romain Michon) has complete guitars
(`pm.guitar`, `pm.nylonGuitar`, `pm.elecGuitar`) and building blocks. The online IDE
(faustide.grame.fr) plays them in the browser with nothing to install:

```faust
import("stdfaust.lib");
process = pm.elecGuitar_ui_MIDI <: _,_;
```

Plan: use Faust to prototype and to find the sound quickly; the final engine is
hand-written Rust (full control over technique changes during a note, our lint rules).
**Check the licenses of the Faust libraries before any generated code goes into the
repository** (section 2: no GPL code without an owner decision; flag LGPL). The Faust
compiler itself is only a tool and is not shipped.

## Steps

0. **Playback sequence with strings and techniques** (shared with M18).
1. **Minimal string engine:** exact tuning, loss filter, dispersion; bends, slides,
   hammer-ons, palm mute, let ring, harmonics. Expressive but still synthetic. Bass
   guitar comes almost for free.
2. **Electric guitar:** pickup position, simple amp (waveshaping) and cabinet impulse
   response. First point where it can match RSE for rock and metal.
3. **Acoustic guitar:** commuted synthesis body, string coupling, two vibration
   directions, better attack. The long stretch toward Strum quality.
4. **Feel:** strum timing, noises, humanizing.
5. Later: amp captures (Neural Amp Modeler format, MIT; captures on ToneHunt, licenses
   per capture), more models (12-string, baritone).

## Evaluation

- A few short reference passages (single notes, chords, strums, each technique), recorded
  by the owner or rendered from Strum and RSE for private comparison only (never
  committed).
- Each step is judged by ear against them by the owner, plus spectrum and decay plots.
- Tests: deterministic rendering (same input, same samples), golden checks on pitch
  (fundamental within a few cents), decay times, bend targets, silence after damping, no
  NaN or clipping, no allocation in `render`, `panic_hunt` over random parameters.

## Reading

- Julius O. Smith, *Physical Audio Signal Processing*, free at
  ccrma.stanford.edu/~jos/pasp/ (plucked strings, commuted synthesis). Start here.
- Karplus and Strong, "Digital Synthesis of Plucked-String and Drum Timbres", Computer
  Music Journal, 1983.
- Jaffe and Smith, "Extensions of the Karplus-Strong Plucked-String Algorithm", Computer
  Music Journal, 1983 (tuning, pick position, dynamics).
- Karjalainen, Välimäki and Tolonen, "Plucked-String Models: From the Karplus-Strong
  Algorithm to Digital Waveguides and Beyond", Computer Music Journal, 1998 (the best
  overview).
- Välimäki et al., "Physical Modeling of Plucked String Instruments with Application to
  Real-Time Sound Synthesis", Journal of the Audio Engineering Society, 1996.
- Stefan Bilbao, *Numerical Sound Synthesis* (finite differences; background).
- Faust documentation and `physmodels.lib` reference at faust.grame.fr.
