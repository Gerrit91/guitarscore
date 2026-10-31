//! Import functionality for Guitar Pro files

use std::panic;
use std::path::Path;

use guitarpro::Song as GpSong;
use gscore_model::Song;

use crate::error::IoError;

/// Import a Guitar Pro file from the given path
pub fn import_guitar_pro_file(file_path: &Path) -> Result<Song, IoError> {
    if !file_path.exists() {
        return Err(IoError::IoError(
            std::io::Error::new(
                std::io::ErrorKind::NotFound, 
                format!("File not found: {:?}", file_path)
            )
        ));
    }

    let bytes = std::fs::read(file_path).map_err(|e| IoError::IoError(e))?;
    import_guitar_pro_from_bytes(&bytes)
}

/// Import a Guitar Pro file from bytes
pub fn import_guitar_pro_from_bytes(bytes: &[u8]) -> Result<Song, IoError> {
    let result = panic::catch_unwind(|| {
        let mut gp_song = GpSong::default();
        gp_song.read_gp5(bytes).map_err(|e| format!("{:?}", e))?;
        
        let tracks = gp_song.tracks.iter().map(|gp_track| {
            let measures = gp_track.measures.iter().map(|gp_measure| {
                let voices = gp_measure.voices.iter().map(|gp_voice| {
                    let beats = gp_voice.beats.iter().map(|gp_beat| {
                        let notes = gp_beat.notes.iter().map(|gp_note| {
                            gscore_model::Note {
                                fret: gp_note.value as i8,
                                string: gp_note.string as u8,
                                duration: 1.0,
                                is_bent: false,
                                is_slide: false,
                            }
                        }).collect();
                        
                        gscore_model::Beat {
                            duration: 1.0,
                            notes,
                        }
                    }).collect();
                    
                    gscore_model::Voice {
                        beats,
                    }
                }).collect();

                gscore_model::Measure {
                    number: gp_measure.number,
                    voices,
                    time_signature: (4, 4),
                }
            }).collect();

            gscore_model::Track {
                name: gp_track.name.clone(),
                measures,
                tuning: vec![64, 59, 55, 50, 45, 40],
            }
        }).collect();

        Ok::<Song, String>(Song::new(
            gp_song.name.clone(),
            "Unknown Artist".to_string(),
            gp_song.tempo as i16,
            tracks,
        ))
    });

    match result {
        Ok(Ok(song)) => Ok(song),
        Ok(Err(e)) => Err(IoError::GuitarProError(format!("GuitarPro parsing error: {}", e))),
        Err(e) => Err(IoError::GuitarProError(format!("Panic during file parsing: {:?}", e))),
    }
}
