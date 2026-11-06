//! The effect chain: impulse responses read from WAV files (and damaged ones refused), a
//! cabinet or body response, a room response in stereo, the computed reverb, bypass, and
//! tails that ring on and then stop. (No allocation: `test_no_alloc`.)

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::sync::Arc;

use gscore_guitar::{
    BLOCKS, BlockKind, BlockSettings, ChainSpec, ImpulseKnobs, ImpulseResponse, RoomKnobs,
};

const RATE: u32 = 48_000;
/// The convolver's lag.
const LAG: usize = 128;

/// A WAV file of `channels` interleaved, `bits` 16 or 24 PCM, or 32 float when `float`.
fn wav(channels: &[Vec<f32>], rate: u32, bits: u16, float: bool) -> Vec<u8> {
    let frames = channels[0].len();
    let count = channels.len() as u16;
    let width = usize::from(bits / 8);
    let mut data = Vec::new();
    for i in 0..frames {
        for channel in channels {
            let x = channel[i];
            match (bits, float) {
                (16, false) => data.extend(((x * 32767.0) as i16).to_le_bytes()),
                (24, false) => data.extend(&((x * 8_388_607.0) as i32).to_le_bytes()[..3]),
                (32, true) => data.extend(x.to_le_bytes()),
                _ => panic!("format"),
            }
        }
    }
    let mut out = Vec::new();
    out.extend(b"RIFF");
    out.extend((36 + data.len() as u32).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16_u32.to_le_bytes());
    out.extend((if float { 3_u16 } else { 1 }).to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(rate.to_le_bytes());
    out.extend((rate * u32::from(count) * width as u32).to_le_bytes());
    out.extend((count * bits / 8).to_le_bytes());
    out.extend(bits.to_le_bytes());
    // A chunk the reader must skip.
    out.extend(b"LIST");
    out.extend(3_u32.to_le_bytes());
    out.extend([1, 2, 3, 0]);
    out.extend(b"data");
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
    out
}

fn decaying(length: usize, seed: f32) -> Vec<f32> {
    (0..length)
        .map(|i| (i as f32 * seed).sin() * (-(i as f32) / (length as f32 / 5.0)).exp())
        .collect()
}

#[test]
fn wav_files_of_every_format_are_read() {
    let left = decaying(2_000, 0.3);
    let right = decaying(2_000, 0.7);
    for (bits, float) in [(16, false), (24, false), (32, true)] {
        let mono =
            ImpulseResponse::from_wav(&wav(std::slice::from_ref(&left), 44_100, bits, float))
                .unwrap();
        assert_eq!(mono.channel_count(), 1);
        assert_eq!(mono.sample_rate(), 44_100);
        let stereo =
            ImpulseResponse::from_wav(&wav(&[left.clone(), right.clone()], RATE, bits, float))
                .unwrap();
        assert_eq!(stereo.channel_count(), 2, "{bits} bits");
        assert!((stereo.seconds() - 2_000.0 / RATE as f32).abs() < 1e-4);
        // Unit energy, also after resampling.
        let energy = |samples: &[f32]| samples.iter().map(|x| x * x).sum::<f32>();
        assert!((energy(&mono.mono_at_rate(44_100)) - 1.0).abs() < 1e-3);
        assert!((energy(&mono.mono_at_rate(RATE)) - 1.0).abs() < 0.05);
    }
}

#[test]
fn damaged_or_unusable_files_are_errors_not_panics() {
    let good = wav(&[decaying(500, 0.3)], RATE, 16, false);
    assert!(ImpulseResponse::from_wav(&good).is_ok());
    for cut in [0, 4, 12, 20, 30, 44, 50] {
        let _ = ImpulseResponse::from_wav(&good[..cut.min(good.len())]);
    }
    assert!(ImpulseResponse::from_wav(b"not a wav file at all").is_err());
    assert!(ImpulseResponse::from_wav(&wav(&[vec![0.0; 100]], RATE, 16, false)).is_err());
    // Random bytes behind a valid header.
    let mut state = 1_u32;
    for _ in 0..200 {
        let mut bytes = good.clone();
        for byte in bytes.iter_mut().skip(12) {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            if state.is_multiple_of(7) {
                *byte = (state >> 8) as u8;
            }
        }
        let _ = ImpulseResponse::from_wav(&bytes);
    }
}

fn settings() -> [BlockSettings; BLOCKS] {
    [BlockSettings::default(); BLOCKS]
}

#[test]
fn a_unit_response_passes_the_signal_one_partition_late() {
    let unit = Arc::new(ImpulseResponse::from_samples(&[1.0], RATE).unwrap());
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Impulse(unit));
    let mut knobs = settings();
    knobs[0].impulse = ImpulseKnobs {
        mix: 1.0,
        gain_db: 0.0,
        low_cut: 20.0,
        high_cut: 20_000.0,
    };
    let mut chain = spec.prepare(RATE).instance(&knobs);
    // About 760 Hz: well inside the cut filters, which barely shift its phase.
    let input: Vec<f32> = (0..2_000).map(|i| (i as f32 * 0.1).sin()).collect();
    let (mut left, mut right) = (vec![0.0; 2_000], vec![0.0; 2_000]);
    chain.process(&input, &mut left, &mut right);
    // The cut filters at their extremes still shift its phase a little (the 20 Hz low
    // cut by about 0.04 rad at 760 Hz).
    for n in 1_000..2_000 {
        assert!((left[n] - input[n - LAG]).abs() < 0.06, "{n}");
        assert_eq!(left[n], right[n], "mono stays mono");
    }
    // Bypassed: the signal passes as it is.
    knobs[0].bypass = true;
    chain.set_settings(&knobs);
    chain.process(&input, &mut left, &mut right);
    assert_eq!(left, input);
}

