//! `panic_hunt`: every registered command, adversarial parameters and varied songs.
//!
//! Nothing here checks that a command does something useful; it checks that parsing and
//! applying it ends in `Ok` or `Err`, never a panic, and that a successful edit leaves a
//! valid song that can be reverted exactly.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

mod common;

use common::{pos, test_songs};
use gscore_edit::{COMMANDS, Command, Editor, parse_line};
use gscore_model::{Fret, MeasureIndex, NoteValue, Song, StringNumber, differences};

/// Values that are negative, zero, huge, not numbers, not note lengths, or empty.
const RAW_VALUES: &[&str] = &[
    "",
    "-1",
    "0",
    "1",
    "2",
    "999999",
    "99999999999999999999",
    "NaN",
    "abc",
    "1/0",
    "1/3",
    "1/4",
    "1/4.",
    "1/8*65:2",
    "18446744073709551615",
];

/// A plausible value for a parameter, used to build commands where the other parameters are
/// valid so the interesting one is actually reached.
fn valid_value(name: &str) -> &'static str {
    match name {
        "fret" => "5",
        "duration" => "1/4",
        "dynamic" => "mf",
        "feel" => "eighth",
        "fermata" => "medium",
        _ => "1",
    }
}

/// All parameters of `info` with plausible values.
fn valid_args(info: &gscore_edit::CommandInfo) -> Vec<String> {
    info.params
        .iter()
        .map(|param| format!("{}={}", param.name, valid_value(param.name)))
        .collect()
}

#[test]
fn every_command_survives_adversarial_text() {
    let mut successes = 0_usize;
    for (song_name, song) in test_songs() {
        for info in COMMANDS {
            let mut lines = vec![
                info.id.to_owned(),
                format!("{} unknown=1", info.id),
                format!("{} not_a_key_value", info.id),
                format!("{} {}", info.id, valid_args(info).join(" ")),
            ];
            for param in info.params {
                for value in RAW_VALUES {
                    lines.push(format!("{} {}={}", info.id, param.name, value));
                    let args: Vec<String> = info
                        .params
                        .iter()
                        .map(|other| {
                            if other.name == param.name {
                                format!("{}={}", other.name, value)
                            } else {
                                format!("{}={}", other.name, valid_value(other.name))
                            }
                        })
                        .collect();
                    lines.push(format!("{} {}", info.id, args.join(" ")));
                }
                lines.push(format!(
                    "{} {}={} {}={}",
                    info.id,
                    param.name,
                    valid_value(param.name),
                    param.name,
                    valid_value(param.name)
                ));
                let without: Vec<String> = info
                    .params
                    .iter()
                    .filter(|other| other.name != param.name)
                    .map(|other| format!("{}={}", other.name, valid_value(other.name)))
                    .collect();
                lines.push(format!("{} {}", info.id, without.join(" ")));
            }
            for line in lines {
                if hunt_line(song_name, &song, &line) {
                    successes = successes.saturating_add(1);
                }
            }
        }
    }
    assert!(successes > 0, "the hunt never reached a successful edit");
}

/// Parses and (if it parses) applies one line. Returns true when the edit was applied.
fn hunt_line(song_name: &str, song: &Song, line: &str) -> bool {
    let parsed = parse_line(line);
    let Ok(Some(command)) = parsed else {
        return false;
    };
    if command.is_history() {
        let mut editor = Editor::new(song.clone());
        let _ = editor.execute(command);
        return false;
    }
    let enabled = command.enabled(song);
    let mut edited = song.clone();
    let result = command.apply(&mut edited);
    assert_eq!(
        enabled,
        result.is_ok(),
        "{song_name}: {} from \"{line}\": enabled={enabled}, apply={result:?}",
        command.id()
    );
    match result {
        Ok(change) => {
            // The model validated the song; asking for its issues must also be safe.
            let _ = edited.issues();
            edited.revert(&change).expect("revert an applied change");
            assert!(
                differences(song, &edited).is_empty(),
                "{song_name}: reverting {} from \"{line}\" changed the song",
                command.id()
            );
            true
        }
        Err(_) => false,
    }
}

#[test]
fn extreme_typed_commands_never_panic() {
    let huge = usize::MAX;
    let commands = [
        Command::SetFret {
            at: pos(huge, huge, huge),
            string: StringNumber::new(255).unwrap(),
            fret: Fret::new(127).unwrap(),
        },
        Command::ClearNote {
            at: pos(huge, 0, 0),
            string: StringNumber::new(1).unwrap(),
        },
        Command::SetDuration {
            at: pos(0, 0, 0),
            duration: NoteValue::new(128, 2, Some((64, 63))).unwrap(),
        },
        Command::InsertBeat {
            at: pos(huge, huge, huge),
            duration: NoteValue::new(1, 2, Some((64, 64))).unwrap(),
        },
        Command::DeleteBeat {
            at: pos(0, 0, huge),
        },
        Command::InsertMeasure {
            measure: MeasureIndex::new(huge),
        },
        Command::DeleteMeasure {
            measure: MeasureIndex::new(huge),
        },
        Command::SetNoteEffect {
            at: pos(0, 0, 0),
            string: StringNumber::new(1).unwrap(),
            effect: gscore_model::NoteEffect::Grace(Some(gscore_model::Grace {
                fret: Fret::new(127).unwrap(),
                speed: gscore_model::GraceSpeed::Fast,
                on_beat: true,
                dead: true,
                hammer: false,
            })),
        },
    ];
    for (name, song) in test_songs() {
        for command in &commands {
            let mut edited = song.clone();
            let result = command.apply(&mut edited);
            assert_eq!(
                command.enabled(&song),
                result.is_ok(),
                "{name}: {} said enabled={}, apply={result:?}",
                command.id(),
                command.enabled(&song)
            );
            if let Ok(change) = result {
                let _ = edited.issues();
                edited.revert(&change).expect("revert an applied change");
                assert!(
                    differences(&song, &edited).is_empty(),
                    "{name}: {}",
                    command.id()
                );
            }
        }
    }
}
