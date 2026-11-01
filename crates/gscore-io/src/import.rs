//! Import of Guitar Pro bytes into a `Song`.

use std::panic;

use gscore_model::{interop, Song};
use guitarpro::Song as GpSong;

use crate::error::IoError;

/// Parses GP5 bytes into a song.
///
/// Malformed input yields an error, never a panic: the guitarpro parser runs inside
/// `catch_unwind`, and songs that break the model's invariants are rejected.
pub fn import_guitar_pro_from_bytes(bytes: &[u8]) -> Result<Song, IoError> {
    let parsed = panic::catch_unwind(|| {
        let mut gp_song = GpSong::default();
        gp_song.read_gp5(bytes).map(|()| gp_song)
    });
    let gp_song = match parsed {
        Ok(Ok(song)) => song,
        Ok(Err(e)) => return Err(IoError::GuitarProError(format!("{e:?}"))),
        Err(payload) => {
            return Err(IoError::GuitarProError(format!(
                "the parser panicked: {}",
                panic_message(payload.as_ref())
            )))
        }
    };
    interop::from_guitarpro(gp_song).map_err(IoError::InvalidSong)
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
        for bytes in [&[][..], &[0xFF; 3][..], b"FICHIER GUITAR PRO v5.00"] {
            assert!(import_guitar_pro_from_bytes(bytes).is_err());
        }
    }
}
