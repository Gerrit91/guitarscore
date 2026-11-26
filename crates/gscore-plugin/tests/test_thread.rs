//! The plugin thread: effects started there, run on another thread (as the audio thread
//! runs them) and stopped again; parameters described with settings loaded; scanning.

#![allow(clippy::unwrap_used)] // Helpers of the tests: a failing unwrap fails the test.
#![allow(unused_crate_dependencies)]

mod common;

use std::time::Duration;

use common::{GAIN, ID, library_path};
use gscore_plugin::{AudioConfig, PluginError, PluginThread};

const WAIT: Duration = Duration::from_secs(10);
const CONFIG: AudioConfig = AudioConfig {
    sample_rate: 48_000,
    max_frames: 128,
};

#[test]
fn an_effect_started_on_the_thread_runs_elsewhere_and_stops() {
    let thread = PluginThread::spawn().unwrap();
    let handle = thread.handle();
    let started = handle
        .start(&library_path(), ID, &[], CONFIG)
        .recv_timeout(WAIT)
        .unwrap()
        .unwrap();
    assert_eq!(started.latency, 64);
    let mut processor = started.processor;
    // The audio thread's side.
    let processor = std::thread::spawn(move || {
        processor.set_param(GAIN, 0.5);
        let (mut left, mut right) = (vec![1.0_f32; 300], vec![1.0_f32; 300]);
        processor.process(&mut left, &mut right).unwrap();
        assert!(left.iter().chain(&right).all(|&s| s == 0.5));
        processor
    })
    .join()
    .unwrap();
    handle.stop(started.instance, processor);
    drop(thread);
}

#[test]
fn a_description_shows_the_parameters_with_the_settings_loaded() {
    let thread = PluginThread::spawn().unwrap();
    let handle = thread.handle();
    let plain = handle
        .describe(&library_path(), ID, &[])
        .recv_timeout(WAIT)
        .unwrap()
        .unwrap();
    assert_eq!(plain.info.id, ID);
    assert_eq!(plain.params.len(), 1);
    assert_eq!(plain.values, vec![1.0]);
    assert_eq!(plain.latency, 64);
    let settings = 1.5_f64.to_le_bytes();
    let loaded = handle
        .describe(&library_path(), ID, &settings)
        .recv_timeout(WAIT)
        .unwrap()
        .unwrap();
    assert_eq!(loaded.values, vec![1.5]);
}

#[test]
fn scanning_finds_the_test_plugin() {
    let thread = PluginThread::spawn().unwrap();
    let dir = library_path().parent().unwrap().to_owned();
    let found = thread.handle().scan(vec![dir]).recv_timeout(WAIT).unwrap();
    // `cargo test` names the library `.so`, `.dll` or `.dylib`, not `.clap`: scan a
    // copy with the plugin file name.
    assert!(found.is_empty() || found.iter().all(|(_, r)| r.is_ok()));
    let folder = std::env::temp_dir().join(format!("gscore-scan-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::copy(library_path(), folder.join("Test.clap")).unwrap();
    let found = thread
        .handle()
        .scan(vec![folder.clone()])
        .recv_timeout(WAIT)
        .unwrap();
    assert_eq!(found.len(), 1);
    let plugins = found[0].1.as_ref().unwrap();
    assert_eq!(plugins[0].id, ID);
    drop(thread);
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn bad_requests_are_errors_and_the_thread_goes_on() {
    let thread = PluginThread::spawn().unwrap();
    let handle = thread.handle();
    let missing = handle
        .start(&library_path(), "org.example.none", &[], CONFIG)
        .recv_timeout(WAIT)
        .unwrap();
    assert!(matches!(missing, Err(PluginError::NotFound(_))));
    let broken = handle
        .start(&library_path(), ID, &[1, 2, 3], CONFIG)
        .recv_timeout(WAIT)
        .unwrap();
    assert!(matches!(broken, Err(PluginError::State { .. })));
    // Still answering.
    assert!(
        handle
            .describe(&library_path(), ID, &[])
            .recv_timeout(WAIT)
            .unwrap()
            .is_ok()
    );
    // A stopped thread answers with an error, never a hang.
    drop(thread);
    let late = handle
        .start(&library_path(), ID, &[], CONFIG)
        .recv_timeout(WAIT);
    assert!(matches!(late, Ok(Err(PluginError::Process(_)))));
}
