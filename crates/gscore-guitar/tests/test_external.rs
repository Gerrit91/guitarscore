//! External blocks (plugins, M24) in a chain: run on whole buffers between the built-in
//! blocks, stereo after them, bypassed, never fed what is not a number, left out when
//! they cannot be made.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use gscore_guitar::{
    BLOCKS, BlockKind, BlockSettings, ChainSpec, External, ExternalBlock, ExternalSource,
};

const RATE: u32 = 48_000;

/// Scales the left side by `gain` and turns the right side over; counts calls and
/// notes any sample that is not a number.
#[derive(Default)]
struct Probe {
    gain: f32,
    fail: bool,
    calls: AtomicUsize,
    resets: AtomicUsize,
    saw_nan: AtomicBool,
}

struct ProbeBlock(Arc<Probe>);

impl ExternalBlock for ProbeBlock {
    fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.0.calls.fetch_add(1, Ordering::Relaxed);
        if left.iter().chain(right.iter()).any(|s| !s.is_finite()) {
            self.0.saw_nan.store(true, Ordering::Relaxed);
        }
        for s in left.iter_mut() {
            *s *= self.0.gain;
        }
        for s in right.iter_mut() {
            *s = -*s;
        }
    }

    fn reset(&mut self) {
        self.0.resets.fetch_add(1, Ordering::Relaxed);
    }
}

struct Source(Arc<Probe>, &'static str);

impl ExternalSource for Source {
    fn key(&self) -> &str {
        self.1
    }

    fn instance(&self, _rate: u32) -> Option<Box<dyn ExternalBlock>> {
        (!self.0.fail).then(|| Box::new(ProbeBlock(Arc::clone(&self.0))) as Box<dyn ExternalBlock>)
    }
}

fn probe(gain: f32) -> (Arc<Probe>, BlockKind) {
    let probe = Arc::new(Probe {
        gain,
        ..Probe::default()
    });
    let kind = BlockKind::External(External(Arc::new(Source(Arc::clone(&probe), "probe"))));
    (probe, kind)
}

fn spec(blocks: [Option<BlockKind>; BLOCKS]) -> ChainSpec {
    ChainSpec { blocks }
}

#[test]
fn an_external_block_runs_on_the_whole_buffer_and_makes_the_sound_stereo() {
    let (probe, kind) = probe(0.5);
    let settings = [BlockSettings::default(); BLOCKS];
    let mut chain = spec([Some(kind), None, None, None, None])
        .prepare(RATE)
        .instance(&settings, 0.0);
    let input = vec![0.4_f32; 256];
    let (mut left, mut right) = (vec![0.0; 256], vec![0.0; 256]);
    chain.process(&input, &mut left, &mut right);
    assert_eq!(probe.calls.load(Ordering::Relaxed), 1);
    assert!(left.iter().all(|&s| (s - 0.2).abs() < 1e-6));
    // The mono input reached both sides, and the block's stereo comes out.
    assert!(right.iter().all(|&s| (s + 0.4).abs() < 1e-6));
}

#[test]
fn built_in_blocks_around_it_play_as_without_it() {
    let run = |middle: Option<BlockKind>| {
        let settings = [BlockSettings::default(); BLOCKS];
        let mut chain = spec([
            Some(BlockKind::Drive),
            middle,
            Some(BlockKind::Eq),
            None,
            None,
        ])
        .prepare(RATE)
        .instance(&settings, -6.0);
        let input: Vec<f32> = (0..512).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let (mut left, mut right) = (vec![0.0; 512], vec![0.0; 512]);
        chain.process(&input, &mut left, &mut right);
        (left, right)
    };
    let (probe, kind) = probe(1.0);
    let (left, right) = run(Some(kind));
    let (plain, _) = run(None);
    assert_eq!(probe.calls.load(Ordering::Relaxed), 1);
    // The drive before it, the EQ after it (each side alike) and the output level: as
    // without the block, which only turned the right side over.
    for i in 0..512 {
        assert!((left[i] - plain[i]).abs() < 1e-6, "{i}");
        assert!((right[i] + plain[i]).abs() < 1e-6, "{i}");
    }
}

#[test]
fn a_bypassed_external_block_is_not_run() {
    let (probe, kind) = probe(0.0);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].bypass = true;
    let mut chain = spec([Some(kind), None, None, None, None])
        .prepare(RATE)
        .instance(&settings, 0.0);
    let (mut left, mut right) = (vec![0.3; 64], vec![0.3; 64]);
    chain.process_stereo(&mut left, &mut right);
    assert_eq!(probe.calls.load(Ordering::Relaxed), 0);
    assert!(left.iter().chain(&right).all(|&s| (s - 0.3).abs() < 1e-6));
}

#[test]
fn an_external_block_is_never_fed_what_is_not_a_number() {
    let (probe, kind) = probe(1.0);
    let settings = [BlockSettings::default(); BLOCKS];
    let mut chain = spec([Some(kind), None, None, None, None])
        .prepare(RATE)
        .instance(&settings, 0.0);
    let mut left = vec![f32::NAN, 0.1, f32::INFINITY, 0.1];
    let mut right = vec![0.1; 4];
    chain.process_stereo(&mut left, &mut right);
    assert!(!probe.saw_nan.load(Ordering::Relaxed));
    assert!(left.iter().chain(&right).all(|s| s.is_finite()));
}

#[test]
fn a_block_that_cannot_be_made_leaves_its_place_empty() {
    let probe = Arc::new(Probe {
        fail: true,
        ..Probe::default()
    });
    let kind = BlockKind::External(External(Arc::new(Source(Arc::clone(&probe), "x"))));
    let settings = [BlockSettings::default(); BLOCKS];
    let mut chain = spec([Some(kind), None, None, None, None])
        .prepare(RATE)
        .instance(&settings, 0.0);
    let (mut left, mut right) = (vec![0.25; 32], vec![0.25; 32]);
    chain.process_stereo(&mut left, &mut right);
    assert!(left.iter().chain(&right).all(|&s| (s - 0.25).abs() < 1e-6));
}

#[test]
fn reset_reaches_the_block_and_equal_keys_are_equal_blocks() {
    let (probe, kind) = probe(1.0);
    let settings = [BlockSettings::default(); BLOCKS];
    let mut chain = spec([Some(kind.clone()), None, None, None, None])
        .prepare(RATE)
        .instance(&settings, 0.0);
    chain.reset();
    assert_eq!(probe.resets.load(Ordering::Relaxed), 1);
    let (_, same_key) = self::probe(2.0);
    assert_eq!(kind, same_key);
    let other = BlockKind::External(External(Arc::new(Source(probe, "other"))));
    assert_ne!(kind, other);
}
