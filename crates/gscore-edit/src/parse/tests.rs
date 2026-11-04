//! Tests for the command-line parser.

use super::*;
use gscore_model::{
    AccentKind, BeatEffect, BeatIndex, BendKind, BendPoint, BendShape, Dynamic, Grace, GraceSpeed,
    Harmonic, HarmonicKind, NoteEffect, StrokeDirection, WhammyShape,
};

fn at(track: usize, measure: usize, voice: usize, beat: usize) -> BeatPosition {
    BeatPosition {
        track: TrackIndex::new(track),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(voice),
        beat: BeatIndex::new(beat),
    }
}

fn value(text: &str) -> NoteValue {
    NoteValue::parse(text).unwrap()
}

#[test]
fn page_commands_take_guitar_pros_defaults_and_aliases() {
    assert_eq!(
        parse_line("set_page_setup width=297 height=210").unwrap(),
        Some(Command::SetPageSetup {
            setup: PageSetup::default().turned(),
        })
    );
    assert_eq!(
        parse_line("set_line_break track=2 measure=5 break=line").unwrap(),
        Some(Command::SetSystemBreak {
            track: TrackIndex::new(1),
            measure: MeasureIndex::new(4),
            kind: SystemBreak::Line,
        })
    );
    assert_eq!(
        parse_line("set_bars_per_line bars=auto").unwrap(),
        Some(Command::SetBarsPerSystem {
            track: TrackIndex::new(0),
            bars: None,
        })
    );
}

