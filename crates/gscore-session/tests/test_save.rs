//! Safe saving: backups, refusals, no leftovers, and that the saved file is the song.

// This whole crate is test code: failing loudly (panic, unwrap, indexing) is how tests
// report problems. clippy's in-test allowances only cover `#[test]` functions, not helpers.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};

use gscore_io::{FileFormat, import_guitar_pro_from_bytes};
use gscore_model::{Song, differences};
use gscore_session::{SaveError, SaveOptions, backup_path, save};

/// A fresh, empty directory for one test.
fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir =
        std::env::temp_dir().join(format!("gscore-save-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn song(relative: &str) -> Song {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative);
    import_guitar_pro_from_bytes(&std::fs::read(path).unwrap())
        .unwrap()
        .song
}

fn gp7() -> SaveOptions {
    SaveOptions {
        format: FileFormat::Gp,
        allow_losses: false,
    }
}

fn read_song(path: &Path) -> Song {
    import_guitar_pro_from_bytes(&std::fs::read(path).unwrap())
        .unwrap()
        .song
}

fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn saved_file_reads_back_as_the_same_song() {
    let dir = temp_dir("same");
    for file in [
        "basic/steel_guitar_dadgad.gp",
        "basic/ornaments.gpx",
        "basic/note_lengths.gp5",
    ] {
        let original = song(file);
        let target = dir.join("out.gp");
        let report = save(&original, &target, gp7()).unwrap();
        assert!(report.losses.is_empty(), "{file}");
        assert_eq!(differences(&original, &read_song(&target)), [], "{file}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn first_overwrite_keeps_the_original_as_backup() {
    let dir = temp_dir("backup");
    let target = dir.join("song.gp");
    let backup = backup_path(&target).unwrap();
    assert_eq!(backup, dir.join("song.gp.bak"));

    // A new file gets no backup.
    let report = save(&song("basic/steel_guitar_dadgad.gp"), &target, gp7()).unwrap();
    assert_eq!(report.backup, None);
    let original = std::fs::read(&target).unwrap();

    // The first overwrite copies the original.
    let report = save(&song("basic/ornaments.gp"), &target, gp7()).unwrap();
    assert_eq!(report.backup.as_deref(), Some(backup.as_path()));
    assert_eq!(std::fs::read(&backup).unwrap(), original);

    // Later overwrites keep that backup unchanged.
    let report = save(&song("basic/note_lengths.gp"), &target, gp7()).unwrap();
    assert_eq!(report.backup, None);
    assert_eq!(std::fs::read(&backup).unwrap(), original);
    assert_eq!(
        read_song(&target).title(),
        song("basic/note_lengths.gp").title()
    );

    // No temporary files are left behind.
    assert_eq!(file_names(&dir), ["song.gp", "song.gp.bak"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unsupported_formats_are_refused_before_writing() {
    let dir = temp_dir("unsupported");
    let target = dir.join("song.gp5");
    for format in [
        FileFormat::Gp5,
        FileFormat::Gpx,
        FileFormat::Gp4,
        FileFormat::Gp3,
    ] {
        let options = SaveOptions {
            format,
            allow_losses: true,
        };
        let err = save(&song("basic/steel_guitar_dadgad.gp"), &target, options).unwrap_err();
        assert!(matches!(err, SaveError::UnsupportedFormat(f) if f == format));
    }
    assert!(file_names(&dir).is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_failed_save_leaves_no_files_and_the_original_untouched() {
    let dir = temp_dir("failure");
    // The directory does not exist: creating the temporary file fails.
    let target = dir.join("missing").join("song.gp");
    let err = save(&song("basic/steel_guitar_dadgad.gp"), &target, gp7()).unwrap_err();
    assert!(matches!(err, SaveError::Io { .. }), "{err}");
    assert!(file_names(&dir).is_empty());

    // A path without a file name is an error, not a panic.
    assert!(save(&Song::default(), Path::new("/"), gp7()).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
