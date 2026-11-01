//! Headless command-line tools for working with Guitar Pro files without the GUI.

use std::path::Path;

use gscore_io::import_guitar_pro_from_bytes;
use gscore_model::Song;

const USAGE: &str = "Usage: gscore-cli info <file.gp>";

/// Installs the panic hook that writes crash reports to the per-user crash directory.
/// Call once at the start of `main`.
pub fn install_crash_reporting() {
    if let Some(dir) = gscore_session::default_crash_dir() {
        gscore_session::install_panic_hook(dir);
    }
}

/// Formats a summary of a song: metadata, then each track's tuning and measure count.
pub fn song_info(song: &Song) -> String {
    let mut out = format!(
        "Title: {}\nArtist: {}\nTempo: {} BPM\nMeasures: {}\nTracks: {}\n",
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
        out.push_str(&format!(
            "Issues: {} voice(s) do not fill their measure\n",
            issues.len()
        ));
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

    #[test]
    fn info_of_an_empty_song() {
        let info = song_info(&Song::default());
        assert!(info.contains("Tracks: 0"), "{info}");
        assert!(info.contains("Tempo: 120 BPM"), "{info}");
    }
}
