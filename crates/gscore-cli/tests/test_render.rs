//! `gscore-cli render`: SVG of the endless page, PDF on pages for every test file; other
//! outputs are refused so a typo cannot overwrite a song.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(unused_crate_dependencies)]

use std::ffi::OsString;
use std::path::PathBuf;

fn testdata(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative)
}

fn temp_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!("gscore-render-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn render(file: &PathBuf, output: &PathBuf) -> Result<String, gscore_cli::CliError> {
    let args: Vec<OsString> = vec!["render".into(), file.into(), "-o".into(), output.into()];
    gscore_cli::run_cli(args)
}

#[test]
fn every_test_file_renders_as_svg_and_as_pdf() {
    let dir = temp_dir();
    for name in [
        "basic/steel_guitar_dadgad.gp",
        "basic/note_lengths.gp5",
        "basic/ornaments.gpx",
    ] {
        let file = testdata(name);
        let svg = dir.join("out.svg");
        render(&file, &svg).unwrap();
        assert!(std::fs::read_to_string(&svg).unwrap().starts_with("<svg"));
        let pdf = dir.join("out.PDF");
        render(&file, &pdf).unwrap();
        let bytes = std::fs::read(&pdf).unwrap();
        assert!(bytes.starts_with(b"%PDF-"), "{name}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn other_outputs_are_refused() {
    let dir = temp_dir();
    let song = dir.join("song.gp");
    std::fs::write(&song, b"keep me").unwrap();
    let error = render(&testdata("basic/ornaments.gp"), &song).unwrap_err();
    assert!(error.to_string().contains(".svg or .pdf"), "{error}");
    assert_eq!(std::fs::read(&song).unwrap(), b"keep me");
    std::fs::remove_dir_all(dir).unwrap();
}
