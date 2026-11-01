//! `convert`: save a Guitar Pro file in another (or the same) format, safely.

use std::path::Path;

use gscore_io::FileFormat;
use gscore_session::{SaveError, SaveOptions, save};

use crate::CliError;
use crate::format_arg::FormatArg;

/// How many differences are listed before summarizing the rest.
const MAX_LISTED: usize = 20;

/// Reads `input` and saves it to `output` as `to` (default: from `output`'s extension).
pub(crate) fn run(
    input: &Path,
    output: &Path,
    to: Option<FormatArg>,
    allow_losses: bool,
) -> Result<String, CliError> {
    let format: FileFormat = to
        .or_else(|| FormatArg::from_path(output))
        .ok_or_else(|| {
            CliError::Usage(format!(
                "cannot tell the format from {}; name it with --to\n",
                output.display()
            ))
        })?
        .into();
    let original = crate::input::read_song(input)?;
    let options = SaveOptions {
        format,
        allow_losses,
    };
    let report = save(&original.song, output, options).map_err(|e| match e {
        SaveError::Losses(diffs) => {
            let mut text = format!(
                "{} as {format} would change the song ({} difference(s)); nothing was \
                 written. Use --allow-losses to save anyway:\n",
                output.display(),
                diffs.len()
            );
            for diff in diffs.iter().take(MAX_LISTED) {
                text.push_str(&format!("  {diff}\n"));
            }
            if let Some(more) = diffs.len().checked_sub(MAX_LISTED).filter(|&n| n > 0) {
                text.push_str(&format!("  ... and {more} more\n"));
            }
            CliError::Failed(text)
        }
        other => CliError::Failed(format!("cannot save {}: {other}", output.display())),
    })?;

    let mut out = format!(
        "Saved {} as {format}, read from {} as {}.\n",
        report.path.display(),
        input.display(),
        original.format
    );
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
        for diff in report.losses.iter().take(MAX_LISTED) {
            out.push_str(&format!("  {diff}\n"));
        }
    }
    Ok(out)
}
