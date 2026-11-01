//! `edit`: apply command-line edits and save the result safely.

use std::io::Read;
use std::path::Path;

use gscore_edit::{Editor, parse_line};
use gscore_io::FileFormat;
use gscore_session::{SaveError, SaveOptions, save};

use crate::CliError;
use crate::format_arg::FormatArg;

/// How many differences are listed before summarizing the rest.
const MAX_LISTED: usize = 20;

/// Largest script file accepted. A script is a batch of short command lines.
const MAX_SCRIPT_BYTES: u64 = 1024 * 1024;

/// Reads `input`, applies `commands` and then the script (if any), and saves to `output`.
pub(crate) fn run(
    input: &Path,
    output: &Path,
    commands: &[String],
    script: Option<&Path>,
    allow_losses: bool,
) -> Result<String, CliError> {
    let imported = crate::input::read_song(input)?;
    let mut editor = Editor::new(imported.song);
    let mut applied = 0_usize;

    for (index, text) in commands.iter().enumerate() {
        let at = format!("-c #{}", index.saturating_add(1));
        applied = applied.saturating_add(apply_line(&mut editor, &at, text)?);
    }
    if let Some(path) = script {
        let text = read_script(path)?;
        for (index, line) in text.lines().enumerate() {
            let at = format!("{}:{}", path.display(), index.saturating_add(1));
            applied = applied.saturating_add(apply_line(&mut editor, &at, line)?);
        }
    }
    if applied == 0 {
        return Err(CliError::Usage(
            "nothing to do: pass -c <command> or --script <file>\n".to_owned(),
        ));
    }

    let format: FileFormat = FormatArg::from_path(output)
        .ok_or_else(|| {
            CliError::Usage(format!(
                "cannot tell the format from {}; name the output .gp\n",
                output.display()
            ))
        })?
        .into();
    let report = save(
        editor.song(),
        output,
        SaveOptions {
            format,
            allow_losses,
        },
    )
    .map_err(|error| match error {
        SaveError::Losses(diffs) => losses_error(output, format, &diffs),
        other => CliError::Failed(format!("cannot save {}: {other}", output.display())),
    })?;

    let mut out = format!(
        "Applied {applied} command(s) to {} and saved {} as {format}.\n",
        input.display(),
        report.path.display()
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

/// Parses and applies one line; returns 1 when an edit was applied, 0 for a blank/comment
/// line.
fn apply_line(editor: &mut Editor, at: &str, line: &str) -> Result<usize, CliError> {
    match parse_line(line) {
        Ok(Some(command)) => {
            editor
                .execute(command)
                .map_err(|error| CliError::Failed(format!("{at}: {error}")))?;
            Ok(1)
        }
        Ok(None) => Ok(0),
        Err(error) => Err(CliError::Failed(format!("{at}: {error}"))),
    }
}

/// Reads a script file, refusing files larger than [`MAX_SCRIPT_BYTES`]. The read itself is
/// capped too, so a file that grows after the size check cannot make the CLI allocate without
/// bound.
fn read_script(path: &Path) -> Result<String, CliError> {
    let fail = |error: std::io::Error| {
        CliError::Failed(format!("cannot read {}: {error}", path.display()))
    };
    let size = std::fs::metadata(path).map_err(fail)?.len();
    if size > MAX_SCRIPT_BYTES {
        return Err(CliError::Failed(format!(
            "{} is {size} bytes; script files may be at most {MAX_SCRIPT_BYTES}",
            path.display()
        )));
    }
    let file = std::fs::File::open(path).map_err(fail)?;
    let mut text = String::new();
    file.take(MAX_SCRIPT_BYTES.saturating_add(1))
        .read_to_string(&mut text)
        .map_err(fail)?;
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > MAX_SCRIPT_BYTES {
        return Err(CliError::Failed(format!(
            "{} grew past the {MAX_SCRIPT_BYTES}-byte script limit while being read",
            path.display()
        )));
    }
    Ok(text)
}

/// A save refused because the format would lose content: list the differences.
fn losses_error(output: &Path, format: FileFormat, diffs: &[gscore_model::Difference]) -> CliError {
    let mut text = format!(
        "{} as {format} would change the song ({} difference(s)); nothing was written. Use \
         --allow-losses to save anyway:\n",
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
