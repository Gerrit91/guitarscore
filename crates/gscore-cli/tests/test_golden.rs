//! `gscore-cli` output for the owner's test files in `testdata/`, compared with reference
//! files in `tests/golden/` (`<file name>.<command>.txt`, e.g.
//! `steel_guitar_dadgad.gp5.info.txt`), and the `dump` tab grid compared with Guitar Pro's
//! own ASCII export of the same song.
//!
//! If the output changes on purpose, regenerate the references with
//! `GSCORE_UPDATE_GOLDEN=1 cargo test -p gscore-cli --test test_golden` and have the owner
//! review the diff before committing.

// This whole crate is test code: failing loudly (panic, unwrap, indexing) is how tests
// report problems. clippy's in-test allowances only cover `#[test]` functions, not helpers.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;

const DADGAD: [&str; 3] = [
    "basic/steel_guitar_dadgad.gp",
    "basic/steel_guitar_dadgad.gpx",
    "basic/steel_guitar_dadgad.gp5",
];

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn testdata(relative: &str) -> PathBuf {
    crate_dir().join("../../testdata").join(relative)
}

/// Runs `gscore-cli <command> <file>` and returns its output.
fn run(command: &str, relative: &str) -> String {
    let file = testdata(relative);
    gscore_cli::run_cli([command.into(), file.clone().into_os_string()])
        .unwrap_or_else(|e| panic!("{command} {} failed: {e}", file.display()))
}

/// Compares the output of `gscore-cli <command> <file>` with its reference file.
fn check_golden(command: &str, relative: &str) {
    let output = run(command, relative);
    let file_name = testdata(relative)
        .file_name()
        .and_then(|n| n.to_str())
        .expect("test file has a name")
        .to_owned();
    let golden = crate_dir()
        .join("tests/golden")
        .join(format!("{file_name}.{command}.txt"));

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
        "{command} output for {relative} differs from {}\n--- expected ---\n{expected}\n--- actual ---\n{output}",
        golden.display()
    );
}

#[test]
fn info_matches_references() {
    for file in DADGAD {
        check_golden("info", file);
    }
}

#[test]
fn dump_matches_references() {
    for file in DADGAD {
        check_golden("dump", file);
    }
}

/// Lines of a tab grid: a string label, then ` |`.
fn tab_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| {
            l.split_once(" |")
                .is_some_and(|(label, _)| !label.is_empty() && label.trim().len() <= 2)
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn dump_matches_guitar_pros_ascii_export() {
    let guitar_pro = std::fs::read_to_string(testdata("basic/steel_guitar_dadgad.txt"))
        .expect("Guitar Pro's export is in testdata");
    let expected = tab_lines(&guitar_pro);
    assert_eq!(expected.len(), 6, "{guitar_pro}");
    for file in DADGAD {
        assert_eq!(tab_lines(&run("dump", file)), expected, "{file}");
    }
}

#[test]
fn dump_options() {
    let file = testdata("basic/steel_guitar_dadgad.gp").into_os_string();
    let dump = |extra: &[&str]| {
        let mut args = vec!["dump".into(), file.clone()];
        args.extend(extra.iter().map(|a| a.into()));
        gscore_cli::run_cli(args)
    };
    let full = dump(&[]).unwrap();
    // A measure wider than the line still gets printed (on its own line).
    assert_eq!(dump(&["--width", "10"]).unwrap(), full);
    assert_eq!(dump(&["--track", "1"]).unwrap(), full);
    for bad in ["0", "2"] {
        let err = dump(&["--track", bad]).unwrap_err();
        assert_eq!(err.exit_code(), 2, "--track {bad}: {err}");
    }
}
