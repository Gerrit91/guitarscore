//! `info`: print a summary of a Guitar Pro file.

use std::path::Path;

use gscore_io::{import_guitar_pro_from_bytes, Imported, MAX_INPUT_BYTES};
use gscore_model::{InstrumentKind, Song, StringTuning, TrackView};

use crate::CliError;

/// How many issues `info` lists before summarizing the rest.
const MAX_LISTED_ISSUES: usize = 10;

/// Reads the file at `path` and returns its summary.
pub(crate) fn run(path: &Path) -> Result<String, CliError> {
    let bytes = read_capped(path)?;
    let imported = import_guitar_pro_from_bytes(&bytes)
        .map_err(|e| CliError::Failed(format!("cannot import {}: {e}", path.display())))?;
    Ok(summary(&imported))
}

/// Reads a file, refusing files larger than the importer accepts before reading them.
fn read_capped(path: &Path) -> Result<Vec<u8>, CliError> {
    let fail = |e: std::io::Error| CliError::Failed(format!("cannot read {}: {e}", path.display()));
    let size = std::fs::metadata(path).map_err(fail)?.len();
    if usize::try_from(size).map_or(true, |s| s > MAX_INPUT_BYTES) {
        return Err(CliError::Failed(format!(
            "{} is {size} bytes; the limit is {MAX_INPUT_BYTES}",
            path.display()
        )));
    }
    std::fs::read(path).map_err(fail)
}

/// Formats the file format and song information, each track, and the issues.
fn summary(imported: &Imported) -> String {
    let song: &Song = &imported.song;
    let notes: Vec<&str> = song.notes().collect();
    let fields: [(&str, String); 14] = [
        ("Format", imported.format.to_string()),
        ("Title", or_not_set(song.title()).to_owned()),
        ("Subtitle", or_not_set(song.subtitle()).to_owned()),
        ("Artist", or_not_set(song.artist()).to_owned()),
        ("Album", or_not_set(song.album()).to_owned()),
        ("Lyrics by", or_not_set(song.lyricist()).to_owned()),
        ("Music by", or_not_set(song.composer()).to_owned()),
        ("Copyright", or_not_set(song.copyright()).to_owned()),
        ("Tab by", or_not_set(song.tab_author()).to_owned()),
        ("Instructions", or_not_set(song.instructions()).to_owned()),
        ("Notes", or_not_set(&notes.join("\n")).to_owned()),
        ("Tempo", format!("{} BPM", song.tempo())),
        ("Measures", song.measure_count().to_string()),
        ("Tracks", song.track_count().to_string()),
    ];
    let mut out = String::new();
    for (label, value) in &fields {
        push_field(&mut out, label, value);
    }
    for track in song.tracks() {
        out.push_str(&format!(
            "  {}: {}: {}\n",
            track.index(),
            or_not_set(track.name()),
            describe_instrument(&track),
        ));
    }
    let issues = song.issues();
    if !issues.is_empty() {
        out.push_str(&format!("Issues: {}\n", issues.len()));
        for issue in issues.iter().take(MAX_LISTED_ISSUES) {
            out.push_str(&format!("  {issue}\n"));
        }
        if let Some(more) = issues
            .len()
            .checked_sub(MAX_LISTED_ISSUES)
            .filter(|&n| n > 0)
        {
            out.push_str(&format!("  ... and {more} more\n"));
        }
    }
    out
}

