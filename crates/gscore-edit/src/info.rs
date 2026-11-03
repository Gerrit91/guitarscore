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

const NAME: ParamInfo = ParamInfo {
    name: "name",
    required: true,
    default: None,
    help: "Text without spaces (the command line has no quoting)",
};

const FROM: ParamInfo = ParamInfo {
    name: "from",
    required: true,
    default: None,
    help: "Track to move, 1-based",
};

const TO: ParamInfo = ParamInfo {
    name: "to",
    required: true,
    default: None,
    help: "Where it ends up, 1-based",
};

const VOLUME: ParamInfo = ParamInfo {
    name: "volume",
    required: true,
    default: None,
    help: "Volume, 0 to 127",
};

const PAN: ParamInfo = ParamInfo {
    name: "pan",
    required: true,
    default: None,
    help: "Pan, 0 (left) to 127 (right), 64 is the middle",
};

const REVERB: ParamInfo = ParamInfo {
    name: "reverb",
    required: true,
    default: None,
    help: "Reverb send, 0 (dry) to 127",
};

const CHORUS: ParamInfo = ParamInfo {
    name: "chorus",
    required: true,
    default: None,
    help: "Chorus send, 0 (none) to 127",
};

const MUTE: ParamInfo = ParamInfo {
    name: "mute",
    required: true,
    default: None,
    help: "true or false",
};

const SOLO: ParamInfo = ParamInfo {
    name: "solo",
    required: true,
    default: None,
    help: "true or false",
};

const COLOR: ParamInfo = ParamInfo {
    name: "color",
    required: true,
    default: None,
    help: "Color as rrggbb in hexadecimal",
};

const PROGRAM: ParamInfo = ParamInfo {
    name: "program",
    required: true,
    default: None,
    help: "General MIDI program, 0 to 127",
};

const DRUMS: ParamInfo = ParamInfo {
    name: "drums",
    required: true,
    default: None,
    help: "true for a drum kit, false for a pitched instrument",
};

const CAPO: ParamInfo = ParamInfo {
    name: "capo",
    required: true,
    default: None,
    help: "Capo fret, 0 (none) to 127",
};

const STRINGS: ParamInfo = ParamInfo {
    name: "strings",
    required: true,
    default: None,
    help: "Number of strings, 1 to 12",
};

const PITCH: ParamInfo = ParamInfo {
    name: "pitch",
    required: true,
    default: None,
    help: "Open-string pitch: a note name (E2, C#4, Bb3) or a MIDI number 0 to 127",
};

const FIELD: ParamInfo = ParamInfo {
    name: "field",
    required: true,
    default: None,
    help: "Song field: title, subtitle, artist, album, lyricist, composer, copyright, \
           tab_author, instructions or notes",
};

const TEXT: ParamInfo = ParamInfo {
    name: "text",
    required: true,
    default: None,
    help: "The new text (percent-encoded for spaces and line breaks, as GuitarScore writes it)",
};

const TEMPO: ParamInfo = ParamInfo {
    name: "tempo",
    required: true,
    default: None,
    help: "Tempo in quarter notes per minute, 1 to 1000",
};

const TIE: ParamInfo = ParamInfo {
    name: "tie",
    required: true,
    default: None,
    help: "true to tie the note to the previous one, false to untie it",
};

const DYNAMIC: ParamInfo = ParamInfo {
    name: "dynamic",
    required: true,
    default: None,
    help: "ppp, pp, p, mp, mf, f, ff or fff",
};

const DEAD: ParamInfo = ParamInfo {
    name: "dead",
    required: true,
    default: None,
    help: "true for a dead (muted, x) note, false for a normal one",
};

const NUMERATOR: ParamInfo = ParamInfo {
    name: "numerator",
    required: true,
    default: None,
    help: "Beats per measure, 1 to 64",
};

const DENOMINATOR: ParamInfo = ParamInfo {
    name: "denominator",
    required: true,
    default: None,
    help: "Note value of one beat: 1, 2, 4, 8, 16, 32, 64 or 128",
};

const KEY: ParamInfo = ParamInfo {
    name: "key",
    required: true,
    default: None,
    help: "Number of sharps (positive) or flats (negative), -7 to 7",
};

const MINOR: ParamInfo = ParamInfo {
    name: "minor",
    required: true,
    default: None,
    help: "true for a minor key, false for a major one",
};

