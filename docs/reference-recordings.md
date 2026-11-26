# Reference recordings

How the owner records the guitars that the modelled sound (M22) is measured against. The
recordings stay with the owner, outside the repository. Only figures taken from them
(spectra, decay times, responses fitted by filters) go into code and tests, never the
audio (see `docs/guitar-modelling.md`, "Evaluation").

What the recordings are for:

- **DI (electric) and piezo pickup (acoustic):** the strings without amp or body, the
  same signal the string model makes. They give the decay of each partial, the
  inharmonicity, the pickup's position, the pick's attack, the two-stage decay, palm
  mutes and noises.
- **Through the amp modeller:** what a good tone sounds like, on exactly the same
  playing. Amp only and amp with cab, so the two can be told apart; the DI can then be
  run through GuitarScore's own amp and cab and compared with the modeller's on
  identical input.
- **Microphones on the acoustic, at the same time as its pickup:** the difference
  between the two is the guitar's body, which the Cab block's body response can be
  fitted to.

## The tabs

Every recording has a tab in `docs/reference-recordings/` that you play along to: one
`.gp` file per WAV file, numbered as the WAV files are named.

| Electric (`electric/`) | Content | Length |
|---|---|---|
| `01-single-notes` | every string open, fret 5 and fret 12, each soft, medium and hard (p, mf, ff) | 7:20 |
| `02-single-notes-neck` | the open strings, medium, on the neck pickup | 0:56 |
| `03-palm-mute` | each string open and fret 5, light then heavy; 8th-note chugs on low E and E5 | 2:08 |
| `04-chords` | open E, open G, A5, F barre: each strummed down, up and as a slow arpeggio | 1:44 |
| `05-techniques` | bends (half and whole step, up, hold, release), vibrato, slides, hammer-on and pull-off, dead notes, natural harmonics, tremolo picking | 2:40 |
| `06-coupling` | open A three times with the other strings free, three times with them damped | 0:56 |
| `07-riff` | your own riff, see below | |

| Acoustic (`acoustic/`) | Content | Length |
|---|---|---|
| `01-single-notes` | as electric 01 | 7:20 |
| `02-chords` | as electric 04 | 1:44 |
| `03-techniques` | as electric 05 (bends and tremolo picking only as far as they work on the guitar) | 2:40 |
| `04-coupling` | as electric 06 | 0:56 |
| `05-fingers-and-pick` | low E, D and B, open and fret 5, with the pick and then with the fingers; open G strummed with the pick and plucked with the fingers | 2:00 |
| `06-strumming` | G, C, D, G in down-up 8ths at 90 BPM, then a ringing G | 0:21 |
| `07-body-taps` | five taps on the bridge with the strings damped, one a bar | 0:32 |
| `08-riff` | your own riff, see below | |

| Bass (`bass/`, 4 strings, E standard) | Content | Length |
|---|---|---|
| `01-single-notes` | every string open, fret 5 and fret 12, each soft, medium and hard (p, mf, ff), with the fingers | 5:00 |
| `02-pick-and-mute` | each string open and fret 5 with a pick; each string open palm-muted with the pick, E and A palm-muted with the thumb | 2:04 |
| `03-lines` | 8ths on open E and A, root-fifth-octave, 16ths on open E, an E major scale in quarters: with the fingers, then with a pick; dead notes | 2:12 |
| `04-techniques` | slides up and down, hammer-on and pull-off, vibrato, natural harmonics (12th fret on every string, 7th and 5th on E and A), slap and pop if you play them | 2:28 |
| `05-coupling` | open A three times with the other strings free, three times with them damped | 1:00 |
| `06-riff` | your own bass line, see below | |

| 5-string bass (`bass5/`, B standard) | Content | Length |
|---|---|---|
| `01-single-notes` | as bass 01, then the low B: open, fret 5 and fret 12, p, mf and ff | 6:12 |
| `02-pick-and-mute` | as bass 02, then the low B open and fret 5 with a pick, open palm-muted with the pick and with the thumb | 2:36 |
| `03-lines` | as bass 03 | 2:12 |
| `04-techniques` | as bass 04, then the low B's natural harmonics at 12, 7 and 5 | 2:52 |
| `05-coupling` | as bass 05: open A with the other strings (the low B too) free, then damped | 1:00 |
| `06-riff` | your own bass line | |

The 5-string tabs are the 4-string ones with the fifth string added. The low B parts
are added at the end, so every bar before them is where it is in the 4-string tab, and
recordings of both basses line up bar by bar.

How the tabs are laid out:

- **60 BPM in 4/4** (one bar is 4 s), except the strumming at 90.
- **Bars 1 and 2 are silent:** hands off the strings. That gives 8 s of noise floor and
  hum, and room to start the recording.
- **Single notes and chords sit on a two-bar grid:** play on beat 1, let it ring for a
  bar and a half (6 s), damp it on beat 3 of the second bar ("damp" is written above
  it), wait 2 s, next.
- Dynamics are written as p, mf and ff, and the text above each beat says what to
  play ("fret 5, soft", "strum up", "slow arpeggio, low to high").
- Section markers name each group, so you can find your place.

**The riff:** play two to four bars of your own (on the bass a line of yours, or the bass
part of a song the guitar riff belongs to), a few times, clean and with gain on the
electric. Write the same riff as tab in GuitarScore at the tempo you play it, and save
it next to the WAV file (`07-riff.gp`, `08-riff.gp` on the acoustic). GuitarScore can
then play the same notes for a note-for-note comparison.

## Playing along

- Open the tab in GuitarScore, mute its track, switch the metronome on (count-in off:
  the two silent bars are the count-in) and listen on **headphones** only (closed ones
  for the acoustic, so no click reaches the microphones).
