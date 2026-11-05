//! Exporting a song to another file format (AGENTS.md "Saving", `File › Export`).
//!
//! Unlike a save, an export is an explicit conversion: the target format may not hold
//! everything the song does, and that is expected. The file is written next to the target
//! first and then moved there (so a failed export leaves nothing behind), and the bytes are
//! read back in memory to report what the format could not keep.

use std::fs;
use std::path::{Path, PathBuf};

use gscore_io::{FileFormat, export_to_bytes, import_guitar_pro_from_bytes};
use gscore_model::{Difference, Song, differences, settings_differences};

use crate::save::{SaveError, sibling};

/// What a successful export did.
#[derive(Debug, Clone)]
pub struct ExportReport {
    /// The file that was written.
    pub path: PathBuf,
    /// What the target format could not keep (empty for a lossless format).
    pub losses: Vec<Difference>,
}

/// Writes `song` to `path` in `format`. Existing files are replaced atomically.
pub fn export(song: &Song, path: &Path, format: FileFormat) -> Result<ExportReport, SaveError> {
    let bytes = export_to_bytes(song, format).map_err(SaveError::Export)?;
    // Read the bytes back in memory and compare: this says what an older or different
    // format drops, without touching the disk yet.
    let losses = match import_guitar_pro_from_bytes(&bytes) {
        Ok(reread) => {
            let mut losses = differences(song, &reread.song);
            losses.extend(settings_differences(song, &reread.song));
            losses
        }
        Err(error) => return Err(SaveError::Reread(error)),
    };

    let temp = sibling(path, |name| {
        let mut temp = std::ffi::OsString::from(".");
        temp.push(name);
        temp.push(format!(".gscore-tmp-{}", std::process::id()));
        temp
    })?;
    if let Err(error) = fs::write(&temp, &bytes) {
        let _ = fs::remove_file(&temp);
        return Err(SaveError::Io {
            what: format!("cannot write {}", temp.display()),
            source: error,
        });
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(SaveError::Io {
            what: format!("cannot replace {}", path.display()),
            source: error,
        });
    }
    Ok(ExportReport {
        path: path.to_path_buf(),
        losses,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_export_writes_the_file_and_replaces_it_atomically() {
        let dir = std::env::temp_dir().join(format!("gscore-export-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("song.gp3");
        let song = Song::default();
        let report = export(&song, &path, FileFormat::Gp3).unwrap();
        assert!(report.losses.is_empty(), "{:?}", report.losses);
        let bytes = fs::read(&path).unwrap();
        assert_eq!(gscore_io::detect_format(&bytes).unwrap(), FileFormat::Gp3);

        // Exporting again replaces the file and leaves no temporary one behind.
        export(&song, &path, FileFormat::Gp4).unwrap();
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("song.gp3")]);

        let missing = dir.join("no such folder").join("song.gp5");
        assert!(export(&song, &missing, FileFormat::Gp5).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