const TITLE: ParamInfo = ParamInfo {
    name: "title",
    required: true,
    default: None,
    help: "Section title (percent-encoded for spaces, as GuitarScore writes it)",
};

const SECTION_COLOR: ParamInfo = ParamInfo {
    name: "color",
    required: false,
    default: Some("ff0000"),
    help: "Marker color as rrggbb in hexadecimal",
};

const START: ParamInfo = ParamInfo {
    name: "start",
    required: true,
    default: None,
    help: "true to start a repeat at the measure, false for none",
};

const END: ParamInfo = ParamInfo {
    name: "end",
    required: true,
    default: None,
    help: "How many times the section is played again, 1 to 127, or 0 for none",
};

const ENDINGS: ParamInfo = ParamInfo {
    name: "endings",
    required: true,
    default: None,
    help: "Alternative endings as a bit set (1 = ending 1, 3 = endings 1 and 2), 0 for none",
};

const DOUBLE_BAR: ParamInfo = ParamInfo {
    name: "double_bar",
    required: true,
    default: None,
    help: "true for a double barline at the end of the measure, false for a normal one",
};

const FEEL: ParamInfo = ParamInfo {
    name: "feel",
    required: true,
    default: None,
    help: "none, eighth, sixteenth, dotted_eighth, dotted_sixteenth, scottish_eighth or scottish_sixteenth",
};

const FERMATA: ParamInfo = ParamInfo {
    name: "fermata",
    required: true,
    default: None,
    help: "none, very_short, short, medium, long or very_long",
};

const EFFECT: ParamInfo = ParamInfo {
    name: "effect",
    required: true,
    default: None,
    help: "Effect id (hammer, bend, slide, harmonic, ghost, accent, palm_mute, ...)",
};

