//! Tests for the command-line parser.

use super::*;
use gscore_model::BeatIndex;

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
        assert_eq!(parse_line(&line).unwrap(), Some(command), "{line}");
    }
}
