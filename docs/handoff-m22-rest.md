# Handoff: what else M22 needs (besides the other handoffs)

M22 (`AGENTS.md` section 9, `docs/guitar-modelling.md`) is done when the owner judges
electric, acoustic and bass playback comparable to Guitar Pro's RSE for composing, on
reference passages covering every technique. The other open work is in
`docs/handoff-mixer-ui.md`, `docs/handoff-drums.md`, `docs/handoff-song-state.md` and
`docs/handoff-navigation.md`; the owner's own licence-free impulse responses (bundled,
before the release) are his. This file lists the rest, in a sensible order.

## 1. Reference passages (first: every later step is judged on them)

- A set of short GP7 files in `testdata/` (our own, written for this; never band files
  from `~/svn/wyw`, never anything from `~/svn/bigfive`), one per technique or a few
  bars each: single notes up the neck, open chords and barre chords strummed down and
  up, palm-muted chugs, a power-chord riff, bends (half, full, release, pre-bend), slides
  (shift, legato, in and out), vibrato and whammy, hammer-ons and pull-offs, tapping,
  natural and artificial harmonics, dead notes, let ring, staccato, a fingerpicked
  pattern, a nylon arpeggio, a bass line with finger, pick, slap and pop, and grace
  notes. One electric (clean and distorted), one steel acoustic, one nylon, one bass.
- A short `docs/` note on how to listen (which tone, which engine) and a CLI line that
  renders all of them to WAV with the modelled strings (`gscore-cli wav --modelled`), so
  each change can be compared by ear before and after.

## 2. The sound

1. **Pick attack** (owner asked for it after the pickup): the pick pushes the string and
   slips off within about a millisecond (a short force pulse whose width follows the
   pick's thickness, `hardness`), a short scrape-and-click noise, and a slightly sharp
   start of a hard pluck that settles within about 50 ms (tension modulation). Today a
   pluck reshapes the loop at once (`gscore-guitar/src/string.rs` `excite`).
2. **The acoustic body:** a body response for steel and nylon (commuted synthesis, as
   planned). It can be computed (a few body resonances around 100-250 Hz and the
   top's modes) rather than recorded, so it need not wait for the owner's IRs.
3. **String coupling:** unplayed strings of a track ringing along through the bridge
   (sympathetic resonance), quietly; the open strings of a chord shape especially.
4. **Noises and feel:** fret and slide squeaks (on wound strings), the release noise when
   a finger lets go, and humanized strum timing (strings of a strum a little apart, up
   and down strokes, a little velocity spread).
5. **Techniques still simplified:** harmonics play the harmonic's pitch with a purer pluck
   instead of damping the string at a node (`docs/guitar-modelling.md`, "Not yet");
   octave signs (8va, 8vb, 15ma) are not applied in playback (noted when they were added).
   Then listen through every technique of item 1 and fix what sounds wrong.

## 3. Performance

- The strings benchmark (`cargo xtask bench`: 16 tracks × 6 strings ringing) is over its
  250 ms budget (about 280 ms on a busy machine, since the pickup). Cheaper pickup: read
  the pickup's tap from the string's own delay line instead of a second one
  (`gscore-guitar/src/pickup.rs`).
- The benchmark measures the strings only. Add a whole-engine benchmark: a 16-track song
  with chains on every track (an amp, an EQ, a compressor, a room), the reverb,
  compressor and chorus buses and a drum track with its parts, rendered by `SynthEngine`;
  the milestone asks for a 16-track song in real time with headroom on the owner's
  machine.
- Skip a drum part whose input is silent and whose chain does not ring (today every part
  chain of every drum track runs every block).

## 4. Smaller fixes from the review

- **MIDI out after a seek while playing** restarts at once, without the hold-back by the
  audio latency that Play has (`gscore-audio/src/midi_out.rs`, `pending_play`): after a
  jump the hardware synthesizer runs ahead. Hold a seek back the same way.
- **The browser build** has no chain window and loads no impulse responses (no file
  dialog; `docs/guitar-modelling.md`, "Not yet"). The owner decides whether M22 covers the
  browser or it waits; at least the built-in tones and the computed blocks (amp, drive,
  EQ, compressor, chorus, reverb) need no files and could play there.

## 5. Before merging into `main`

- `docs/gaps.md` and `docs/parity.md`: the chains, the mixer, PM synthesis, drum parts,
  MIDI out per track, what is still missing.
- A short section of the user guide (or `docs/usage.md`) on the mixer, the sound chain
  window and tones.
- `cargo xtask ci`, the benchmark, and a listen through the reference passages with the
  owner.
