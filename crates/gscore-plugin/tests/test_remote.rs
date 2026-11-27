//! Plugins in host processes (M24 step 3): the test plugin run by `guitarscore-plugin-host`
//! (built with these tests), its sound back at the latency it states, its parameters,
//! describing and scanning in hosts of their own; and a plugin that crashes or hangs only
//! bypasses itself.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)] // A failing unwrap fails the test.
#![allow(unused_crate_dependencies)]

mod common;

use std::time::{Duration, Instant};

use common::{GAIN, ID, library_path};
use gscore_plugin::remote::{self, PROCESS_LATENCY, RemoteEffect, WATCHDOG};
use gscore_plugin::{AudioConfig, PluginError};

const CONFIG: AudioConfig = AudioConfig {
    sample_rate: 48_000,
    max_frames: 64,
};
const CRASH: u32 = 2;
const HANG: u32 = 3;

/// Plays `blocks` blocks of 0.5 through `effect` in real time (as an audio device asks
/// for them), returning everything that came back.
fn play(effect: &mut RemoteEffect, blocks: usize) -> Vec<f32> {
    let mut out = Vec::new();
    for _ in 0..blocks {
        let (mut left, mut right) = (vec![0.5_f32; 64], vec![0.5_f32; 64]);
        effect.process(&mut left, &mut right);
        out.extend(left);
        std::thread::sleep(Duration::from_micros(1300));
    }
    out
}

#[test]
fn the_host_program_is_found() {
    let found = remote::host_program().unwrap();
    assert!(found.is_file(), "{}", found.display());
}

#[test]
fn sound_comes_back_from_the_host_at_the_stated_latency() {
    let mut effect = RemoteEffect::start(&library_path(), ID, &[], CONFIG).unwrap();
    assert_eq!(effect.latency(), 64 + PROCESS_LATENCY);
    effect.set_param(GAIN, 0.5);
    let out = play(&mut effect, 60);
    let lead = PROCESS_LATENCY as usize;
    assert!(out[..lead].iter().all(|&s| s == 0.0), "silence first");
    // The test plugin reports latency but does not delay: its sound is back after the
    // pipeline, halved.
    let back = &out[lead..];
    let good = back.iter().filter(|&&s| (s - 0.25).abs() < 1e-6).count();
    assert!(
        good * 10 >= back.len() * 9,
        "most blocks are back in time: {good} of {}",
        back.len()
    );
    assert_eq!(effect.failure(), None);
}

#[test]
fn offline_every_block_is_waited_for() {
    // An export renders as fast as it can: nothing may be lost to the pace.
    let mut effect = RemoteEffect::start(&library_path(), ID, &[], CONFIG).unwrap();
    effect.set_offline(true);
    effect.set_param(GAIN, 0.5);
    let mut out = Vec::new();
    for _ in 0..200 {
        let (mut left, mut right) = (vec![0.5_f32; 64], vec![0.5_f32; 64]);
        effect.process(&mut left, &mut right);
        out.extend(left);
    }
    let lead = PROCESS_LATENCY as usize;
    assert!(out[lead..].iter().all(|&s| (s - 0.25).abs() < 1e-6));
}

#[test]
fn a_plugin_that_crashes_only_bypasses_itself() {
    let mut effect = RemoteEffect::start(&library_path(), ID, &[], CONFIG).unwrap();
    let _ = play(&mut effect, 20);
    effect.set_param(CRASH, 1.0);
    let started = Instant::now();
    while effect.failure().is_none() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the crash is noticed"
        );
        let _ = play(&mut effect, 1);
    }
    assert!(
        effect.failure().unwrap().contains("crashed"),
        "{:?}",
        effect.failure()
    );
    // The sound passes unchanged from now on.
    let (mut left, mut right) = (vec![0.3_f32; 64], vec![0.3_f32; 64]);
    effect.process(&mut left, &mut right);
    assert!(left.iter().chain(&right).all(|&s| s == 0.3));
}

#[test]
fn a_plugin_that_hangs_is_stopped_by_the_watchdog() {
    let mut effect = RemoteEffect::start(&library_path(), ID, &[], CONFIG).unwrap();
    let _ = play(&mut effect, 10);
    effect.set_param(HANG, 1.0);
    let started = Instant::now();
    while effect.failure().is_none() {
        assert!(
            started.elapsed() < WATCHDOG * 4,
            "the hang is noticed within the watchdog's time"
        );
        let _ = play(&mut effect, 1);
    }
    assert!(
        effect.failure().unwrap().contains("stopped answering"),
        "{:?}",
        effect.failure()
    );
    // Dropping it does not wait for the hung process.
    let dropped = Instant::now();
    drop(effect);
    assert!(dropped.elapsed() < Duration::from_secs(5));
}

#[test]
fn describe_and_scan_run_in_hosts_of_their_own() {
    let settings = 1.5_f64.to_le_bytes();
    let (info, params, values, latency) = remote::describe(&library_path(), ID, &settings).unwrap();
    assert_eq!(info.id, ID);
    assert_eq!(params.len(), 1);
    assert_eq!(values, vec![1.5]);
    assert_eq!(latency, 64);
    let plugins = remote::scan(&library_path()).unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0].id, ID);
}

#[test]
fn a_missing_plugin_or_library_is_an_error_not_a_crash() {
    assert!(matches!(
        RemoteEffect::start(&library_path(), "org.example.none", &[], CONFIG),
        Err(PluginError::Start { .. })
    ));
    let missing = library_path().with_file_name("none.clap");
    assert!(matches!(
        remote::scan(&missing),
        Err(PluginError::Open { .. })
    ));
    assert!(remote::describe(&missing, ID, &[]).is_err());
}
