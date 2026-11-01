//! `gscore-cli info` output for the owner's test files in `testdata/`, compared with
//! reference files in `tests/golden/` (`<file name>.info.txt`, e.g.
//! `steel_guitar_dadgad.gp5.info.txt`).
//!
//! If the output changes on purpose, regenerate the references with
//! `GSCORE_UPDATE_GOLDEN=1 cargo test -p gscore-cli --test test_info_golden` and have the
//! owner review the diff before committing.

// This whole crate is test code: failing loudly (panic, unwrap, indexing) is how tests
// report problems. clippy's in-test allowances only cover `#[test]` functions, not helpers.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

// Integration tests are separate crates; these dependencies are used by the library only.
use clap as _;
use gscore_io as _;
use gscore_model as _;
use gscore_session as _;

use std::path::PathBuf;

fn check_info(relative: &str) {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let file = crate_dir.join("../../testdata").join(relative);
    let file_name = file
        .file_name()
        .and_then(|n| n.to_str())
        .expect("test file has a name");
    let golden = crate_dir
        .join("tests/golden")
        .join(format!("{file_name}.info.txt"));
    let output = gscore_cli::run_cli(["info".into(), file.clone().into_os_string()])
        .unwrap_or_else(|e| panic!("info {} failed: {e}", file.display()));

    if std::env::var_os("GSCORE_UPDATE_GOLDEN").is_some() {
        std::fs::write(&golden, &output).expect("write golden file");
        return;
    }
    let expected = std::fs::read_to_string(&golden).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e} (create it with GSCORE_UPDATE_GOLDEN=1)",
            golden.display()
        )
    });
    assert!(
        output == expected,
        "info output for {relative} differs from {}\n--- expected ---\n{expected}\n--- actual ---\n{output}",
        golden.display()
    );
}

#[test]
fn info_steel_guitar_dadgad_gp7() {
    check_info("basic/steel_guitar_dadgad.gp");
}

#[test]
fn info_steel_guitar_dadgad_gp6() {
    check_info("basic/steel_guitar_dadgad.gpx");
}

#[test]
fn info_steel_guitar_dadgad_gp5() {
    check_info("basic/steel_guitar_dadgad.gp5");
}
