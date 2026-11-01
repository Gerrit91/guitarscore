//! Reading input files for the subcommands.

use std::path::Path;

use gscore_io::{Imported, MAX_INPUT_BYTES, import_guitar_pro_from_bytes};

use crate::CliError;

/// Reads and imports the Guitar Pro file at `path`.
pub(crate) fn read_song(path: &Path) -> Result<Imported, CliError> {
    let bytes = read_capped(path)?;
    import_guitar_pro_from_bytes(&bytes)
        .map_err(|e| CliError::Failed(format!("cannot import {}: {e}", path.display())))
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