#[test]
fn parses_every_command_and_fills_defaults() {
    let cases = [
        (
            "set_fret track=2 measure=3 beat=4 string=6 fret=5",
            Command::SetFret {
                at: at(1, 2, 0, 3),
                string: StringNumber::new(6).unwrap(),
                fret: Fret::new(5).unwrap(),
            },
        ),
        (
            "enter_fret measure=3 beat=4 string=6 fret=5",
            Command::SetFret {
                at: at(0, 2, 0, 3),
                string: StringNumber::new(6).unwrap(),
                fret: Fret::new(5).unwrap(),
            },
        ),
        (
            "clear_note track=1 measure=1 voice=2 beat=1 string=3",
            Command::ClearNote {
                at: at(0, 0, 1, 0),
                string: StringNumber::new(3).unwrap(),
            },
        ),
        (
            "set_duration measure=2 beat=1 duration=1/8.",
            Command::SetDuration {
                at: at(0, 1, 0, 0),
                duration: value("1/8."),
            },
        ),
        (
            "insert_beat measure=2 beat=3 duration=1/16..",
            Command::InsertBeat {
                at: at(0, 1, 0, 2),
                duration: value("1/16.."),
            },
        ),
        (
            "delete_beat measure=2 beat=3",
            Command::DeleteBeat { at: at(0, 1, 0, 2) },
        ),
        (
            "insert_measure measure=4",
            Command::InsertMeasure {
                measure: MeasureIndex::new(3),
            },
        ),
        (
            "delete_measure measure=4",
            Command::DeleteMeasure {
                measure: MeasureIndex::new(3),
            },
        ),
        (
            "insert_track",
            Command::InsertTrack {
                at: TrackIndex::new(0),
            },
        ),
        (
            "insert_track track=3",
            Command::InsertTrack {
                at: TrackIndex::new(2),
            },
        ),
        (
            "delete_track track=2",
            Command::DeleteTrack {
                track: TrackIndex::new(1),
            },
        ),
        (
            "duplicate_track",
            Command::DuplicateTrack {
                track: TrackIndex::new(0),
            },
        ),
        (
            "move_track from=1 to=3",
            Command::MoveTrack {
                from: TrackIndex::new(0),
                to: TrackIndex::new(2),
            },
        ),
        (
            "rename_track track=2 name=Lead",
            Command::RenameTrack {
                track: TrackIndex::new(1),
                name: "Lead".to_owned(),
            },
        ),
        (
            "set_track_color track=2 color=0x00ff00",
            Command::SetTrackColor {
                track: TrackIndex::new(1),
                color: gscore_model::TrackColor::new(0x00FF00),
            },
        ),
        (
            "set_track_staves track=2 staves=tab",
            Command::SetTrackStaves {
                track: TrackIndex::new(1),
                staves: TrackStaves::TabOnly,
            },
        ),
        (
            "set_master_volume volume=64",
            Command::SetMasterVolume { volume: 64 },
        ),
        ("set_master_pan pan=0", Command::SetMasterPan { pan: 0 }),
        (
            "set_track_volume track=2 volume=30",
            Command::SetTrackVolume {
                track: TrackIndex::new(1),
                volume: 30,
            },
        ),
        (
            "set_track_pan pan=64",
            Command::SetTrackPan {
                track: TrackIndex::new(0),
                pan: 64,
            },
        ),
        (
            "set_track_reverb track=2 reverb=40",
            Command::SetTrackReverb {
                track: TrackIndex::new(1),
                reverb: 40,
            },
        ),
        (
            "set_track_chorus chorus=0",
            Command::SetTrackChorus {
                track: TrackIndex::new(0),
                chorus: 0,
            },
        ),
        (
            "set_track_mute track=2 mute=true",
            Command::SetTrackMute {
                track: TrackIndex::new(1),
                mute: true,
            },
        ),
        (
            "set_track_solo solo=yes",
            Command::SetTrackSolo {
                track: TrackIndex::new(0),
                solo: true,
            },
        ),
        (
            "set_track_program track=2 program=29",
            Command::SetTrackProgram {
                track: TrackIndex::new(1),
                program: 29,
            },
        ),
        (
            "set_track_drums drums=true",
            Command::SetTrackDrums {
                track: TrackIndex::new(0),
                drums: true,
            },
        ),
        (
            "set_track_capo track=2 capo=3",
            Command::SetTrackCapo {
                track: TrackIndex::new(1),
                capo: 3,
            },
        ),
        (
            "set_track_strings track=2 strings=7",
            Command::SetTrackStrings {
                track: TrackIndex::new(1),
                strings: 7,
            },
        ),
        (
            "set_track_string_pitch track=2 string=6 pitch=E2",
            Command::SetTrackStringPitch {
                track: TrackIndex::new(1),
                string: StringNumber::new(6).unwrap(),
                pitch: Pitch::parse("E2").unwrap(),
            },
        ),
        (
            "set_track_string_pitch string=1 pitch=60",
            Command::SetTrackStringPitch {
                track: TrackIndex::new(0),
                string: StringNumber::new(1).unwrap(),
                pitch: Pitch::parse("60").unwrap(),
            },
        ),
        (
            "set_song_text field=title text=Hello",
            Command::SetSongText {
                field: SongTextField::Title,
                text: "Hello".to_owned(),
            },
        ),
        ("set_tempo tempo=140", Command::SetTempo { tempo: 140 }),
        (
            "set_beat_rest measure=2 beat=3",
            Command::SetBeatRest { at: at(0, 1, 0, 2) },
        ),
        (
            "set_dynamic measure=2 beat=3 dynamic=mp",
            Command::SetDynamic {
                at: at(0, 1, 0, 2),
                dynamic: Dynamic::Mp,
            },
        ),
        (
            "set_dead_note measure=2 beat=3 string=4 dead=yes",
            Command::SetDeadNote {
                at: at(0, 1, 0, 2),
                string: StringNumber::new(4).unwrap(),
                dead: true,
            },
        ),
        (
            "set_tie measure=2 beat=3 string=4 tie=yes",
            Command::SetTie {
                at: at(0, 1, 0, 2),
                string: StringNumber::new(4).unwrap(),
                tie: true,
            },
        ),
        (
            "set_measure_tempo measure=3 tempo=90",
            Command::SetMeasureTempo {
                measure: MeasureIndex::new(2),
                tempo: 90,
            },
        ),
        (
            "set_time_signature measure=3 numerator=6 denominator=8",
            Command::SetTimeSignature {
                measure: MeasureIndex::new(2),
                numerator: 6,
                denominator: 8,
            },
        ),
        (
            "set_key_signature measure=3 key=-2 minor=false",
            Command::SetKeySignature {
                measure: MeasureIndex::new(2),
                key: -2,
                minor: false,
            },
        ),
        (
            "set_section measure=3 title=Intro",
            Command::SetSection {
                measure: MeasureIndex::new(2),
                title: "Intro".to_owned(),
                color: gscore_model::Section::DEFAULT_COLOR,
            },
        ),
        (
            "clear_section measure=3",
            Command::ClearSection {
                measure: MeasureIndex::new(2),
            },
        ),
        (
            "set_repeat measure=3 start=true end=2 endings=1",
            Command::SetRepeat {
                measure: MeasureIndex::new(2),
                repeat: Repeat {
                    start: true,
                    end_repeats: Some(2),
                    endings: 1,
                },
            },
        ),
        (
            "set_double_bar measure=3 double_bar=true",
            Command::SetDoubleBar {
                measure: MeasureIndex::new(2),
                double_bar: true,
            },
        ),
        (
            "set_feel measure=3 feel=eighth",
            Command::SetFeel {
                measure: MeasureIndex::new(2),
                feel: Feel::Eighth,
            },
        ),
        (
            "set_fermata measure=3 fermata=long",
            Command::SetFermata {
                measure: MeasureIndex::new(2),
                fermata: Some(Fermata::Long),
            },
        ),
        (
            "set_note_effect measure=1 beat=1 string=1 effect=hammer value=true",
            Command::SetNoteEffect {
                at: at(0, 0, 0, 0),
                string: StringNumber::new(1).unwrap(),
                effect: NoteEffect::Hammer(true),
            },
        ),
        (
            "set_note_effect measure=1 beat=1 string=1 effect=accent value=heavy",
            Command::SetNoteEffect {
                at: at(0, 0, 0, 0),
                string: StringNumber::new(1).unwrap(),
                effect: NoteEffect::Accent(Some(AccentKind::Heavy)),
            },
        ),
        (
            "set_beat_effect measure=1 beat=1 effect=pick_stroke value=down",
            Command::SetBeatEffect {
                at: at(0, 0, 0, 0),
                effect: BeatEffect::PickStroke(Some(StrokeDirection::Down)),
            },
        ),
        (
            "batch",
            Command::Batch {
                commands: Vec::new(),
            },
        ),
        ("undo", Command::Undo),
        ("redo", Command::Redo),
    ];
    for (line, expected) in cases {
        assert_eq!(parse_line(line).unwrap(), Some(expected), "{line}");
    }
}

