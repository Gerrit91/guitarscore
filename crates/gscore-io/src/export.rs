//! Export of a `Song` to Guitar Pro bytes.
//!
//! Bytes only: writing files safely (temporary file, atomic rename, backup, re-read and
//! compare) is `gscore-session`'s job (milestone M3).

use gscore_model::{Song, interop};

use crate::contain::contain;
use crate::error::IoError;
use crate::format::FileFormat;

/// Serializes `song` in `format`. The writer runs inside `catch_unwind`, so a writer bug
/// becomes an error, never a crash.
///
/// The Guitar Pro 3-5 writers produce version 3.00, 4.06 and 5.10 files.
pub fn export_to_bytes(song: &Song, format: FileFormat) -> Result<Vec<u8>, IoError> {
    let gp = interop::as_gp(song);
    contain(format, "writer", || match format {
        FileFormat::Gp => gp.write_gp(),
        FileFormat::Gpx => gp.write_gpx(),
        FileFormat::Gp5 => gp.write((5, 1, 0), None),
        FileFormat::Gp4 => gp.write((4, 0, 6), None),
        FileFormat::Gp3 => gp.write((3, 0, 0), None),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::import_guitar_pro_from_bytes;

    #[test]
    fn a_song_without_tracks_writes_and_reads_back_empty() {
        // The GP3-5 writer used to index the first track and panic (contained here as an
        // error); it now writes an empty song, as the GP6/GP7 writers do.
        for format in [FileFormat::Gp3, FileFormat::Gp4, FileFormat::Gp5] {
            let bytes = export_to_bytes(&Song::default(), format).unwrap();
            let imported = import_guitar_pro_from_bytes(&bytes).unwrap();
            assert_eq!(imported.format, format);
            assert_eq!(imported.song.track_count(), 0, "{format}");
        }
    }

    #[test]
    fn exported_bytes_are_detected_as_their_format() {
        let song = Song::default();
        for format in [FileFormat::Gp, FileFormat::Gpx] {
            let bytes = export_to_bytes(&song, format).unwrap();
            let imported = import_guitar_pro_from_bytes(&bytes).unwrap();
            assert_eq!(imported.format, format);
        }
    }
}
