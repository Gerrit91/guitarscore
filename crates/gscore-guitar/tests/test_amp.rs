//! The amp block on modelled electric strings: a power chord through each voice is loud
//! but bounded, clean stays close to the strings' own level, the knobs are taken while
//! it plays, and the chain falls silent once the strings do. A drive pedal in front keeps
//! the chug's low end, or thins it with its Tight knob.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::f32::consts::TAU;

use gscore_guitar::{
    AmpKnobs, AmpVoice, Attack, BLOCKS, BlockKind, BlockSettings, ChainSpec, DriveKnobs,
    GuitarModel, Pickup, Pluck, StringModel, StringVoice, frequency,
};

const RATE: u32 = 48_000;
const BLOCK: usize = 64;

/// An E5 power chord (E2, B2, E3) picked hard on electric strings, `seconds` long.
fn power_chord(seconds: f32) -> Vec<f32> {
    // Fixed strings without a pickup (the tone the amp was calibrated with), so the tests
    // measure the amp and the drive alone, whatever the electric preset becomes (a bridge
    // pickup, for one, thins the low E's fundamental by itself).
    let model = StringModel {
        pickup: Pickup::None,
        tone: 4_000.0,
        resonance: 1.3,
        ..GuitarModel::Electric.strings()
    };
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
    // A string adds itself to what is there (a scratch block shared by the strings and
    // never cleared summed everything played so far).
    for chunk in out.chunks_mut(BLOCK) {
        for voice in &mut strings {
            voice.render(chunk);
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
    let mut chain = amp_spec().prepare(RATE).instance(&settings(amp), 0.0);
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
            // Bounded: the stages clip, so even full gain stays near full scale.
            assert!(top < 2.0, "{voice:?} {gain}: peak {top}");
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
        .instance(&settings(AmpKnobs::default()), 0.0);
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

#[test]
fn a_drive_in_front_keeps_the_chug_unless_tight() {
    // The metal trick: Drive low, Level up, into the lead voice. Tight sets how much of
    // the low E's fundamental goes along: loose keeps the chug's body, tight thins it.
    let chord = power_chord(1.0);
    let lead = AmpKnobs {
        voice: AmpVoice::Lead,
        gain: 0.6,
        speaker: 0.0,
        ..AmpKnobs::default()
    };
    let run = |spec: &ChainSpec, settings: &[BlockSettings; BLOCKS]| {
        let mut chain = spec.prepare(RATE).instance(settings, 0.0);
        let mut out = vec![0.0_f32; chord.len()];
        let mut right = [0.0_f32; BLOCK];
        for (i, o) in chord.chunks(BLOCK).zip(out.chunks_mut(BLOCK)) {
            chain.process(i, o, &mut right[..i.len()]);
        }
        out
    };
    // The chug's body: the level below about 250 Hz (two one-pole lowpasses) against the
    // whole, after the attack.
    let low_share = |signal: &[f32]| {
        let a = (-TAU * 250.0 / RATE as f32).exp();
        let (mut first, mut second) = (0.0_f32, 0.0_f32);
        let low: Vec<f32> = signal
            .iter()
            .map(|&x| {
                first = (1.0 - a) * x + a * first;
                second = (1.0 - a) * first + a * second;
                second
            })
            .collect();
        rms(&low[12_000..]) / rms(&signal[12_000..])
    };
    let alone = low_share(&run(&amp_spec(), &settings(lead)));
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Drive);
    spec.blocks[1] = Some(BlockKind::Amp);
    let pushed_at = |drive: f32, tight: f32| {
        let mut both = [BlockSettings::default(); BLOCKS];
        both[0].drive = DriveKnobs {
            drive,
            tone: 0.6,
            level_db: 6.0,
            tight,
        };
        both[1].amp = lead;
        let out = run(&spec, &both);
        assert!(out.iter().all(|x| x.is_finite()));
        low_share(&out)
    };
    // With some Drive: the classic tight pedal (what the drive was until 2026-11-11)
    // thins the chug, the way the owner heard it; loose keeps it as full as the amp alone.
    for drive in [0.5, 1.0] {
        let (loose, middle, tight) = (
            pushed_at(drive, 0.0),
            pushed_at(drive, 0.5),
            pushed_at(drive, 1.0),
        );
        assert!(
            loose > middle && middle > tight,
            "drive {drive}: loose {loose}, middle {middle}, tight {tight}"
        );
        assert!(
            loose > 0.85 * alone,
            "drive {drive}: {loose} against {alone}"
        );
        assert!(
            tight < 0.7 * alone,
            "drive {drive}: {tight} against {alone}"
        );
    }
}

#[test]
fn silence_after_a_note_becomes_exact_zeros() {
    // A long reverb keeps a drive and an amp running on the silence after the strings
    // stop. Their filters must come to exactly zero rather than ever tinier numbers, which
    // the processor handles up to a hundred times slower (half a core per track, owner).
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Drive);
    spec.blocks[1] = Some(BlockKind::Amp);
    let mut chain = spec
        .prepare(RATE)
        .instance(&[BlockSettings::default(); BLOCKS], 0.0);
    let (mut left, mut right) = ([0.0_f32; BLOCK], [0.0_f32; BLOCK]);
    for block in power_chord(0.3).chunks(BLOCK) {
        chain.process(block, &mut left, &mut right);
    }
    let silence = [0.0_f32; BLOCK];
    for _ in 0..(RATE as usize / BLOCK) {
        chain.process(&silence, &mut left, &mut right);
    }
    assert!(
        left.iter().chain(&right).all(|x| *x == 0.0),
        "{:?}",
        &left[..4]
    );
}

#[test]
fn real_strings_crunch_at_half_gain_and_stay_clean_on_the_clean_voice() {
    // The calibration: the strings' own level (the electric preset, a hard chord) is what
    // the input pad is for. Distortion squashes the chord's peaks against its body.
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
    let mut chord = vec![0.0_f32; 48_000];
    for chunk in chord.chunks_mut(BLOCK) {
        for voice in &mut strings {
            voice.render(chunk);
        }
    }
    // Peak over the body, after the pick's attack.
    let crest = |signal: &[f32]| peak(&signal[4_800..]) / rms(&signal[4_800..]);
    let voiced = |voice: AmpVoice| {
        crest(&through(
            &chord,
            AmpKnobs {
                voice,
                gain: 0.5,
                speaker: 0.0,
                ..AmpKnobs::default()
            },
        ))
    };
    let (dry, clean, crunch, lead) = (
        crest(&chord),
        voiced(AmpVoice::Clean),
        voiced(AmpVoice::Crunch),
        voiced(AmpVoice::Lead),
    );
    assert!(
        clean > 0.75 * dry,
        "clean stays clean: {clean} against {dry}"
    );
    assert!(
        crunch < 0.4 * dry && crunch > 1.5,
        "a crunch crunches but keeps some peak: {crunch} against {dry}"
    );
    assert!(lead < crunch, "a lead saturates more: {lead}");
}