#[test]
fn blank_and_comment_lines_are_ignored() {
    for line in ["", "   ", "\t", "# a comment", "  # indented"] {
        assert_eq!(parse_line(line), Ok(None), "{line:?}");
    }
    assert!(
        parse_line("set_fret measure=1 beat=1 string=1 fret=1 # done").is_ok(),
        "trailing comments are ignored"
    );
}

#[test]
fn bad_lines_are_errors_with_useful_messages() {
    let cases = [
        ("bogus", "unknown command"),
        (
            "set_fret measure=1 beat=1 string=1 fret=1 extra=2",
            "unknown parameter",
        ),
        (
            "set_fret measure=1 measure=2 beat=1 string=1 fret=1",
            "given twice",
        ),
        ("set_fret measure=1 beat=1 string=1", "missing parameter"),
        (
            "set_fret measure=1 beat=1 string=1 fret=128",
            "fret from 0 to 127",
        ),
        (
            "set_fret measure=1 beat=1 string=0 fret=1",
            "string number from 1 up",
        ),
        ("set_fret measure=0 beat=1 string=1 fret=1", "1-based"),
        ("set_fret measure=-1 beat=1 string=1 fret=1", "1-based"),
        ("set_fret measure=NaN beat=1 string=1 fret=1", "1-based"),
        ("set_duration measure=1 beat=1 duration=1/3", "note length"),
        (
            "set_fret measure=1 beat=1 string=1 fret=1 oops",
            "key=value",
        ),
        ("set_fret measure=1 beat=1 string=1 fret=", "key=value"),
        ("move_track from=1", "missing parameter"),
        ("rename_track track=1", "missing parameter"),
        ("set_track_volume volume=128", "0 to 127"),
        ("set_track_pan pan=abc", "0 to 127"),
        ("set_track_reverb reverb=200", "0 to 127"),
        ("set_track_chorus", "missing parameter"),
        ("set_track_staves staves=slash", "both, tab or notation"),
        ("set_track_staves", "missing parameter"),
        ("set_master_volume volume=128", "0 to 127"),
        ("set_master_pan", "missing parameter"),
        ("set_track_mute mute=maybe", "true or false"),
        ("set_track_color color=zzz", "rrggbb"),
        ("set_track_color color=1000000", "rrggbb"),
        ("set_track_program program=128", "0 to 127"),
        ("set_track_capo capo=-1", "0 to 127"),
        ("set_track_strings strings=13", "1 to 12"),
        ("set_track_strings strings=0", "1 to 12"),
        ("set_track_string_pitch string=1 pitch=H2", "note name"),
        ("set_track_string_pitch string=1 pitch=C-2", "note name"),
        ("set_song_text field=bogus text=x", "title"),
        ("set_song_text field=title", "missing parameter"),
        ("set_page_setup width=20", "70 to 1000"),
        ("set_page_setup scale=500", "50 to 200"),
        ("set_page_setup left=150 right=150", "40 mm"),
        ("set_page_setup width=a4", "millimetres"),
        ("set_page_setup header=perhaps", "true or false"),
        (
            "set_system_break measure=3 break=column",
            "line, page or none",
        ),
        ("set_system_break break=line", "missing parameter"),
        ("set_bars_per_system bars=0", "1 to 32, or auto"),
        ("set_bars_per_system bars=40", "1 to 32, or auto"),
        ("set_tempo tempo=0", "1 to 1000"),
        ("set_tempo tempo=1001", "1 to 1000"),
        ("set_tempo tempo=fast", "1 to 1000"),
        (
            "set_time_signature measure=1 numerator=0 denominator=4",
            "1 to 64",
        ),
        (
            "set_time_signature measure=1 numerator=4 denominator=5",
            "note value",
        ),
        ("set_key_signature measure=1 key=8 minor=false", "-7 to 7"),
        ("set_section measure=1 title=", "key=value"),
        ("batch %%notACloser", "unknown command"),
        (
            "set_note_effect measure=1 beat=1 string=1 effect=bogus value=x",
            "note effect",
        ),
        (
            "set_note_effect measure=1 beat=1 string=1 effect=bend value=up:13:0",
            "note effect",
        ),
        (
            "set_beat_effect measure=1 beat=1 effect=whammy value=0:0,0:1",
            "beat effect",
        ),
        (
            "set_note_effect measure=1 beat=1 string=1 effect=grace value=5:quick:before",
            "note effect",
        ),
        (
            "set_note_effect measure=1 beat=1 string=1 effect=grace value=200:normal:before",
            "note effect",
        ),
    ];
    for (line, expected) in cases {
        let error = parse_line(line).unwrap_err().to_string();
        assert!(error.contains(expected), "{line}: {error}");
    }
}