/// The instrument, and for string instruments the strings and tuning:
/// "Electric Guitar, 6 strings, tuned E standard (E2 A2 D3 G3 B3 E4)", "Drums", "Voice".
fn describe_instrument(track: &TrackView<'_>) -> String {
    let kind = track.instrument_kind();
    let name = track.instrument_name().unwrap_or(match kind {
        InstrumentKind::Strings => "",
        InstrumentKind::Drums => "Drums",
        InstrumentKind::Other => "other instrument",
    });
    if kind != InstrumentKind::Strings {
        return name.to_owned();
    }
    let tuning = match (track.tuning(), track.tuning_name()) {
        (Ok(Some(strings)), Ok(Some(name))) => {
            format!("tuned {name} ({})", note_names(&strings))
        }
        (Ok(Some(strings)), _) => format!("tuned {}", note_names(&strings)),
        (Ok(None), _) => "no tuning".to_owned(),
        (Err(e), _) => format!("<{e}>"),
    };
    let strings = format!("{} strings, {tuning}", track.string_count());
    if name.is_empty() {
        strings
    } else {
        format!("{name}, {strings}")
    }
}

/// Note names from the lowest string up, as musicians read a tuning ("E2 A2 D3 G3 B3 E4").
fn note_names(strings: &[StringTuning]) -> String {
    let names: Vec<String> = strings.iter().rev().map(|s| s.pitch.to_string()).collect();
    names.join(" ")
}

/// Width of the label column, so values line up ("Instructions: " is the longest label).
const LABEL_WIDTH: usize = "Instructions: ".len();

/// Appends `Label: value`, aligned; continuation lines of a multi-line value are indented
/// to the value column.
fn push_field(out: &mut String, label: &str, value: &str) {
    let mut lines = value.lines();
    let first = lines.next().unwrap_or_default();
    out.push_str(&format!("{:<LABEL_WIDTH$}{first}\n", format!("{label}:")));
    for line in lines {
        out.push_str(&format!("{:LABEL_WIDTH$}{line}\n", ""));
    }
}

/// Shows an empty (or whitespace-only) text field explicitly instead of leaving it blank.
fn or_not_set(text: &str) -> &str {
    if text.trim().is_empty() {
        "(not set)"
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gscore_io::FileFormat;

    /// The value printed after `label:`, ignoring the alignment padding.
    fn field<'a>(out: &'a str, label: &str) -> Option<&'a str> {
        out.lines()
            .find_map(|line| line.strip_prefix(&format!("{label}:")))
            .map(str::trim)
    }

    #[test]
    fn summary_of_an_empty_song_shows_every_field() {
        let out = summary(&Imported {
            song: Song::default(),
            format: FileFormat::Gp,
        });
        assert_eq!(field(&out, "Format"), Some("Guitar Pro 7+ (.gp)"), "{out}");
        assert_eq!(field(&out, "Tempo"), Some("120 BPM"), "{out}");
        assert_eq!(field(&out, "Tracks"), Some("0"), "{out}");
        for label in [
            "Title",
            "Subtitle",
            "Artist",
            "Album",
            "Lyrics by",
            "Music by",
            "Copyright",
            "Tab by",
            "Instructions",
            "Notes",
        ] {
            assert_eq!(field(&out, label), Some("(not set)"), "{label}: {out}");
        }
    }

    #[test]
    fn fields_are_aligned_and_multi_line_values_indented() {
        let mut out = String::new();
        push_field(&mut out, "Notes", "first\nsecond");
        push_field(&mut out, "Instructions", "x");
        assert_eq!(
            out,
            "Notes:        first\n              second\nInstructions: x\n"
        );
    }

    #[test]
    fn tunings_are_note_names_lowest_string_first() {
        let tuning = |pitches: &[u8]| -> Vec<StringTuning> {
            (1_u8..)
                .zip(pitches)
                .map(|(string, &pitch)| StringTuning {
                    string: gscore_model::StringNumber::new(string).unwrap(),
                    pitch: gscore_model::Pitch::new(pitch).unwrap(),
                })
                .collect()
        };
        assert_eq!(
            note_names(&tuning(&[64, 59, 55, 50, 45, 40])),
            "E2 A2 D3 G3 B3 E4"
        );
        assert_eq!(note_names(&tuning(&[43, 38, 33, 28, 23])), "B0 E1 A1 D2 G2");
    }
}
