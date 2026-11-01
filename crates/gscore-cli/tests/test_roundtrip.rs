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
fn gpx_and_gp5_round_trip_without_differences() {
    for file in [
        "basic/steel_guitar_dadgad.gpx",
        "basic/steel_guitar_dadgad.gp5",
    ] {
        let out = roundtrip(file, &[]).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert!(out.starts_with("Round trip OK"), "{file}: {out}");
    }
}

#[test]
#[ignore = "known gap: the GP7/GPX writer drops the instrument (docs/gaps.md #2, M3)"]
fn gp7_round_trips_without_differences() {
    let out = roundtrip("basic/steel_guitar_dadgad.gp", &[]).unwrap_or_else(|e| panic!("{e}"));
    assert!(out.starts_with("Round trip OK"), "{out}");
}

#[test]
fn differences_are_reported_as_failure() {
    // Today GP7 -> GP7 loses the instrument name; this checks the report itself.
    let err = roundtrip("basic/steel_guitar_dadgad.gp", &[]).unwrap_err();
    assert_eq!(err.exit_code(), 1);
    let text = err.to_string();
    assert!(text.contains("difference(s)"), "{text}");
    assert!(text.contains("track 1: instrument name"), "{text}");
}

#[test]
#[ignore = "known gap: the GP5 writer panics on songs read from GP6/GP7 (lyrics), M3"]
fn gpif_formats_convert_to_gp5() {
    for file in [
        "basic/steel_guitar_dadgad.gp",
        "basic/steel_guitar_dadgad.gpx",
    ] {
        let out = roundtrip(file, &["--to", "gp5"]).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert!(out.starts_with("Round trip OK"), "{file}: {out}");
    }
}

#[test]
#[ignore = "known gap: GP5 -> GP6/GP7 turns an empty second voice into a beat, M3"]
fn gp5_converts_to_gpif_formats() {
    for to in ["gp", "gpx"] {
        let out = roundtrip("basic/steel_guitar_dadgad.gp5", &["--to", to])
            .unwrap_or_else(|e| panic!("-> {to}: {e}"));
        assert!(out.starts_with("Round trip OK"), "-> {to}: {out}");
    }
}

/// Whatever the writers get wrong, every combination ends in `Ok` or `Err`; a panic in a
/// writer is contained and reported as an error.
#[test]
fn every_conversion_finishes_without_panicking() {
    for file in [
        "basic/steel_guitar_dadgad.gp",
        "basic/steel_guitar_dadgad.gpx",
        "basic/steel_guitar_dadgad.gp5",
    ] {
        for to in ["gp", "gpx", "gp5", "gp4", "gp3"] {
            if let Err(err) = roundtrip(file, &["--to", to]) {
                assert_eq!(err.exit_code(), 1, "{file} -> {to}: {err}");
            }
        }
    }
}
