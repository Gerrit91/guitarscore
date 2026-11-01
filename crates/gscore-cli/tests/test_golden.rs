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

const NOTE_LENGTHS: [&str; 3] = [
    "basic/note_lengths.gp",
    "basic/note_lengths.gpx",
    "basic/note_lengths.gp5",
];

const ORNAMENTS: [&str; 3] = [
    "basic/ornaments.gp",
    "basic/ornaments.gpx",
    "basic/ornaments.gp5",
];

/// Every owner test file, in the formats Guitar Pro exports it to.
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
    for file in ALL_FILES {
        check_golden("info", file);
    }
}

#[test]
fn dump_matches_references() {
    for file in ALL_FILES {
        check_golden("dump", file);
    }
}

/// Guitar Pro 5 has no double-dotted notes: its export of `note_lengths` turns the 7/16
/// note in measure 9 into 3/8, so the measure is a sixteenth short. The info reference
/// records the issue and the dump reference the shorter note. GP7 and GPX keep it.
#[test]
fn note_lengths_gp5_cannot_store_double_dots() {
    let gp5 = run("info", "basic/note_lengths.gp5");
    assert!(
        gp5.contains("measure 9, voice 1: measure not filled (15/16 of 1/1)"),
        "{gp5}"
    );
    for file in &NOTE_LENGTHS[..2] {
        let info = run("info", file);
        assert!(!info.contains("Issues:"), "{file}:\n{info}");
    }
}

/// The note grid (frets, dead notes, barlines) must be the same in every export: no
/// ornament is a note or a rhythm. GP7 and GPX also dump the same effects; GP5 marks a
/// heavy accent and a staccato note with its accent bit too, which is why only the grid is
/// compared across all three.
#[test]
fn ornaments_note_grid_is_the_same_in_every_export() {
    let expected = note_grid(&run("dump", ORNAMENTS[0]));
    assert!(!expected.is_empty(), "{expected:#?}");
    for file in &ORNAMENTS[1..] {
        assert_eq!(note_grid(&run("dump", file)), expected, "{file}");
    }
    assert_eq!(run("dump", ORNAMENTS[0]), run("dump", ORNAMENTS[1]));
}

/// The dump uses the usual ASCII-tab symbols (`ornaments.txt` is Guitar Pro's own export
/// with its legend). These are the cells the reference marks.
#[test]
fn ornaments_show_effect_suffixes() {
    let dump = run("dump", ORNAMENTS[0]);
    let tab = tab_lines(&dump).join("\n");
    for cell in [
        "0L", "0h", "2p", "15b17", "(0)", "0>", "0.", "7~", "7*", "7[19]", "15w", "15\\12", "12/15",
    ] {
        assert!(tab.contains(cell), "missing {cell:?} in\n{dump}");
    }
    // One `h`/`p` per hammer/pull pair, on the origin note only (the direction comes from
    // the next note's pitch, since Guitar Pro marks both ends the same way). Only notes
    // that really have a slide get a slide mark, and a direct slide sits right against the
    // note it slides to.
    assert!(tab.contains("0h----2-----2p----0"), "{dump}");
    assert!(!tab.contains("15\\-12"), "{dump}");
    assert_eq!(tab.matches('h').count(), 1, "{dump}");
    assert_eq!(tab.matches('p').count(), 1, "{dump}");
    // Palm mutes are not a suffix on the fret but a mark under the staff.
    assert!(!tab.contains('M'), "{dump}");
    // The tie is marked on its origin, and the continuation stays a plain fret.
    assert_eq!(tab.matches('L').count(), 1, "{dump}");
    assert!(tab.contains("0L----------0"), "{dump}");
}

/// The palm-mute row sits under the staff: `PM` starts at the first muted note, dashes
/// carry the run and `|` ends it.
#[test]
fn palm_mutes_are_under_the_staff() {
    let dump = run("dump", ORNAMENTS[0]);
    assert_annotation_row_is_aligned(&dump, "PM");
}

