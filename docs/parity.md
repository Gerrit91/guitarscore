# Guitar Pro Feature Parity Checklist

This document tracks which Guitar Pro features are implemented in GuitarScore.
Updated 2026-11-02 (M14: edit palette and effects; M12: app shell; M15: track panel).

## Implemented Features

| Feature | Status |
|--------|--------|
| Read GP3, GP4, GP5, GPX (GP6) and GP7+ files | Live |
| Save GP7+ files safely (`.bak`, atomic replace, re-read check) | Live; track color, mixer volume/pan, mute and solo are kept, and anything else the format cannot keep is reported before writing |
| `info`, ASCII-tab `dump`, `roundtrip-check` | Live |
| Song information, tracks, instruments, tunings, time signatures | Live |
| Track and song grid (bottom panel) | Partial: one row per track with color, instrument, name, mute/solo, volume slider and pan knob; a measure matrix with note squares, measure numbers, section markers, repeats and alternative endings, the playing measure highlighted, and a click/drag selection of measures. Add/delete/duplicate/rename/move tracks by right-click, all undoable |
| Track panel (right panel, M15) | Live: name, instrument kind, General MIDI program (the SoundFont preset), drum kit, color, string count, capo and each string's open pitch, all as commands with undo; sound and tuning changes are heard while playing |
| App shell and settings (M12) | Live: docked panels with remembered sizes and visibility, settings window (song information, audio/MIDI, editor), JSON settings in the config directory, recent files, drag and drop, window position/size, zoom keys, progress and cancel for opening and SoundFont loading, cursor following playback |
| Song information editing | Live, undoable: title, subtitle, artist, album, lyricist, composer, copyright, tab author, instructions, notes and the initial tempo (settings window or `set_song_text`/`set_tempo`) |
| Edit palette (left panel, M14) | Live: note lengths (whole to 64th), dots, tuplets, rests, ties, dead notes, voice 1/2, measure tempo/time/key signatures and section markers, and the effect matrix (bends and whammy with point editors) |
| Selection and clipboard (M14) | Live: a beat range in one track/voice (across bar lines; Shift+arrows, dragging, Shift+Up/Down for the whole beat) or a measure range across tracks; Delete clears the selected notes; copy, cut and paste as one undo step, carrying notes, durations, ties, effects, time signature, tempo and section |
| Note and beat effects (read, dump, save) | Live |
| Measure issues (under/overfilled bars, notes on missing strings) | Live |

## In Progress / Partial

| Feature | Status |
|--------|--------|
| Basic tablature editing | Live: in the app at a cursor (frets with two-digit entry, clear note, shorter/longer, insert/delete beat and measure, Right fills the measure and appends measures, durations/dots/tuplets, rests, ties, dead notes, effects, voice 2) and as headless commands (`gscore-cli edit`) |
| Undo/Redo | Partial: in memory in `gscore-edit`, in the Edit menu and Ctrl+Z/Ctrl+Y; not persisted |
| Cursor and selection | Live: cursor (keys, page click, grid click), beat selection by Shift+arrows or dragging on the page (across bar lines), measure ranges in the grid |
| Copy / cut / paste | Live for beat and measure selections (M14) |
| Multiple voices per track | Partial: read, viewed and edited per voice; no cross-voice editing |
| Tempo changes | Partial: kept in the file; only the initial tempo is exposed and editable commands do not set them |
| Time signature changes | Partial: kept and copied to new measures; no command to change one |
| Lyrics, chord diagrams, fingering | Partial: kept in the file; no accessors or editing |
| Keymap configuration | Live: text file of typed characters / key names to action ids, layout independent; `gscore-cli keymap` |
| Desktop app | Partial (`guitarscore`): open file (dialog, recent files, drag and drop or command line), keyboard editing with undo/redo, save/save as `.gp` (with the unsaved-changes question), track picker, zoom keys, light/dark theme, settings window, play/pause/stop with the built-in synthesizer or to a MIDI port, the cursor following playback, and the track and song grid with live mixing, plus the track panel |
| Playback | Partial: built-in SoundFont synthesizer, MIDI output and `.mid` export: all tracks with their instruments, volume and pan, tempo changes, repeats and alternative endings, ties, dead notes, harmonics, bends and slides (on borrowed channels, so the rest of the chord keeps its pitch), mute and solo. No vibrato, let ring, palm mute, grace notes, trills, whammy bar, D.C./D.S./coda, swing |
| Tablature display | Partial: one track as standard notation above tab, in the app and as SVG (`gscore-cli render`): clef, time signatures, notes with accidentals, stems, flags, beams, dots, rests, ties and tuplet numbers; tab fret numbers with ornaments (slurs, bends, slides, vibrato, harmonics, accents, palm mute, let ring, whammy). No key signatures, second voice in notation, repeats, pages or multi-track systems yet |

## Future Milestones

Planned in `AGENTS.md` section 9, in this order: M16 bundled SoundFonts, M17 layout
polish (whammy, bends, repeats, pages, PDF), M18 playback polish (loops, techniques, WAV
export), M19 release packages. M11, M12, M14 and M15 are done.

## Testing Policy

All imported/exported files must pass round-trip tests and preserve musical content.
Every editing command is covered by apply-then-undo, `panic_hunt`, a property test, and a
save/reload check.
