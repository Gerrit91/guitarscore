//! Import of Guitar Pro bytes into a `Song`.

use std::panic;

use gscore_model::{Beat, Measure, Note, Song, Track, Voice};
use guitarpro::Song as GpSong;

use crate::error::IoError;

/// Parses GP5 bytes into a song.
///
/// Malformed input yields an error, never a panic: the guitarpro parser runs inside
/// `catch_unwind`, and values the model cannot hold are rejected, not truncated.
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
    convert_song(&gp_song)
}

fn convert_song(gp: &GpSong) -> Result<Song, IoError> {
    let tracks = gp
        .tracks
        .iter()
        .map(|gp_track| {
            Ok(Track {
                name: gp_track.name.clone(),
                measures: gp_track
                    .measures
                    .iter()
                    .map(convert_measure)
                    .collect::<Result<_, _>>()?,
                // Not read from the file yet; see docs/gaps.md.
                tuning: vec![64, 59, 55, 50, 45, 40],
            })
        })
        .collect::<Result<_, IoError>>()?;
    Ok(Song::new(
        gp.name.clone(),
        "Unknown Artist".to_owned(),
        gp.tempo,
        tracks,
    ))
}

fn convert_measure(gp: &guitarpro::Measure) -> Result<Measure, IoError> {
    let voices = gp
        .voices
        .iter()
        .map(|gp_voice| {
            let beats = gp_voice
                .beats
                .iter()
                .map(|gp_beat| {
                    let notes = gp_beat
                        .notes
                        .iter()
                        .map(convert_note)
                        .collect::<Result<_, _>>()?;
                    Ok(Beat {
                        duration: 1.0,
                        notes,
                    })
                })
                .collect::<Result<_, IoError>>()?;
            Ok(Voice { beats })
        })
        .collect::<Result<_, IoError>>()?;
    Ok(Measure {
        number: gp.number,
        voices,
        // Not read from the file yet; see docs/gaps.md.
        time_signature: (4, 4),
    })
}

fn convert_note(gp: &guitarpro::Note) -> Result<Note, IoError> {
    let fret = i8::try_from(gp.value)
        .map_err(|_| IoError::ParseError(format!("fret {} is out of range", gp.value)))?;
    let string = u8::try_from(gp.string)
        .map_err(|_| IoError::ParseError(format!("string {} is out of range", gp.string)))?;
    Ok(Note {
        fret,
        string,
        duration: 1.0,
        is_bent: false,
        is_slide: false,
    })
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