#[test]
fn ids_may_be_repeated_in_scripts_with_their_own_errors() {
    // The parser is stateless: every line starts fresh.
    assert!(parse_line("set_fret measure=1 beat=1 string=1 fret=1").is_ok());
    assert!(parse_line("set_fret measure=1 beat=1 string=1 fret=1").is_ok());
    assert!(parse_line("not_a_command").is_err());
}

#[test]
fn every_command_writes_a_line_that_parses_back_to_it() {
    let at = |track, measure, voice, beat| BeatPosition {
        track: TrackIndex::new(track),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(voice),
        beat: BeatIndex::new(beat),
    };
    let string = StringNumber::new(3).unwrap();
    let durations = ["1/4", "1/8.", "1/16..", "1/8*3:2", "1/1", "1/128.*5:4"];
    let mut commands = vec![
        Command::SetFret {
            at: at(2, 40, 1, 7),
            string,
            fret: Fret::new(127).unwrap(),
        },
        Command::ClearNote {
            at: at(0, 0, 0, 0),
            string,
        },
        Command::DeleteBeat { at: at(0, 9, 0, 3) },
        Command::InsertMeasure {
            measure: MeasureIndex::new(0),
        },
        Command::DeleteMeasure {
            measure: MeasureIndex::new(499),
        },
        Command::Undo,
        Command::Redo,
    ];
    let track = TrackIndex::new(2);
    commands.extend([
        Command::InsertTrack { at: track },
        Command::DeleteTrack { track },
        Command::DuplicateTrack { track },
        Command::MoveTrack {
            from: TrackIndex::new(0),
            to: TrackIndex::new(5),
        },
        Command::SetTrackColor {
            track,
            color: gscore_model::TrackColor::new(0x00_0a_bc),
        },
        Command::SetTrackVolume { track, volume: 127 },
        Command::SetTrackPan { track, pan: 0 },
        Command::SetTrackReverb { track, reverb: 127 },
        Command::SetTrackChorus { track, chorus: 5 },
        Command::SetTrackStaves {
            track,
            staves: TrackStaves::NotationOnly,
        },
        Command::SetMasterVolume { volume: 90 },
        Command::SetMasterPan { pan: 127 },
        Command::SetTrackMute { track, mute: true },
        Command::SetTrackSolo { track, solo: false },
        Command::SetTrackProgram {
            track,
            program: 127,
        },
        Command::SetTrackDrums { track, drums: true },
        Command::SetTrackCapo { track, capo: 5 },
        Command::SetTrackStrings { track, strings: 8 },
        Command::SetTrackStringPitch {
            track,
            string,
            pitch: Pitch::parse("C#4").unwrap(),
        },
        Command::SetSongText {
            field: SongTextField::Title,
            text: "A title with spaces, # and\nnew lines – Ü".to_owned(),
        },
        Command::SetTempo { tempo: 140 },
        Command::SetPageSetup {
            setup: PageSetup {
                width_mm: 279,
                height_mm: 216,
                margins: gscore_model::Margins {
                    top: 20,
                    right: 12,
                    bottom: 18,
                    left: 11,
                },
                scale_percent: 85,
                page_numbers: false,
                copyright: true,
                running_header: true,
            },
        },
        Command::SetSystemBreak {
            track,
            measure: MeasureIndex::new(4),
            kind: SystemBreak::Page,
        },
        Command::SetBarsPerSystem {
            track,
            bars: Some(4),
        },
        Command::SetBarsPerSystem { track, bars: None },
        Command::SetBeatRest { at: at(0, 1, 0, 2) },
        Command::SetTie {
            at: at(0, 2, 0, 3),
            string,
            tie: true,
        },
        Command::SetBeatTie {
            at: at(0, 2, 0, 3),
            tie: true,
        },
        Command::SetBeatText {
            at: at(0, 2, 0, 3),
            text: "let ring".to_owned(),
        },
        Command::SetDeadNote {
            at: at(0, 2, 0, 3),
            string,
            dead: true,
        },
        Command::SetDynamic {
            at: at(0, 2, 0, 3),
            dynamic: Dynamic::Fff,
        },
        Command::SetMeasureTempo {
            measure: MeasureIndex::new(7),
            tempo: 96,
        },
        Command::SetTimeSignature {
            measure: MeasureIndex::new(2),
            numerator: 7,
            denominator: 8,
        },
        Command::SetKeySignature {
            measure: MeasureIndex::new(3),
            key: -3,
            minor: true,
        },
        Command::SetSection {
            measure: MeasureIndex::new(1),
            title: "Chorus – loud #2".to_owned(),
            color: gscore_model::TrackColor::new(0x00_11_22),
        },
        Command::ClearSection {
            measure: MeasureIndex::new(4),
        },
        Command::Batch {
            commands: vec![
                Command::SetBeatRest { at: at(0, 0, 0, 0) },
                Command::SetTempo { tempo: 100 },
                Command::Batch {
                    commands: vec![Command::SetMeasureTempo {
                        measure: MeasureIndex::new(0),
                        tempo: 110,
                    }],
                },
            ],
        },
        Command::SetNoteEffect {
            at: at(0, 1, 0, 1),
            string,
            effect: NoteEffect::Bend(Some(
                BendShape::new(
                    BendKind::Up,
                    vec![
                        BendPoint {
                            position: 0,
                            value: 0,
                        },
                        BendPoint {
                            position: 6,
                            value: 4,
                        },
                        BendPoint {
                            position: 12,
                            value: 0,
                        },
                    ],
                )
                .unwrap(),
            )),
        },
        Command::SetNoteEffect {
            at: at(0, 1, 0, 1),
            string,
            effect: NoteEffect::Harmonic(Some(Harmonic {
                kind: HarmonicKind::Artificial,
                node: Some(19),
            })),
        },
        Command::SetNoteEffect {
            at: at(0, 1, 0, 1),
            string,
            effect: NoteEffect::Grace(Some(Grace {
                fret: Fret::new(7).unwrap(),
                speed: GraceSpeed::Slow,
                on_beat: true,
                dead: true,
                hammer: false,
            })),
        },
        Command::SetNoteEffect {
            at: at(0, 1, 0, 1),
            string,
            effect: NoteEffect::Grace(Some(Grace::before(Fret::new(0).unwrap()))),
        },
        Command::SetNoteEffect {
            at: at(0, 1, 0, 1),
            string,
            effect: NoteEffect::Grace(None),
        },
        Command::SetBeatEffect {
            at: at(0, 2, 0, 0),
            effect: BeatEffect::Whammy(Some(
                WhammyShape::new(vec![
                    BendPoint {
                        position: 0,
                        value: 0,
                    },
                    BendPoint {
                        position: 6,
                        value: -4,
                    },
                    BendPoint {
                        position: 12,
                        value: 0,
                    },
                ])
                .unwrap(),
            )),
        },
        Command::SetRepeat {
            measure: MeasureIndex::new(1),
            repeat: Repeat {
                start: true,
                end_repeats: Some(2),
                endings: 1,
            },
        },
        Command::SetDoubleBar {
            measure: MeasureIndex::new(1),
            double_bar: true,
        },
        Command::SetFeel {
            measure: MeasureIndex::new(1),
            feel: Feel::DottedEighth,
        },
        Command::SetFermata {
            measure: MeasureIndex::new(1),
            fermata: Some(Fermata::VeryLong),
        },
    ]);
    // Names a plain command line could not hold: empty, spaces, the comment sign, `=`,
    // `%`, line breaks, non-ASCII, and what looks like the encoding's own marker.
    for name in [
        "",
        "Lead guitar #2",
        "a=b c",
        "100% fuzz",
        "two\nlines\ttab",
        "Gitarre – Ü 🎸",
        "%%41",
        "plain",
    ] {
        commands.push(Command::RenameTrack {
            track,
            name: name.to_owned(),
        });
    }
    for text in durations {
        let duration: NoteValue = text.parse().unwrap();
        commands.push(Command::SetDuration {
            at: at(0, 1, 0, 2),
            duration,
        });
        commands.push(Command::InsertBeat {
            at: at(1, 0, 1, 0),
            duration,
        });
    }
    // Every command id is covered.
    for info in crate::COMMANDS {
        assert!(
            commands.iter().any(|c| c.info().id == info.id),
            "{} has no round-trip case",
            info.id
        );
    }
    for command in commands {
        let line = command.to_line();
        assert!(!line.contains('\n'), "one line: {line}");
        assert_eq!(parse_line(&line).unwrap(), Some(command.clone()), "{line}");
        // A recorded entry keeps whether it was merged into the step before.
        for merged in [false, true] {
            let entry = crate::Recorded {
                command: command.clone(),
                merged,
            };
            assert_eq!(
                crate::Recorded::parse(&entry.to_line()).unwrap(),
                Some(entry),
                "{line}"
            );
        }
    }
    // Hand-written names stay as written; a broken escape is an error, not a guess.
    assert_eq!(
        parse_line("rename_track name=Bass%20VI").unwrap(),
        Some(Command::RenameTrack {
            track: TrackIndex::new(0),
            name: "Bass%20VI".to_owned()
        })
    );
    assert!(
        parse_line("rename_track name=%%%4").is_err(),
        "an incomplete escape"
    );
    assert!(parse_line("rename_track name=%%%FF").is_err(), "not UTF-8");
}
