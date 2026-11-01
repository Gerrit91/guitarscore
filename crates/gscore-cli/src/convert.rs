//! `convert`: save a Guitar Pro file in another (or the same) format, safely.

use std::path::Path;

use gscore_io::FileFormat;
use gscore_session::{SaveOptions, save};

use crate::CliError;
use crate::format_arg::FormatArg;
use crate::report;

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
    let report = save(&original.song, output, options)
        .map_err(|error| report::save_error(output, format, error))?;

    let mut out = format!(
        "Saved {} as {format}, read from {} as {}.\n",
        report.path.display(),
        input.display(),
        original.format
    );
    report::push_save_details(&mut out, &report);
    Ok(out)
}
