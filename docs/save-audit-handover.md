# Handover: what a GP7 save still loses

Branch `gp-save-audit` (2026-11-24). The goal is the owner's: **a save must not lose
information**. This document says how losses are found, what has been fixed, and what is
left, with a suggested approach for each. Read `AGENTS.md` first; everything there holds
(definition of done `cargo xtask ci`, a devlog entry per change, commit on this branch, never
push or open PRs, check the branch right before each commit).

## How losses are found

- `gscore-cli audit FILE|FOLDER... [--check] [--verbose]` reads each `.gp` file, writes it
  as GP7 again in memory and compares the two `score.gpif` files: every element, property
  (`Property name`, `XProperty id`), automation type, beat dynamic value and lyrics line with
  text is counted, keyed by where it sits (`Note Property Muted`, `Beat <Hairpin>`,
  `Automation type Tempo`, ...). What the saved file has fewer of is a loss. Code:
  `gscore_gp::gpif_fingerprint` (`crates/gscore-gp/src/io/gpif/fingerprint.rs`),
  `gscore_io::audit` (`crates/gscore-io/src/audit.rs`), `crates/gscore-cli/src/audit.rs`.
- Every key a save may lose has a **verdict** in `VERDICTS` in `crates/gscore-io/src/audit.rs`:
  `Rebuilt` (Guitar Pro works it out itself: a note's pitch copies, stem directions, default
  pickup values, drum notation tables, empty lyrics lines, tuning and capo written on the track
  as Guitar Pro 6 does, ...) or `Known` (a real loss, still to fix). A key without a verdict is
  a **new loss**: `--check` fails, `cargo xtask corpus` fails, and the test
  `saving_the_test_files_loses_nothing_unexplained` (`crates/gscore-io/tests/test_testdata.rs`)
  fails if a test file shows one.
- **When a loss is fixed, delete its verdict.** The key then counts as new if it ever comes
  back. A verdict is never a way to silence a loss: it says why, precisely (narrow the reason
  when only an edge case is left, e.g. `Beat <Variation>`).
- `cargo xtask corpus` runs the check over `testdata/` and `corpus/` (gitignored; the owner may
  put files there). For the owner's band files use paths:
  `gscore-cli audit testdata ~/svn/wyw/main/tabs ~/Downloads`. Those files are the owner's and
  read-only: never copy them into the repo, tests or goldens. `~/svn/bigfive` holds other bands'
  songs: never use them in the repo at all.
- The audit prints an example file for a new loss only. To find which files lose a known key,
  audit them one by one (a shell loop over the files), then compare the XML before and after
  for that file (`gscore-cli convert FILE -o out.gp --allow-losses`, then unzip both
  `Content/score.gpif`; our writer puts the XML on one line, so compare parsed elements, not
  lines).
- Only GP7 is audited (GP7 in, GP7 out). GP3-5 and GPX input are not covered.

## What has been fixed (this branch, oldest first)

| Commit subject | What was lost |
|---|---|
| Save audit: gscore-cli audit and cargo xtask corpus | (the tool) |
| dead notes as Muted, hammer-on targets, bend and harmonic flags, every dynamic | dead notes written as `Dead` (Guitar Pro reads `Muted`), `HopoDestination`, `Bended`, `Harmonic`, dynamics only where they changed |
| tapping is written on the notes | tapping (a note property in Guitar Pro; slap and pop are beat properties) |
| a section keeps its letter | `MasterBar <Section><Letter>` |
| hairpins, wah, variations, a held or flat whammy and a rasgueado's pattern are kept | `<Hairpin>`, `<Wah>`, `<Variation>`, `<WhammyExtend/>`, a flat `<Whammy>`, `<Rasgueado>pattern` |
| tempo changes inside a bar are kept on their beats | tempo automations at a position inside a bar (a written ritardando) |
| a rest keeps the dynamic it was read with | a rest's `<Dynamic>` |
| typed lyrics on other tracks (backing vocals) are kept on their track | the typed lyric lines of every track but the song's lyric track |
| a track's sound changes are kept on their bars | `Automation type Sound` after bar 0 (kept on each bar's measure and written again, bar 0 too) |
| Guitar Pro's playback settings, mix and templates are kept | the per-track playback settings, the master effect chain (bypass, parameters, bypassed automations, automation points), GP8's audio tracks, the score views, the page header and footer templates, the words-and-music credit, the systems layouts (empty ones too), the channel strip's bypassed automations and the tempo's name |
| the staff block and the last small losses are kept | `<Staves><Staff>` for every track (tuning, capo and fret count from the model; partial capo, flats, instrument, label visibility, name and the chord and diagram libraries as written), a note's own `<XProperties>` and tie origin, a zero `Bended` marker, a fermata's length, a tempo at a bar's very end, and a grace beat with a `<Variation>` (no longer folded) |

