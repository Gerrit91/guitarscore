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

/// Formats metadata, then each track's tuning, then the number of issues.
fn summary(imported: &Imported) -> String {
    let song: &Song = &imported.song;
    let mut out = format!(
        "Format: {}\nTitle: {}\nArtist: {}\nTempo: {} BPM\nMeasures: {}\nTracks: {}\n",
        imported.format,
        or_not_set(song.title()),
        or_not_set(song.artist()),
        song.tempo(),
        song.measure_count(),
        song.track_count()
    );
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
/// "Electric Guitar, 6 strings, tuned E2 A2 D3 G3 B3 E4", "Drums", "Voice".
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
    let tuning = match track.tuning() {
        Ok(Some(strings)) => describe_tuning(&strings),
        Ok(None) => "no tuning".to_owned(),
        Err(e) => format!("<{e}>"),
    };
    let strings = format!("{} strings, {tuning}", track.string_count());
    if name.is_empty() {
        strings
    } else {
        format!("{name}, {strings}")
    }
}

/// Note names from the lowest string up, as musicians read a tuning ("E2 A2 D3 G3 B3 E4").
fn describe_tuning(strings: &[StringTuning]) -> String {
    let names: Vec<String> = strings.iter().rev().map(|s| s.pitch.to_string()).collect();
    format!("tuned {}", names.join(" "))
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

    #[test]
    fn summary_of_an_empty_song() {
        let out = summary(&Imported {
            song: Song::default(),
            format: FileFormat::Gp,
        });
        assert!(out.contains("Format: Guitar Pro 7+ (.gp)"), "{out}");
        assert!(out.contains("Tracks: 0"), "{out}");
        assert!(out.contains("Tempo: 120 BPM"), "{out}");
        assert!(out.contains("Title: (not set)"), "{out}");
        assert!(out.contains("Artist: (not set)"), "{out}");
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
            describe_tuning(&tuning(&[64, 59, 55, 50, 45, 40])),
            "tuned E2 A2 D3 G3 B3 E4"
        );
        assert_eq!(
            describe_tuning(&tuning(&[43, 38, 33, 28, 23])),
            "tuned B0 E1 A1 D2 G2"
        );
    }
}
