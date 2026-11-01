//! `gscore-cli commands` and `gscore-cli edit`: listing, scripts, undo, errors.

// This whole crate is test code: failing loudly (panic, unwrap, indexing) is how tests
// report problems. clippy's in-test allowances only cover `#[test]` functions, not helpers.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn testdata(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative)
}

fn script(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/scripts")
        .join(name)
}

fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir =
        std::env::temp_dir().join(format!("gscore-edit-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cli(args: &[&OsString]) -> Result<String, gscore_cli::CliError> {
    gscore_cli::run_cli(args.iter().map(|a| (*a).clone()))
}

fn edit_script(
    input: &Path,
    output: &Path,
    script_name: &str,
) -> Result<String, gscore_cli::CliError> {
    cli(&[
        &"edit".into(),
        &input.as_os_str().to_owned(),
        &"-o".into(),
        &output.as_os_str().to_owned(),
        &"--script".into(),
        &script(script_name).into_os_string(),
    ])
}

#[test]
fn commands_lists_every_command_with_its_parameters() {
    let out = cli(&[&"commands".into()]).unwrap();
    assert!(out.contains("positions are 1-based"), "{out}");
    for info in gscore_edit::COMMANDS {
        assert!(out.contains(info.id), "{} missing: {out}", info.id);
        assert!(out.contains(info.label), "{} label: {out}", info.id);
        for param in info.params {
            assert!(
                out.contains(param.name),
                "{}: {} missing: {out}",
                info.id,
                param.name
            );
        }
    }
    assert!(out.contains("set_fret (alias: enter_fret)"), "{out}");
    // Every command's id shows up; `undo`/`redo` need no parameters.
    assert!(out.contains("undo - Undo"), "{out}");
}

#[test]
fn help_lists_the_new_commands() {
    let help = cli(&[&"--help".into()]).unwrap();
    assert!(help.contains("commands"), "{help}");
    assert!(help.contains("Apply editing commands"), "{help}");
    let own = cli(&[&"edit".into(), &"--help".into()]).unwrap();
    assert!(own.contains("--script"), "{own}");
    assert!(own.contains("set_fret"), "{own}");
}

#[test]
fn script_applies_the_edits_and_the_result_round_trips() {
    let dir = temp_dir("script");
    let input = testdata("basic/steel_guitar_dadgad.gp");
    let output = dir.join("out.gp");
    let message = edit_script(&input, &output, "edit_basic.gscript").unwrap();
    assert!(message.contains("Applied 7 command(s)"), "{message}");

    let dump = cli(&[&"dump".into(), &output.as_os_str().to_owned()]).unwrap();
    assert_eq!(
        dump,
        read(&golden("edit_basic.dump.txt")),
        "edited tablature changed"
    );

    let info = cli(&[&"info".into(), &output.as_os_str().to_owned()]).unwrap();
    assert!(info.contains("Measures:     2"), "{info}");

    let roundtrip = cli(&[&"roundtrip-check".into(), &output.as_os_str().to_owned()]).unwrap();
    assert!(roundtrip.starts_with("Round trip OK"), "{roundtrip}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn dash_c_and_a_script_apply_the_same_edits() {
    let dir = temp_dir("same");
    let input = testdata("basic/steel_guitar_dadgad.gp");
    let from_script = dir.join("script.gp");
    let from_flags = dir.join("flags.gp");
    edit_script(&input, &from_script, "edit_basic.gscript").unwrap();

    let commands = [
        "set_fret track=1 measure=1 beat=1 string=6 fret=7",
        "set_duration track=1 measure=1 beat=1 duration=1/8",
        "insert_measure measure=2",
        "set_fret track=1 measure=2 beat=1 string=1 fret=12",
        "set_duration track=1 measure=2 beat=1 duration=1/2",
        "insert_beat track=1 measure=2 voice=1 beat=2 duration=1/4",
        "insert_beat track=1 measure=2 voice=1 beat=3 duration=1/4",
    ];
    let mut args: Vec<OsString> = vec![
        "edit".into(),
        input.as_os_str().to_owned(),
        "-o".into(),
        from_flags.as_os_str().to_owned(),
    ];
    for command in commands {
        args.push("-c".into());
        args.push(command.into());
    }
    gscore_cli::run_cli(args).unwrap();

    let script_dump = cli(&[&"dump".into(), &from_script.as_os_str().to_owned()]).unwrap();
    let flags_dump = cli(&[&"dump".into(), &from_flags.as_os_str().to_owned()]).unwrap();
    assert_eq!(script_dump, flags_dump);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn undoing_every_script_edit_restores_the_input() {
    let dir = temp_dir("undo");
    let input = testdata("basic/steel_guitar_dadgad.gp");
    let output = dir.join("undo.gp");
    edit_script(&input, &output, "undo_all.gscript").unwrap();
    let restored = cli(&[&"dump".into(), &output.as_os_str().to_owned()]).unwrap();
    let original = cli(&[&"dump".into(), &input.as_os_str().to_owned()]).unwrap();
    assert_eq!(restored, original);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_bad_command_fails_and_writes_nothing() {
    let dir = temp_dir("bad");
    let input = testdata("basic/steel_guitar_dadgad.gp");
    let output = dir.join("out.gp");
    let cases = [
        "not_a_command",
        "set_fret measure=1 beat=1 string=1",
        "set_fret measure=1 beat=1 string=1 fret=200",
        "set_fret measure=99 beat=1 string=1 fret=5",
        "set_fret measure=1 beat=1 string=1 fret=5 extra=1",
    ];
    for command in cases {
        let err = cli(&[
            &"edit".into(),
            &input.as_os_str().to_owned(),
            &"-o".into(),
            &output.as_os_str().to_owned(),
            &"-c".into(),
            &command.into(),
        ])
        .unwrap_err();
        assert_eq!(err.exit_code(), 1, "{command}: {err}");
        assert!(!output.exists(), "{command} wrote a file");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_script_error_names_the_line_and_writes_nothing() {
    let dir = temp_dir("script-error");
    let input = testdata("basic/steel_guitar_dadgad.gp");
    let output = dir.join("out.gp");
    let bad_script = dir.join("bad.gscript");
    std::fs::write(
        &bad_script,
        "# a valid edit, then a typo\n\
         set_fret measure=1 beat=1 string=1 fret=5\n\
         set_fret measure=1 beat=1 string=1 frot=5\n",
    )
    .unwrap();
    let err = cli(&[
        &"edit".into(),
        &input.as_os_str().to_owned(),
        &"-o".into(),
        &output.as_os_str().to_owned(),
        &"--script".into(),
        &bad_script.as_os_str().to_owned(),
    ])
    .unwrap_err();
    assert_eq!(err.exit_code(), 1);
    assert!(
        err.to_string()
            .contains(&format!("{}:3", bad_script.display())),
        "{err}"
    );
    assert!(!output.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_commands_and_unknown_output_formats_are_usage_errors() {
    let dir = temp_dir("usage");
    let input = testdata("basic/steel_guitar_dadgad.gp");
    let output = dir.join("out.gp");
    let err = cli(&[
        &"edit".into(),
        &input.as_os_str().to_owned(),
        &"-o".into(),
        &output.as_os_str().to_owned(),
    ])
    .unwrap_err();
    assert_eq!(err.exit_code(), 2, "{err}");
    assert!(err.to_string().contains("nothing to do"), "{err}");

    let unknown = dir.join("out.txt");
    let err = cli(&[
        &"edit".into(),
        &input.as_os_str().to_owned(),
        &"-o".into(),
        &unknown.as_os_str().to_owned(),
        &"-c".into(),
        &"set_fret measure=1 beat=1 string=1 fret=5".into(),
    ])
    .unwrap_err();
    assert_eq!(err.exit_code(), 2, "{err}");
    assert!(!unknown.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn saving_to_a_format_that_cannot_be_written_is_refused() {
    let dir = temp_dir("format");
    let input = testdata("basic/steel_guitar_dadgad.gp");
    let output = dir.join("out.gp5");
    let err = cli(&[
        &"edit".into(),
        &input.as_os_str().to_owned(),
        &"-o".into(),
        &output.as_os_str().to_owned(),
        &"-c".into(),
        &"set_fret measure=1 beat=1 string=1 fret=5".into(),
    ])
    .unwrap_err();
    assert_eq!(err.exit_code(), 1, "{err}");
    assert!(err.to_string().contains("not supported"), "{err}");
    assert!(!output.exists());
    std::fs::remove_dir_all(dir).unwrap();
}
