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

1. **Palm mutes** (done 2026-11-09 on branch `m22-palm-mutes`, with a Palm mute knob
   (light to heavy) added 2026-11-10 after the owner found it "merely improved"; see the
   devlog; open: the owner's listening check clean and distorted. Owner, 2026-11-07: what
   he dislikes most about the PM sound today):
   they sound only percussive and thin. A real palm mute is a beefy note: the hand at the
   bridge takes away the highs and shortens the ring, but the note's pitch and much of its
   character stay, and it lasts clearly longer than a click. Today:
   - `gscore-play/src/effects.rs` `shape_note` cuts every palm-muted note to an eighth
     note at most (`PALM_MUTE_TICKS`), plays it at 85% velocity and stops it dead at its
     end (`damp`);
   - the string (`gscore-guitar/src/string.rs`) decays in 0.18 s at the fundamental and
     0.03 s around 2 kHz (`Damping::PalmMuted`), and the pick's hardness is halved.

   Wanted:
   - **Longer:** no eighth-note cap for the modelled strings; the note keeps its written
     length and fades with the palm's damping, and the next note on the string or the
     note's end stops it as usual, not with a dead stop. The SoundFont path has no
     palm-mute samples and may keep a cap (`shape_note` is shared by all engines, so the
     length rule must depend on what plays the track).
   - **Beefier:** full strength (no velocity cut; a chug is played hard), the pick's
     attack kept, the low end and low mids full; the hand damps the highs much more than
     the fundamental and the first harmonics.
   - **More of the note:** the fundamental and the low harmonics ring long enough to hear
     the pitch clearly (low strings longer than high ones), with no detuning from the
     damping. The figures in `Damping::PalmMuted` are starting points to tune by ear,
     not a design.
   - Maybe a palm-mute amount in the string sound (light to heavy, the hand nearer to or
     further from the bridge); the owner decides.
   - Through a high-gain amp it still chugs tightly (the amp's and drive's Tight setting).

   Tests: a palm-muted note against the same note open (same velocity): its level near
   the open note's, its fundamental still sounding well past an eighth note, clearly less
   energy above a few kHz, and the same pitch; a palm-muted note followed by another on
   the same string stops at the new note. Then the palm-muted chugs and the power-chord
   riff of the reference passages (item 1 of section 1), by ear with the owner, clean and
   distorted.
2. **Pick attack** (done 2026-11-10 on branch `m22-pick-attack`: Pick noise and Attack
   pitch knobs, see `docs/guitar-modelling.md`; open: the owner's ear. Owner asked for it
   after the pickup): the pick pushes the string and
   slips off within about a millisecond (a short force pulse whose width follows the
   pick's thickness, `hardness`), a short scrape-and-click noise, and a slightly sharp
   start of a hard pluck that settles within about 50 ms (tension modulation). Today a
   pluck reshapes the loop at once (`gscore-guitar/src/string.rs` `excite`).
3. **The acoustic body** (done 2026-11-11 on branch `m22-body-delay`: computed modal
   bodies per track, see `docs/guitar-modelling.md`; open: the owner's ear): a body
   response for steel and nylon (commuted synthesis, as
   planned). It can be computed (a few body resonances around 100-250 Hz and the
   top's modes) rather than recorded, so it need not wait for the owner's IRs.
4. **String coupling:** unplayed strings of a track ringing along through the bridge
   (sympathetic resonance), quietly; the open strings of a chord shape especially.
5. **Noises and feel:** fret and slide squeaks (on wound strings), the release noise when
   a finger lets go, and humanized strum timing (strings of a strum a little apart, up
   and down strokes, a little velocity spread).
6. **Techniques still simplified:** harmonics play the harmonic's pitch with a purer pluck
   instead of damping the string at a node (`docs/guitar-modelling.md`, "Not yet");
   octave signs (8va, 8vb, 15ma) are not applied in playback (noted when they were added).
   Then listen through every technique of item 1 and fix what sounds wrong.

7. **The amp and the drive were calibrated on a wrong test signal** (found 2026-11-10;
   recalibrated 2026-11-11 on branch `m22-amp-calibration` with the owner's go-ahead:
   input pad 0.5, trims keeping each voice's level, a Tight knob on the drive; see
   `docs/guitar-modelling.md`, "Calibration"; open: the owner's ear).
   Until then, `gscore-guitar/tests/test_amp.rs` `power_chord` summed the strings into a
   scratch block it never cleared: a running sum of everything played, hugely bass-heavy
   and about 25 dB too hot (RMS 0.9, peak 3.8; real strings give RMS 0.05, peak 1.45).
   - `INPUT_PAD` ("the strings come in hot, a chord peaks near 4") and the drive's 720 Hz
     cut before clipping were tuned against it.
   - With real strings the drive raises the low E's share instead of tightening it
     (`a_drive_in_front_tightens_the_low_end` is ignored until this is settled).
   - The owner hears the same in practice: the drive takes away the chuggy bass and
     low mids, and gain 10 is "not 100 % metal" (2026-11-10).
   - Re-calibrating the amp's input and re-voicing the drive (a Tight knob, less cut)
     changes every tone dialled in by ear: the owner decides.

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
