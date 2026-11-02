//! Opening files from disk: good files, missing files, files too large, and garbage.

#![allow(clippy::unwrap_used, clippy::panic)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;

use gscore_io::MAX_INPUT_BYTES;
use gscore_session::{OpenError, open, open_with_progress};

fn testdata(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative)
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gscore-open-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_test_file_opens() {
    let imported = open(&testdata("basic/steel_guitar_dadgad.gp")).unwrap();
    assert_eq!(imported.song.title(), "Steel Guitar DADGAD");
}

#[test]
fn opening_reports_read_progress_and_can_be_cancelled() {
    let path = testdata("basic/steel_guitar_dadgad.gp");
    let total = std::fs::metadata(&path).unwrap().len();
    let mut calls = Vec::new();
    let (imported, bytes) =
        open_with_progress(&path, |done, size| calls.push((done, size)), || false).unwrap();
    assert_eq!(u64::try_from(bytes.len()).unwrap(), total);
    assert!(calls.iter().all(|(_, size)| *size == total));
    assert_eq!(calls.last(), Some(&(total, total)));
    assert_eq!(imported.song.title(), "Steel Guitar DADGAD");

    let error = open_with_progress(&path, |_, _| {}, || true).unwrap_err();
    assert!(matches!(error, OpenError::Cancelled), "{error}");
}

#[test]
fn a_missing_file_is_a_read_error() {
    let error = open(&testdata("basic/no_such_file.gp")).unwrap_err();
    assert!(matches!(error, OpenError::Read { .. }), "{error}");
    assert!(error.to_string().contains("no_such_file.gp"), "{error}");
}

#[test]
fn a_file_over_the_limit_is_refused_before_reading() {
    let dir = temp_dir("large");
    let path = dir.join("huge.gp");
    // A sparse file: large on paper, no disk space used.
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(u64::try_from(MAX_INPUT_BYTES).unwrap() + 1)
        .unwrap();
    let error = open(&path).unwrap_err();
    assert!(matches!(error, OpenError::TooLarge { .. }), "{error}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn garbage_is_an_import_error() {
    let dir = temp_dir("garbage");
    let path = dir.join("garbage.gp");
    std::fs::write(&path, b"not a guitar pro file").unwrap();
    let error = open(&path).unwrap_err();
    assert!(matches!(error, OpenError::Import { .. }), "{error}");
    assert!(error.to_string().starts_with("cannot import"), "{error}");
    std::fs::remove_dir_all(&dir).unwrap();
}