/// Let ring gets the same row treatment as palm mutes, with the `LR` mark.
#[test]
fn let_ring_is_under_the_staff() {
    let dump = run("dump", ORNAMENTS[0]);
    let row = assert_annotation_row_is_aligned(&dump, "LR");
    assert!(row.ends_with('|'), "{row:?}");
}

/// Palm mute and let ring are independent rows, so a system that uses both gets two
/// under-staff lines. A later picking row would stack the same way.
#[test]
fn palm_mute_and_let_ring_stack_under_the_staff() {
    let file = testdata(ORNAMENTS[0]).into_os_string();
    let dump =
        gscore_cli::run_cli(["dump".into(), file, "--width".into(), "10000".into()]).expect("dump");

    let pm = annotation_row_index(&dump, "PM");
    let lr = annotation_row_index(&dump, "LR");
    assert_eq!(lr, pm + 1, "the rows are not stacked\n{dump}");
    assert_annotation_row_is_aligned(&dump, "PM");
    assert_annotation_row_is_aligned(&dump, "LR");
}

/// Checks that a mark starts a run under the staff and that the mark sits under a fret in
/// one of the six string rows above it.
fn assert_annotation_row_is_aligned(dump: &str, mark: &str) -> String {
    let lines: Vec<&str> = dump.lines().collect();
    let index = annotation_row_index(dump, mark);
    let row = lines.get(index).copied().unwrap_or_default().to_owned();
    assert_eq!(row.matches(mark).count(), 1, "{row:?}");
    let column = row.find(mark).expect("mark in the row");
    let string_rows: Vec<&str> = lines
        .iter()
        .take(index)
        .filter(|line| is_tab_line(line))
        .rev()
        .take(6)
        .copied()
        .collect();
    let aligned = string_rows
        .iter()
        .any(|line| line.chars().nth(column).is_some_and(|c| c.is_ascii_digit()));
    assert!(aligned, "{mark} is not above a note in\n{dump}");
    row
}

/// The line number of the first annotation row with `mark`, or a panic.
fn annotation_row_index(dump: &str, mark: &str) -> usize {
    dump.lines()
        .position(|line| line.trim_start().starts_with(mark))
        .unwrap_or_else(|| panic!("no {mark} row in\n{dump}"))
}

/// True if a line belongs to a tab grid: a string label, then ` |`.
fn is_tab_line(line: &str) -> bool {
    line.split_once(" |")
        .is_some_and(|(label, _)| !label.is_empty() && label.trim().len() <= 2)
}

/// Lines of a tab grid: a string label, then ` |`.
fn tab_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| is_tab_line(l))
        .map(str::to_owned)
        .collect()
}

/// A tab grid with effect symbols, dashes and spaces removed: the frets, dead notes and
/// barlines in order. Effect symbols change a beat's column width, so the exact columns are
/// not comparable between formats, but the notes must be. A bracketed harmonic node is
/// dropped because GP5 cannot store it (the written fret stays).
fn note_grid(text: &str) -> Vec<String> {
    tab_lines(text)
        .into_iter()
        .map(|line| match line.split_once(" |") {
            Some((label, tab)) => {
                let mut grid = format!("{label} |");
                let mut in_node = false;
                for c in tab.chars() {
                    match c {
                        '[' => in_node = true,
                        ']' => in_node = false,
                        _ if in_node => {}
                        _ if c.is_ascii_digit() || matches!(c, 'x' | '?' | '|') => grid.push(c),
                        _ => {}
                    }
                }
                grid
            }
            None => line,
        })
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
fn legend_matches_reference() {
    let output = gscore_cli::run_cli(["legend"]).expect("legend");
    for mark in ["h", "p", "b", "r", "/", "\\", "PM", "LR"] {
        assert!(output.contains(mark), "{output}");
    }
    let golden = crate_dir().join("tests/golden/legend.txt");
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
        "legend output differs from {}\n--- expected ---\n{expected}\n--- actual ---\n{output}",
        golden.display()
    );
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
