# Basic test files

Made by the owner in Guitar Pro and exported to every format it offers. Do not edit them;
re-export from Guitar Pro instead. Expected values in tests come from these files.

## `steel_guitar_dadgad.*`

One measure of 4/4 on a 6-string steel guitar tuned DADGAD (D2 A2 D3 G3 A3 D4): the six
open strings as eighths from low to high, then a quarter rest. All song information fields
are filled in (notes: "Used for testing", instructions empty).

| File | Content | Used by |
|---|---|---|
| `.gp` | Guitar Pro 7+ | import tests (`crates/gscore-io/tests/test_testdata.rs`), `info` reference |
| `.gpx` | Guitar Pro 6 | same |
| `.gp5` | Guitar Pro 5 (Guitar Pro drops the notes and the instrument name) | same |
| `.txt` | Guitar Pro's ASCII tab export | layout reference for `gscore-cli dump` (M2) |
| `.mid` | Guitar Pro's MIDI export | reference for playback / `.mid` export (M8) |
| `.pdf`, `.png`, `user_screenshots/` | how Guitar Pro renders it | visual reference for layout (M5) |

The expected `gscore-cli info` output is in `crates/gscore-cli/tests/golden/`, not here, so
this directory only holds files made with Guitar Pro.

## `note_lengths.*`

Nine measures of 4/4 on a 6-string guitar in E standard, one length per measure: a whole
note; a half and two quarters; eighths down to 64ths; a whole rest; rests of the same
lengths; a 3-tuplet and a 5-tuplet; a 6-tuplet and a 7-tuplet; and 3/8, 7/16 (a
double-dotted quarter) and 3/16. All song information fields are filled in.

Guitar Pro 5 cannot store double dots. Its `.gp5` export writes the 7/16 note as 3/8,
leaving measure 9 a sixteenth short; the file reports that as an issue and
`gscore-cli info` shows it.

## `ornaments.*`

Twelve measures of 4/4 on the same guitar, one ornament per measure: tied note, dead
note, hammer-on and pull-off, bend, slides, vibratos, ghost note/accent/staccato,
let ring, dive bomb (whammy bar), legato, natural and pinch harmonics, and palm mute.
All song information fields are filled in.

`.txt`, `.mid`, `.pdf`, `.png` are Guitar Pro's exports, as for `steel_guitar_dadgad`.
`.txt` is only a layout reference for `gscore-cli dump`: the dump follows the usual
Wikipedia ASCII-tab symbols (`h`/`p`, `b`/`r` with the target fret, `/` and `\`, `~`,
`tr`, `t`, `TP`, `*`, pinch harmonic nodes as `7[19]`, `(fret)` for ghosts, `L` for a
tie's origin) and puts palm mutes and let ring on rows under the staff (`PM----|`,
`LR----|`) instead of using Guitar Pro's letters (`g`, `s`, `M`, `+`, `=`). The rows are
independent, so a system that uses both gets both lines. `gscore-cli legend` lists every
symbol.

