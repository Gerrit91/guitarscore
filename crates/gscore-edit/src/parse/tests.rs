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
        ("set_track_mute mute=maybe", "true or false"),
        ("set_track_color color=zzz", "rrggbb"),
        ("set_track_color color=1000000", "rrggbb"),
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
