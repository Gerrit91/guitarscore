//! The formats the File › Export menu offers: a printed score (PDF), a Standard MIDI File,
//! and the Guitar Pro formats an existing song can be converted to.

use gscore_session::FileFormat;

/// One entry of the Export menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    /// The score as the page view shows it, on pages.
    Pdf,
    /// A Standard MIDI File of every track.
    Midi,
    /// The song rendered to audio with the SoundFont.
    Wav,
    /// Guitar Pro 7+.
    Gp,
    /// Guitar Pro 6 (`.gpx`).
    Gpx,
    /// Guitar Pro 5.
    Gp5,
    /// Guitar Pro 4.
    Gp4,
    /// Guitar Pro 3.
    Gp3,
}

impl ExportFormat {
    /// Every entry, in menu order.
    pub(crate) const ALL: [ExportFormat; 8] = [
        ExportFormat::Pdf,
        ExportFormat::Midi,
        ExportFormat::Wav,
        ExportFormat::Gp,
        ExportFormat::Gpx,
        ExportFormat::Gp5,
        ExportFormat::Gp4,
        ExportFormat::Gp3,
    ];

    /// What the menu and the dialog title call it.
    pub(crate) fn label(self) -> &'static str {
        match self {
            ExportFormat::Pdf => "PDF (printed score)",
            ExportFormat::Midi => "MIDI (.mid)",
            ExportFormat::Wav => "Audio (WAV)",
            ExportFormat::Gp => "Guitar Pro 7 (.gp)",
            ExportFormat::Gpx => "Guitar Pro 6 (.gpx)",
            ExportFormat::Gp5 => "Guitar Pro 5 (.gp5)",
            ExportFormat::Gp4 => "Guitar Pro 4 (.gp4)",
            ExportFormat::Gp3 => "Guitar Pro 3 (.gp3)",
        }
    }

    /// The file name extension (without the dot).
    pub(crate) fn extension(self) -> &'static str {
        match self {
            ExportFormat::Pdf => "pdf",
            ExportFormat::Midi => "mid",
            ExportFormat::Wav => "wav",
            ExportFormat::Gp => "gp",
            ExportFormat::Gpx => "gpx",
            ExportFormat::Gp5 => "gp5",
            ExportFormat::Gp4 => "gp4",
            ExportFormat::Gp3 => "gp3",
        }
    }

    /// The Guitar Pro format, for the score formats.
    pub(crate) fn score_format(self) -> Option<FileFormat> {
        match self {
            ExportFormat::Pdf | ExportFormat::Midi | ExportFormat::Wav => None,
            ExportFormat::Gp => Some(FileFormat::Gp),
            ExportFormat::Gpx => Some(FileFormat::Gpx),
            ExportFormat::Gp5 => Some(FileFormat::Gp5),
            ExportFormat::Gp4 => Some(FileFormat::Gp4),
            ExportFormat::Gp3 => Some(FileFormat::Gp3),
        }
    }

    /// Whether the format is an older Guitar Pro version that cannot hold everything the
    /// song may use, so the user is warned before choosing it.
    pub(crate) fn loses_details(self) -> Option<&'static str> {
        match self {
            ExportFormat::Pdf
            | ExportFormat::Midi
            | ExportFormat::Wav
            | ExportFormat::Gp
            | ExportFormat::Gpx => None,
            ExportFormat::Gp5 | ExportFormat::Gp4 | ExportFormat::Gp3 => Some(
                "An older Guitar Pro version holds less than the song may use. The export \
                 says how many details it could not keep.",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_export_has_a_unique_extension_and_the_score_formats_map_to_their_file_format() {
        let mut seen = Vec::new();
        for format in ExportFormat::ALL {
            assert!(!seen.contains(&format.extension()), "{format:?}");
            seen.push(format.extension());
            assert_eq!(
                format.score_format().is_some(),
                !matches!(
                    format,
                    ExportFormat::Pdf | ExportFormat::Midi | ExportFormat::Wav
                )
            );
        }
    }
}
