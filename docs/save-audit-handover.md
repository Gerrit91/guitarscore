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

## What is left (64 kinds over the owner's 69 files)

Ordered by what matters most. Counts are "lost occurrences in N files".

### 1. A track's sound changes during the song (`Automation type Sound`, 561 in 69)

549 of them are each track's sound at bar 0, which the kept `<Sounds>` already holds (the
track `<Automations>` element is not written at all). **12 are real sound changes later in a
song**: lost. They point at bars by number, so keeping them verbatim goes stale on bar edits.
Suggested: keep a track's sound automations per bar (on the bar's measure of that track, or on
the bar header with the track index) as `(position, value)` and write `<Track><Automations>`
again (bar 0 too, as Guitar Pro writes it). Then the verdict can go.

### 2. The staff block (`Staff ...`, about 20 keys, in every file)

Our writer writes tuning and capo as track-level properties (Guitar Pro 6 style); Guitar Pro 7
writes `<Staves><Staff><Properties>` with: `Tuning` (plus `<Flat>`, `<Instrument>`, `<Label>`,
`<LabelVisible>`), `CapoFret`, `FretCount`, `PartialCapoFret`, `PartialCapoStringFlags`,
`TuningFlat`, `<Name>` (the tuning's name, e.g. "Standard"), `ChordCollection`,
`ChordWorkingSet`, `DiagramCollection`, `DiagramWorkingSet` (the track's chord diagram library:
`<Item>`, `<Chord>`, `<KeyNote>`, `<BassNote>`, `<Diagram>`, `<Position>`, `<Fingering>`,
`<Degree>`, `ShowName`, `ShowDiagram`, `ShowFingering`; in 3 files).

- The model already reads the fret count (`TrackView::fret_count`, from `Staff Property
  FretCount`): write it back.
- Tuning and capo are edited in GuitarScore, so write them from the model; keep the rest
  (label, flats, instrument, name, partial capo) as written per staff.
- The chord library: beats refer to chords by id (`<Chord>` on a beat). Our writer writes the
  diagrams beats use (`track_diagrams` in `gpif_export/mod.rs`); a library entry no beat uses is
  lost. Check how ids line up before keeping the library.
- This changes the written structure: **Guitar Pro check needed** (does Guitar Pro 7 read the
  staff block we write, and still the track-level properties we wrote so far?).

### 3. Guitar Pro's per-track playback settings (`Track <...>`, 7 keys, in every file)

`<PalmMute>`, `<AutoAccentuation>`, `<AutoBrush>`, `<PlayingStyle>`, `<UseOneChannelPerString>`,
`<IconId>`, `<ForcedSound>`. GuitarScore does not use them. They are track-level and point at
nothing: keep each verbatim on the track (as `gpif_sounds` is; `capture_track_raw` can capture
more elements) and write it back.

### 4. Song-level templates and Guitar Pro's own mix

- `Score <PageHeader>`, `<PageFooter>`, `<FirstPageHeader>`, `<FirstPageFooter>`,
  `<WordsAndMusic>` (in every file): the page header and footer templates. Keep verbatim on the
  song; check how the page setup is already read and written (`page_setup`, `stylesheet` on
  `gscore_gp::model::Song`).
- `RSE <Effect>`, `<ByPass>`, `<Parameters>` (every file), `<ByPassedAutomations>` (18 files):
  Guitar Pro's master effects. Only `I01_VolumeAndPan` is read (as the master volume and pan);
  keep the rest verbatim and write it beside it.
- `GPIF <AudioTracks>` (63 files): Guitar Pro 8's audio track list, mostly empty. Keep verbatim.
- `GPIF <ScoreViews>` / `<ScoreView>` (6 files): Guitar Pro's score views. They refer to
  tracks; GuitarScore has its own score views (kept in the song's notes). Look at how they
  relate before deciding (mapping them to GuitarScore's views would be best).
- `Track <SystemsLayout>`, `<SystemsDefautLayout>` (12 files), `Score <ScoreSystemsLayout>`
  (8 files): bars per line. The writer writes them, so some cases are lost: trace which.

### 5. The small rest (1-2 files each)

- `Note <Tie>` (7 in 2): a tie written as a `<Tie>` element; check how the reader reads ties.
- `MasterBar <Length>` (2 in 2): a bar's own length.
- `Automation <Text>` (2 in 2): an automation's text label.
- `Note XProperty 688062467` with `<Int>`, `<XProperties>` (2 in 2): Guitar Pro's extra note
  properties; beats already keep theirs (`Beat::x_properties`), notes do not.
- `Note <Float>` (7 in 1), and one bend's shape (`Bended`, `Bend*` in 1 file): trace.
- `Beat <Variation>` (2 in 1): on grace-note beats, which the reader folds into the next note
  (`fold_grace_beats`), so the beat's kept elements have nowhere to go.
- `Automation type Tempo` (1 in 1): a tempo change at the very end of a bar (position 1)
  when the next bar starts with its own: the same moment, only one is heard. Arguably no loss.

## Useful facts found on the way

- Guitar Pro 7 property placement: `Slapped`, `Popped` are beat properties; `Tapped` is a note
  property; a dead note is `Muted`, a palm mute `PalmMuted`.
- Guitar Pro writes a `<Dynamic>` on every beat, rests included, and an empty `<Letter>` on
  sections without a letter.
- Guitar Pro shares identical beats between voices by id; we write each beat out, so our counts
  of beats are higher. That is not a loss.
- A tempo automation's `<Position>` is a fraction of the bar; 1.0 is the bar's end.
