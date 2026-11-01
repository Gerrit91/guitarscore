//! `info`: print a summary of a Guitar Pro file.

use std::path::Path;

use gscore_io::{import_guitar_pro_from_bytes, Imported, MAX_INPUT_BYTES};
use gscore_model::Song;

use crate::CliError;

/// How many issues `info` lists before summarizing the rest.
const MAX_LISTED_ISSUES: usize = 10;

/// Reads the file at `path` and returns its summary.
pub(crate) fn run(path: &str) -> Result<String, CliError> {
    let bytes = read_capped(Path::new(path))?;
    let imported = import_guitar_pro_from_bytes(&bytes)
        .map_err(|e| CliError::Failed(format!("cannot import {path}: {e}")))?;
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
        song.title(),
        song.artist(),
        song.tempo(),
        song.measure_count(),
        song.track_count()
    );
    for track in song.tracks() {
        let tuning = match track.tuning() {
            Ok(strings) => strings
                .iter()
                .map(|s| s.pitch.get().to_string())
                .collect::<Vec<_>>()
                .join(" "),
            Err(e) => format!("<{e}>"),
        };
        out.push_str(&format!(
            "  {}: {} ({} strings, tuning {tuning}{})\n",
            track.index(),
            track.name(),
            track.string_count(),
            if track.is_percussion() {
                ", percussion"
            } else {
                ""
            },
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
    }
}
