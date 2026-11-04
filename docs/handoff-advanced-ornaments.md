# Handoff: advanced ornaments (`testdata/basic/advanced_ornaments.gp`)

Branch `beat-effects` (in the main folder, no worktree). The owner's goal: everything in
`testdata/basic/advanced_ornaments.gp` (Guitar Pro's export: `advanced_ornaments#1.png`,
`.pdf`, `.txt` legend, `.gpx`/`.gp5` copies) must be **read, written, drawn, played and
editable from the palette**, with palette tiles like Guitar Pro's (owner's screenshot:
glyphs on a bare panel; arrows ↑ ↓ ↑~ ↓~, `rasg.`, ⊓ V, `tr.`, tremolo with a corner
triangle, `<` `>` `<>`, left/right hand tap hands, ⑥ string number, dead slapped X, pick
scrape x~). `cargo xtask ci` is green at the commit that added this file.

## Done

Commit 1 (`0b36837`, reviewed by the owner as "good so far"): beat effects drawn and
played, notation articulations/slurs/slides, notation-only lanes (see `docs/devlog.md`,
2026-11-04 "Beat effects and notation articulations").

Commit 2 (this one), data layer only — **nothing new is drawn or played yet**:

- `gscore-gp`: reads/writes GPIF `DeadSlapped`, `Arpeggio` (Up/Down), `Fadding`
  FadeOut/VolumeSwell, beat `<Tremolo>` "1/2|1/4|1/8" as note tremolo picking speed
  (8th/16th/32nd; it was misread as a whammy value and dropped), `LeftFingering` /
  `RightFingering` (P I M A C, left `Open` → new `Fingering::OpenString`), note
  `LeftHandTapped`, `ShowStringNumber`, slide flags 64/128 (`SlideType::PickSlideDown/Up`),
  and the trill: GPIF stores the trill note as a **MIDI pitch**; `convert_trills` turns it
  into a fret on import and back on export (it used to be read as fret 59).
  New fields: `BeatEffects::{arpeggio, dead_slapped, fade_out, volume_swell}`,
  `NoteEffect::{left_hand_tapped, show_string_number}`.
- `gscore-model`: new `techniques.rs` with `Trill {fret: Option<u8>, speed: TrillSpeed}`,
  `TremoloSpeed`, `Hand`, `Finger` (`sign()` gives 0-4/t and p i m a c), `Fade`,
  accessors `BeatView::{arpeggio, is_dead_slapped, has_rasgueado, fade, text}` and
  `NoteView::{trill, tremolo_speed, finger, is_left_hand_tapped, shows_string_number}`;
  `SlideKind::{PickSlideDown, PickSlideUp}`.
  `NoteEffect::Trill(Option<Trill>)`, `TremoloPicking(Option<TremoloSpeed>)` (were
  bools), new `LeftHandTap`, `StringNumber`, `Finger(Hand, Option<Finger>)`;
  `BeatEffect::{Arpeggio, DeadSlapped, Rasgueado, FadeOut, VolumeSwell}` (a fade
  switched on clears the other two). `compare.rs` covers all of them, so a save that
  would lose them (GP3-5 have none of: arpeggio, dead slapped, fade out/swell, fingering
  "open", left-hand tap, string numbers, pick slides) reports the loss.
- `gscore-edit`: command language (`technique_text.rs`): `trill=4:16|auto:32|none`
  (`true`/`false` still read), `tremolo_picking=8|16|32|none`, `finger=left:1`,
  `left_hand_tap`, `string_number`, `arpeggio=up|down|none`, `dead_slapped`,
  `rasgueado`, `fade_out`, `volume_swell`, `slide=pick_slide_down:true`. Copy/paste
  carries them. The palette's trill/tremolo tiles still toggle with defaults.

## To do (in this order; each step: tests, `cargo xtask ci`, commit)

1. **Brush direction is backwards** (my mistake in commit 1): Guitar Pro draws *brush
   down* as an arrow pointing **up** (and `Arpeggio=Down` too; see bar 11 of the PNG).
   Flip `draw_arrow`'s `down` argument in `gscore-layout/src/ornaments/beat.rs`
   (`draw_tab_brush`) and `notation/marks.rs` (`draw_brush`), and swap the two brush
   tile glyphs in `gscore-app/src/app/edit_effects.rs` (`BEAT_ITEMS`). Guitar Pro draws
   the brush on the **tab only**; keep the notation arrow only when the tab is hidden.
