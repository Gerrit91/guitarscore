# Guitar Pro Feature Parity Checklist

This document tracks which Guitar Pro features are implemented in GuitarScore.
Updated 2026-11-01 (M6: first read-only app).

## Implemented Features

| Feature | Status |
|--------|--------|
| Read GP3, GP4, GP5, GPX (GP6) and GP7+ files | Live |
| Save GP7+ files safely (`.bak`, atomic replace, re-read check) | Live |
| `info`, ASCII-tab `dump`, `roundtrip-check` | Live |
| Song information, tracks, instruments, tunings, time signatures | Live |
| Note and beat effects (read, dump, save) | Live |
| Measure issues (under/overfilled bars, notes on missing strings) | Live |

## In Progress / Partial

| Feature | Status |
|--------|--------|
| Basic tablature editing | Partial: headless commands (`set_fret`, `set_duration`, insert/delete beat and measure) |
| Undo/Redo | Partial: in memory in `gscore-edit`; not persisted, no UI yet |
| Cursor and selection | Planned (M8) |
| Copy / cut / paste | Planned (M8, with selection) |
| Multiple voices per track | Partial: read, viewed and edited per voice; no cross-voice editing |
| Tempo changes | Partial: kept in the file; only the initial tempo is exposed and editable commands do not set them |
| Time signature changes | Partial: kept and copied to new measures; no command to change one |
| Lyrics, chord diagrams, fingering | Partial: kept in the file; no accessors or editing |
| Keymap configuration | Planned (M8) |
| Desktop app | Partial: read-only viewer (`guitarscore`): open file (dialog or command line), track picker, zoom, light/dark theme |
| Tablature display | Partial: one track's tab staff in the app and as SVG (`gscore-cli render`): title block, tuning, tempo, tab clef, time signatures, fret numbers (ties, dead notes), rests, measure numbers, line breaking and justification, light/dark theme. No effects, rhythm, repeats, pages or standard notation yet |

## Future Milestones

- Tablature layout: effects, rhythm under the tab, pages (rest of M5)
- Read-only egui app: view state persistence, recent files (rest of M6)
- MIDI output and `.mid` export (M7)
- Interactive editing, cursor, keymap and recovery journal (M8)
- Internal synth (M9)
- Standard notation, more techniques, GP3/4/5 writing

## Testing Policy

All imported/exported files must pass round-trip tests and preserve musical content.
Every editing command is covered by apply-then-undo, `panic_hunt`, a property test, and a
save/reload check.
