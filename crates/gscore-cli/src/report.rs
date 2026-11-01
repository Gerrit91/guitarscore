//! Text shared by the commands that compare or save songs: difference lists and save
//! outcomes.

use std::path::Path;

use gscore_io::FileFormat;
use gscore_model::Difference;
use gscore_session::{SaveError, SaveReport};

use crate::CliError;

/// How many differences are listed before summarizing the rest.
const MAX_LISTED: usize = 20;

/// Appends one indented line per difference, at most [`MAX_LISTED`], then how many more.
pub(crate) fn push_differences(out: &mut String, diffs: &[Difference]) {
    for diff in diffs.iter().take(MAX_LISTED) {
        out.push_str(&format!("  {diff}\n"));
    }
    if let Some(more) = diffs.len().checked_sub(MAX_LISTED).filter(|&n| n > 0) {
        out.push_str(&format!("  ... and {more} more\n"));
    }
}

/// The CLI error for a failed save of `output` as `format`; refused losses are listed.
pub(crate) fn save_error(output: &Path, format: FileFormat, error: SaveError) -> CliError {
    match error {
        SaveError::Losses(diffs) => {
            let mut text = format!(
                "{} as {format} would change the song ({} difference(s)); nothing was \
                 written. Use --allow-losses to save anyway:\n",
                output.display(),
                diffs.len()
            );
            push_differences(&mut text, &diffs);
            CliError::Failed(text)
        }
        other => CliError::Failed(format!("cannot save {}: {other}", output.display())),
    }
}

/// Appends what a successful save also did: the backup it made and the accepted losses.
pub(crate) fn push_save_details(out: &mut String, report: &SaveReport) {
    if let Some(backup) = &report.backup {
        out.push_str(&format!(
            "The previous file was kept as {}.\n",
            backup.display()
        ));
    }
    if !report.losses.is_empty() {
        out.push_str(&format!(
            "Lost in the conversion ({} difference(s)):\n",
            report.losses.len()
        ));
        push_differences(out, &report.losses);
    }
}
