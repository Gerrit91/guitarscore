//! Parsing command lines: `id key=value key=value ...` with 1-based positions.
//!
//! The same parser serves `gscore-cli edit -c "..."` and script files, so scripts written for
//! one are exactly the regression tests for the other. Blank lines and comments are ignored;
//! a comment starts at a `#` that begins a word (so a note name such as `C#4` keeps its
//! sharp). There is no quoting, because no value contains a space.

use std::collections::BTreeMap;
use std::fmt;

use gscore_model::{
    BeatEffect, BeatIndex, BeatPosition, Clef, Dynamic, Feel, Fermata, Fret, MAX_BARS_PER_SYSTEM,
    MAX_STRINGS, MAX_TEMPO, MeasureIndex, NoteEffect, NoteValue, PageSetup, Pitch, Repeat,
    SimileMark, SongTextField, StringNumber, SystemBreak, TrackIndex, TrackStaves, VoiceIndex,
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
    parse_line_depth(line, 0)
}

/// Most nested `batch` commands accepted (a hostile script must not overflow the stack).
const MAX_BATCH_DEPTH: usize = 32;

fn parse_line_depth(line: &str, depth: usize) -> Result<Option<Command>, ParseError> {
    if depth > MAX_BATCH_DEPTH {
        return Err(ParseError::Syntax {
            command: "batch".to_owned(),
            token: "batches nested too deeply".to_owned(),
        });
    }
    // A comment starts at a `#` that begins a word; a `#` inside a value (the sharp of
    // `C#4`) stays. The `#` itself may start the line.
    let line = line
        .char_indices()
        .find(|&(index, ch)| {
            ch == '#'
                && (index == 0
                    || line
                        .get(..index)
                        .is_some_and(|before| before.ends_with(char::is_whitespace)))
        })
        .and_then(|(index, _)| line.get(..index))
        .unwrap_or(line);
    let mut words = line.split_whitespace();
    let Some(id) = words.next() else {
        return Ok(None);
    };
    let info = info::find_command(id).ok_or_else(|| ParseError::UnknownCommand(id.to_owned()))?;

    // A batch is a list of encoded command lines, not `key=value` pairs.
    if info.id == "batch" {
        let mut commands = Vec::new();
        for word in words {
            let text = decode_text_value(info.id, "command", word)?;
            let command = parse_line_depth(&text, depth.saturating_add(1))?.ok_or_else(|| {
                ParseError::Syntax {
                    command: info.id.to_owned(),
                    token: "an empty command".to_owned(),
                }
            })?;
            commands.push(command);
        }
        return Ok(Some(Command::Batch { commands }));
    }

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
        "insert_track" => Ok(Command::InsertTrack {
            at: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
        }),
        "delete_track" => Ok(Command::DeleteTrack {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
        }),
        "duplicate_track" => Ok(Command::DuplicateTrack {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
        }),
        "move_track" => Ok(Command::MoveTrack {
            from: TrackIndex::new(required_position(args, "from")?),
            to: TrackIndex::new(required_position(args, "to")?),
        }),
        "rename_track" => Ok(Command::RenameTrack {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            name: text_arg(args, "name")?,
        }),
        "set_track_color" => Ok(Command::SetTrackColor {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            color: color_arg(args)?,
        }),
        "set_track_symbol" => Ok(Command::SetTrackSymbol {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            symbol: symbol_arg(args)?,
        }),
        "set_track_volume" => Ok(Command::SetTrackVolume {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            volume: midi_value_arg(args, "volume")?,
        }),
        "set_track_staves" => {
            let track = TrackIndex::new(optional_position(args, "track")?.unwrap_or(0));
            let text = args.required("staves")?;
            let staves = TrackStaves::from_id(&text).ok_or_else(|| {
                bad(
                    args.command,
                    "staves",
                    &text,
                    "both, tab, notation, tab+slash, notation+slash, both+slash or slash",
                )
            })?;
            Ok(Command::SetTrackStaves { track, staves })
        }
        "set_master_volume" => Ok(Command::SetMasterVolume {
            volume: midi_value_arg(args, "volume")?,
        }),
        "set_master_pan" => Ok(Command::SetMasterPan {
            pan: midi_value_arg(args, "pan")?,
        }),
        "set_track_pan" => Ok(Command::SetTrackPan {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            pan: midi_value_arg(args, "pan")?,
        }),
        "set_track_reverb" => Ok(Command::SetTrackReverb {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            reverb: midi_value_arg(args, "reverb")?,
        }),
        "set_track_chorus" => Ok(Command::SetTrackChorus {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            chorus: midi_value_arg(args, "chorus")?,
        }),
        "set_track_mute" => Ok(Command::SetTrackMute {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            mute: bool_arg(args, "mute")?,
        }),
        "set_track_solo" => Ok(Command::SetTrackSolo {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            solo: bool_arg(args, "solo")?,
        }),
        "set_track_program" => Ok(Command::SetTrackProgram {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            program: midi_value_arg(args, "program")?,
        }),
        "set_track_drums" => Ok(Command::SetTrackDrums {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            drums: bool_arg(args, "drums")?,
        }),
        "set_track_capo" => Ok(Command::SetTrackCapo {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            capo: midi_value_arg(args, "capo")?,
        }),
        "set_track_strings" => Ok(Command::SetTrackStrings {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            strings: string_count_arg(args)?,
        }),
        "set_track_string_pitch" => Ok(Command::SetTrackStringPitch {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            string: string_arg(args)?,
            pitch: pitch_arg(args)?,
        }),
        "set_song_text" => Ok(Command::SetSongText {
            field: field_arg(args)?,
            text: text_arg(args, "text")?,
        }),
        "set_page_setup" => Ok(Command::SetPageSetup {
            setup: page_setup_arg(args)?,
        }),
        "set_system_break" => Ok(Command::SetSystemBreak {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            measure: MeasureIndex::new(required_position(args, "measure")?),
            kind: system_break_arg(args)?,
        }),
        "set_score_line_break" => Ok(Command::SetScoreLineBreak {
            measure: MeasureIndex::new(required_position(args, "measure")?),
            on: bool_arg(args, "on")?,
        }),
        "set_bars_per_system" => Ok(Command::SetBarsPerSystem {
            track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
            bars: bars_arg(args)?,
        }),
        "set_tempo" => Ok(Command::SetTempo {
            tempo: tempo_arg(args)?,
        }),
        "set_beat_rest" => Ok(Command::SetBeatRest {
            at: position(args)?,
        }),
        "set_tie" => {
            let at = position(args)?;
            let string = string_arg(args)?;
            let tie = bool_arg(args, "tie")?;
            Ok(Command::SetTie { at, string, tie })
        }
        "set_beat_tie" => {
            let at = position(args)?;
            let tie = bool_arg(args, "tie")?;
            Ok(Command::SetBeatTie { at, tie })
        }
        "set_beat_text" => Ok(Command::SetBeatText {
            at: position(args)?,
            text: text_arg(args, "text")?,
        }),
        "set_lyric_line" => {
            let text = args.required("line")?;
            let line: u8 = text
                .parse()
                .map_err(|_| bad(args.command, "line", &text, "a line from 1 to 5"))?;
            if !(1..=5).contains(&line) {
                return Err(bad(args.command, "line", &text, "a line from 1 to 5"));
            }
            Ok(Command::SetLyricLine {
                line: line.saturating_sub(1),
                start: MeasureIndex::new(required_position(args, "measure")?),
                text: text_arg(args, "text")?,
            })
        }
        "set_dead_note" => {
            let at = position(args)?;
            let string = string_arg(args)?;
            let dead = bool_arg(args, "dead")?;
            Ok(Command::SetDeadNote { at, string, dead })
        }
        "set_chord_diagram" => {
            let at = position(args)?;
            let name = text_arg(args, "name")?;
            let base_text = args.take("base").unwrap_or_else(|| "0".to_owned());
            let base: u8 = base_text
                .parse()
                .map_err(|_| bad(args.command, "base", &base_text, "a fret from 0 to 24"))?;
            let frets_text = text_arg(args, "frets")?;
            let mut diagram = gscore_model::ChordDiagram::new(name);
            diagram.base_fret = base;
            let mut frets = Vec::new();
            for (string, value) in frets_text.split(',').enumerate() {
                let value = value.trim();
                let fret: i8 = if value.eq_ignore_ascii_case("x") {
                    -1
                } else {
                    value.parse().map_err(|_| {
                        bad(args.command, "frets", value, "a fret from 0 to 24, or x")
                    })?
                };
                frets.push((u8::try_from(string).unwrap_or(0), fret));
            }
            diagram.frets = frets;
            Ok(Command::SetChordDiagram {
                at,
                diagram: Some(diagram),
            })
        }
        "set_dynamic" => Ok(Command::SetDynamic {
            at: position(args)?,
            dynamic: dynamic_arg(args)?,
        }),
        "set_measure_tempo" => Ok(Command::SetMeasureTempo {
            measure: MeasureIndex::new(required_position(args, "measure")?),
            tempo: tempo_arg(args)?,
        }),
        "set_time_signature" => Ok(Command::SetTimeSignature {
            measure: MeasureIndex::new(required_position(args, "measure")?),
            numerator: numerator_arg(args)?,
            denominator: denominator_arg(args)?,
        }),
        "set_key_signature" => Ok(Command::SetKeySignature {
            measure: MeasureIndex::new(required_position(args, "measure")?),
            key: key_arg(args)?,
            minor: bool_arg(args, "minor")?,
        }),
        "set_section" => Ok(Command::SetSection {
            measure: MeasureIndex::new(required_position(args, "measure")?),
            title: text_arg(args, "title")?,
            color: optional_color_arg(args)?,
        }),
        "clear_section" => Ok(Command::ClearSection {
            measure: MeasureIndex::new(required_position(args, "measure")?),
        }),
        "set_repeat" => {
            let end = midi_value_arg(args, "end")?;
            Ok(Command::SetRepeat {
                measure: MeasureIndex::new(required_position(args, "measure")?),
                repeat: Repeat {
                    start: bool_arg(args, "start")?,
                    end_repeats: (end > 0).then_some(end),
                    endings: endings_arg(args)?,
                },
            })
        }
        "set_double_bar" => Ok(Command::SetDoubleBar {
            measure: MeasureIndex::new(required_position(args, "measure")?),
            double_bar: bool_arg(args, "double_bar")?,
        }),
        "set_feel" => {
            let text = args.required("feel")?;
            let feel = Feel::from_id(&text).ok_or_else(|| {
                bad(
                    args.command,
                    "feel",
                    &text,
                    "none, eighth, sixteenth, dotted_eighth, dotted_sixteenth, scottish_eighth or scottish_sixteenth",
                )
            })?;
            Ok(Command::SetFeel {
                measure: MeasureIndex::new(required_position(args, "measure")?),
                feel,
            })
        }
        "set_fermata" => {
            let text = args.required("fermata")?;
            let fermata = if text == "none" {
                None
            } else {
                Some(Fermata::from_id(&text).ok_or_else(|| {
                    bad(
                        args.command,
                        "fermata",
                        &text,
                        "none, very_short, short, medium, long or very_long",
                    )
                })?)
            };
            Ok(Command::SetFermata {
                measure: MeasureIndex::new(required_position(args, "measure")?),
                fermata,
            })
        }
        "set_clef" => {
            let text = args.required("clef")?;
            let clef = Clef::from_id(&text)
                .ok_or_else(|| bad(args.command, "clef", &text, "treble, bass, alto or tenor"))?;
            Ok(Command::SetClef {
                track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
                measure: MeasureIndex::new(required_position(args, "measure")?),
                clef,
            })
        }
        "set_free_time" => Ok(Command::SetFreeTime {
            measure: MeasureIndex::new(required_position(args, "measure")?),
            free_time: bool_arg(args, "free_time")?,
        }),
        "set_simile_mark" => {
            let text = args.required("mark")?;
            let mark = if text == "none" {
                None
            } else {
                Some(SimileMark::from_id(&text).ok_or_else(|| {
                    bad(
                        args.command,
                        "mark",
                        &text,
                        "none, simple, first_of_double or second_of_double",
                    )
                })?)
            };
            Ok(Command::SetSimileMark {
                track: TrackIndex::new(optional_position(args, "track")?.unwrap_or(0)),
                measure: MeasureIndex::new(required_position(args, "measure")?),
                mark,
            })
        }
        "set_note_effect" => {
            let at = position(args)?;
            let string = string_arg(args)?;
            let effect = note_effect_arg(args)?;
            Ok(Command::SetNoteEffect { at, string, effect })
        }
        "set_beat_effect" => {
            let at = position(args)?;
            let effect = beat_effect_arg(args)?;
            Ok(Command::SetBeatEffect { at, effect })
        }
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

/// The `pitch` parameter: a note name such as `E2`, or a MIDI number.
fn pitch_arg(args: &mut Args) -> Result<Pitch, ParseError> {
    let text = args.required("pitch")?;
    Pitch::parse(&text).map_err(|_| {
        bad(
            args.command,
            "pitch",
            &text,
            "a note name such as E2 or C#4, or a MIDI number from 0 to 127",
        )
    })
}

/// The `field` parameter: which song text field to set.
fn field_arg(args: &mut Args) -> Result<SongTextField, ParseError> {
    let text = args.required("field")?;
    let expected = "title, subtitle, artist, album, lyricist, composer, copyright, \
                    tab_author, instructions or notes";
    SongTextField::from_id(&text).ok_or_else(|| bad(args.command, "field", &text, expected))
}

/// The `dynamic` parameter: a mark from `ppp` to `fff`.
fn dynamic_arg(args: &mut Args) -> Result<Dynamic, ParseError> {
    let text = args.required("dynamic")?;
    Dynamic::from_id(&text).ok_or_else(|| {
        bad(
            args.command,
            "dynamic",
            &text,
            "ppp, pp, p, mp, mf, f, ff or fff",
        )
    })
}

/// The `tempo` parameter: quarter notes per minute.
fn tempo_arg(args: &mut Args) -> Result<u32, ParseError> {
    let text = args.required("tempo")?;
    let expected = "a tempo from 1 to 1000 BPM";
    let tempo: u32 = text
        .parse()
        .map_err(|_| bad(args.command, "tempo", &text, expected))?;
    if !(1..=MAX_TEMPO).contains(&tempo) {
        return Err(bad(args.command, "tempo", &text, expected));
    }
    Ok(tempo)
}

/// The `strings` parameter: how many strings a track has.
fn string_count_arg(args: &mut Args) -> Result<usize, ParseError> {
    let text = args.required("strings")?;
    let expected = "a whole number of strings from 1 to 12";
    let value: usize = text
        .parse()
        .map_err(|_| bad(args.command, "strings", &text, expected))?;
    if !(1..=MAX_STRINGS).contains(&value) {
        return Err(bad(args.command, "strings", &text, expected));
    }
    Ok(value)
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

/// The id of a line break in the command language.
pub(crate) fn system_break_id(kind: SystemBreak) -> &'static str {
    match kind {
        SystemBreak::None => "none",
        SystemBreak::Line => "line",
        SystemBreak::Page => "page",
    }
}

/// The `break` parameter.
fn system_break_arg(args: &mut Args) -> Result<SystemBreak, ParseError> {
    let text = args.required("break")?;
    [SystemBreak::None, SystemBreak::Line, SystemBreak::Page]
        .into_iter()
        .find(|kind| system_break_id(*kind) == text)
        .ok_or_else(|| bad(args.command, "break", &text, "line, page or none"))
}

/// The `bars` parameter: 1 to 32, or `auto`.
fn bars_arg(args: &mut Args) -> Result<Option<u8>, ParseError> {
    let text = args.required("bars")?;
    if text == "auto" {
        return Ok(None);
    }
    let expected = "a whole number from 1 to 32, or auto";
    let bars: u8 = text
        .parse()
        .map_err(|_| bad(args.command, "bars", &text, expected))?;
    if !(1..=MAX_BARS_PER_SYSTEM).contains(&bars) {
        return Err(bad(args.command, "bars", &text, expected));
    }
    Ok(Some(bars))
}

/// A whole number parameter, or `default` when it is left out.
fn number_or(
    args: &mut Args,
    name: &'static str,
    default: u16,
    expected: &'static str,
) -> Result<u16, ParseError> {
    match args.take(name) {
        Some(text) => text
            .parse()
            .map_err(|_| bad(args.command, name, &text, expected)),
        None => Ok(default),
    }
}

/// A `true`/`false` parameter, or `default` when it is left out.
fn bool_or(args: &mut Args, name: &'static str, default: bool) -> Result<bool, ParseError> {
    if args.values.contains_key(name) {
        bool_arg(args, name)
    } else {
        Ok(default)
    }
}

/// The page setup's parameters; left-out ones take Guitar Pro's A4 defaults.
fn page_setup_arg(args: &mut Args) -> Result<PageSetup, ParseError> {
    let d = PageSetup::default();
    let mm = "a whole number of millimetres";
    let setup = PageSetup {
        width_mm: number_or(args, "width", d.width_mm, mm)?,
        height_mm: number_or(args, "height", d.height_mm, mm)?,
        margins: gscore_model::Margins {
            top: number_or(args, "top", d.margins.top, mm)?,
            right: number_or(args, "right", d.margins.right, mm)?,
            bottom: number_or(args, "bottom", d.margins.bottom, mm)?,
            left: number_or(args, "left", d.margins.left, mm)?,
        },
        scale_percent: number_or(args, "scale", d.scale_percent, "a whole percentage")?,
        page_numbers: bool_or(args, "page_numbers", d.page_numbers)?,
        copyright: bool_or(args, "copyright", d.copyright)?,
        running_header: bool_or(args, "header", d.running_header)?,
    };
    setup.check().map_err(|_| {
        bad(
            args.command,
            "width",
            &format!("{}x{}", setup.width_mm, setup.height_mm),
            "a page of 70 to 1000 mm with margins leaving 40 mm for the music, and a scale \
             of 50 to 200 %",
        )
    })?;
    Ok(setup)
}

/// A `true`/`false` parameter.
fn bool_arg(args: &mut Args, name: &'static str) -> Result<bool, ParseError> {
    let text = args.required(name)?;
    match text.as_str() {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        _ => Err(bad(args.command, name, &text, "true or false")),
    }
}

/// What starts a text written by [`encode_text`]. Percent-encoding never produces two
/// `%` in a row, so such a value is never mistaken for a plain one.
const ENCODED: &str = "%%";

/// `text` as one command-line word that [`text_arg`] reads back exactly: `%%`, then every
/// byte other than ASCII letters, digits, `-`, `_`, `.` and `~` as `%XX`. Never empty.
pub(crate) fn encode_text(text: &str) -> String {
    let mut out = String::from(ENCODED);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// A text parameter: as written, or decoded if it was written by [`encode_text`].
fn text_arg(args: &mut Args, name: &'static str) -> Result<String, ParseError> {
    let text = args.required(name)?;
    decode_text_value(args.command, name, &text)
}

/// A command-line word as text: as written, or decoded if it was written by
/// [`encode_text`].
fn decode_text_value(
    command: &'static str,
    name: &'static str,
    text: &str,
) -> Result<String, ParseError> {
    let Some(encoded) = text.strip_prefix(ENCODED) else {
        return Ok(text.to_owned());
    };
    let expected = "text written by GuitarScore (%XX escapes)";
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut rest = encoded.as_bytes();
    while let Some((&byte, after)) = rest.split_first() {
        if byte == b'%' {
            let digits = after
                .get(..2)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .ok_or_else(|| bad(command, name, text, expected))?;
            bytes.push(digits);
            rest = after.get(2..).unwrap_or_default();
        } else {
            bytes.push(byte);
            rest = after;
        }
    }
    String::from_utf8(bytes).map_err(|_| bad(command, name, text, expected))
}

/// Decodes a text written by [`encode_text`], for display (for example in the history
/// table). `None` if `text` is not an encoded value or is malformed.
pub(crate) fn decode_text(text: &str) -> Option<String> {
    let encoded = text.strip_prefix(ENCODED)?;
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut rest = encoded.as_bytes();
    while let Some((&byte, after)) = rest.split_first() {
        if byte == b'%' {
            let hex = after.get(..2)?;
            let value = u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
            bytes.push(value);
            rest = after.get(2..)?;
        } else {
            bytes.push(byte);
            rest = after;
        }
    }
    String::from_utf8(bytes).ok()
}

/// The `numerator` parameter: beats per measure.
fn numerator_arg(args: &mut Args) -> Result<u8, ParseError> {
    let text = args.required("numerator")?;
    let expected = "a whole number of beats from 1 to 64";
    let value: u8 = text
        .parse()
        .map_err(|_| bad(args.command, "numerator", &text, expected))?;
    if !(1..=64).contains(&value) {
        return Err(bad(args.command, "numerator", &text, expected));
    }
    Ok(value)
}

/// The `denominator` parameter: the note value of one beat.
fn denominator_arg(args: &mut Args) -> Result<u16, ParseError> {
    let text = args.required("denominator")?;
    let expected = "a note value: 1, 2, 4, 8, 16, 32, 64 or 128";
    let value: u16 = text
        .parse()
        .map_err(|_| bad(args.command, "denominator", &text, expected))?;
    if value > 128 || !value.is_power_of_two() {
        return Err(bad(args.command, "denominator", &text, expected));
    }
    Ok(value)
}

/// The `effect` and `value` parameters of a note effect.
fn note_effect_arg(args: &mut Args) -> Result<NoteEffect, ParseError> {
    let effect = args.required("effect")?;
    let value = text_arg(args, "value")?;
    let text = format!("{effect}={value}");
    crate::effect_text::parse_note_effect(&effect, &value).map_err(|_| {
        bad(
            args.command,
            "value",
            &text,
            "a known note effect id and its value",
        )
    })
}

/// The `effect` and `value` parameters of a beat effect.
fn beat_effect_arg(args: &mut Args) -> Result<BeatEffect, ParseError> {
    let effect = args.required("effect")?;
    let value = text_arg(args, "value")?;
    let text = format!("{effect}={value}");
    crate::effect_text::parse_beat_effect(&effect, &value).map_err(|_| {
        bad(
            args.command,
            "value",
            &text,
            "a known beat effect id and its value",
        )
    })
}

/// The `key` parameter: sharps or flats.
fn key_arg(args: &mut Args) -> Result<i8, ParseError> {
    let text = args.required("key")?;
    let expected = "a whole number from -7 to 7";
    let value: i8 = text
        .parse()
        .map_err(|_| bad(args.command, "key", &text, expected))?;
    if !(-7..=7).contains(&value) {
        return Err(bad(args.command, "key", &text, expected));
    }
    Ok(value)
}

/// A 0-127 parameter (volume, pan).
fn midi_value_arg(args: &mut Args, name: &'static str) -> Result<u8, ParseError> {
    let text = args.required(name)?;
    let expected = "a whole number from 0 to 127";
    let value: u8 = text
        .parse()
        .map_err(|_| bad(args.command, name, &text, expected))?;
    if value > 127 {
        return Err(bad(args.command, name, &text, expected));
    }
    Ok(value)
}

/// A bit set of alternative endings (bit 0 is ending 1).
fn endings_arg(args: &mut Args) -> Result<u8, ParseError> {
    let text = args.required("endings")?;
    text.parse().map_err(|_| {
        bad(
            args.command,
            "endings",
            &text,
            "a whole number from 0 to 255",
        )
    })
}

/// A `rrggbb` color, optionally written as `0xrrggbb`.
fn color_arg(args: &mut Args) -> Result<gscore_model::TrackColor, ParseError> {
    let text = args.required("color")?;
    parse_color(args.command, "color", &text)
}

/// An instrument symbol by its stored id ("electricGuitar", "drums", ...).
fn symbol_arg(args: &mut Args) -> Result<gscore_model::InstrumentSymbol, ParseError> {
    let text = args.required("symbol")?;
    gscore_model::InstrumentSymbol::from_id(&text).ok_or_else(|| ParseError::BadValue {
        command: args.command.to_owned(),
        parameter: "symbol".to_owned(),
        value: text,
        expected: "an instrument symbol id (see `gscore-cli commands`)",
    })
}

/// A section color, or the default when the parameter is omitted.
fn optional_color_arg(args: &mut Args) -> Result<gscore_model::TrackColor, ParseError> {
    match args.take("color") {
        Some(text) => parse_color(args.command, "color", &text),
        None => Ok(gscore_model::Section::DEFAULT_COLOR),
    }
}

/// A color value as `rrggbb` or `0xrrggbb`.
fn parse_color(
    command: &'static str,
    name: &'static str,
    text: &str,
) -> Result<gscore_model::TrackColor, ParseError> {
    let expected = "a color as rrggbb in hexadecimal";
    let hex = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    let rgb = u32::from_str_radix(hex, 16)
        .ok()
        .filter(|rgb| *rgb <= 0x00FF_FFFF)
        .ok_or_else(|| bad(command, name, text, expected))?;
    Ok(gscore_model::TrackColor::new(rgb))
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
mod tests;
