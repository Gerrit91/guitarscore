# Guitar Pro Feature Parity Checklist

This document tracks which Guitar Pro features are implemented in GuitarScore.
Updated 2026-11-03 (Staff section dialogs, separate repeat/endings icons, swing feel and
fermatas, repeat and double-barline editing; before: drum notation, dynamics, section
names, tempo changes, repeats and endings in the score).

## Implemented Features

| Feature | Status |
|--------|--------|
| Read GP3, GP4, GP5, GPX (GP6) and GP7+ files | Live |
| Save GP7+ files safely (`.bak`, atomic replace, re-read check) | Live; track color, mixer volume/pan, mute and solo are kept, and anything else the format cannot keep is reported before writing |
| `info`, ASCII-tab `dump`, `roundtrip-check` | Live |
| Song information, tracks, instruments, tunings, time signatures | Live |
| Track and song grid (bottom panel) | Partial: one row per track with color, instrument, name, mute/solo, volume slider (always-visible, editable decibel readout), pan knob and a small green activity indicator; a measure matrix with note squares, measure numbers, section markers, repeats and alternative endings, the playing measure highlighted, and a click/drag selection of measures. Add/delete/duplicate/rename/move tracks by right-click, all undoable; a header to switch to the mixer, add a track (with a name/color/symbol popup), move the shown one, mute all and solo all; a master row with mute/solo-all, volume, pan and activity. Clicking a track or a cell during playback switches what the page shows without stopping or seeking (only another measure seeks) |
| Mixer view (M18) | Partial: one strip per track with a logarithmic fader and gain readouts that match the synthesizer (a track tops out at 0 dB; the master is 0 dB at its default and has +4.7 dB of headroom), gain and meter scales, two-segment L/R estimated activity meter (recent note velocities, not measured audio), editable dB level, mute/solo, pan, reverb and chorus send knobs (heard at once, undoable, saved), name, instrument preset and color bar, plus a master strip with mute/solo-all; toggled from the grid header and remembered with its own height |
| Track panel (right panel, M15) | Live: name, instrument kind, General MIDI program (the SoundFont preset), drum kit, color (presets or any), shown staves (notes and tabs, tabs only, notes only), string count, capo and each string's open note by name, all as commands with undo; sound and tuning changes are heard while playing |
| App shell and settings (M12) | Live: docked panels with remembered sizes and visibility, a centered settings window with the app's title bar, tabs Song / Editor / Audio I/O (sound font dropdown, MIDI ports), in-app shortcut editor stored in the settings JSON, JSON settings in the config directory, recent files, drag and drop, window position/size, zoom keys, progress and cancel for opening and SoundFont loading, cursor following playback, reopening the last file at startup (on by default, with an Editor setting; a missing file starts blank) |
| Song information editing | Live, undoable: title, subtitle, artist, album, lyricist, composer, copyright, tab author, instructions, notes and the initial tempo (settings window or `set_song_text`/`set_tempo`) |
| Edit palette (left panel, M14) | Live: note lengths (whole to 64th), dots, tuplets, rests, ties, dead notes, voice 1/2, dynamics (ppp to fff; new notes take the dynamic in effect, copy/paste keeps it), and the effect matrix; a Staff section at the top opens small dialogs for the bar's tempo (note + `=`), time signature (4/4), key signature, section marker, start and end repeats (separate icons), alternative endings (their own icon), swing feel (straight, triplet or dotted), fermatas, double barline, line/page break and bars per line (2026-11-03); the bend and whammy tiles (next to each other) open graph editors with draggable points, built-in presets (Bend, Bend/Release, Pre-Bend, ...; Dip, Dive, Release up, ...) and the user's own saved presets (2026-11-03) |
| Selection and clipboard (M14) | Live: a beat range in one track/voice (across bar lines; Shift+arrows, dragging, Shift+Up/Down for the whole beat) or a measure range across tracks; Delete clears the selected notes; copy, cut and paste as one undo step, carrying notes, durations, ties, effects, time signature, tempo and section |
| Note and beat effects (read, dump, save) | Live |
| Measure issues (under/overfilled bars, notes on missing strings) | Live |

## In Progress / Partial

