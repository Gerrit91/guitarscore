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
| `.txt` | Guitar Pro's ASCII tab export | reference for `gscore-cli dump` (M2) |
| `.mid` | Guitar Pro's MIDI export | reference for playback / `.mid` export (M8) |
| `.pdf`, `.png`, `user_screenshots/` | how Guitar Pro renders it | visual reference for layout (M5) |

The expected `gscore-cli info` output is in `crates/gscore-cli/tests/golden/`, not here, so
this directory only holds files made with Guitar Pro.
