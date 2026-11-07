//! `StringVoice::render` and everything the audio thread calls on a voice (pluck, bend,
//! release, level) never allocate: the delay line is made once, when the voice is. Nor
//! does a prepared effect chain while it runs or takes new knob values.
//!
//! One test only: the counter sees every thread, so nothing may run beside it.
//!
//! The global allocator counts allocations for this test binary only.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::alloc::System;

use std::sync::Arc;

use gscore_guitar::{
    Attack, BLOCKS, BlockKind, BlockSettings, ChainSpec, GuitarModel, ImpulseResponse, Pluck,
    Release, StringVoice, frequency,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn playing_strings_and_running_a_chain_never_allocate() {
    let mut voices: Vec<StringVoice> = [
        GuitarModel::Nylon,
        GuitarModel::Steel,
        GuitarModel::Electric,
        GuitarModel::Bass,
    ]
    .iter()
    .map(|model| StringVoice::new(48_000.0, 20.0, model.strings()))
    .collect();
    let mut block = [0.0_f32; 64];
    let decaying = |length: usize, seed: f32| -> Vec<f32> {
        (0..length)
            .map(|i| (i as f32 * seed).sin() * (-(i as f32) / 2_000.0).exp())
            .collect()
    };
    let body = Arc::new(ImpulseResponse::from_samples(&decaying(4_000, 0.4), 48_000).unwrap());
    let room = Arc::new(
        ImpulseResponse::from_channels(vec![decaying(30_000, 0.17), decaying(30_000, 0.3)], 48_000)
            .unwrap(),
    );
    let spec = ChainSpec {
        blocks: [
            Some(BlockKind::Impulse(body)),
            Some(BlockKind::Amp),
            Some(BlockKind::Reverb),
            Some(BlockKind::room(room, 2.0)),
            Some(BlockKind::Drive),
        ],
    };
    let mut knobs = [BlockSettings::default(); BLOCKS];
    let mut chain = spec.prepare(48_000).instance(&knobs);
    let (mut left, mut right) = ([0.0_f32; 64], [0.0_f32; 64]);

    // An EQ too: it retunes its bands while it runs.
    let mut eq_spec = ChainSpec::default();
    eq_spec.blocks[0] = Some(BlockKind::Eq);
    let mut eq_knobs = [BlockSettings::default(); BLOCKS];
    let mut eq = eq_spec.prepare(48_000).instance(&eq_knobs);

    let mut compressor_spec = ChainSpec::default();
    compressor_spec.blocks[0] = Some(BlockKind::Compressor);
    let mut compressor_knobs = [BlockSettings::default(); BLOCKS];
    let mut compressor = compressor_spec.prepare(48_000).instance(&compressor_knobs);
    let loud: Vec<f32> = (0..64).map(|i| (i as f32 * 0.3).sin()).collect();

    // A chorus: its delays are made with it, never while it runs.
    let mut chorus_spec = ChainSpec::default();
    chorus_spec.blocks[0] = Some(BlockKind::Chorus);
    let chorus_knobs = [BlockSettings::default(); BLOCKS];
    let mut chorus = chorus_spec.prepare(48_000).instance(&chorus_knobs);

    let region = Region::new(GLOBAL);
    for _ in 0..200 {
        let (mut left, mut right) = ([0.0_f32; 64], [0.0_f32; 64]);
        left.copy_from_slice(&loud);
        right.copy_from_slice(&loud);
        chorus.process_stereo(&mut left, &mut right);
    }
    for round in 0..500_usize {
        if round % 50 == 0 {
            eq_knobs[0].eq.band_gain = [0.0, (round % 100) as f32 / 5.0, 0.0, 0.0];
            eq_knobs[0].eq.band_freq[2] = 500.0 + (round % 4_000) as f32;
            eq.set_settings(&eq_knobs);
        }
        let (mut left, mut right) = ([0.0_f32; 64], [0.0_f32; 64]);
        left.copy_from_slice(&loud);
        right.copy_from_slice(&loud);
        eq.process_stereo(&mut left, &mut right);
        if round % 50 == 0 {
            compressor_knobs[0].compressor.input_db = (round % 200) as f32 / 10.0;
            compressor_knobs[0].compressor.ratio =
                gscore_guitar::CompRatio::ALL_RATIOS[round / 50 % 5];
            compressor.set_settings(&compressor_knobs);
        }
        let (mut left, mut right) = ([0.0_f32; 64], [0.0_f32; 64]);
        left.copy_from_slice(&loud);
        right.copy_from_slice(&loud);
        compressor.process_stereo(&mut left, &mut right);
        let _ = compressor.take_gain_reduction();
    }
    for round in 0..2_000_usize {
        if round % 100 == 0 {
            knobs[3].room.mix = (round % 300) as f32 / 300.0;
            knobs[2].bypass = round % 200 == 0;
            knobs[1].amp.gain = (round % 700) as f32 / 700.0;
            chain.set_settings(&knobs);
        }
        let input = block;
        chain.process(&input, &mut left, &mut right);
    }
    for (index, voice) in voices.iter_mut().enumerate() {
        for step in 0..400_usize {
            match step % 50 {
                0 => voice.pluck(Pluck {
                    frequency: frequency(40 + index as u8 * 5, 0.0),
                    velocity: 0.8,
                    attack: Attack::Pick,
                    palm_mute: false,
                    dead: false,
                    harmonic: false,
                }),
                10 => voice.pluck(Pluck {
                    frequency: frequency(42 + index as u8 * 5, 0.0),
                    velocity: 0.6,
                    attack: Attack::Legato,
                    palm_mute: false,
                    dead: false,
                    harmonic: false,
                }),
                20 => voice.set_bend(150.0),
                30 => voice.set_level(0.5),
                40 => voice.release(Release::Damped),
                _ => {}
            }
            block.fill(0.0);
            voice.render(&mut block);
        }
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0, "{stats:?}");
    assert_eq!(stats.reallocations, 0, "{stats:?}");
}
