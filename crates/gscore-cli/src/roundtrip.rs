//! `roundtrip-check` (hidden, for development): write a song, read it back, compare.

use std::path::Path;

use gscore_io::{FileFormat, export_to_bytes, import_guitar_pro_from_bytes};
use gscore_model::differences;

use crate::CliError;
use crate::format_arg::FormatArg;

/// How many differences are listed before summarizing the rest.
const MAX_LISTED: usize = 20;

/// Reads `path`, writes it as `to` (default: its own format), reads that back and lists
/// what changed. Differences are a failure (exit code 1).
pub(crate) fn run(path: &Path, to: Option<FormatArg>) -> Result<String, CliError> {
    let original = crate::input::read_song(path)?;
    let target = to.map_or(original.format, FileFormat::from);
    let fail = |what: &str, e: &dyn std::fmt::Display| {
        CliError::Failed(format!("{}: {what}: {e}", path.display()))
    };

    let bytes = export_to_bytes(&original.song, target)
        .map_err(|e| fail(&format!("writing as {target} failed"), &e))?;
    let reread = import_guitar_pro_from_bytes(&bytes)
        .map_err(|e| fail(&format!("reading the written {target} back failed"), &e))?;

    let route = format!("{} -> {target} -> read back", original.format);
    let diffs = differences(&original.song, &reread.song);
    if diffs.is_empty() {
        return Ok(format!(
            "Round trip OK: {route}, no differences.\n\
             Compared: song information, tracks, tunings, time signatures, beats, notes and \
             their effects.\n"
        ));
    }
    let mut report = format!(
        "{}: round trip {route} changed the song ({} difference(s)):\n",
        path.display(),
        diffs.len()
    );
    for diff in diffs.iter().take(MAX_LISTED) {
        report.push_str(&format!("  {diff}\n"));
    }
    if let Some(more) = diffs.len().checked_sub(MAX_LISTED).filter(|&n| n > 0) {
        report.push_str(&format!("  ... and {more} more\n"));
    }
    Err(CliError::Failed(report))
}