2. **Layout**, compare with `advanced_ornaments#1.png` (render: `gscore-cli render FILE
   -o x.svg`, then `rsvg-convert` with the fontconfig from `docs/development.md`):
   - beat text (`BeatView::text`, "Tie beat", "Fermata", ...) in a row above the top
     staff: add a row to `marks::Rows`, like the section row;
   - tab: a tie continuation in a measure's first column is written `(7)`
     (`measure/columns.rs` makes it an empty tie destination; give it text, and let
     `Fret::draw`/`half_width` skip only empty ones);
   - tuplet brackets with `n:m` (bar 4, 11:4): today only a number by a beam is drawn
     (`notation/beams.rs`); `Chord` keeps only `tuplet: Option<u8>` — carry `(n, m)`, put
     rests in the group too, draw a bracket when the group is not one beam group, label
     `n` when m is the largest power of two below n, else `n:m`;
   - dead slapped (bar 5): a large X across each staff instead of the rest (flag in
     `ColumnMarks`, skip the rest item in `columns.rs` and in `notation::beat_item`);
   - pick slide (bar 6): "P.S." above the tab and a wiggle along the string line from
     the note to the next one (factor the wiggle out of `ornaments/notes.rs::draw_vibrato`);
     notation: wiggle from the cross head;
   - left-hand tap (bar 7): circled T above the note (both staves) and a small arc left
     of the fret number on the tab;
   - fingering (bars 8, 9): small text left of the notehead (left: 0-4/t, right: p i m a
     c, italic in GP; `TextFont` has Regular/Bold only), with room in `notation::extent`;
   - string numbers (bar 10): circled digits above (or below) the notes;
   - arpeggio (bar 11, beats 3-4): a wavy vertical line with an arrowhead on both staves;
   - `rasg.` (bar 12) in the beat-effects row (`ornaments/beat.rs::draw_beat_marks`);
   - grace note hammer/pull (bar 14): "H"/"P" and a slur from grace to main note. The
     model's `Grace` has no transition yet: add `hammer: bool` (gp `GraceEffect.transition
     == Hammer`; GPX/GP7 set it, GP5 does not);
   - trill (bar 15): `tr` above the notation staff too, and the trill fret small in
     parentheses after the tab number: `2 (4)`;
   - tremolo speeds (bar 16): 1/2/3 strokes (`tremolo1..3`, U+E220-E222) through the
     stem, and under the tab number (GP), instead of always three;
   - fade out `>` and volume swell `<>` hairpins next to the fade-in one.
3. **Playback** (`gscore-play`, mostly `strokes.rs` / `effects.rs`): arpeggio = spread
   over an eighth in its direction; rasgueado = 3-4 quick re-strikes; dead slapped = short
   muted hits on all strings; pick slide = quick descending/ascending muted notes; trill =
   alternate with the trill fret at its speed (like `tremolo_picking`, which should use
   its speed instead of always 32nds); fade out / swell = expression ramps (see
   `fade_in_events`); left-hand tap and grace hammer = `strokes::legato` (softer). Update
   the crate doc in `lib.rs` (trills).
4. **Palette** (`gscore-app/src/app/edit_effects.rs`, tiles in `tiles.rs`/`tile_faces.rs`,
   actions in `gscore-edit/src/action.rs` + keymap): tiles for dead slapped, pick slide,
   left-hand tap, string number, arpeggio up/down, `rasg.`, fade out, volume swell; trill
   and tremolo tiles with a corner triangle opening a small dialog (trill fret + speed;
   tremolo speed), like the tuplet tile/dialog (`tuplet_dialog.rs`). Fingering has no
   tile in GP; a dialog is optional. Owner's design rules: memory "graphics style" (flat,
   serious, few details), dialogs pick by clicking engraved signs with an explanation.
5. Tests: add `advanced_ornaments` to `crates/gscore-layout/tests/common/mod.rs`
   `TESTDATA` and to the golden list in `test_golden.rs` (owner reviews the SVG), import
   tests for each new GPIF field in `gscore-gp/src/io/gpif_import/effect_tests.rs`, a
   round-trip test (`.gp` → `.gp` without losses; it passes today), playback tests.
6. Devlog entry in `docs/devlog.md`; delete this file when done.

Notes: the owner merges branches; never push or use GitHub. Files over ~400 lines were
flagged by the owner before — split (`measure/effects.rs` is an example).
`gscore-model/src/effects.rs` is already 840 lines: put new code in `techniques.rs`.