const EFFECT_VALUE: ParamInfo = ParamInfo {
    name: "value",
    required: true,
    default: None,
    help: "The effect's state: true/false, an id such as heavy, or a bend shape",
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

pub(crate) const INSERT_TRACK: CommandInfo = CommandInfo {
    id: "insert_track",
    aliases: &[],
    label: "Insert a track",
    help: "Inserts a new, rest-filled track before the given one; one past the last track \
           appends.",
    params: &[TRACK],
};

pub(crate) const DELETE_TRACK: CommandInfo = CommandInfo {
    id: "delete_track",
    aliases: &[],
    label: "Delete a track",
    help: "Removes a track. The last remaining track cannot be deleted.",
    params: &[TRACK],
};

pub(crate) const DUPLICATE_TRACK: CommandInfo = CommandInfo {
    id: "duplicate_track",
    aliases: &[],
    label: "Duplicate a track",
    help: "Copies a track (notes, tuning, color and sound) directly after it.",
    params: &[TRACK],
};

pub(crate) const MOVE_TRACK: CommandInfo = CommandInfo {
    id: "move_track",
    aliases: &[],
    label: "Move a track",
    help: "Moves a track to another position; `to` is its final 1-based position.",
    params: &[FROM, TO],
};

pub(crate) const RENAME_TRACK: CommandInfo = CommandInfo {
    id: "rename_track",
    aliases: &[],
    label: "Rename a track",
    help: "Gives a track a new name. The command line cannot pass spaces.",
    params: &[TRACK, NAME],
};

pub(crate) const SET_TRACK_COLOR: CommandInfo = CommandInfo {
    id: "set_track_color",
    aliases: &[],
    label: "Set the track color",
    help: "Sets the color a track is shown in, as 24-bit rrggbb.",
    params: &[TRACK, COLOR],
};

pub(crate) const SET_TRACK_VOLUME: CommandInfo = CommandInfo {
    id: "set_track_volume",
    aliases: &[],
    label: "Set the track volume",
    help: "Sets a track's volume; other tracks keep theirs even if they shared a channel.",
    params: &[TRACK, VOLUME],
};

const STAVES: ParamInfo = ParamInfo {
    name: "staves",
    required: true,
    default: None,
    help: "both (notation and tab), tab or notation",
};

pub(crate) const SET_TRACK_STAVES: CommandInfo = CommandInfo {
    id: "set_track_staves",
    aliases: &[],
    label: "Choose the shown staves",
    help: "Shows standard notation and tablature, only the tablature, or only notation.",
    params: &[TRACK, STAVES],
};

pub(crate) const SET_MASTER_VOLUME: CommandInfo = CommandInfo {
    id: "set_master_volume",
    aliases: &[],
    label: "Set the master volume",
    help: "Scales every track's volume: 97 (Guitar Pro's default) plays each at its own \
           volume, lower is quieter, higher louder, 0 is silent.",
    params: &[VOLUME],
};

pub(crate) const SET_MASTER_PAN: CommandInfo = CommandInfo {
    id: "set_master_pan",
    aliases: &[],
    label: "Set the master pan",
    help: "Shifts every track's pan: 64 leaves them, lower moves all left, higher right.",
    params: &[PAN],
};

pub(crate) const SET_TRACK_PAN: CommandInfo = CommandInfo {
    id: "set_track_pan",
    aliases: &[],
    label: "Set the track pan",
    help: "Sets a track's pan: 0 is left, 64 the middle and 127 right.",
    params: &[TRACK, PAN],
};

pub(crate) const SET_TRACK_REVERB: CommandInfo = CommandInfo {
    id: "set_track_reverb",
    aliases: &[],
    label: "Set the track reverb",
    help: "Sets how much of a track goes to the synthesizer's reverb (MIDI controller 91).",
    params: &[TRACK, REVERB],
};

pub(crate) const SET_TRACK_CHORUS: CommandInfo = CommandInfo {
    id: "set_track_chorus",
    aliases: &[],
    label: "Set the track chorus",
    help: "Sets how much of a track goes to the synthesizer's chorus (MIDI controller 93).",
    params: &[TRACK, CHORUS],
};

pub(crate) const SET_TRACK_MUTE: CommandInfo = CommandInfo {
    id: "set_track_mute",
    aliases: &[],
    label: "Mute or unmute a track",
    help: "A muted track does not play.",
    params: &[TRACK, MUTE],
};

pub(crate) const SET_TRACK_SOLO: CommandInfo = CommandInfo {
    id: "set_track_solo",
    aliases: &[],
    label: "Solo or unsolo a track",
    help: "While any track is soloed, only soloed tracks play.",
    params: &[TRACK, SOLO],
};

pub(crate) const SET_TRACK_PROGRAM: CommandInfo = CommandInfo {
    id: "set_track_program",
    aliases: &[],
    label: "Set the track sound",
    help: "Sets the General MIDI program (the SoundFont preset) a track plays with.",
    params: &[TRACK, PROGRAM],
};

pub(crate) const SET_TRACK_DRUMS: CommandInfo = CommandInfo {
    id: "set_track_drums",
    aliases: &[],
    label: "Make a track a drum kit",
    help: "A drum kit's notes are General MIDI drum sounds on the percussion channel.",
    params: &[TRACK, DRUMS],
};

pub(crate) const SET_TRACK_CAPO: CommandInfo = CommandInfo {
    id: "set_track_capo",
    aliases: &[],
    label: "Set the track capo",
    help: "Sets the capo fret: 0 means no capo.",
    params: &[TRACK, CAPO],
};

pub(crate) const SET_TRACK_STRINGS: CommandInfo = CommandInfo {
    id: "set_track_strings",
    aliases: &[],
    label: "Set the string count",
    help: "Changes how many strings a track has. Removing strings that still hold notes \
           is refused; added strings are tuned in fourths below the lowest.",
    params: &[TRACK, STRINGS],
};

pub(crate) const SET_TRACK_STRING_PITCH: CommandInfo = CommandInfo {
    id: "set_track_string_pitch",
    aliases: &[],
    label: "Tune one string",
    help: "Sets the open pitch of one string, as a note name (E2, C#4) or a MIDI number.",
    params: &[TRACK, STRING, PITCH],
};

pub(crate) const SET_SONG_TEXT: CommandInfo = CommandInfo {
    id: "set_song_text",
    aliases: &[],
    label: "Set song information",
    help: "Sets one text field of the song (title, artist, notes, ...).",
    params: &[FIELD, TEXT],
};

const fn page_mm(name: &'static str, default: &'static str, help: &'static str) -> ParamInfo {
    ParamInfo {
        name,
        required: false,
        default: Some(default),
        help,
    }
}

const PAGE_PARAMS: &[ParamInfo] = &[
    page_mm(
        "width",
        "210",
        "Paper width in mm, 70 to 1000 (wider than tall is landscape)",
    ),
    page_mm("height", "297", "Paper height in mm, 70 to 1000"),
    page_mm("top", "15", "Top margin in mm, 0 to 200"),
    page_mm("right", "10", "Right margin in mm, 0 to 200"),
    page_mm("bottom", "10", "Bottom margin in mm, 0 to 200"),
    page_mm("left", "10", "Left margin in mm, 0 to 200"),
    page_mm("scale", "100", "Score size in percent, 50 to 200"),
    page_mm(
        "page_numbers",
        "true",
        "Page numbers at the bottom: true or false",
    ),
    page_mm(
        "copyright",
        "true",
        "The copyright on the first page: true or false",
    ),
    page_mm(
        "header",
        "false",
        "Title and artist atop later pages: true or false",
    ),
];

pub(crate) const SET_PAGE_SETUP: CommandInfo = CommandInfo {
    id: "set_page_setup",
    aliases: &[],
    label: "Set up the page",
    help: "Sets the paper size, margins, score size, and what the header and footer show.",
    params: PAGE_PARAMS,
};

const BREAK: ParamInfo = ParamInfo {
    name: "break",
    required: true,
    default: None,
    help: "line (new line), page (new page) or none",
};

pub(crate) const SET_SYSTEM_BREAK: CommandInfo = CommandInfo {
    id: "set_system_break",
    aliases: &["set_line_break"],
    label: "Set a line break",
    help: "Starts a new line or page at the measure (not the first), or removes the break.",
    params: &[TRACK, MEASURE, BREAK],
};

const BARS: ParamInfo = ParamInfo {
    name: "bars",
    required: true,
    default: None,
    help: "At most this many bars per line, 1 to 32, or auto",
};

pub(crate) const SET_BARS_PER_SYSTEM: CommandInfo = CommandInfo {
    id: "set_bars_per_system",
    aliases: &["set_bars_per_line"],
    label: "Set the bars per line",
    help: "Limits how many bars a line of the track holds; auto fits as many as the width allows.",
    params: &[TRACK, BARS],
};

pub(crate) const SET_TEMPO: CommandInfo = CommandInfo {
    id: "set_tempo",
    aliases: &[],
    label: "Set the tempo",
    help: "Sets the initial tempo in quarter notes per minute.",
    params: &[TEMPO],
};

pub(crate) const SET_BEAT_REST: CommandInfo = CommandInfo {
    id: "set_beat_rest",
    aliases: &["make_rest"],
    label: "Make the beat a rest",
    help: "Removes the beat's notes and leaves a rest of the same length.",
    params: &[TRACK, MEASURE, VOICE, BEAT],
};

pub(crate) const SET_TIE: CommandInfo = CommandInfo {
    id: "set_tie",
    aliases: &[],
    label: "Tie or untie a note",
    help: "Ties a note to the previous one on the same string, or removes the tie.",
    params: &[TRACK, MEASURE, VOICE, BEAT, STRING, TIE],
};

pub(crate) const SET_DEAD_NOTE: CommandInfo = CommandInfo {
    id: "set_dead_note",
    aliases: &[],
    label: "Make a note dead",
    help: "Makes a note a dead (muted, x) note, or a normal note again.",
    params: &[TRACK, MEASURE, VOICE, BEAT, STRING, DEAD],
};

pub(crate) const SET_DYNAMIC: CommandInfo = CommandInfo {
    id: "set_dynamic",
    aliases: &[],
    label: "Set the dynamic",
    help: "Sets how loud a beat's notes are played (ppp to fff); later notes keep theirs.",
    params: &[TRACK, MEASURE, VOICE, BEAT, DYNAMIC],
};

pub(crate) const SET_MEASURE_TEMPO: CommandInfo = CommandInfo {
    id: "set_measure_tempo",
    aliases: &[],
    label: "Set the tempo at a measure",
    help: "Sets the tempo the measure starts at (a tempo change).",
    params: &[MEASURE, TEMPO],
};

pub(crate) const SET_TIME_SIGNATURE: CommandInfo = CommandInfo {
    id: "set_time_signature",
    aliases: &[],
    label: "Set the time signature",
    help: "Sets the time signature from the measure on, in every track, up to the next \
           measure that has a different one.",
    params: &[MEASURE, NUMERATOR, DENOMINATOR],
};

pub(crate) const SET_KEY_SIGNATURE: CommandInfo = CommandInfo {
    id: "set_key_signature",
    aliases: &[],
    label: "Set the key signature",
    help: "Sets the key signature from the measure on, in every track, up to the next \
           measure that has a different one.",
    params: &[MEASURE, KEY, MINOR],
};

pub(crate) const SET_SECTION: CommandInfo = CommandInfo {
    id: "set_section",
    aliases: &[],
    label: "Set a section marker",
    help: "Names a section that starts at the measure.",
    params: &[MEASURE, TITLE, SECTION_COLOR],
};

pub(crate) const CLEAR_SECTION: CommandInfo = CommandInfo {
    id: "clear_section",
    aliases: &[],
    label: "Remove a section marker",
    help: "Removes the section marker at the measure, if there is one.",
    params: &[MEASURE],
};

pub(crate) const SET_REPEAT: CommandInfo = CommandInfo {
    id: "set_repeat",
    aliases: &["set_repeats"],
    label: "Set repeat signs and endings",
    help: "Sets the repeat start, the repeat end with how many times it plays again, and the \
           alternative endings of a measure.",
    params: &[MEASURE, START, END, ENDINGS],
};

pub(crate) const SET_DOUBLE_BAR: CommandInfo = CommandInfo {
    id: "set_double_bar",
    aliases: &["set_double_barline"],
    label: "Set a double barline",
    help: "Ends the measure with a double barline, which divides sections of the score.",
    params: &[MEASURE, DOUBLE_BAR],
};

pub(crate) const SET_FEEL: CommandInfo = CommandInfo {
    id: "set_feel",
    aliases: &["set_triplet_feel"],
    label: "Set the feel",
    help: "Sets how notes swing from the measure on, up to the next measure with another \
           feel: straight, triplet, dotted or Scottish (dotted and Scottish need GP7/GPX).",
    params: &[MEASURE, FEEL],
};

pub(crate) const SET_FERMATA: CommandInfo = CommandInfo {
    id: "set_fermata",
    aliases: &["set_fermatas"],
    label: "Set a fermata",
    help: "Puts a fermata of the chosen length on the measure, or removes it.",
    params: &[MEASURE, FERMATA],
};

pub(crate) const SET_NOTE_EFFECT: CommandInfo = CommandInfo {
    id: "set_note_effect",
    aliases: &[],
    label: "Set a note effect",
    help: "Sets or clears one effect of a note, leaving its other effects alone.",
    params: &[TRACK, MEASURE, VOICE, BEAT, STRING, EFFECT, EFFECT_VALUE],
};

pub(crate) const SET_BEAT_EFFECT: CommandInfo = CommandInfo {
    id: "set_beat_effect",
    aliases: &[],
    label: "Set a beat effect",
    help: "Sets or clears one effect of a whole beat (stroke, slap, whammy, ...).",
    params: &[TRACK, MEASURE, VOICE, BEAT, EFFECT, EFFECT_VALUE],
};

pub(crate) const BATCH: CommandInfo = CommandInfo {
    id: "batch",
    aliases: &[],
    label: "Several edits at once",
    help: "A group of edits applied and undone as one step (used by the palette and paste).",
    params: &[],
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
    INSERT_TRACK,
    DELETE_TRACK,
    DUPLICATE_TRACK,
    MOVE_TRACK,
    RENAME_TRACK,
    SET_TRACK_COLOR,
    SET_TRACK_VOLUME,
    SET_TRACK_PAN,
    SET_TRACK_REVERB,
    SET_TRACK_CHORUS,
    SET_TRACK_STAVES,
    SET_MASTER_VOLUME,
    SET_MASTER_PAN,
    SET_TRACK_MUTE,
    SET_TRACK_SOLO,
    SET_TRACK_PROGRAM,
    SET_TRACK_DRUMS,
    SET_TRACK_CAPO,
    SET_TRACK_STRINGS,
    SET_TRACK_STRING_PITCH,
    SET_SONG_TEXT,
    SET_PAGE_SETUP,
    SET_SYSTEM_BREAK,
    SET_BARS_PER_SYSTEM,
    SET_TEMPO,
    SET_BEAT_REST,
    SET_TIE,
    SET_DEAD_NOTE,
    SET_DYNAMIC,
    SET_MEASURE_TEMPO,
    SET_TIME_SIGNATURE,
    SET_KEY_SIGNATURE,
    SET_SECTION,
    CLEAR_SECTION,
    SET_REPEAT,
    SET_DOUBLE_BAR,
    SET_FEEL,
    SET_FERMATA,
    SET_NOTE_EFFECT,
    SET_BEAT_EFFECT,
    BATCH,
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