| Feature | Status |
|--------|--------|
| Basic tablature editing | Live: in the app at a cursor (frets with two-digit entry, clear note, shorter/longer, insert/delete beat and measure, Right fills the measure and appends measures, durations/dots/tuplets, rests, ties, dead notes, effects, voice 2) and as headless commands (`gscore-cli edit`) |
| Undo/Redo | Partial: in memory in `gscore-edit`, in the Edit menu and Ctrl+Z/Ctrl+Y, and in the History window (newest first, with UTC time, command and arguments; a table to undo/redo any change); not persisted across a normal restart |
| Cursor and selection | Live: cursor (keys, page click, grid click), beat selection by Shift+arrows or dragging on the page (across bar lines), measure ranges in the grid |
| Copy / cut / paste | Live for beat and measure selections (M14) |
| Multiple voices per track | Partial: read, viewed and edited per voice; no cross-voice editing |
| Tempo changes | Partial: per measure (`set_measure_tempo`, palette) and as read from GP3-5 beats; written in the score where the tempo changes. No gradual tempo changes (accelerando, ritardando) |
| Dynamics | Live: ppp to fff per beat (`set_dynamic`, palette), played as note velocity, written under the notation (or the tab) where they change; mf, Guitar Pro's default, is not written. No hairpins (crescendo/diminuendo) |
| Drum notation | Live: drum tracks on a five-line percussion staff (percussion clef, each General MIDI drum sound in its usual place with its notehead: crosses for hi-hat and cymbals, circled crosses for open hi-hat and side stick, ...), stems up; entered as MIDI note numbers on the kit's "strings". No drum tab, no custom drum maps, no second (feet) voice |
| Time signature changes | Live: per measure from the Staff dialog and `set_time_signature`, in every track; kept and copied to new measures |
| Repeat signs, alternative endings and double barlines | Live: edited from the Staff dialogs and `set_repeat` / `set_double_bar`; drawn and played (repeats and endings); no D.C./D.S./coda |
| Feel (swing, triplet, dotted) | Live: per measure from the Staff dialog and `set_feel`; playback and `.mid` export swing eighth/sixteenth notes (two-thirds/one-third) or dotted pairs (three-quarters/one-quarter); GP7/GPX keep all five values, GP3-5 the eighth/sixteenth triplet feel |
| Fermatas | Partial: one per measure from the Staff dialog and `set_fermata`, drawn above the staff; GP7/GPX keep it, GP3-5 do not (reported by the save check); playback ignores it |
| Lyrics, chord diagrams, fingering | Partial: kept in the file; no accessors or editing |
| Keymap configuration | Live: a table in Settings → Editor for every action (typed characters / key names, layout independent), key capture with conflict replacement, per-row revert and reset-all, stored in the settings; `gscore-cli keymap` still lists ids |
| Desktop app | Partial (`guitarscore`): new song, open file (dialog, recent files, drag and drop or command line), keyboard editing with undo/redo and the History window, entered notes heard at once, save/save as `.gp` (with the unsaved-changes question), track picker, zoom keys, light/dark theme, settings window, play/pause/stop/navigation with the built-in synthesizer or to a MIDI port, the cursor following playback, and the track and song grid with live mixing, plus the track panel; right-click menus on the score and Track/Bar/Note menus in the menu bar (insert/delete bars, add/duplicate/move/delete tracks, techniques with their keys); opens maximized (full screen if closed so), F11 full screen (2026-11-03) |
| Playback | Partial: built-in SoundFont synthesizer, MIDI output and `.mid` export: all tracks with their instruments, volume, pan, reverb and chorus, tempo changes, repeats and alternative endings, swing/triplet, dotted and Scottish feel, ties, dead notes, harmonics, bends and slides (on borrowed channels, so the rest of the chord keeps its pitch), mute and solo; metronome clicks (accented on the first beat of each measure) and a one-bar count-in, both toggled in the player, with the readout already showing where the song will start; rewind/forward one bar, jump to the top/end, and looping of the selected measures; position and total length ("00:00 / 03:45") that follow a measure selection. vibrato, ghost notes, accents, staccato, palm mute (shortened), let ring, tremolo picking and brush strokes; a palette effect or dynamic plays its beat. Grace notes play as short notes (about 75 ms) just before their beat, or on it as appoggiaturas, and stop the note still ringing on their string. No trills, whammy bar, fade-in, fermatas, D.C./D.S./coda, or sample-accurate loop boundaries |
| Tablature display | Partial: one track as standard notation above tab, in the app and as SVG (`gscore-cli render`): clef, key signatures (at every system start and at key changes, on the notation staff or on the tab staff when notation is hidden), time signatures, notes with key-aware accidentals, stems, flags, beams, dots, rests, ties (also across barlines), tuplet numbers, grace notes (cue-sized with their own accidentals and a slashed flag, smaller fret numbers on the tab, kept clear of the main note) and two or more voices per staff (stems up/down); tab fret numbers with ornaments (slurs, bends, slides, vibrato, harmonics, accents, palm mute, let ring, whammy); section names (framed), tempo changes, repeat signs with counts, alternative-ending brackets, fermatas and dynamics; drum tracks on a percussion staff. On pages or one endless page (View menu; pages side by side when zoomed out), with the song's page setup (paper, orientation, margins, score size; Settings › Page layout, read from and saved to GP7's style sheet), page numbers, copyright and an optional running header, lines spread down full pages, line and page breaks per bar and a bars-per-line limit (Bar and Track menus; GP7 line breaks kept), and PDF export with embedded fonts (File › Export PDF…, `gscore-cli render -o x.pdf`) (2026-11-03). No D.C./D.S./coda, native printing or multi-track systems yet |

## Future Milestones

Planned in `AGENTS.md` section 9, in this order: M16 app design, M17 layout
polish (whammy, bends, repeats, pages, PDF), M18 playback polish (first slice live:
time readout, metronome, count-in, live track switching; still loops, techniques and WAV
export), M19 bundled SoundFonts, M20 release packages, and M21 pages, multi-track scores and
print (owner, 2026-11-03: next after M17, before the rest of M18). M11, M12, M14 and M15 are done.

## Testing Policy

All imported/exported files must pass round-trip tests and preserve musical content.
Every editing command is covered by apply-then-undo, `panic_hunt`, a property test, and a
save/reload check.