#[test]
fn a_room_is_stereo_and_its_tail_rings_on_then_stops() {
    let room = Arc::new(
        ImpulseResponse::from_channels(vec![decaying(24_000, 0.21), decaying(24_000, 0.53)], RATE)
            .unwrap(),
    );
    let mut spec = ChainSpec::default();
    spec.blocks[2] = Some(BlockKind::Room(room));
    let mut knobs = settings();
    knobs[2].room = RoomKnobs {
        mix: 0.5,
        ..RoomKnobs::default()
    };
    let mut chain = spec.prepare(RATE).instance(&knobs);
    let mut input = vec![0.0_f32; 4_800];
    input[0] = 1.0;
    let (mut left, mut right) = (vec![0.0; 4_800], vec![0.0; 4_800]);
    chain.process(&input, &mut left, &mut right);
    assert!(
        left.iter().zip(&right).any(|(l, r)| (l - r).abs() > 1e-4),
        "stereo"
    );
    assert!(chain.ringing(), "the room still sounds");
    let silence = vec![0.0_f32; 4_800];
    for _ in 0..200 {
        chain.process(&silence, &mut left, &mut right);
    }
    assert!(!chain.ringing(), "and stops in the end");
    assert!(left.iter().all(|x| x.abs() < 1e-5));
}

#[test]
fn the_computed_reverb_and_every_block_together_stay_finite() {
    let mut chain = gscore_guitar_chain_all_blocks();
    let input: Vec<f32> = (0..64)
        .map(|i| ((i * 31) % 17) as f32 / 8.5 - 1.0)
        .collect();
    let (mut left, mut right) = ([0.0_f32; 64], [0.0_f32; 64]);
    for _ in 0..2_000 {
        chain.process(&input, &mut left, &mut right);
        assert!(
            left.iter()
                .chain(&right)
                .all(|x| x.is_finite() && x.abs() < 50.0)
        );
    }
    assert!(chain.ringing());
}

/// A chain with a body, the computed reverb and a room.
fn gscore_guitar_chain_all_blocks() -> gscore_guitar::Chain {
    let body = Arc::new(ImpulseResponse::from_samples(&decaying(4_000, 0.4), RATE).unwrap());
    let room = Arc::new(ImpulseResponse::from_samples(&decaying(30_000, 0.17), RATE).unwrap());
    let spec = ChainSpec {
        blocks: [
            Some(BlockKind::Impulse(body)),
            None,
            Some(BlockKind::Reverb),
            Some(BlockKind::Room(room)),
            None,
        ],
    };
    let mut knobs = settings();
    knobs[2].reverb.mix = 0.4;
    spec.prepare(RATE).instance(&knobs)
}
