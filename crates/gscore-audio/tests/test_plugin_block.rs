//! A plugin in a sound chain (M24): GuitarScore's test plugin (a gain effect, built by
//! `cargo test --workspace` as an example of `gscore-plugin`) placed in a chain, its gain
//! changed while it plays, and a plugin that cannot start leaving its place to the sound.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;
use std::sync::Arc;

use gscore_audio::{PluginControl, PluginSource};
use gscore_guitar::{BLOCKS, BlockKind, BlockSettings, ChainSpec, External};
use gscore_plugin::PluginThread;

const ID: &str = "org.guitarscore.test.gain";
const GAIN: u32 = 1;

/// The test plugin, where `cargo test --workspace` builds `gscore-plugin`'s examples.
fn test_plugin() -> Option<PathBuf> {
    let name = format!(
        "{}test_plugin{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let exe = std::env::current_exe().unwrap();
    let found = exe
        .ancestors()
        .skip(1)
        .take(6)
        .map(|dir| dir.join("examples").join(&name))
        .find(|path| path.exists());
    if found.is_none() {
        eprintln!("the test plugin is not built: run `cargo test --workspace`");
    }
    found
}

fn chain_with(source: PluginSource) -> gscore_guitar::Chain {
    let mut blocks: [Option<BlockKind>; BLOCKS] = Default::default();
    blocks[0] = Some(BlockKind::External(External(Arc::new(source))));
    ChainSpec { blocks }
        .prepare(48_000)
        .instance(&[BlockSettings::default(); BLOCKS], 0.0)
}

#[test]
fn the_plugin_plays_in_the_chain_and_follows_its_parameters() {
    let Some(library) = test_plugin() else {
        return;
    };
    let thread = PluginThread::spawn().unwrap();
    let control = Arc::new(PluginControl::new(vec![(GAIN, 0.5)]));
    let source = PluginSource::new(
        thread.handle(),
        &library,
        ID,
        Vec::new(),
        Arc::clone(&control),
    );
    let mut chain = chain_with(source);
    assert_eq!(control.problem(), None);
    assert_eq!(control.latency(), 64);

    let input = vec![0.8_f32; 64];
    let (mut left, mut right) = (vec![0.0; 64], vec![0.0; 64]);
    chain.process(&input, &mut left, &mut right);
    assert!(left.iter().chain(&right).all(|&s| (s - 0.4).abs() < 1e-6));

    control.set_values(&[(GAIN, 2.0)]);
    chain.process(&input, &mut left, &mut right);
    assert!(left.iter().chain(&right).all(|&s| (s - 1.6).abs() < 1e-6));
    chain.reset();
    drop(chain);
    drop(thread);
}

#[test]
fn the_saved_settings_are_loaded_and_two_chains_share_the_control() {
    let Some(library) = test_plugin() else {
        return;
    };
    let thread = PluginThread::spawn().unwrap();
    let control = Arc::new(PluginControl::default());
    let settings = 0.25_f64.to_le_bytes().to_vec();
    let make = || {
        chain_with(PluginSource::new(
            thread.handle(),
            &library,
            ID,
            settings.clone(),
            Arc::clone(&control),
        ))
    };
    let (mut a, mut b) = (make(), make());
    let input = vec![1.0_f32; 32];
    for chain in [&mut a, &mut b] {
        let (mut left, mut right) = (vec![0.0; 32], vec![0.0; 32]);
        chain.process(&input, &mut left, &mut right);
        assert!(left.iter().all(|&s| (s - 0.25).abs() < 1e-6));
    }
    control.set_values(&[(GAIN, 1.0)]);
    for chain in [&mut a, &mut b] {
        let (mut left, mut right) = (vec![0.0; 32], vec![0.0; 32]);
        chain.process(&input, &mut left, &mut right);
        assert!(left.iter().all(|&s| (s - 1.0).abs() < 1e-6));
    }
}

#[test]
fn a_plugin_that_cannot_start_leaves_the_sound_unchanged_and_says_why() {
    let thread = PluginThread::spawn().unwrap();
    let control = Arc::new(PluginControl::default());
    let missing = PathBuf::from("/nonexistent/plugin.clap");
    let mut chain = chain_with(PluginSource::new(
        thread.handle(),
        &missing,
        ID,
        Vec::new(),
        Arc::clone(&control),
    ));
    let (mut left, mut right) = (vec![0.3_f32; 16], vec![0.3_f32; 16]);
    chain.process_stereo(&mut left, &mut right);
    assert!(left.iter().chain(&right).all(|&s| (s - 0.3).abs() < 1e-6));
    assert!(control.problem().unwrap().contains("cannot open"));
}

#[test]
fn the_same_plugin_and_settings_are_the_same_block() {
    let thread = PluginThread::spawn().unwrap();
    let path = PathBuf::from("/x/a.clap");
    let source = |state: Vec<u8>| {
        BlockKind::External(External(Arc::new(PluginSource::new(
            thread.handle(),
            &path,
            ID,
            state,
            Arc::new(PluginControl::default()),
        ))))
    };
    assert_eq!(source(vec![1, 2]), source(vec![1, 2]));
    assert_ne!(source(vec![1, 2]), source(vec![1, 3]));
}
