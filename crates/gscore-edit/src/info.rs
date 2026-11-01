//! Metadata for every editing command: id, aliases, label, help text and parameters.

/// One parameter of a command.
#[derive(Debug, Clone, Copy)]
pub struct ParamInfo {
    /// The key as written in a command line (`track=1`).
    pub name: &'static str,
    /// Whether the parameter must be given.
    pub required: bool,
    /// The value used when the parameter is omitted, if any.
    pub default: Option<&'static str>,
    /// What the parameter means.
    pub help: &'static str,
}

/// A command's metadata, as listed by `gscore-cli commands` and shown in the UI.
#[derive(Debug, Clone, Copy)]
pub struct CommandInfo {
    /// The command id `apply` and command lines use.
    pub id: &'static str,
    /// Other accepted ids for the same command.
    pub aliases: &'static [&'static str],
    /// A short, imperative label.
    pub label: &'static str,
    /// What the command does.
    pub help: &'static str,
    /// The parameters, in the order they are normally written.
    pub params: &'static [ParamInfo],
}

const TRACK: ParamInfo = ParamInfo {
    name: "track",
    required: false,
    default: Some("1"),
    help: "Track, 1-based",
};

const MEASURE: ParamInfo = ParamInfo {
    name: "measure",
    required: true,
    default: None,
    help: "Measure, 1-based",
};

const VOICE: ParamInfo = ParamInfo {
    name: "voice",
    required: false,
    default: Some("1"),
    help: "Voice, 1-based",
};

const BEAT: ParamInfo = ParamInfo {
    name: "beat",
    required: true,
    default: None,
    help: "Beat, 1-based",
};

const STRING: ParamInfo = ParamInfo {
    name: "string",
    required: true,
    default: None,
    help: "String, 1-based (1 = highest)",
};

const FRET: ParamInfo = ParamInfo {
    name: "fret",
    required: true,
    default: None,
    help: "Fret, 0 to 127",
};

const DURATION: ParamInfo = ParamInfo {
    name: "duration",
    required: true,
    default: None,
    help: "Note length: 1/4, 1/8., 1/16.., optionally *3:2 for a tuplet",
};

pub(crate) const SET_FRET: CommandInfo = CommandInfo {
    id: "set_fret",
    aliases: &["enter_fret"],
    label: "Set a fret",
    help: "Puts a note at this fret on one string, replacing any note already there but \
           keeping its effects.",
    params: &[TRACK, MEASURE, VOICE, BEAT, STRING, FRET],
};

pub(crate) const CLEAR_NOTE: CommandInfo = CommandInfo {
    id: "clear_note",
    aliases: &[],
    label: "Remove a note",
    help: "Removes the note on one string. Removing the last note turns the beat into a rest.",
    params: &[TRACK, MEASURE, VOICE, BEAT, STRING],
};

pub(crate) const SET_DURATION: CommandInfo = CommandInfo {
    id: "set_duration",
    aliases: &[],
    label: "Set the beat length",
    help: "Sets the length of one beat (its note value, dots and tuplet).",
    params: &[TRACK, MEASURE, VOICE, BEAT, DURATION],
};

pub(crate) const INSERT_BEAT: CommandInfo = CommandInfo {
    id: "insert_beat",
    aliases: &[],
    label: "Insert a beat",
    help: "Inserts a rest beat before the given beat. Use one past the last beat to append.",
    params: &[TRACK, MEASURE, VOICE, BEAT, DURATION],
};

pub(crate) const DELETE_BEAT: CommandInfo = CommandInfo {
    id: "delete_beat",
    aliases: &[],
    label: "Delete a beat",
    help: "Removes one beat from a voice.",
    params: &[TRACK, MEASURE, VOICE, BEAT],
};

pub(crate) const INSERT_MEASURE: CommandInfo = CommandInfo {
    id: "insert_measure",
    aliases: &[],
    label: "Insert a measure",
    help: "Inserts a rest-filled measure in every track, keeping the time signature in \
           effect. Use one past the last measure to append.",
    params: &[MEASURE],
};

pub(crate) const DELETE_MEASURE: CommandInfo = CommandInfo {
    id: "delete_measure",
    aliases: &[],
    label: "Delete a measure",
    help: "Removes one measure from every track. The last remaining measure cannot be \
           deleted.",
    params: &[MEASURE],
};

pub(crate) const UNDO: CommandInfo = CommandInfo {
    id: "undo",
    aliases: &[],
    label: "Undo",
    help: "Undoes the last applied edit.",
    params: &[],
};

pub(crate) const REDO: CommandInfo = CommandInfo {
    id: "redo",
    aliases: &[],
    label: "Redo",
    help: "Re-applies the last undone edit.",
    params: &[],
};

/// Every command, in the order `gscore-cli commands` prints them.
pub const COMMANDS: &[CommandInfo] = &[
    SET_FRET,
    CLEAR_NOTE,
    SET_DURATION,
    INSERT_BEAT,
    DELETE_BEAT,
    INSERT_MEASURE,
    DELETE_MEASURE,
    UNDO,
    REDO,
];

/// The command with this id or alias, if any.
pub fn find_command(id: &str) -> Option<&'static CommandInfo> {
    COMMANDS
        .iter()
        .find(|info| info.id == id || info.aliases.contains(&id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn ids_and_aliases_are_unique() {
        let mut seen = BTreeSet::new();
        for info in COMMANDS {
            assert!(seen.insert(info.id), "duplicate id {}", info.id);
            for alias in info.aliases {
                assert!(seen.insert(*alias), "duplicate alias {alias}");
            }
        }
    }

    #[test]
    fn parameter_metadata_is_consistent() {
        for info in COMMANDS {
            assert!(!info.label.is_empty(), "{}", info.id);
            assert!(!info.help.is_empty(), "{}", info.id);
            let mut names = BTreeSet::new();
            for param in info.params {
                assert!(names.insert(param.name), "{}: {}", info.id, param.name);
                assert!(
                    !(param.required && param.default.is_some()),
                    "{}: {} is required and has a default",
                    info.id,
                    param.name
                );
                assert!(
                    param.required || param.default.is_some(),
                    "{}: optional {} has no default",
                    info.id,
                    param.name
                );
            }
        }
    }

    #[test]
    fn every_command_is_found_by_id_and_alias() {
        for info in COMMANDS {
            assert_eq!(find_command(info.id).map(|found| found.id), Some(info.id));
            for alias in info.aliases {
                assert_eq!(find_command(alias).map(|found| found.id), Some(info.id));
            }
        }
        assert!(find_command("not_a_command").is_none());
    }
}
