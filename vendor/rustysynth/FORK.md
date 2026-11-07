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
