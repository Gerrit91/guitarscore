//! Parsing command lines: `id key=value key=value ...` with 1-based positions.
//!
//! The same parser serves `gscore-cli edit -c "..."` and script files, so scripts written for
//! one are exactly the regression tests for the other. Blank lines and `#`-comment lines are
//! ignored; there is no quoting, because no value contains a space.

use std::collections::BTreeMap;
use std::fmt;

use gscore_model::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, NoteValue, StringNumber, TrackIndex, VoiceIndex,
};

use crate::command::Command;
use crate::info::{self, CommandInfo};

/// Why a command line could not be turned into a [`Command`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// No command has this id.
    UnknownCommand(String),
    /// The command has no such parameter.
    UnknownParameter {
        /// The command id.
        command: String,
        /// The parameter key.
        parameter: String,
    },
    /// A parameter was given twice.
    DuplicateParameter {
        /// The command id.
        command: String,
        /// The parameter key.
        parameter: String,
    },
    /// A required parameter is missing.
    MissingParameter {
        /// The command id.
        command: String,
        /// The parameter name.
        parameter: &'static str,
    },
    /// A parameter value is invalid.
    BadValue {
        /// The command id.
        command: String,
        /// The parameter name.
        parameter: String,
        /// What was given.
        value: String,
        /// What was expected.
        expected: &'static str,
    },
    /// The line is not a sequence of `key=value` words.
    Syntax {
        /// The command id.
        command: String,
        /// The offending word.
        token: String,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnknownCommand(command) => {
                write!(
                    f,
                    "unknown command \"{command}\"; run `gscore-cli commands`"
                )
            }
            ParseError::UnknownParameter { command, parameter } => {
                write!(f, "{command}: unknown parameter \"{parameter}\"")
            }
            ParseError::DuplicateParameter { command, parameter } => {
                write!(f, "{command}: parameter \"{parameter}\" given twice")
            }
            ParseError::MissingParameter { command, parameter } => {
                write!(f, "{command}: missing parameter \"{parameter}\"")
            }
            ParseError::BadValue {
                command,
                parameter,
                value,
                expected,
            } => write!(
                f,
                "{command}: invalid value \"{value}\" for \"{parameter}\" (expected {expected})"
            ),
            ParseError::Syntax { command, token } => {
                write!(f, "{command}: expected key=value, found \"{token}\"")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses one command line. `Ok(None)` means a blank or comment line.
pub fn parse_line(line: &str) -> Result<Option<Command>, ParseError> {
    // A comment starts at the first `#`; values never contain one.
    let line = line.split('#').next().unwrap_or_default();
    let mut words = line.split_whitespace();
    let Some(id) = words.next() else {
        return Ok(None);
    };
    let info = info::find_command(id).ok_or_else(|| ParseError::UnknownCommand(id.to_owned()))?;

    let mut args = Args::new(info.id);
    for word in words {
        let Some((key, value)) = word.split_once('=') else {
            return Err(ParseError::Syntax {
                command: info.id.to_owned(),
                token: word.to_owned(),
            });
        };
        if key.is_empty() || value.is_empty() {
            return Err(ParseError::Syntax {
                command: info.id.to_owned(),
                token: word.to_owned(),
            });
        }
        if args.values.contains_key(key) {
            return Err(ParseError::DuplicateParameter {
                command: info.id.to_owned(),
                parameter: key.to_owned(),
            });
        }
        args.values.insert(key.to_owned(), value.to_owned());
    }

    let command = build(info, &mut args)?;
    args.finish()?;
    Ok(Some(command))
}

/// Turns the remaining `key=value` pairs into the typed command.
fn build(info: &'static CommandInfo, args: &mut Args) -> Result<Command, ParseError> {
    match info.id {
        "set_fret" => {
            let at = position(args)?;
            let string = string_arg(args)?;
            let fret = fret_arg(args)?;
            Ok(Command::SetFret { at, string, fret })
        }
        "clear_note" => {
            let at = position(args)?;
            let string = string_arg(args)?;
            Ok(Command::ClearNote { at, string })
        }
        "set_duration" => {
            let at = position(args)?;
            let duration = duration_arg(args)?;
            Ok(Command::SetDuration { at, duration })
        }
        "insert_beat" => {
            let at = position(args)?;
            let duration = duration_arg(args)?;
            Ok(Command::InsertBeat { at, duration })
        }
        "delete_beat" => {
            let at = position(args)?;
            Ok(Command::DeleteBeat { at })
        }
        "insert_measure" => Ok(Command::InsertMeasure {
            measure: MeasureIndex::new(required_position(args, "measure")?),
        }),
        "delete_measure" => Ok(Command::DeleteMeasure {
            measure: MeasureIndex::new(required_position(args, "measure")?),
        }),
        "undo" => Ok(Command::Undo),
        "redo" => Ok(Command::Redo),
        // Every entry in COMMANDS is handled above; a new one must be added here.
        other => Err(ParseError::UnknownCommand(other.to_owned())),
    }
}

/// The beat address from `track` (default 1), `measure`, `voice` (default 1) and `beat`.
fn position(args: &mut Args) -> Result<BeatPosition, ParseError> {
    let track = optional_position(args, "track")?.unwrap_or(0);
    let measure = required_position(args, "measure")?;
    let voice = optional_position(args, "voice")?.unwrap_or(0);
    let beat = required_position(args, "beat")?;
    Ok(BeatPosition {
        track: TrackIndex::new(track),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(voice),
        beat: BeatIndex::new(beat),
    })
}

/// A 1-based position, converted to the model's 0-based index.
fn optional_position(args: &mut Args, name: &'static str) -> Result<Option<usize>, ParseError> {
    let Some(text) = args.take(name) else {
        return Ok(None);
    };
    let value: u64 = text.parse().map_err(|_| {
        bad(
            args.command,
            name,
            &text,
            "a positive whole number (positions are 1-based)",
        )
    })?;
    let zero_based = value.checked_sub(1).ok_or_else(|| {
        bad(
            args.command,
            name,
            &text,
            "a positive whole number (positions are 1-based)",
        )
    })?;
    usize::try_from(zero_based).map(Some).map_err(|_| {
        bad(
            args.command,
            name,
            &text,
            "a whole number that fits this platform",
        )
    })
}

/// A required 1-based position, converted to the model's 0-based index.
fn required_position(args: &mut Args, name: &'static str) -> Result<usize, ParseError> {
    optional_position(args, name)?.ok_or_else(|| ParseError::MissingParameter {
        command: args.command.to_owned(),
        parameter: name,
    })
}

/// The `string` parameter.
fn string_arg(args: &mut Args) -> Result<StringNumber, ParseError> {
    let text = args.required("string")?;
    let number: u8 = text
        .parse()
        .map_err(|_| bad(args.command, "string", &text, "a string number from 1 up"))?;
    StringNumber::new(number)
        .map_err(|_| bad(args.command, "string", &text, "a string number from 1 up"))
}

/// The `fret` parameter.
fn fret_arg(args: &mut Args) -> Result<Fret, ParseError> {
    let text = args.required("fret")?;
    let value: u8 = text
        .parse()
        .map_err(|_| bad(args.command, "fret", &text, "a fret from 0 to 127"))?;
    Fret::new(value).map_err(|_| bad(args.command, "fret", &text, "a fret from 0 to 127"))
}

/// The `duration` parameter.
fn duration_arg(args: &mut Args) -> Result<NoteValue, ParseError> {
    let text = args.required("duration")?;
    text.parse::<NoteValue>().map_err(|_| {
        bad(
            args.command,
            "duration",
            &text,
            "a note length such as 1/4, 1/8. or 1/16.. (optionally *3:2)",
        )
    })
}

/// A bad-value error for `command`.
fn bad(
    command: &'static str,
    parameter: &'static str,
    value: &str,
    expected: &'static str,
) -> ParseError {
    ParseError::BadValue {
        command: command.to_owned(),
        parameter: parameter.to_owned(),
        value: value.to_owned(),
        expected,
    }
}

/// The parameter pairs of one command line, consumed as the command is built.
struct Args {
    command: &'static str,
    values: BTreeMap<String, String>,
}

impl Args {
    fn new(command: &'static str) -> Self {
        Args {
            command,
            values: BTreeMap::new(),
        }
    }

    /// Takes a parameter value, if it was given.
    fn take(&mut self, name: &'static str) -> Option<String> {
        self.values.remove(name)
    }

    /// Takes a required parameter value.
    fn required(&mut self, name: &'static str) -> Result<String, ParseError> {
        self.take(name).ok_or_else(|| ParseError::MissingParameter {
            command: self.command.to_owned(),
            parameter: name,
        })
    }

    /// Reports parameters the command does not know.
    fn finish(self) -> Result<(), ParseError> {
        match self.values.first_key_value() {
            Some((parameter, _)) => Err(ParseError::UnknownParameter {
                command: self.command.to_owned(),
                parameter: parameter.clone(),
            }),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
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
}