Earlier on `main`: slap and pop as beat properties (`2bfebad`).

## Two ways to fix a loss

1. **Model it** when GuitarScore uses or edits it (the tempo inside a bar went on the beat's
   mix table change, which playback already plays).
2. **Keep it as written** when nothing uses it yet: the reader stores the raw value next to
   what it belongs to, the writer puts it back. Examples: `GpifBeatKept` on
   `gscore_gp::model::Beat` (`crates/gscore-gp/src/model/beat.rs`), `Marker::letter`,
   `Track::gpif_lyrics`, `Track::gpif_sounds` (a whole `<Sounds>` element kept verbatim by
   `capture_track_raw` in `crates/gscore-gp/src/io/gpif/mod.rs`).
   - Keep it on the object it belongs to (beat, note, track, bar header), so edits that copy,
     move or delete that object carry it along.
   - **Beware of data that points at bars, beats or tracks by index** (automations by bar
     number, score views by track): kept verbatim it goes stale when bars or tracks are
     inserted, deleted or moved. Model those, or attach them to the bar header or track.

Add a round-trip test for each fix (the existing ones are in
`crates/gscore-gp/src/io/gpif_export/repeat_tests.rs`). `gscore-gp` is a core crate: tests must
not use `std::fs` (the layers check fails); embed test files with
`include_bytes!("../../../../../testdata/basic/....gp")`. If the test files hold the feature, the
audit test guards it too once the verdict is gone.

**Anything that changes the structure of what is written must be checked in Guitar Pro by the
owner** (GP7 on Windows; files go through `~/Dropbox`; one file per fresh Guitar Pro start).
Our reader reading it back proves nothing about Guitar Pro.

## What is left

`gscore-cli audit` now reports **no loss at all** over the 59 GP7 files in
`~/svn/wyw/main/tabs` and over `testdata/`: 0 unexplained and 0 known (the 64 kinds in the
first handover are gone). `cargo xtask corpus` (and the test over `testdata/`) fails again
as soon as a new file shows a key without a verdict, which is the intended guard.

What is left to do is not a loss but a check in Guitar Pro by the owner, because these
changes alter the written structure (GP7 on Windows; files through `~/Dropbox`; one file
per fresh Guitar Pro start):

- the `<Staves><Staff>` block is now written for every track, a one-staff guitar included,
  beside the track-level properties Guitar Pro 6 used (does Guitar Pro 7 read the staff
  block and ignore the track-level one where both exist?).
- the staff's chord and diagram libraries are kept as written, while the diagrams beats
  use are still numbered in the track-level `<DiagramCollection>`; Guitar Pro 7 may resolve
  a beat's `<Chord>` id in the staff library. If it shows the wrong or no diagram on a
  chord-heavy file, match the ids (the diagrams are identical values, so a match by value
  is possible) rather than writing the library twice.
- a tempo automation at the very end of a bar (position 1) is written back beside the next
  bar's own tempo. Guitar Pro may show both; the next bar's is the one heard.
- `GPIF <ScoreViews>` is kept verbatim, so its ids can go stale when tracks are deleted;
  mapping it to GuitarScore's own views is still the better fix.

## Useful facts found on the way

- Guitar Pro 7 property placement: `Slapped`, `Popped` are beat properties; `Tapped` is a note
  property; a dead note is `Muted`, a palm mute `PalmMuted`.
- Guitar Pro writes a `<Dynamic>` on every beat, rests included, and an empty `<Letter>` on
  sections without a letter.
- Guitar Pro shares identical beats between voices by id; we write each beat out, so our counts
  of beats are higher. That is not a loss.
- A tempo automation's `<Position>` is a fraction of the bar; 1.0 is the bar's end.
