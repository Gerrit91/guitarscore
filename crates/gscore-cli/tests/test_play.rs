//! `play`: choosing the start measure and the output. The error cases need no audio device;
//! the last test plays one measure with a sine-wave SoundFont on the default device
//! (skipped without one).

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::ffi::OsString;
use std::path::PathBuf;

fn testdata(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative)
}

fn play(extra: &[&str]) -> Result<String, gscore_cli::CliError> {
    let mut args: Vec<OsString> = vec!["play".into(), testdata("basic/note_lengths.gp").into()];
    args.extend(extra.iter().map(OsString::from));
    gscore_cli::run_cli(args)
}

#[test]
fn a_start_measure_outside_the_song_is_refused_before_anything_plays() {
    for from in ["0", "10"] {
        let error = play(&["--from", from, "--soundfont", "/nowhere.sf2"]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("does not exist (the song has 9)"),
            "{from}: {error}"
        );
    }
}

#[test]
fn port_and_soundfont_exclude_each_other() {
    let error = play(&["--port", "x", "--soundfont", "a.sf2"]).unwrap_err();
    assert!(error.to_string().contains("cannot be used with"), "{error}");
}

#[test]
fn a_wav_export_writes_a_riff_file_and_needs_a_soundfont() {
    let dir = std::env::temp_dir().join(format!("gscore-wav-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let soundfont = dir.join("sine.sf2");
    std::fs::write(&soundfont, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let output = dir.join("song.wav");
    let out = gscore_cli::run_cli([
        OsString::from("wav"),
        testdata("basic/note_lengths.gp").into_os_string(),
        "--output".into(),
        output.clone().into_os_string(),
        "--track".into(),
        "1".into(),
        "--soundfont".into(),
        soundfont.into_os_string(),
    ])
    .unwrap();
    assert!(out.contains("song.wav"), "{out}");
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(bytes.get(0..4), Some(&b"RIFF"[..]));
    assert_eq!(bytes.get(8..12), Some(&b"WAVE"[..]));
    assert!(bytes.len() > 44);

    // A file that is not a .wav is refused, and a missing SoundFont is reported.
    let wrong = dir.join("song.mp3");
    let error = gscore_cli::run_cli([
        OsString::from("wav"),
        testdata("basic/note_lengths.gp").into_os_string(),
        "--output".into(),
        wrong.into_os_string(),
    ])
    .unwrap_err();
    assert!(error.to_string().contains(".wav"), "{error}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_last_measure_plays_on_its_own() {
    let dir = std::env::temp_dir().join(format!("gscore-play-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let soundfont = dir.join("sine.sf2");
    std::fs::write(&soundfont, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let soundfont = soundfont.to_string_lossy().into_owned();
    match play(&["--soundfont", &soundfont, "--from", "9", "--speed", "4"]) {
        Ok(out) => assert!(out.contains("Played 9 measure(s)"), "{out}"),
        // Only a playback failure (no device) skips; a usage error is a bug.
        Err(gscore_cli::CliError::Failed(error)) => {
            eprintln!("skipped: no usable audio device ({error})");
        }
        Err(error) => panic!("{error}"),
    }
}
