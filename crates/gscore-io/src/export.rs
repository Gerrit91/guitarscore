//! Export functionality for Guitar Pro files

use std::panic;
use std::path::Path;
use std::fs;

use guitarpro::Song as GpSong;
use gscore_model::Song;

use crate::error::IoError;

/// Export a Song to a Guitar Pro file at the given path
pub fn export_guitar_pro_file(song: &Song, file_path: &Path) -> Result<(), IoError> {
    let result = panic::catch_unwind(|| {
        let mut gp_song = GpSong::default();
        gp_song.name = song.title.clone();
        
        let bytes = gp_song.write_gp().map_err(|e| format!("{:?}", e))?;
        fs::write(file_path, bytes).map_err(|e| format!("{:?}", e))?;
        Ok::<(), String>(())
    });

    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(IoError::GuitarProError(format!("GuitarPro export error: {}", e))),
        Err(e) => Err(IoError::GuitarProError(format!("Panic during file export: {:?}", e))),
    }
}

/// Export a Song to bytes
pub fn export_guitar_pro_to_bytes(song: &Song) -> Result<Vec<u8>, IoError> {
    let result = panic::catch_unwind(|| {
        let mut gp_song = GpSong::default();
        gp_song.name = song.title.clone();
        
        let bytes = gp_song.write_gpx().map_err(|e| format!("{:?}", e))?;
        Ok::<Vec<u8>, String>(bytes)
    });

    match result {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(e)) => Err(IoError::GuitarProError(format!("GuitarPro export error: {}", e))),
        Err(e) => Err(IoError::GuitarProError(format!("Panic during file export: {:?}", e))),
    }
}
