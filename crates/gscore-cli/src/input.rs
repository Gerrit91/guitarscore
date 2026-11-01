//! Reading input files for the subcommands.

use std::path::Path;

use gscore_io::Imported;

use crate::CliError;

/// Reads and imports the Guitar Pro file at `path`.
pub(crate) fn read_song(path: &Path) -> Result<Imported, CliError> {
    gscore_session::open(path).map_err(|e| CliError::Failed(e.to_string()))
}
