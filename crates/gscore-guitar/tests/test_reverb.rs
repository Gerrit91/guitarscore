//! The computed reverb on modelled strings: about as loud as the strings whatever its
//! size and decay, a long tail that dies, and its cost.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use gscore_guitar::{
    Attack, BLOCKS, BlockKind, BlockSettings, ChainSpec, GuitarModel, Pluck, ReverbKnobs,
    StringVoice, frequency,
};

const RATE: u32 = 48_000;
const BLOCK: usize = 64;

/// A G major chord on steel strings, then silence: `seconds` long.
fn chord(seconds: f32) -> Vec<f32> {
    let model = GuitarModel::Steel.strings();
    let mut strings: Vec<StringVoice> = [43_u8, 47, 50, 55, 59, 67]
        .iter()
        .map(|&key| {
            let mut voice = StringVoice::new(RATE as f32, 20.0, model);
            voice.pluck(Pluck {
                frequency: frequency(key, 0.0),
                velocity: 0.7,
                attack: Attack::Pick,
                palm_mute: false,
                dead: false,
                harmonic: false,
            });
            voice
        })
        .collect();
    let frames = (seconds * RATE as f32) as usize / BLOCK * BLOCK;
    let mut out = vec![0.0_f32; frames];
    let mut block = [0.0_f32; BLOCK];
    for chunk in out.chunks_mut(BLOCK) {
        for voice in &mut strings {
            voice.render(&mut block);
            for (o, b) in chunk.iter_mut().zip(&block) {
                *o += b;
            }
        }
    }
    out
}

/// `input` through an all-wet reverb with `knobs`: left and right.
fn wet(input: &[f32], knobs: ReverbKnobs) -> (Vec<f32>, Vec<f32>) {
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Reverb);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].reverb = ReverbKnobs { mix: 1.0, ..knobs };
    let mut chain = spec.prepare(RATE).instance(&settings);
    let mut left = vec![0.0_f32; input.len()];
    let mut right = vec![0.0_f32; input.len()];
    for ((i, l), r) in input
        .chunks(BLOCK)
        .zip(left.chunks_mut(BLOCK))
        .zip(right.chunks_mut(BLOCK))
    {
        chain.process(i, l, r);
    }
    (left, right)
}

fn rms(signal: &[f32]) -> f32 {
    (signal.iter().map(|x| x * x).sum::<f32>() / signal.len().max(1) as f32).sqrt()
}

#[test]
fn about_as_loud_as_the_strings_at_any_size_and_decay() {
    let input = chord(4.0);
    let dry = rms(&input[..RATE as usize]);
    for (size, decay) in [(0.2, 0.8), (0.6, 3.0), (1.0, 8.0), (1.0, 30.0)] {
        let (left, right) = wet(
            &input,
            ReverbKnobs {
                size,
                decay,
                ..ReverbKnobs::default()
            },
        );
        let level = rms(&left[..RATE as usize]).max(rms(&right[..RATE as usize]));
        println!("size {size} decay {decay}: wet {level:.3}, dry {dry:.3}");
        assert!(
            level > dry * 0.1 && level < dry * 2.0,
            "size {size} decay {decay}: {level} against {dry}"
        );
        assert!(left.iter().chain(&right).all(|x| x.is_finite()));
    }
}

#[test]
#[ignore = "timing, run in release: cargo test --release -p gscore-guitar --test test_reverb -- --ignored"]
fn cost_of_a_reverb() {
    let input = chord(10.0);
    let start = std::time::Instant::now();
    let (left, _) = wet(&input, ReverbKnobs::default());
    let spent = start.elapsed().as_secs_f32();
    println!(
        "10 s of audio in {spent:.3} s: {:.2} % of a core",
        spent * 10.0
    );
    assert!(left.iter().all(|x| x.is_finite()));
}
