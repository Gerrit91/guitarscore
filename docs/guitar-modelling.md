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
- **Knobs:** every `StringModel` value has a range (`gscore_guitar::ranges`); the Sound
  chain window (track panel › Sound › the stomp box) turns them per kind of string, next
  to the board, heard at once (`Command::StringModel`), kept in the settings, used by WAV
  export.
- **Presets** (`GuitarModel`): nylon, steel (the owner's dialed-in sound), electric,
  bass; chosen by default from the track's General MIDI program (24 nylon, 25 steel, 26-31
  electric, 32-39 bass) through `PerformanceTrack::engine`, or per track in the Sound
  chain window.
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
- **Not yet:** pickup position (step 2), body, coupling and two polarizations
  (step 3), noises and feel (step 4). Everything is one generic string model now; a
  distorted part sounds distorted once its chain has an Amp block (below). Harmonics play the
  harmonic's pitch with a purer pluck rather than damping at a node.

## The effect chain (2026-11-06)

- **The board** (owner's design): per kind of string, six boxes in a fixed row joined by
  cables, like a pedal board; the order cannot change and boxes do not move. The strings
  come first and cannot be removed (their box shows the engine, the kind of string and
  their sound in the window itself); the five places after them hold nothing or a block.
  Opened from the track panel's Sound section.
- **Blocks:** *Cabinet / body*, an impulse response from a WAV file played mono with
  mix, gain, low cut and high cut: what amp modelers call a cab block, and an acoustic
  guitar's body too. *Room*, a room's impulse response (stereo if recorded so): a
  convolution reverb with its length (the decay stretched 0.5× to 4×, below), mix, gain,
  cuts and width. *Reverb*, computed (below): size,
  decay, pre-delay, damping, modulation, width, mix, and a low and high cut on its sound.
  *Amp* and *Drive*, computed (below).
- **Amp** (`gscore-guitar::amp`, 2026-11-06): no capture of a real amp, the parts that
  make a tube preamp sound as it does. An input pad (the strings come in hot: a power chord
  picked hard peaks near 4), a low cut before the clipping (*tight*: 40, 90, 160 Hz by
  voice) and a mid bell (the lead voice pushes 750 Hz by 7 dB, like a boost pedal in
  front), the Gain knob, then one to three soft-clipping stages, a little lopsided (a
  tube's bias: even harmonics), each followed by a coupling low cut and a plate high
  cut. The stages run 4× oversampled (sixth-order Butterworth up and down), so the
  harmonics they make above half the sample rate do not fold back as fizz (test: a 3 kHz
  tone clipped hard leaves under 3 % between its harmonics). After them the tone stack
  (bass shelf 120 Hz, mid bell 700 Hz, treble shelf 3 kHz, ±12 dB each) and a built-in
  speaker: a closed 4×12 approximated by filters (low cut 75 Hz, 110 Hz resonance, a dip
  at 450 Hz, presence at 2.4 kHz, fourth-order roll-off at 5 kHz). The Speaker knob blends
  it out for a cabinet's impulse response in a Cab block after the amp. Voices: *Clean*
  (one stage, the pad given back so it plays at the strings' level), *Crunch* (two),
  *Lead* (three); their outputs are trimmed to within about 3 dB of one another on a
  power chord. About 0.9 % of a core per track (release).
- **Drive** (`gscore-guitar::drive`): an overdrive pedal after the green mid-hump kind.
  Only the signal above 720 Hz is driven (6 to 42 dB) into a symmetric soft clip (4×
  oversampled, smoothed at 6.5 kHz); the lows pass clean beside it, then a tone lowpass
  (900 Hz to 7 kHz) and Level. In front of the Lead voice with Drive low and Level up,
  the low E's fundamental is a clearly smaller share of the sound (test): the metal
  player's tight low end.
- **Reverb** (`gscore-guitar::reverb`, 2026-11-06, replacing the Freeverb copy the owner
  did not like): a feedback delay network after Geraint Luff's design for Signalsmith
  Audio (MIT article; no code taken). Pre-delay up to 250 ms; the stereo signal spread to
  eight channels; four diffusion stages (each: eight delays spread over its length,
  shuffle with sign flips, Hadamard mix; lengths doubling up to 0.6 × the room) for a
  dense onset; a tail of eight delay lines (one to two times the room, 25 to 220 ms by
  Size) fed back through a Householder matrix. The loop gain per line gives the Decay
  time exactly (0.3 to 30 s); a shelf at 2.5 kHz in the loop gives the highs a shorter
  decay (Damping: down to 0.1×). Each tail line drifts by up to 4 ms at its own slow rate
  (0.11 to 0.83 Hz; Modulation), so long tails shimmer instead of ringing metallic. The
  diffusion's output also goes out (early part, 0.35). Size and pre-delay glide (80 ms),
  so turning them while playing does not click. All wet it stays 4-6 dB under the dry
  strings from a small room to a 30 s cathedral. Allocated once for the largest size
  (about 1 MB at 48 kHz), 1.3 % of a core per instance (release; linear interpolation
  in the diffusers, third-order in the drifting tail).
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
- **Real time:** a new block or file prepares the track's chain on the synth's own thread
  (which knows the device rate; a 6 s stereo room needs about 9 MB) and swaps it into the
  callback with its knob values; the old one is freed on that thread. Knob values go
  straight to the callback, so tails are not cut while a knob turns. A tail counts as over
  after 0.5 s of silence. WAV export uses the same chains.
- **Per track** (2026-11-06, see Tones): the engine keeps a string model and a chain per
  track (`gscore-audio::chains::TrackTones`, room for 64 tracks made with the engine), by
  the song's track index, which a one-track beat excerpt keeps; the kind's model is the
  fallback. Playback remembers what each track was given and sends only what differs
  (a model, a new chain, or knob values). Export applies the tracks' engine choices too
  (it used to apply only the global switch).
- **Longer (or shorter) rooms** (`ImpulseResponse::lengthened`, committed 2026-11-06): the
  decay time is measured from the energy decay curve (fitted -5 to -25 dB); the recording
  keeps its start under an envelope that turns its decay into the new one, and where it
  fades out a diffuse noise tail coloured by the late response carries it on, darker as it
  goes, down to -80 dB, at most 16 s. Deterministic; 30-400 ms to compute per room. The
  Room block's *Length* knob sets the factor (0.5-4). Because it changes the response
  itself, not a value the audio thread can take, the room is built again when the knob is
  let go (a click or an arrow key applies at once), not on every step of a drag.
- **Not yet:** the browser build has no chains (no file dialog, no message for them). A
  track's own tone lives with the open tab by track index (like its engine choice): it
  is not saved yet, and moving, deleting or inserting a track does not carry it along.
  Both are fixed by keeping tones on the track in the song and in the GP7 file (Tones,
  next step). The built-in tones per track preset do not exist yet: each kind's default
  from the settings plays.

## Tones (owner's decision, 2026-11-06)

- **A tone** is what a modelled track sounds like: what plays it (MIDI or PM synthesis,
  the kind of string), the sound of its strings and its effect chain. Called *Tone* in
  the program, so it is not mistaken for the track's *preset* (its General MIDI program).
- **Every track preset ships with a default tone for physical modelling** (built-in,
  designed by the owner): 29 Overdriven Guitar and 30 Distortion Guitar each start from
  their own. Until a preset has one, its kind of string's default plays.
- **Per track:** a track's tone can be changed for that track alone; no other track
  changes, even one with the same preset or kind of string.
- **Saved tones:** the user can save tones under a name (their library, in our format)
  and use them on other tracks.
- **In the song file:** a track's own tone is saved inside the GP7 file (a ZIP), as an
  entry of ours next to `Content/score.gpif`, so people who use GuitarScore exchange their
  tweaks with the song. Impulse responses are referenced by name and checksum, not
  embedded. Guitar Pro most likely drops the entry when it saves the file again: we live
  with that, and users share their tone presets themselves. GP3-5 and GPX have no room
  for it.

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
