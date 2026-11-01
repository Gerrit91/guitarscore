//! `gscore-cli convert`: every test file converts to GP7 without changing what `info` and
//! `dump` show; refusals and usage errors.

// This whole crate is test code: failing loudly (panic, unwrap, indexing) is how tests
// report problems. clippy's in-test allowances only cover `#[test]` functions, not helpers.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

const ALL_FILES: [&str; 9] = [
    "basic/steel_guitar_dadgad.gp",
    "basic/steel_guitar_dadgad.gpx",
    "basic/steel_guitar_dadgad.gp5",
    "basic/note_lengths.gp",
    "basic/note_lengths.gpx",
    "basic/note_lengths.gp5",
    "basic/ornaments.gp",
    "basic/ornaments.gpx",
    "basic/ornaments.gp5",
];

fn testdata(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative)
}

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!(
        "gscore-convert-{name}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cli(args: &[&OsString]) -> Result<String, gscore_cli::CliError> {
    gscore_cli::run_cli(args.iter().map(|a| (*a).clone()))
}

fn run(command: &str, file: &Path) -> String {
    cli(&[&command.into(), &file.as_os_str().to_owned()]).unwrap()
}

/// `info` without its first line (the format, which is what a conversion changes).
fn info_without_format(file: &Path) -> String {
    run("info", file)
        .lines()
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_test_file_converts_to_gp7_unchanged() {
    let dir = temp_dir("all");
    for relative in ALL_FILES {
        let input = testdata(relative);
        let output = dir.join("out.gp");
        let message = cli(&[
            &"convert".into(),
            &input.clone().into(),
            &"-o".into(),
            &output.clone().into(),
        ])
        .unwrap_or_else(|e| panic!("{relative}: {e}"));
        assert!(message.starts_with("Saved"), "{relative}: {message}");
        assert_eq!(
            info_without_format(&output),
            info_without_format(&input),
            "{relative}"
        );
        assert_eq!(run("dump", &output), run("dump", &input), "{relative}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn overwriting_keeps_a_backup_and_the_message_says_so() {
    let dir = temp_dir("backup");
    let output = dir.join("song.gp");
    let convert = |relative: &str| {
        cli(&[
            &"convert".into(),
            &testdata(relative).into(),
            &"-o".into(),
            &output.clone().into(),
        ])
        .unwrap()
    };
    let first = convert("basic/steel_guitar_dadgad.gp");
    assert!(!first.contains("previous file"), "{first}");
    let second = convert("basic/ornaments.gp");
    assert!(second.contains("song.gp.bak"), "{second}");
    assert!(dir.join("song.gp.bak").is_file());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unsupported_targets_and_unknown_extensions_are_refused() {
    let dir = temp_dir("refuse");
    let input: OsString = testdata("basic/ornaments.gp").into();
    let gp5: OsString = dir.join("song.gp5").into();
    let err = cli(&[&"convert".into(), &input, &"-o".into(), &gp5]).unwrap_err();
    assert_eq!(err.exit_code(), 1);
    assert!(err.to_string().contains("not supported yet"), "{err}");

    let txt: OsString = dir.join("song.txt").into();
    let err = cli(&[&"convert".into(), &input, &"-o".into(), &txt]).unwrap_err();
    assert_eq!(err.exit_code(), 2, "{err}");

    // --to overrides the extension.
    let ok = cli(&[
        &"convert".into(),
        &input,
        &"-o".into(),
        &txt,
        &"--to".into(),
        &"gp".into(),
    ]);
    assert!(ok.is_ok(), "{ok:?}");
    assert!(dir.join("song.txt").is_file());
    assert!(!dir.join("song.gp5").exists());
    std::fs::remove_dir_all(dir).unwrap();
}
