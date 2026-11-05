//! `gscore-cli roundtrip-check` on the owner's test files: write, read back, compare.

// This whole crate is test code: failing loudly (panic, unwrap, indexing) is how tests
// report problems. clippy's in-test allowances only cover `#[test]` functions, not helpers.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;

fn roundtrip(relative: &str, extra: &[&str]) -> Result<String, gscore_cli::CliError> {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative);
    let mut args = vec!["roundtrip-check".into(), file.into_os_string()];
    args.extend(extra.iter().map(|a| a.into()));
    gscore_cli::run_cli(args)
}

#[test]
fn every_format_round_trips_without_differences() {
    for file in [
        "basic/steel_guitar_dadgad.gp",
        "basic/steel_guitar_dadgad.gpx",
        "basic/steel_guitar_dadgad.gp5",
        "basic/advanced_ornaments.gp",
        // Six tracks, and Guitar Pro's line breaks for all tracks shown together.
        "basic/multi_staff.gp",
    ] {
        let out = roundtrip(file, &[]).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert!(out.starts_with("Round trip OK"), "{file}: {out}");
    }
}

#[test]
fn conversions_keep_the_compared_content() {
    for (file, to) in [
        ("basic/steel_guitar_dadgad.gp5", "gp"),
        ("basic/steel_guitar_dadgad.gp5", "gpx"),
        ("basic/steel_guitar_dadgad.gpx", "gp5"),
        ("basic/steel_guitar_dadgad.gpx", "gp"),
        ("basic/steel_guitar_dadgad.gp", "gpx"),
    ] {
        let out = roundtrip(file, &["--to", to]).unwrap_or_else(|e| panic!("{file} -> {to}: {e}"));
        assert!(out.starts_with("Round trip OK"), "{file} -> {to}: {out}");
    }
}

#[test]
fn losses_are_reported_as_failure() {
    // GP5 has no field for the instrument name, so GP7 -> GP5 loses it.
    let err = roundtrip("basic/steel_guitar_dadgad.gp", &["--to", "gp5"]).unwrap_err();
    assert_eq!(err.exit_code(), 1);
    let text = err.to_string();
    assert!(text.contains("1 difference(s)"), "{text}");
    assert!(
        text.contains(r#"track 1: instrument name Some("Steel Guitar") became None"#),
        "{text}"
    );
}

/// Whatever the writers get wrong, every combination ends in `Ok` or `Err`; a panic in a
/// writer is contained and reported as an error.
#[test]
fn every_conversion_finishes_without_panicking() {
    for file in [
        "basic/steel_guitar_dadgad.gp",
        "basic/steel_guitar_dadgad.gpx",
        "basic/steel_guitar_dadgad.gp5",
        "basic/advanced_ornaments.gp",
    ] {
        for to in ["gp", "gpx", "gp5", "gp4", "gp3"] {
            if let Err(err) = roundtrip(file, &["--to", to]) {
                assert_eq!(err.exit_code(), 1, "{file} -> {to}: {err}");
            }
        }
    }
}
