# Guitar Pro Feature Parity Checklist

This document tracks which Guitar Pro features are implemented in GuitarScore.
Updated 2026-11-01 (M7: bends and slides with channel borrowing).

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
| Desktop app | Partial: read-only viewer (`guitarscore`): open file (dialog or command line), track picker, zoom, light/dark theme, play/pause/stop to a MIDI port |
| Playback | Partial: MIDI output and `.mid` export: all tracks with their instruments, volume and pan, tempo changes, repeats and alternative endings, ties, dead notes, harmonics, bends and slides (on borrowed channels, so the rest of the chord keeps its pitch). No vibrato, let ring, palm mute, grace notes, trills, whammy bar, D.C./D.S./coda, swing, mute/solo |
| Tablature display | Partial: one track as standard notation above tab, in the app and as SVG (`gscore-cli render`): clef, time signatures, notes with accidentals, stems, flags, beams, dots, rests, ties and tuplet numbers; tab fret numbers with ornaments (slurs, bends, slides, vibrato, harmonics, accents, palm mute, let ring, whammy). No key signatures, second voice in notation, repeats, pages or multi-track systems yet |

## Future Milestones

- Tablature layout: effects, rhythm under the tab, pages (rest of M5)
- Read-only egui app: view state persistence, recent files (rest of M6)
- Playback: remaining techniques (vibrato, let ring, palm mute, grace notes, trills, whammy), playhead on the page, mute/solo (rest of M7)
- Interactive editing, cursor, keymap and recovery journal (M8)
- Internal synth (M9)
- Standard notation, more techniques, GP3/4/5 writing

## Testing Policy

All imported/exported files must pass round-trip tests and preserve musical content.
Every editing command is covered by apply-then-undo, `panic_hunt`, a property test, and a
save/reload check.
