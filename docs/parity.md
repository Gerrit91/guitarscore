# Guitar Pro Feature Parity Checklist

This document tracks which Guitar Pro features are implemented in GuitarScore.
Updated 2026-11-02 (M13: track and song grid).

## Implemented Features

| Feature | Status |
|--------|--------|
| Read GP3, GP4, GP5, GPX (GP6) and GP7+ files | Live |
| Save GP7+ files safely (`.bak`, atomic replace, re-read check) | Live |
| `info`, ASCII-tab `dump`, `roundtrip-check` | Live |
| Song information, tracks, instruments, tunings, time signatures | Live |
| Track and song grid (bottom panel) | Partial: one row per track with color, instrument, name, mute/solo, volume slider and pan knob; a measure matrix with note squares, measure numbers, section markers, repeats and alternative endings, the playing measure highlighted, and a click/drag selection of measures. Add/delete/duplicate/rename/move tracks by right-click, all undoable |
| Note and beat effects (read, dump, save) | Live |
| Measure issues (under/overfilled bars, notes on missing strings) | Live |

## In Progress / Partial

| Feature | Status |
|--------|--------|
| Basic tablature editing | Partial: in the app at a cursor (frets with two-digit entry, clear note, shorter/longer, insert/delete beat and measure, Right fills the measure and appends measures) and as headless commands (`gscore-cli edit`) |
| Undo/Redo | Partial: in memory in `gscore-edit`, in the Edit menu and Ctrl+Z/Ctrl+Y; not persisted |
| Cursor and selection | Partial: cursor (keys, a click on the page, a click in the song grid); a measure range can be selected in the grid; no beat selection and no selection in the score yet |
| Copy / cut / paste | Missing (needs selection); the keys report "not implemented yet" |
| Multiple voices per track | Partial: read, viewed and edited per voice; no cross-voice editing |
| Tempo changes | Partial: kept in the file; only the initial tempo is exposed and editable commands do not set them |
| Time signature changes | Partial: kept and copied to new measures; no command to change one |
| Lyrics, chord diagrams, fingering | Partial: kept in the file; no accessors or editing |
| Keymap configuration | Live: text file of typed characters / key names to action ids, layout independent; `gscore-cli keymap` |
| Desktop app | Partial (`guitarscore`): open file (dialog or command line), keyboard editing with undo/redo, save/save as `.gp` (with the unsaved-changes question), track picker, zoom, light/dark theme, play/pause/stop with the built-in synthesizer or to a MIDI port, and the track and song grid with live mixing |
| Playback | Partial: built-in SoundFont synthesizer, MIDI output and `.mid` export: all tracks with their instruments, volume and pan, tempo changes, repeats and alternative endings, ties, dead notes, harmonics, bends and slides (on borrowed channels, so the rest of the chord keeps its pitch), mute and solo. No vibrato, let ring, palm mute, grace notes, trills, whammy bar, D.C./D.S./coda, swing |
| Tablature display | Partial: one track as standard notation above tab, in the app and as SVG (`gscore-cli render`): clef, time signatures, notes with accidentals, stems, flags, beams, dots, rests, ties and tuplet numbers; tab fret numbers with ornaments (slurs, bends, slides, vibrato, harmonics, accents, palm mute, let ring, whammy). No key signatures, second voice in notation, repeats, pages or multi-track systems yet |

## Future Milestones

Planned in `AGENTS.md` section 9, in this order: M11 recovery journal and autosave, M12
app shell and styling
(panels, progress bar, settings window, play from the selected note, cursor follows
playback), M14 edit palette (durations and effects), M15 track
panel, M16 bundled SoundFonts, M17 layout polish (whammy, bends, repeats, pages, PDF), M18
playback polish (loops, techniques, WAV export), M19 release packages.

## Testing Policy

All imported/exported files must pass round-trip tests and preserve musical content.
Every editing command is covered by apply-then-undo, `panic_hunt`, a property test, and a
save/reload check.
