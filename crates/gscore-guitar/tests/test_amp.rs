//! The amp block on modelled electric strings: a power chord through each voice is loud
//! but bounded, clean stays close to the strings' own level, the knobs are taken while
//! it plays, and the chain falls silent once the strings do.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use gscore_guitar::{
    AmpKnobs, AmpVoice, Attack, BLOCKS, BlockKind, BlockSettings, ChainSpec, GuitarModel, Pluck,
    StringVoice, frequency,
};

const RATE: u32 = 48_000;
const BLOCK: usize = 64;

/// An E5 power chord (E2, B2, E3) picked hard on electric strings, `seconds` long.
fn power_chord(seconds: f32) -> Vec<f32> {
    let model = GuitarModel::Electric.strings();
    let mut strings: Vec<StringVoice> = [40_u8, 47, 52]
        .iter()
        .map(|&key| {
            let mut voice = StringVoice::new(RATE as f32, 20.0, model);
            voice.pluck(Pluck {
                frequency: frequency(key, 0.0),
                velocity: 0.9,
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

fn amp_spec() -> ChainSpec {
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Amp);
    spec
}

fn settings(amp: AmpKnobs) -> [BlockSettings; BLOCKS] {
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].amp = amp;
    settings
}

/// Left channel of `input` through an amp chain with `amp`.
fn through(input: &[f32], amp: AmpKnobs) -> Vec<f32> {
    let mut chain = amp_spec().prepare(RATE).instance(&settings(amp));
    let mut out = vec![0.0_f32; input.len()];
    let mut right = [0.0_f32; BLOCK];
    for (i, o) in input.chunks(BLOCK).zip(out.chunks_mut(BLOCK)) {
        chain.process(i, o, &mut right[..i.len()]);
    }
    out
}

fn rms(signal: &[f32]) -> f32 {
    (signal.iter().map(|x| x * x).sum::<f32>() / signal.len().max(1) as f32).sqrt()
}

fn peak(signal: &[f32]) -> f32 {
    signal.iter().fold(0.0_f32, |p, x| p.max(x.abs()))
}

#[test]
fn every_voice_plays_a_chord_loud_but_bounded() {
    let chord = power_chord(1.0);
    let dry = rms(&chord);
    for voice in AmpVoice::ALL {
        for gain in [0.0, 0.5, 1.0] {
            let out = through(
                &chord,
                AmpKnobs {
                    voice,
                    gain,
                    ..AmpKnobs::default()
                },
            );
            assert!(out.iter().all(|x| x.is_finite()));
            let (level, top) = (rms(&out), peak(&out));
            assert!(top < 1.2 * peak(&chord), "{voice:?} {gain}: peak {top}");
            assert!(
                level > dry * 0.25,
                "{voice:?} {gain}: {level} against {dry}"
            );
        }
    }
}

#[test]
fn distortion_squashes_the_attack() {
    // A driven amp compresses: its peaks stand far less above its average than the
    // strings' own (the pick's attack).
    let chord = power_chord(1.0);
    let crest = |signal: &[f32]| peak(signal) / rms(signal);
    let lead = through(
        &chord,
        AmpKnobs {
            voice: AmpVoice::Lead,
            gain: 0.7,
            speaker: 0.0,
            ..AmpKnobs::default()
        },
    );
    assert!(
        crest(&lead) < 0.7 * crest(&chord),
        "{} against {}",
        crest(&lead),
        crest(&chord)
    );
}

#[test]
fn the_tail_ends_and_knobs_are_taken_while_playing() {
    let mut chain = amp_spec()
        .prepare(RATE)
        .instance(&settings(AmpKnobs::default()));
    let chord = power_chord(0.5);
    let (mut left, mut right) = ([0.0_f32; BLOCK], [0.0_f32; BLOCK]);
    for (index, block) in chord.chunks(BLOCK).enumerate() {
        if index == 100 {
            chain.set_settings(&settings(AmpKnobs {
                voice: AmpVoice::Lead,
                treble: 0.9,
                ..AmpKnobs::default()
            }));
        }
        chain.process(block, &mut left, &mut right);
        assert!(left.iter().chain(&right).all(|x| x.is_finite()));
        assert_eq!(left, right, "mono in, both sides alike");
    }
    let silence = [0.0_f32; BLOCK];
    for _ in 0..(RATE as usize / BLOCK) {
        chain.process(&silence, &mut left, &mut right);
    }
    assert!(!chain.ringing(), "an amp has no tail of its own");
}

#[test]
#[ignore = "timing, run in release: cargo test --release -p gscore-guitar --test test_amp -- --ignored"]
fn cost_of_an_amp() {
    let input = power_chord(10.0);
    let start = std::time::Instant::now();
    let out = through(
        &input,
        AmpKnobs {
            voice: AmpVoice::Lead,
            ..AmpKnobs::default()
        },
    );
    let spent = start.elapsed().as_secs_f32();
    println!(
        "10 s of audio in {spent:.3} s: {:.2} % of a core",
        spent * 10.0
    );
    assert!(out.iter().all(|x| x.is_finite()));
}
