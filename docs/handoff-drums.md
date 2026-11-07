# Handoff: a drum kit split into its parts (idea, not started)

Owner's wish (2026-11-07): almost every Guitar Pro song has **one** drum track. Its sound
should still be mixed like a real drum kit: the kick, snare, hi-hat, toms and cymbals
each with their own processing (and good defaults), summed on a drum bus. This would
improve the drums a lot. Builds on the mixer (`docs/handoff-mixer.md`); do it after the
mixer's step 4.

## Why it can work

- A General MIDI drum track already carries the parts apart: every drum sound is its own
  note (35/36 kick, 37/38/40 snare and side stick, 42/44/46 hi-hat closed/pedal/open,
  41/43/45/47/48/50 toms, 49/51/52/53/55/57/59 cymbals and ride, the rest percussion).
- Our RustySynth fork (`vendor/rustysynth`) renders every sounding note as a voice that
  knows its key (`Voice::key` is set in `Voice::start`). On a drum channel the voices can
  be summed into one buffer per **part** instead of one per channel.
- SoundFont drum kits are clean single hits (no spill between drums), so the split is
  cleaner than splitting a recorded kit.
- The hi-hat choke (open/closed) keeps working: the SoundFont's exclusive classes act on
  the voices before they are summed.

## Plan

1. **An EQ block** in `gscore-guitar` (needed first: drum tones are mostly EQ plus
   compression, and the amp's tone stack does not fit): low cut, low shelf, one or two
   sweepable bells (frequency, gain, Q), high shelf, high cut; `Biquad` has
   `peaking` and `shelf` already (`filters.rs`). A chain block like the others, with knobs
   in the window and a small curve drawing if cheap. Useful for every track, not only
   drums.
2. **The fork: parts.** For percussion channels, a key → part table (Kick, Snare, Hi-hat,
   Toms, Cymbals, Percussion; the table above) and per-part stereo blocks next to the
   per-channel ones (`render_split_block` in `vendor/rustysynth/src/synthesizer.rs`), read
   with a `render_parts`-style method. Keep the channel sum as it is (the parts of a drum
   channel add up to it). Record the change in `vendor/rustysynth/FORK.md`. Test that the
   parts add up to the channel exactly (as `test_split_channels.rs` does for channels).
3. **The engine: a drum bus.** In `gscore-audio::mixer`, a drum track's strip takes its
   parts: each part through its own chain, then summed, then the drum track's own chain,
   fader, pan and sends as now (so the strip stays one strip in the mixer). Part chains
   are prepared off the audio thread like track chains (`chains.rs`, `synth_player.rs`
   housekeeping); no allocation in the callback. Meters per part are nice to have.
4. **Default tones per part** (the owner designs the final ones; start with):
   - Kick: low cut ~30 Hz, a bell around 60 Hz up, a dip near 300-400 Hz, a click around
     3-4 kHz up; the CA-76 compressor (ratio 4, medium attack so the beater comes
     through).
   - Snare: low cut ~80 Hz, body around 200 Hz, crack around 5 kHz; compressor (ratio 4-8);
     a short room or plate (computed reverb, small size, short decay) mixed in a little.
   - Hi-hat: low cut ~300 Hz, a little quieter.
   - Toms: low cut, a dip in the boxy mids, compressor, a little room.
   - Cymbals: low cut ~400 Hz, slightly wide.
   - Drum bus (the drum track's own chain): light compression for glue (ratio 4, slow
     attack, fast release).
   Store them like tones: built in, and overridable per kit preset in the settings (the
   way `preset_tones` work for General MIDI programs).
5. **The window:** the drum track's chain window gets a row of the parts (Kick, Snare,
   Hi-hat, Toms, Cymbals, Percussion), each opening its own chain board; the drum bus
   is the track's own board. Maybe a level per part on the drum track's mixer strip.
6. **Export:** WAV export renders the same (it uses the engine). MIDI out and MIDI files
   are not affected.

## Notes

- A drum track with notes outside the table goes to Percussion; a song whose drum track
  uses a different kit layout (not General MIDI) still plays, only less well sorted.
- Several drum tracks (each has its own channel already, `ChannelAllocator`) each get
  their own parts.
- Cost: a few EQs and compressors per drum track, far below one convolution room.
- Later, a transient shaper block (more or less attack on kick and snare) would fit here.
