# GuitarScore's fork of RustySynth

RustySynth 1.3.8 by Nobuaki Tanaka (https://github.com/sinshu/rustysynth, MIT, see
`LICENSE.txt`), as published on crates.io, with these changes for GuitarScore's mixer.
The workspace uses it in place of the crates.io release through `[patch.crates-io]` in
the root `Cargo.toml`; it is kept outside the workspace, so the workspace's lints do not
apply to it.

- `SynthesizerSettings::split_channels` and `Synthesizer::render_channels`: each MIDI
  channel is rendered into its own stereo block (the voices of a channel summed with
  their gains and pans), so every track can have its own chain, fader, sends and meter.
  Reverb and chorus are off in that mode (the mixer has its own). `render` still plays
  the sum of the channels.
- No `unsafe`: the sample data is read in 64 KB chunks and decoded little-endian
  (`BinaryReader::read_wave_data`), and the SoundFont3 check reads the first samples'
  bytes safely (`SoundFontSampleData::new`).
- `Synthesizer::set_percussion_channel` and `is_percussion_channel`: any channel can play
  drums (the percussion banks), not only channel 9, so several drum tracks each get a
  channel and a mixer strip of their own. A channel that becomes percussion starts on the
  standard kit.
- **Drum parts:** a percussion channel's voices are also summed by part (`PARTS` = 6:
  kick, snare, hi-hat, toms, cymbals, the rest, by General MIDI key; `drum_part`), apart
  from the channel's own block, so the mixer can give each drum its own chain and still
  have one strip per drum track. `render_channels` fills them; `part_block(channel, part,
  side)` reads one back. The parts of a channel add up to exactly its block.
  `part_block` gives the parts of exactly the samples the last `render_channels` call
  gave (copied from the same read position), so a call that does not line up with the
  synthesizer's blocks still gets matching parts (2026-11-11; before, it gave the last
  whole block, and the parts dropped and repeated stretches against the channels).
- **Exclusive classes and stereo samples** (2026-11-11): every note-on has an id
  (`Voice::note`), and a region never takes the voice of another region of the same
  note-on in its exclusive class. A stereo hi-hat is two regions (left, right) of one
  class; upstream the second took the first's voice, so FluidR3's hi-hats (and snare)
  played on one side only. Earlier notes of the class are still choked.
- **Drums ring out** (2026-11-11): on a percussion channel a note-off does not release a
  one-shot sample (a drum rings whatever the written note's length, as a real one); a
  looped sample is still released. A drum struck again releases its last hit instead.
- `Synthesizer::active_voice_count`: how many voices sound (for diagnostics and tests).
