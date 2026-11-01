//! Headless command-line tools for working with Guitar Pro files without the GUI.

use std::path::Path;

use gscore_io::import_guitar_pro_from_bytes;
use gscore_model::Song;

const USAGE: &str = "Usage: gscore-cli info <file.gp>";

/// Formats a summary of a song (title, tempo, tracks, measures).
pub fn song_info(song: &Song) -> String {
    let mut out = format!(
        "Song Information:\n  Title: {}\n  Artist: {}\n  Tempo: {} BPM\n  Number of tracks: {}\n",
        song.title,
        song.artist,
        song.get_tempo(),
        song.track_count()
    );
    for (number, track) in (1_usize..).zip(song.tracks()) {
        out.push_str(&format!(
            "  Track {number}: {}\n    Number of measures: {}\n",
            track.name,
            track.measures.len()
        ));
        if let Some(first) = track.measures.first() {
            out.push_str(&format!(
                "    First measure voices: {}\n",
                first.voices.len()
            ));
        }
    }
    out
}

/// Runs the CLI with the arguments after the program name. Returns text to print on success,
/// or a message for the user on failure.
pub fn run_cli(args: &[String]) -> Result<String, String> {
    match args {
        [command, path] if command == "info" => {
            let bytes =
                std::fs::read(Path::new(path)).map_err(|e| format!("cannot read {path}: {e}"))?;
            let song = import_guitar_pro_from_bytes(&bytes)
                .map_err(|e| format!("cannot import {path}: {e}"))?;
            Ok(song_info(&song))
        }
        [command, ..] if command == "info" => Err(USAGE.to_owned()),
        [command, ..] => Err(format!("unknown command `{command}`\n{USAGE}")),
        [] => Err(USAGE.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_arguments_are_errors_not_panics() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(run_cli(&args(&[])).is_err());
        assert!(run_cli(&args(&["info"])).is_err());
        assert!(run_cli(&args(&["bogus"])).is_err());
        assert!(run_cli(&args(&["info", "/nonexistent/file.gp"])).is_err());
    }
}
