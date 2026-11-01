//! Import of Guitar Pro bytes into a `Song`.

use std::panic;

use gscore_gp::Song as GpSong;
use gscore_model::{Song, interop};

use crate::error::IoError;
use crate::format::{FileFormat, detect_format};

/// Largest input accepted, in bytes. Real scores are far smaller (even GP7+ files with
/// embedded audio rarely exceed a few MB); the cap bounds what a hostile file can make the
/// parser allocate.
pub const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;

/// A song read from a file, and the format it was stored in.
#[derive(Debug, Clone)]
pub struct Imported {
    /// The song.
    pub song: Song,
    /// The file's format (needed to warn before saving in a different one).
    pub format: FileFormat,
}

/// Parses Guitar Pro bytes (GP3, GP4, GP5, GPX or GP7+, detected from the content).
///
/// Malformed input yields an error, never a panic: the guitarpro parser runs inside
/// `catch_unwind`, and songs that break the model's invariants are rejected.
pub fn import_guitar_pro_from_bytes(bytes: &[u8]) -> Result<Imported, IoError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(IoError::UnsupportedFormat(format!(
            "file is {} bytes; the limit is {MAX_INPUT_BYTES}",
            bytes.len()
        )));
    }
    let format = detect_format(bytes)?;
    let parsed = panic::catch_unwind(|| {
        let mut gp_song = GpSong::default();
        let result = match format {
            FileFormat::Gp3 => gp_song.read_gp3(bytes),
            FileFormat::Gp4 => gp_song.read_gp4(bytes),
            FileFormat::Gp5 => gp_song.read_gp5(bytes),
            FileFormat::Gpx => gp_song.read_gpx(bytes),
            FileFormat::Gp => gp_song.read_gp(bytes),
        };
        result.map(|()| gp_song)
    });
    let gp_song = match parsed {
        Ok(Ok(song)) => song,
        Ok(Err(e)) => return Err(IoError::GuitarProError(format!("{format}: {e:?}"))),
        Err(payload) => {
            return Err(IoError::GuitarProError(format!(
                "{format}: the parser panicked: {}",
                panic_message(payload.as_ref())
            )));
        }
    };
    let song = interop::from_gp(gp_song).map_err(IoError::InvalidSong)?;
    Ok(Imported { song, format })
}

/// Best-effort text of a panic payload (`panic!` passes a `&str` or a `String`).
fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic payload")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_bytes_are_an_error_not_a_panic() {
        let truncated_headers: [&[u8]; 5] = [
            b"PK\x03\x04",
            b"PK\x03\x04garbage-after-zip-magic",
            b"BCFZ\xff\xff\xff\xff",
            b"BCFS",
            b"\x18FICHIER GUITAR PRO v5.00",
        ];
        for bytes in [&[][..], &[0xFF; 3][..], b"FICHIER GUITAR PRO v5.00"]
            .into_iter()
            .chain(truncated_headers)
        {
            assert!(import_guitar_pro_from_bytes(bytes).is_err(), "{bytes:?}");
        }
    }
}
