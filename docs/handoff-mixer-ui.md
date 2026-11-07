# Handoff: the mixer's next round (owner's list of 2026-11-07)

Builds on the mixer of `docs/handoff-mixer.md` and the drum parts of
`docs/handoff-drums.md` (both done, on `m22-string-engine`). Items marked **Owner** are
decided; items marked **Proposed** came from the review and need the owner's OK before
they are built.

## 1. A chorus bus, and the stomp box on every strip (Owner)

### The chorus bus (owner, 2026-11-07)

In General MIDI a track's chorus value (MIDI CC 93, Guitar Pro's per-track Chorus) is a
send into one shared chorus, like the reverb value; every SoundFont synthesizer does it so
(RustySynth too, whose own chorus has been off since the mixer). The owner chose that
reading: a **chorus bus** next to the reverb and compressor buses.

- Every track's Chorus knob sends into it after the fader, whatever plays the track (the
  MIDI synth, PM synthesis, a drum kit): consistent for every source, and songs made in
  Guitar Pro keep their chorus.
- The bus runs our chorus block (`gscore-guitar/src/chorus.rs`) **all wet** and returns
  only the chorused signal, so the dry signal stays as it is (in the middle) and the bus
  widens it. A stereo bus like the compressor bus, but hearing a mono sum of the sends
  (like the reverb bus) suits a chorus too: pick one and say so.
- Default tuned like a classic General MIDI chorus (a slow rate, moderate depth, about
  15 ms delay); its knobs in a bus window like the reverb bus's
  (`app/reverb_bus_window.rs`), level and mute on its strip.
- Engine: `gscore-audio::mixer` `Bus` gets a `Chorus` variant (a third `BusState`); the
  mixer takes CC 93 like CC 91 (`Mixer::take`: today 93 goes on to the synthesizer, whose chorus is off, so it does nothing) into the strip's chorus
  send. Settings and playback like the compressor bus (`settings.rs` `CompressorBus`,
  `playback/chains.rs` `send_compressor_bus`); WAV export renders it; it travels with the
  song like the other buses (`docs/handoff-song-state.md`, "Song-wide").
- The **chorus block** stays as it is, for deliberate sound design inside a chain (a
  chorus pedal before an amp or a reverb). The bus is for the tracks' Chorus values.

### The strip: the Chorus knob stays, the stomp box joins

- The Chorus knob in every track strip stays (it is the send into the chorus bus now).
  Strip layout: `app/grid/strips.rs` (`knobs`, "four knobs across the top: pan, reverb,
  chorus, comp").
- The **stomp-box icon** (`app/icons.rs` `Icon::StompBox`, the one the track panel's
  Sound section uses) opens that strip's sound chain window (`open_sound_chain`) from
  every strip: on its own small row next to (or under) the strip's name, so the four
  knobs keep their places. A track, a drum part (item 3), a bus and the master (item 2)
  all have it.
- The buses' "Edit…" buttons (`strips.rs` `BusChange::Edit`, reverb and compressor bus,
  and the new chorus bus) become the same icon, so every strip opens its chain the same
  way.

## 2. A sound chain on the master (Owner)

- The master strip gets a chain too (its stomp box opens it): after the sum of the tracks
  and buses, before the master fader. Engine: `gscore-audio::mixer` `Mixer::mix` sums into
  `left`/`right`; run a master `Chain` there (stereo, `process_stereo`), prepared off the
  audio thread like the buses (`Bus` and `synth_player.rs` housekeeping; a third bus-like
  slot or a `Master` variant). Meter after it (`meter_master`).
- **The metronome's click is added after the master chain** (`synth.rs` adds `click` after
  `mixer.mix`): a compressor or limiter on the master must not pump with the click, and an
  export never contains the click anyway.
- Stored with the song (song-wide, see `docs/handoff-song-state.md`, "Song-wide"), the
  machine's default when the song has none (empty).
- **Proposed: a limiter block** for the master (brick-wall, a ceiling of about -0.3 dBFS,
  a little look-ahead, release). Today the output is hard clipped to -1..1
  (`synth.rs` `render_interleaved`), which sounds harsh when a mix is too loud; a latching
  red clip light on the master strip would show it.

## 3. The drum kit: no default tones, part meters and part strips (Owner)

- **Clear every built-in drum tone:** `chain_settings.rs` `default_drum_part_chain`
  returns an empty chain for every part, and `default_drum_bus_chain` an empty one too.
  The kit then plays exactly as RustySynth renders it (the parts summed are the channel:
  check with a test that a drum track without any part chain is sample for sample the
  same as before the split). The owner designs the defaults later. Remove or rewrite the
  tests that expect the old defaults.