- Start the recording in the DAW first, then press play in GuitarScore. The two need
  not be in sync: the notes are found in the recording afterwards, as long as you play
  in time with the click.
- A mistake: keep going, and write down the bar number. Do not stop and restart.

## Rules for every recording

- **48 kHz, 24-bit** (or 32-bit float), **mono**, one WAV file per signal. Do not
  normalize on export; leave silence at the start and end as it is.
- **No processing** on the way in: no noise gate (above all on the DI), EQ, compressor,
  limiter or input effect. A tuner that mutes is switched off.
- **One gain for the whole session.** Set it so the hardest note (ff, a hard chord)
  peaks between −10 and −6 dBFS, then leave it. The level difference between soft and
  hard is part of what is measured.
- **Tune exactly** (A = 440 Hz, a strobe tuner if you have one) before every file.
- **The same pick and the same picking spot** throughout. Measure the spot once (cm from
  the bridge saddle) and write it down.

## Electric guitar

- Into the interface's **Hi-Z / instrument input**, or a good DI box: the load changes
  how passive pickups sound.
- Volume and tone knobs fully up. The bridge pickup, except in `02-single-notes-neck`.
- Record the DI of all files `01` to `07`.

### Through the amp modeller

Best: record the DI and the modeller's output **at the same time**, on two tracks. Most
modellers (Helix, Quad Cortex, Axe-Fx, Kemper) send both over USB, sample-aligned.
Otherwise reamp the DI files afterwards.

- **Two or three dry tones:** clean, crunch, high gain. No reverb, delay or global EQ;
  the output must not clip.
- **Each tone twice:** amp only (cab block off), and amp with cab.
- **What to run through them:** `01` (medium notes are enough), `04`, `05` and `07`.
- **The cab alone:** export the cab block's impulse response if the modeller can, as a
  WAV file. It can be loaded into a Cab block in GuitarScore right away (Load file…).
- **For your ear:** one finished tone with its ambience, playing the riff only.
- Test signals (sweeps and noise at several levels, for fitting the amp, and the cab if
  it cannot be exported) come with the comparison tool; they are reamped later.

## Acoustic guitar

Record the **pickup and the microphones at the same time**, each on its own track of
the same interface, so they stay sample-aligned.

- **Pickup:** an under-saddle piezo is best (it measures the string at the bridge, what
  the string model makes). With a blend system, the piezo alone, no internal mic.
- **Mic 1:** a small-diaphragm condenser about 30 cm away, aimed where the neck meets
  the body (frets 12 to 14).
- **Mic 2 (optional):** about 30 cm in front of the bridge or the lower bout, or a
  large-diaphragm mic a little further away.
- **The room:** as dry as you can make it (blankets, no hard wall right behind you).
  Mark where the chair, the mics and the guitar are, and do not move between files.
  Watch for chair and clothing noise.
- **Body taps** (`07-body-taps`): damp all strings with the fretting hand, then tap the
  bridge lightly with a fingertip or a pencil's eraser, once a bar. That is close to the
  body's impulse response itself.
- **A nylon-string guitar,** if you have one: the same files, with the fingers only.

## Bass

- **The DI only**, into the Hi-Z / instrument input or a good DI box, like the electric.
  An active bass: a fresh battery, its EQ flat (every boost and cut in the middle).
- **The pickups as you usually play** (both pickups, or the blend in the middle), volume
  and tone fully up, and the same all session. Write down which.
- **With the fingers** unless the tab says pick or thumb: two fingers alternating, the
  usual plucking spot (measure it from the bridge saddle as with the pick). The pick
  files: the same pick throughout.
- **Gain:** the hardest note (ff open E, a slap) peaks between -10 and -6 dBFS.
- **Through an amp modeller** (optional, as for the electric): one clean and one driven
  bass tone, amp only and amp with cab, on `01` (medium notes), `03` and `06`.
- **A 5-string bass:** the tabs in `bass5/` (see the table above). Write its tuning
  in the notes if it is not B standard.
- Leave the slap and pop bars of `04` out if you do not slap.

## Files and notes

One folder per guitar, outside the repository, for example
`~/guitarscore-references/strat-di/` and `~/guitarscore-references/dreadnought/`.
Name each WAV file after its tab, with the signal added:

```
01-single-notes_di.wav
01-single-notes_crunch-amp.wav
01-single-notes_crunch-amp-cab.wav
01-single-notes_pickup.wav
01-single-notes_mic1.wav
```

And a `notes.txt` in each folder:

- the guitar or bass, its pickups (active or passive), string gauge, how old the strings
  are;
- the pick (type, thickness) and the picking spot in cm from the saddle;
- the interface, its input and gain setting;
- for the acoustic: the pickup type, each microphone (model, pattern, distance, where it
  points), a photo of the setup;
- for the modeller: each tone's name and settings (screenshots are fine);
- per file: the bar number of anything that went wrong.

About an hour per guitar, setup included.

## Making the tabs again

The tabs were written with `gscore-cli edit` scripts (one command per line: bars, frets,
ties, effects, dynamics, texts) applied to an empty one-bar song; the bass tabs to the
bass track of `testdata/basic/multi_staff.gp`, cut down to one empty bar and four strings.
The 5-string tabs were made from the 4-string ones (2026-11-26): `set_track_strings
strings=5`, then each added low B bar copied from the E string's bars, fret, length,
tie, palm mute, harmonic and text alike. They can be changed
in GuitarScore like any song; if one is changed, keep the grid (two bars per note, 60
BPM) so recordings made before and after still compare.
