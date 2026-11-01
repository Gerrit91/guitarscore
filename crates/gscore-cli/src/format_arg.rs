//! The `--to` format argument shared by `convert` and `roundtrip-check`.

use std::path::Path;

use clap::ValueEnum;
use gscore_io::FileFormat;

/// A format to write, as given on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum FormatArg {
    /// Guitar Pro 7+ (.gp)
    Gp,
    /// Guitar Pro 6 (.gpx)
    Gpx,
    /// Guitar Pro 5 (.gp5)
    Gp5,
    /// Guitar Pro 4 (.gp4)
    Gp4,
    /// Guitar Pro 3 (.gp3)
    Gp3,
}

impl From<FormatArg> for FileFormat {
    fn from(arg: FormatArg) -> Self {
        match arg {
            FormatArg::Gp => FileFormat::Gp,
            FormatArg::Gpx => FileFormat::Gpx,
            FormatArg::Gp5 => FileFormat::Gp5,
            FormatArg::Gp4 => FileFormat::Gp4,
            FormatArg::Gp3 => FileFormat::Gp3,
        }
    }
}

impl FormatArg {
    /// The format a file name's extension stands for (`song.gp5` -> GP5), if any.
    pub(crate) fn from_path(path: &Path) -> Option<FormatArg> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        FormatArg::value_variants()
            .iter()
            .copied()
            .find(|arg| FileFormat::from(*arg).extension() == extension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_come_from_extensions() {
        assert_eq!(
            FormatArg::from_path(Path::new("a/song.gp")),
            Some(FormatArg::Gp)
        );
        assert_eq!(
            FormatArg::from_path(Path::new("song.GP5")),
            Some(FormatArg::Gp5)
        );
        assert_eq!(
            FormatArg::from_path(Path::new("song.gpx")),
            Some(FormatArg::Gpx)
        );
        assert_eq!(FormatArg::from_path(Path::new("song.txt")), None);
        assert_eq!(FormatArg::from_path(Path::new("song")), None);
    }
}
