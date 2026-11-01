# Guitar Pro Feature Parity Checklist

This document tracks which Guitar Pro features are implemented in GuitarScore.

## Implemented Features

| Feature | Status |
|--------|--------|
| Basic tablature support | Missing |
| Multiple voices per track | Missing |
| Tempo changes | Missing |
| Time signature changes | Missing |
| Lyrics | Missing |
| Chord diagrams | Missing |
| Custom fonts and styling | Missing |

## In Progress Features

| Feature | Status |
|--------|--------|
| Keymap configuration | Planning |
| Playback engine | Planning |
| Undo/Redo system | Planning |

## Future Milestones

- Save Guitar Pro files (M3)
- Implement basic tablature editing, headless via the CLI (M4), then in the app (M7)
- Add playback with MIDI support (M8)
- Add internal synth support (M9)
- Add standard notation rendering
- Support all GP file formats

## Testing Policy

All imported/exported files must pass round-trip tests and preserve musical content.