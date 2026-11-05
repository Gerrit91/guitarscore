//! `gscore-cli render`: SVG of the endless page, PDF on pages for every test file, several
//! tracks together; other outputs are refused so a typo cannot overwrite a song.

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

fn render_with(
    file: &PathBuf,
    output: &PathBuf,
    extra: &[&str],
) -> Result<String, gscore_cli::CliError> {
    let mut args: Vec<OsString> = vec!["render".into(), file.into(), "-o".into(), output.into()];
    args.extend(extra.iter().map(OsString::from));
    gscore_cli::run_cli(args)
}

#[test]
fn several_tracks_render_together() {
    let dir = temp_dir();
    let file = testdata("basic/multi_staff.gp");
    let svg = dir.join("all.svg");
    let out = render_with(&file, &svg, &["--all-tracks"]).unwrap();
    assert!(out.contains("all tracks"), "{out}");
    let all = std::fs::read_to_string(&svg).unwrap();
    let pdf = dir.join("all.pdf");
    render_with(&file, &pdf, &["--all-tracks"]).unwrap();
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF-"));
    let chosen = dir.join("two.svg");
    let out = render_with(&file, &chosen, &["--track", "2", "--track", "5"]).unwrap();
    assert!(out.contains("tracks 2, 5"), "{out}");
    let two = std::fs::read_to_string(&chosen).unwrap();
    assert!(two.len() < all.len(), "two tracks draw less than six");
    // A track given twice, track 0 and a missing track are refused.
    for bad in [
        &["--track", "2", "--track", "2"][..],
        &["--track", "0"],
        &["--track", "9"],
    ] {
        assert!(render_with(&file, &chosen, bad).is_err(), "{bad:?}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
