# Guitar Pro reader/writer: origin and changes

GuitarScore reads and writes Guitar Pro files with code derived from the
[`guitarpro`](https://github.com/slundi/guitarpro) crate by **slundi**, MIT licensed.
Thank you!

- **Upstream commit:** `b62be7c5e4bedb0c6eb283ac40aa8371adab5c2a` (version 0.4.4)
- **License:** MIT; the notice is kept next to the code as
  `crates/gscore-gp/LICENSE-guitarpro`, and listed in `THIRD_PARTY.md`.

## Where the code lives

The `gscore-gp` crate (lowest layer):

| Module | Contents |
|---|---|
| `src/model/` | `Song`, `Track`, `Measure`, `Beat`, `Note`, effects, enums, `MidiChannel`: structs, enums, defaults, small helpers (public) |
| `src/io/` | GP3/GP4/GP5 binary codec (`Song*Ops` traits), GPIF XML import/export, GPX (BCFZ/BCFS) and GP7 (zip) containers, byte primitives, `GpError` (private; entry points are `Song::read_*` and `Song::write*`) |

Only `gscore-model` (which wraps `Song`) and `gscore-io` (format detection, size limits,
`catch_unwind`) may depend on it (`cargo xtask layers`).

Not taken over: upstream's CLI, web server, benches, test files, and the library modules
GuitarScore does not use (`analysis`, `convert`, the optimized/MusicXML/MNX/MuseScore
models; about 21k of 32k lines).

## History

1. 2026-11-01: vendored unchanged as `vendor/guitarpro` to fix the GP7 tuning bug.
2. 2026-11-01: fixes and additions below (changes 1-13).
3. 2026-11-01: unused modules removed; briefly split into `gscore-model/src/gp` (data) and
   `gscore-io/src/gp` (reading/writing), then, by owner decision, kept together as the
   crate `gscore-gp` (data in `src/model`, reading/writing in `src/io`). Moved to Rust
   edition 2024 with the workspace. The crate is still exempt from the workspace's no-panic
   lints (crate-level `allow`); see `docs/gaps.md`.

## Changes to upstream

Listed in order; paths refer to upstream's layout (`src/io/...`, `src/model/legacy/...`).
Code comments mark changed spots with "GuitarScore". Each fix could be offered upstream
(the owner handles any contact with upstream).

1. `src/lib.rs`: removed `mod tests;` and `mod test_audit;` (files not vendored).
2. `src/io/gpif_import.rs`, GP6/GP7 track import: reset `track.strings` before reading the
   tuning. `..Default::default()` filled it with E-standard 6-string tuning, so the
   "no tuning yet" checks never fired and every GP7 track came out in E standard.
3. `src/io/gpif_import.rs`, `extract_tuning`: reverse the pitch list. GPIF lists pitches
   from the lowest string up; the legacy model's string 1 is the highest.
4. `src/io/gpif_export.rs`: write the pitch list lowest string first (mirror of 3).
5. `src/model/legacy/track.rs`: new field `Track::has_tuning` (default `true`). Set to
   `false` by the GPIF importer when the file has no tuning (GP7 vocal/piano tracks); the
   placeholder E-standard strings stay because code indexes `strings`. The GPIF exporter
   writes a `Tuning` property only when `has_tuning` is true.
   `src/convert/legacy/mod.rs` sets it to `true` (unchanged behaviour).
6. `src/io/gpif.rs`, `src/io/gpif_import.rs`, `src/model/legacy/track.rs`: read the GP7
   `<InstrumentSet>` (type and name) into new fields `Track::instrument_type` and
   `Track::instrument_name`; tracks of type `drumKit` are marked `percussion_track`.
   Not written back by the exporter yet. `src/convert/legacy/mod.rs` sets both to `None`.
7. `src/io/gpif.rs`, `src/io/gpif_import.rs`, `src/model/legacy/track.rs`: read the
   tuning `<Label>` into new field `Track::tuning_label` (non-empty only). Not written back
   yet. `src/convert/legacy/mod.rs` sets it to `None`.
8. `src/io/gpif_import.rs`, `src/io/gpif_export.rs`: score metadata uses the same fields as
   the GP3-5 reader: `Tabber` <-> `writer` (was: import wrote the composer into `writer`
   and the tabber into `transcriber`; export read `transcriber`), `Instructions` <->
   `instructions` (was `comments`). `WordsAndMusic` (new `Score::words_and_music`) fills
   empty `words`/`author`.
9. `src/io/gpif_import.rs` (`flip_note_strings`), `src/io/gpif_export.rs`: note string
   numbers are converted between GPIF order (string 1 = lowest) and the legacy order
   (string 1 = highest) on import and export. Upstream stored GPIF order unchanged, which
   only matched its equally reversed tunings (change 3). Notes outside the track's strings
   stay unchanged.
10. `src/io/gpif_import.rs`: regression tests `tuning_tests` for 2–9.
11. `src/io/gpx.rs`: memory limits for untrusted input (`MAX_UNPACKED_BYTES`, 128 MiB):
    bounded reads of `score.gpif` and embedded audio from GP7 zips (zip bombs); BCFZ
    rejects declared sizes above the limit (was: reserved up to 2 GB); BCFS clamps file
    sizes (a negative size became a huge `usize`). Failed allocations abort the process,
    which `catch_unwind` cannot contain. `read_gpx` also accepts uncompressed BCFS.
    Tests in `limit_tests`.
12. `src/io/gpx.rs` (`read_le_i32`) and `src/model/legacy/headers/io.rs` (directions):
    index panics on truncated GPX containers and out-of-range GP5 direction measure numbers,
    found by the truncation/mutation test in `crates/gscore-io/tests/test_testdata.rs`.
13. Removed the modules GuitarScore does not use (about 21k lines): `analysis/`,
    `convert/`, `model/{optimized,musicxml,mnx,mscz}/`, `io/mscz/`, and their re-exports in
    `lib.rs`. (Change 5's edit to `convert/legacy/mod.rs` went with them.)
14. GP3-5 writer (`src/io/lyric.rs`, `src/io/song.rs`, `src/io/rse.rs`): always writes the 5
    lyric lines (indexing panicked for songs read from GP6/GP7); generates the 64-entry
    MIDI channel table when a song has none (one channel per track, drums on channel 10;
    panicked or produced an unreadable file before); writes exactly 11 master and 4 track
    equalizer values, as the reader expects (a default song held 10 and shifted the rest
    of the file).
15. GPIF writer (`src/io/gpif_export.rs`): voices holding only placeholder beats are
    written as empty and placeholder beats are skipped (GP5 -> GP6/GP7 used to turn them
    into real beats); writes `<InstrumentSet>` (name, type; drum tracks without a type as
    `drumKit`) and the tuning `<Label>`.
16. `src/io/gpx.rs` (`pack_bcfs`): refuses files whose block index table does not fit into
    the directory sector (about 4 MB) instead of overwriting the file data with it.

17. `src/io/gpif_import.rs` (`convert_note`): a hammer-on/pull-off is kept only on the
    origin (`HopoOrigin`); `HopoDestination` is ignored. The legacy model has one `hammer`
    flag that means "origin", as GP3-5 store it, so GP7/GPX and GP5 now agree. Regression
    test `effect_tests::hammer_is_kept_on_the_origin_only`.
18. `src/io/gpif.rs`, `src/io/gpif_import.rs`: read the whammy bar, which used to be
    dropped (`ornaments` measure 9). GP7 stores `<Whammy originValue=...
    destinationValue=...>`, GP6 stores the `WhammyBarOriginValue` /
    `WhammyBarDestinationValue` beat properties (on every beat, 0 = none); both become
    `BeatEffects::tremolo_bar`. Tests in `effect_tests`.
19. `src/io/gpif_import.rs` (`convert_note`): the harmonic fret is applied after all note
    properties are read, because Guitar Pro writes `HarmonicFret` before `HarmonicType`
    and the fret used to be dropped. Test
    `effect_tests::harmonic_fret_is_read_before_the_harmonic_type`.
20. Whammy bar (`src/io/gpif.rs`, `src/io/gpif_import.rs`, `src/io/gpif_export.rs`): the
    reader keeps GP7's `middleValue` and offsets (and GP6's `WhammyBarMiddleValue`) as a
    three-point shape (`build_whammy_effect`), and the writer writes `<Whammy>` with origin,
    middle, destination and offsets instead of the legacy `<Tremolo>` destination only.
    Dips (down and back up) used to vanish on save (54 beats in 10 owner files). Whether
    Guitar Pro accepts the written element is not verified. Test
    `effect_tests::whammy_dip_survives_writing`.
21. Note bends (`src/io/gpif_import.rs`, `src/io/gpif_export.rs`): the reader keeps
    `BendMiddleValue` and the offsets as a three-point shape (`build_note_bend`, sharing
    `shape_points` with the whammy bar) and derives the kind from it (bend, release,
    bend-and-release, prebend); bend-and-release (origin = destination) was read as no bend
    at all. The writer writes origin, middle (furthest point) and destination with offsets;
    it used `points[2]` and skipped bends with fewer than three points. Test
    `effect_tests::bend_and_release_survives_reading_and_writing`.
22. `src/io/gpif_export.rs` is held to the workspace's no-panic lints: `write_gpif_xml`
    returns `GpResult`; ids and bar numbers are computed with checked arithmetic; a track
    with fewer measures than the song is an error instead of an index panic; simile marks are
    stored instead of parsed back out of a string (which could panic) and are XML-escaped.

23. Mix table tempo (`src/model/mix_table.rs`, `src/io/mix_table.rs`): `MixTableItem::value`
    is a `u16` instead of a `u8`. The GP3-5 reader clamped tempo changes to 255 BPM, so
    faster tempos were lost on reading and saving. Regression test
    `tempo_changes_above_255_bpm_survive_gp5`.
24. Repeat counts in GPIF (`src/io/gpif_import.rs`, `src/io/gpif_export.rs`): GPIF's
    `count` is the number of plays (2 for a plain repeat), while the model, like GP3-5 after
    reading, stores the number of repeats. Both directions passed the number through, so GP7
    repeats played once too often and converting between GP5 and GP7 shifted the count.
    Regression test `repeat_counts_convert_between_repeats_and_plays`.
25. GP7 MIDI program (`src/io/gpif.rs`, `src/io/gpif_import.rs`): GP7 names a track's
    program in `<Sounds><Sound><MIDI><Program>`, not in `<GeneralMidi>` as GP6 does; the
    reader now falls back to the first sound's program. Before, every GP7 track got the
    default instrument. Covered by `gscore-play`'s comparison with Guitar Pro's MIDI export
    (`notes_and_programs_match_guitar_pros_midi_export`).
26. Note vibrato (`src/model/note.rs`, `src/io/gpif_import.rs`, `src/io/gpif_export.rs`):
    GP6/GP7 name the kind (`<Vibrato>Slight</Vibrato>` or `Wide`). The model used to keep a
    single `vibrato: bool`, so saving turned a wide vibrato into a slight one. It now carries
    `wide_vibrato`; the reader sets it from the tag and the writer writes the tag back.
    GP3-5 have no distinct flag, so a wide vibrato saved to GP5 becomes slight. Test
    `effect_tests::wide_vibrato_survives_reading_and_writing`.
27. Beat-level legato (`src/model/beat.rs`, `src/io/gpif.rs`, `src/io/gpif_import.rs`,
    `src/io/gpif_export.rs`): GP6/GP7 write `<Legato origin="true" destination="false" />`
    over a run of beats. The tag used to be ignored, so the slur vanished on save; it is now
    `BeatEffects::legato` and is read and written back. This is not a hammer-on or a slide
    (those stay note effects). Test `effect_tests::beat_legato_survives_reading_and_writing`.
28. Beat vibrato (`src/model/beat.rs`, `src/io/gpif.rs`, `src/io/gpif_import.rs`,
    `src/io/gpif_export.rs`): GP6/GP7 store a beat's own vibrato as the
    `VibratoWTremBar` beat property with a `<Strength>Slight</Strength>` or `Wide`. The
    property used to be ignored, so the wavy lines over notes 11 and 13 in `ornaments`
    disappeared. `BeatEffects::vibrato`/`wide_vibrato` now carry it, and the writer emits
    the property again. This is separate from a note's own `<Vibrato>` tag. Test
    `effect_tests::beat_vibrato_survives_reading_and_writing`.
29. Workspace standards for the whole crate (2026-11-02): the crate-level `allow` of every
    lint is gone. All code passes the workspace's no-panic lints (no `unwrap`, `expect`,
    `panic`, indexing or slicing) plus `arithmetic_side_effects` and the lossy-cast lints
    (`cast_possible_truncation`, `cast_possible_wrap`, `cast_sign_loss`); deliberate
    truncations go through named helpers in `io/primitive` (`low_i8`, `low_u8`, `low_i16`,
    `low_u16`, `byte_as_i8`, ...), so the bytes read and written are unchanged. Every file is
    under 400 lines: `gpif_import`, `gpif_export`, `gpif`, `gpx`, `headers`, `mix_table` and
    `primitive` became directories. Crashes found and fixed on the way, each with a test
    that panicked before (`io/crash_tests.rs`, `model::chord`, `io::gpx`):
    - GP3-5 writing: a track name over 40 bytes or a chord name over 22 bytes underflowed
      the padding (and non-ASCII names were padded by their UTF-8 length, shifting the
      file); names are now cut to their field (`write_fixed_string`).
    - GP5 writing: a repeat count of 127 overflowed (GP5 counts plays); a note on string 8
      (8-string guitars from GP7) overflowed the string-flag shift and is now refused with
      an error; a page-setup copyright without a line break was indexed past its end.
    - `Note::real_value` indexed past the tuning for a string the track lacks.
    - `PitchClass` display indexed the note tables with 12 (B#) or -1 (Cb).
    - BCFS: a negative or huge block number overflowed while computing the offset.
    The library no longer prints (`println!` in the clipboard reader, `eprintln!` warnings
    in the GPIF importer). GPIF fields that are parsed but not used yet are kept and marked
    `expect(dead_code)`. Mix-table values above 127 are clamped when writing GP3-5 (the
    low byte was written before). `gscore-io` still calls the crate behind
    `catch_unwind`, as a second line of defense.
30. Capo (`src/io/gpif_import/helpers.rs`, `src/io/gpif_import/mod.rs`,
    `src/io/gpif_export/mod.rs`, 2026-11-02): GP6 stores the capo as a `CapoFret` property
    on the track and GP7 on its staff; the importer ignored both, so every GP6/GP7 track
    came in without its capo, and the writer did not write it either. Both are read now
    (track first, then staff, clamped to 0-127) and `CapoFret` is written in the track's
    `<Properties>` next to the tuning. GP3-5 already store it as `Track::offset`. Test
    `tuning_tests::capo_fret_is_read_and_written`.
31. Master volume and pan (`src/model/song.rs`, `src/io/gpif/mod.rs`,
    `src/io/gpif_import/mod.rs`, `src/io/gpif_export/mod.rs`, 2026-11-02): GP7 stores
    them as the `I01_VolumeAndPan` master effect (`<MasterTrack><RSE><Master>`, two values
    0.0-1.0); neither the reader nor the writer handled it. New field
    `Song::master_volume_pan`, read and written (only when the song has one). Untouched
    GP7 files store `0.76 0.5`. Tests `display_tests`.
32. Shown staves (`src/io/gpx/part_configuration.rs`, `src/io/gpx/mod.rs`,
    `src/io/gpx/write.rs`, `src/io/song.rs`, 2026-11-02): GP7 keeps per track whether
    standard notation and tablature are shown in the binary `Content/PartConfiguration`
    (layout in the module docs; it matches alphaTab's reader). It is read into
    `TrackSettings::notation`/`tablature` (which GP3-5 already had) and written with every
    `.gp` file; the bytes for a one-track file equal Guitar Pro's. A track showing neither
    keeps its tablature.
33. Measure feel (`src/model/enums.rs`, `src/io/gpif/bars.rs`, `src/io/gpif_import/mod.rs`,
    `src/io/gpif_export/mod.rs`, `src/io/enums.rs`, `src/io/song.rs`, 2026-11-03): GPIF's
    `<MasterBar>` has a `<TripletFeel>` element (`Triplet8th`, `Triplet16th`, `Dotted8th`,
    `Dotted16th`, `Scottish8th`, `Scottish16th`) that was neither read nor written, so a
    GP7/GPX file lost its swing. `TripletFeel` gained the dotted and Scottish feels and
    both directions use the element. GP5 headers know only the triplet feels and GP3/4 only
    a song-wide eighth-note swing (which was also written for a sixteenth feel); other
    feels save as straight and the save check reports it. Test
    `feels_round_trip_through_gpif`.
34. Section titles (`src/io/gpif_import/mod.rs`, 2026-11-03): Guitar Pro writes a section's
    text as a CDATA block on its own lines (`<Text>\n<![CDATA[Chorus]]>\n</Text>`). The
    reader kept those newlines, so every marker title read from a GP6/GP7 file began and
    ended with a line break: the score drew the text below its frame (the first line of the
    galley is empty) and the section dialog showed the line breaks as boxes. Titles are now
    trimmed, an empty text falls back to the section letter, and only a marker with neither
    keeps the `Section` placeholder. Tests in `section_tests`.
35. Default section color (`src/model/headers.rs`, `src/io/gpif_import/mod.rs`,
    2026-11-03): GP6/GP7 files store no marker color, so the reader gave every section
    Guitar Pro's aggressive red (`0xFF0000`). It is now `Marker::DEFAULT_COLOR`, a light
    grey (`0xB0B0B0`) that the editor also offers as "Light grey" in its color palette;
    GP3-5 files keep the color stored in them. `gscore-model`'s `Section::DEFAULT_COLOR`
    uses the same value.

36. Grace-note beats (`src/model/beat.rs`, `src/io/gpif_import/beats.rs`,
    `src/io/gpif_export/beats.rs`, 2026-11-03): GP6/7 mark a whole beat as grace notes with
    `<GraceNotes>BeforeBeat</GraceNotes>` or `OnBeat`. The reader kept the beat as an
    ordinary one, so its (32nd/16th...) length made measures longer than their time
    signature, and the writer guessed "all notes have a grace effect" to write the marker.
    `Beat` now has a `grace` flag, set by the reader and used by the writer; the model,
    layout and playback treat such a beat as taking no time.
37. Grace notes in both forms (`src/model/grace.rs`, `src/io/gpif_import/mod.rs`,
    `src/io/song.rs`, 2026-11-04): GP3-5 attach a grace note to its main note, GP6/GP7
    give it a beat of its own, and each writer only knew its own form (attached grace
    notes were dropped when writing GPIF; grace beats were written to GP3-5 as ordinary
    beats, overfilling the measure). The GPIF reader now folds a grace beat into the next
    beat where nothing is lost (`fold_grace_beats`: plain notes, at most a hammer-on or
    shift slide, a 64th/32nd/16th, a main note on each string), the GPIF writers turn
    attached grace notes back into grace beats (`Song::with_grace_beats`), and the GP3-5
    writer folds grace beats and leaves out those that cannot be folded
    (`Song::with_attached_graces`). The GPIF reader stored every grace length as 1; the
    folded length is the beat's value. Tests in `model/grace.rs`.
38. The track's instrument symbol (`src/model/track.rs`, `src/io/gpif/bars.rs`,
    `src/io/gpif_import/mod.rs`, `src/io/gpif_export/mod.rs`, 2026-11-05): the app lets
    the user choose which instrument symbol a track shows, independent of the played
    sound, so it needs a place in the file. `Track` gains `symbol`, written as the GPIF
    track property `GuitarScoreSymbol` with a `<Text>` value and read back by
    `extract_symbol`. GP3-5 and the legacy formats drop it (their import leaves `None`).
    A round-trip test checks that it survives and that the instrument type is unchanged.
39. Brush and arpeggio speed and timing (`src/model/beat.rs`, `src/io/gpif_import/beats.rs`,
    `src/io/gpif_export/beats.rs`, 2026-11-05): GP3-5 already store a brush stroke's speed
    (`BeatStroke::value`, a note-value denominator), and the model now uses it instead of a
    fixed 32nd. GPIF has only the Brush direction and the `<Arpeggio>` direction, so the
    speed and the "starts/ends on the beat" timing are written as GuitarScore properties:
    `GuitarScoreBrush` and `GuitarScoreArpeggio`, each with `<Duration>` and `<Text>`
    (`on`/`before`). They are read back on import; Guitar Pro ignores the extra properties.
    `BeatStroke` gains `on_beat` (default true) and `BeatEffects` gains `arpeggio_speed` and
    `arpeggio_on_beat`. GP3-5 keep the brush speed but not the timing, and no arpeggio
    speed/timing; the loss is reported by the export comparison (`differences`).
40. Slash notation (`src/io/gpx/part_configuration.rs`, `src/io/gpif_import/helpers.rs`,
    `src/io/gpif_import/mod.rs`, `src/io/gpif_export/mod.rs`, 2026-11-05): bit 2 of a
    track's `PartConfiguration` flags is its slash staff, read into
    `TrackSettings::slash` and written back, with or without notation (Guitar Pro's print of
    `testdata/basic/staff.gp` shows notation, tab and slash for flags `0x07`). A
    `GuitarScoreSlash` track property carries the same switch; the flags win when a file
    has both.
41. Bars per staff (`src/io/gpif_import/mod.rs`, 2026-11-05): a GPIF master bar lists one
    bar per staff, not per track. A track with two staves (a piano's or synth's treble and
    bass) took two places, and every later track read its neighbour's bars (`Ballad.gp`:
    "Vocal ALEX" showed "Vocal SAMIR"). Each track now reads its first staff's bar; a
    missing entry is an empty bar so later bars stay in place. The second staff itself is
    not read yet. Test `a_track_with_two_staves_does_not_shift_the_tracks_after_it`.
42. Lyrics and chord diagrams (`src/model/beat.rs`, `src/io/gpif/bars.rs`,
    `src/io/gpif_import/beats.rs`, `src/io/gpif_export/beats.rs`,
    `src/io/gpif_export/mod.rs`, 2026-11-06): `Beat` gains `lyrics` (GP6/GP7's repeated
    `<Lyrics>` on a beat, one per line) and the GPIF writer now writes it. The writer also
    writes each track's chord diagrams as a `DiagramCollection` property and each beat's
    diagram id as `<Chord>`, which the importer already read; diagrams used to be dropped
    on save. Tests cover both through a GP7 round trip.