- **Part levels and meters:** in the engine every part gets a fader (gain, 0-127 square
  law like a track's, default unity) and a meter (`meters.rs`: per track and part, with
  the same ballistics) after its chain, before the parts are summed; a part's mute and
  solo inside the kit are cheap and useful. Commands like `PartChainSettings` for the
  part's level and mute.
- **Mixer:** a drum track's strip gets a small side arrow (▸ / ◂) that opens its parts as
  narrower strips right next to it (Kick, Snare, Hi-hat, Toms, Cymbals, Percussion): each
  with its meter, fader, mute, solo and stomp box (opens that part's chain board; the sound
  chain window already has the parts row, `sound_chain.rs` `part`). Closed by the arrow
  again; the open state per track is remembered in the tab.
- Part levels, mutes and chains travel with the song, per drum track
  (`docs/handoff-song-state.md` lists them already).
- **Proposed:** skip a part whose input is silent and whose chain does not ring (today
  every part chain of every drum track runs every block, even in silence).

## 4. The EQ: a picture and a better layout (Owner)

The block already has four bands (a low shelf, two bells, a high shelf) plus low and high
cut (`gscore-guitar/src/eq.rs` `EqKnobs`). What it needs:

- **A frequency response curve** above its knobs: 20 Hz to 20 kHz on a log scale, ±18 dB,
  the sum of all bands and both cuts (compute each `Biquad`'s gain on a grid, as
  `filters.rs` `gain_at` does in tests; make it available outside tests), with each band's
  own curve faint behind it and the 0 dB line.
- **Handles on the curve:** one dot per band (in its color) at its frequency and gain;
  drag moves frequency and gain, the mouse wheel (or Shift+drag) changes its width (Q);
  the cuts have a handle on the 0 dB line at their frequency; double-click resets a band.
  Hovering shows the values. Knob and handle stay in step.
- **Knobs in columns, low to high, left to right:** Low cut | Band 1 (Freq, Gain, Shape)
  | Band 2 (Freq, Gain, Q) | Band 3 (Freq, Gain, Q) | Band 4 (Freq, Gain, Shape) | High cut,
  each column titled and tinted in its band's color, the same order as the handles on
  the curve. Today they wrap in one row (`app/amp_knobs.rs` `eq_knobs`).
- **Proposed:** the outer bands switchable between shelf and bell, and the cuts' slope
  (12 or 24 dB per octave); a live spectrum of the signal behind the curve later (needs an
  analysis path from the audio thread, like the meters).

## 5. A sound chain for the metronome (owner's question)

Not recommended for now. The click is a short synthetic tick that plays only while
practising and is never exported; a chain would add little. It should stay outside the
master chain (item 2). Better value: a few more click sounds and an accent level, which
`click.rs` can do without the mixer.

## 6. More improvements (Proposed)

- **Undo and "modified" for sound changes:** now that chains, tones and sends travel with
  the song (`song_state.rs`), changing them should mark the document as modified and be
  undoable like a fader move. Today they are not part of the edit history.
- **Copy and paste a chain or a block** between tracks and drum parts (right-click on a
  box: Copy / Paste), and drag a block onto another track's board.
- **A drum room:** the parts sent into one short shared room (a small "room" bus inside
  the kit) glue a kit together like a real room's overheads; one Room knob per part.
- **Export stems:** with every track a strip, a WAV per track (and per bus) is a small step
  from the WAV export (`gscore-audio::wav`) and useful for people who mix elsewhere.
- **Input meter / trim:** a small meter before the chain (or a trim knob) helps set how
  hard an amp or compressor is driven.
- **Solo-safe buses:** soloing a track keeps the reverb and compressor buses playing its
  sends (check that it does today).

## Tests

- The strip opens the chain window from its stomp box (track, bus, master) (snapshot of
  the mixer).
- A track's Chorus value (and a live change of the knob) reaches the chorus bus, for a
  SoundFont track and a modelled one alike; level 0 or mute silences the bus; a song with
  chorus 0 everywhere leaves the bus silent.
- A drum track without part chains plays exactly as before the split; a part's level and
  mute change only that part; the part strips open and close with the arrow and their
  meters move (engine test with `sine_drum_soundfont`, app test for the arrow).
- The master chain processes the sum, not the click; a limiter keeps the output under its
  ceiling.
- The EQ curve matches the filters' gain (a few frequencies against `gain_at`); dragging a
  handle moves its knobs (app test).
