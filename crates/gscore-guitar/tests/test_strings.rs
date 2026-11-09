//! The string model's behaviour: in tune across the guitar and bass range, ringing and
//! damped decays, bends and hammer-ons reaching their pitch, deterministic output, and no
//! NaN or runaway values whatever the parameters.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use gscore_guitar::{
    Attack, GuitarModel, Pickup, Pluck, Release, StringModel, StringVoice, frequency,
};

const RATE: f32 = 48_000.0;
const BLOCK: usize = 64;

fn pick(frequency: f32) -> Pluck {
    Pluck {
        frequency,
        velocity: 0.8,
        attack: Attack::Pick,
        palm_mute: false,
        dead: false,
        harmonic: false,
    }
}

/// Renders `seconds` in blocks, calling `each_block` before every block.
fn render(
    voice: &mut StringVoice,
    seconds: f32,
    mut each_block: impl FnMut(&mut StringVoice),
) -> Vec<f32> {
    let frames = (seconds * RATE) as usize;
    let mut out = vec![0.0_f32; frames];
    for block in out.chunks_mut(BLOCK) {
        each_block(voice);
        voice.render(block);
    }
    out
}

/// The strongest frequency near `expected` (within 60 cents), found by a windowed DFT
/// on a fine grid, as cents away from `expected`.
fn cents_off(samples: &[f32], expected: f32) -> f64 {
    let n = samples.len();
    let window: Vec<f64> = (0..n)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos())
        .collect();
    let power = |cents: f64| {
        let f = f64::from(expected) * (cents / 1200.0).exp2();
        let w = std::f64::consts::TAU * f / f64::from(RATE);
        let (mut re, mut im) = (0.0, 0.0);
        for (i, (&x, &h)) in samples.iter().zip(&window).enumerate() {
            let phase = w * i as f64;
            re += f64::from(x) * h * phase.cos();
            im += f64::from(x) * h * phase.sin();
        }
        re * re + im * im
    };
    let mut best = (0.0, 0.0);
    let mut cents = -60.0;
    while cents <= 60.0 {
        let p = power(cents);
        if p > best.1 {
            best = (cents, p);
        }
        cents += 1.0;
    }
    let mut fine = best.0 - 1.0;
    let end = best.0 + 1.0;
    while fine <= end {
        let p = power(fine);
        if p > best.1 {
            best = (fine, p);
        }
        fine += 0.02;
    }
    best.0
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

fn seconds(samples: &[f32], from: f32, to: f32) -> &[f32] {
    &samples[(from * RATE) as usize..(to * RATE) as usize]
}

#[test]
fn every_model_plays_in_tune_over_its_range() {
    let cases = [
        (GuitarModel::Bass, [23_u8, 28, 33, 40, 50, 60]),
        (GuitarModel::Steel, [40, 45, 52, 59, 69, 84]),
        (GuitarModel::Nylon, [40, 47, 55, 64, 72, 81]),
        (GuitarModel::Electric, [35, 40, 52, 64, 76, 88]),
    ];
    for (model, keys) in cases {
        for key in keys {
            let hz = frequency(key, 0.0);
            let mut voice = StringVoice::new(RATE, 20.0, model.strings());
            voice.pluck(pick(hz));
            let out = render(&mut voice, 0.8, |_| {});
            let off = cents_off(seconds(&out, 0.2, 0.8), hz);
            assert!(off.abs() < 2.0, "{model:?} key {key}: {off:.2} cents off");
        }
    }
}

#[test]
fn tuning_holds_at_other_sample_rates() {
    for rate in [44_100.0_f32, 96_000.0] {
        let hz = frequency(64, 0.0);
        let mut voice = StringVoice::new(rate, 20.0, GuitarModel::Steel.strings());
        voice.pluck(pick(hz));
        let mut out = vec![0.0_f32; (0.6 * rate) as usize];
        for block in out.chunks_mut(BLOCK) {
            voice.render(block);
        }
        // Resample-free check: count the period by the DFT at this rate.
        let n = out.len();
        let segment = &out[n / 3..];
        let probe = |f: f32| {
            let w = std::f32::consts::TAU * f / rate;
            let (re, im) =
                segment
                    .iter()
                    .enumerate()
                    .fold((0.0_f64, 0.0_f64), |(re, im), (i, &x)| {
                        let p = f64::from(w) * i as f64;
                        (re + f64::from(x) * p.cos(), im + f64::from(x) * p.sin())
                    });
            re * re + im * im
        };
        let flat = probe(hz * 2.0_f32.powf(-10.0 / 1200.0));
        let sharp = probe(hz * 2.0_f32.powf(10.0 / 1200.0));
        let exact = probe(hz);
        assert!(exact > flat && exact > sharp, "{rate} Hz: off pitch");
    }
}

#[test]
fn a_ringing_note_sustains_and_a_damped_one_stops() {
    let hz = frequency(52, 0.0);
    let mut ringing = StringVoice::new(RATE, 20.0, GuitarModel::Electric.strings());
    ringing.pluck(pick(hz));
    let out = render(&mut ringing, 2.0, |_| {});
    let early = rms(seconds(&out, 0.05, 0.15));
    let late = rms(seconds(&out, 1.8, 2.0));
    assert!(early > 0.01, "audible: {early}");
    // The attack is bright (the bridge passes its highs, which die quickly); what rings
    // on is the lower partials, well within 40 dB.
    assert!(
        late > early * 0.01,
        "an electric string still rings after two seconds: {late} against {early}"
    );
    assert!(late < early, "but quieter");

    let mut damped = StringVoice::new(RATE, 20.0, GuitarModel::Electric.strings());
    damped.pluck(pick(hz));
    let mut released = false;
    let mut blocks = 0;
    let out = render(&mut damped, 0.6, |voice| {
        blocks += 1;
        if blocks * BLOCK >= (0.1 * RATE) as usize && !released {
            voice.release(Release::Damped);
            released = true;
        }
    });
    assert!(
        rms(seconds(&out, 0.4, 0.6)) < 1e-4,
        "silent after a damped end"
    );
    assert!(
        !damped.is_sounding(),
        "a silent string is no longer computed"
    );
}

#[test]
fn palm_mutes_and_dead_notes_are_short() {
    let hz = frequency(40, 0.0);
    for (palm_mute, dead) in [(true, false), (false, true)] {
        let mut voice = StringVoice::new(RATE, 20.0, GuitarModel::Electric.strings());
        voice.pluck(Pluck {
            palm_mute,
            dead,
            ..pick(hz)
        });
        let out = render(&mut voice, 1.0, |_| {});
        let attack = rms(seconds(&out, 0.0, 0.05));
        assert!(attack > 1e-3, "the strike is heard");
        assert!(
            rms(seconds(&out, 0.7, 1.0)) < attack * 0.01,
            "palm {palm_mute} dead {dead}: dies away"
        );
    }
}

#[test]
fn a_bend_reaches_its_pitch_while_the_string_rings() {
    let hz = frequency(57, 0.0);
    let mut voice = StringVoice::new(RATE, 20.0, GuitarModel::Electric.strings());
    voice.pluck(pick(hz));
    let mut blocks = 0_usize;
    let out = render(&mut voice, 1.0, |voice| {
        blocks += 1;
        // A whole step up over the first 0.2 seconds, then held.
        let t = (blocks * BLOCK) as f32 / RATE;
        voice.set_bend(200.0 * (t / 0.2).min(1.0));
    });
    let target = frequency(57, 200.0);
    let off = cents_off(seconds(&out, 0.4, 1.0), target);
    assert!(off.abs() < 3.0, "{off:.2} cents off the bent pitch");
}

#[test]
fn a_hammer_on_changes_the_pitch_without_a_new_pick() {
    let from = frequency(57, 0.0);
    let to = frequency(59, 0.0);
    let mut voice = StringVoice::new(RATE, 20.0, GuitarModel::Electric.strings());
    voice.pluck(pick(from));
    let before = render(&mut voice, 0.3, |_| {});
    voice.pluck(Pluck {
        attack: Attack::Legato,
        velocity: 0.6,
        ..pick(to)
    });
    let after = render(&mut voice, 0.6, |_| {});
    let off = cents_off(&after[(0.1 * RATE) as usize..], to);
    assert!(off.abs() < 3.0, "{off:.2} cents off the hammered note");
    let ringing = rms(&before[before.len() - 960..]);
    let next = rms(&after[..960]);
    assert!(next > ringing * 0.5, "the string keeps its energy");
    assert!(next < ringing * 2.0, "no fresh pick attack");

    // A pick on a sounding string strikes it again.
    let mut picked = StringVoice::new(RATE, 20.0, GuitarModel::Electric.strings());
    picked.pluck(pick(from));
    let ring = render(&mut picked, 0.6, |_| {});
    picked.pluck(pick(from));
    let struck = render(&mut picked, 0.05, |_| {});
    assert!(rms(&struck) > rms(&ring[ring.len() - 2400..]));
}

#[test]
fn a_released_note_fades_within_a_fraction_of_a_second() {
    let mut voice = StringVoice::new(RATE, 20.0, GuitarModel::Steel.strings());
    voice.pluck(pick(frequency(64, 0.0)));
    let held = render(&mut voice, 0.2, |_| {});
    voice.release(Release::Natural);
    let out = render(&mut voice, 1.0, |_| {});
    assert!(rms(seconds(&out, 0.7, 1.0)) < rms(&held[held.len() - 2400..]) * 0.01);
}

#[test]
fn rendering_is_deterministic() {
    let play = || {
        let mut voice = StringVoice::new(RATE, 20.0, GuitarModel::Steel.strings());
        voice.pluck(pick(frequency(50, 0.0)));
        render(&mut voice, 0.3, |voice| voice.set_bend(30.0))
    };
    let a = play();
    let b = play();
    assert!(a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()));
}

/// A tiny deterministic generator for the parameter hunt.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 40) as f32 / (1_u64 << 24) as f32
    }

    /// A value from a wide range, sometimes not finite.
    fn wild(&mut self) -> f32 {
        match (self.next() * 12.0) as u32 {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            3 => -1e9,
            4 => 1e9,
            5 => 0.0,
            _ => (self.next() - 0.5) * 4_000.0,
        }
    }
}

#[test]
fn no_parameters_make_nan_or_runaway_output() {
    let mut random = Lcg(42);
    for round in 0..200 {
        let model = StringModel {
            sustain: random.wild(),
            sustain_exponent: random.wild(),
            brilliance: random.wild(),
            softness: random.wild(),
            tone: random.wild(),
            resonance: random.wild(),
            bridge: random.wild(),
            low_cut: random.wild(),
            second: random.wild(),
            detune: random.wild(),
            stiffness: random.wild(),
            stiffness_stages: (random.next() * 10.0) as usize,
            pick_position: random.wild(),
            hardness: random.wild(),
            level: random.wild(),
            pickup: [Pickup::None, Pickup::SingleCoil, Pickup::Humbucker][round % 3],
            pickup_position: random.wild(),
        };
        let rate = [8_000.0, 44_100.0, 192_000.0, random.wild()][round % 4];
        let mut voice = StringVoice::new(rate, random.wild(), model);
        let mut out = vec![0.0_f32; BLOCK];
        for step in 0..40 {
            match (random.next() * 6.0) as u32 {
                0 => voice.pluck(Pluck {
                    frequency: random.wild(),
                    velocity: random.wild(),
                    attack: [
                        Attack::Pick,
                        Attack::Legato,
                        Attack::Tap,
                        Attack::Slap,
                        Attack::Pop,
                        Attack::Scrape,
                    ][(random.next() * 6.0) as usize % 6],
                    palm_mute: random.next() < 0.3,
                    dead: random.next() < 0.2,
                    harmonic: random.next() < 0.2,
                }),
                1 => voice.set_bend(random.wild()),
                2 => voice.release(if random.next() < 0.5 {
                    Release::Natural
                } else {
                    Release::Damped
                }),
                3 => voice.set_level(random.wild()),
                4 => voice.set_model(GuitarModel::Bass.strings()),
                _ => {}
            }
            out.fill(0.0);
            voice.render(&mut out);
            for &sample in &out {
                assert!(sample.is_finite(), "round {round} step {step}: not finite");
                assert!(
                    sample.abs() < 20.0,
                    "round {round} step {step}: runaway {sample}"
                );
            }
        }
    }
}

/// The power at `frequency` (a windowed DFT bin).
fn power_at(samples: &[f32], frequency: f32) -> f64 {
    let n = samples.len() as f64;
    let w = std::f64::consts::TAU * f64::from(frequency) / f64::from(RATE);
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &x) in samples.iter().enumerate() {
        let hann = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n).cos();
        re += f64::from(x) * hann * (w * i as f64).cos();
        im += f64::from(x) * hann * (w * i as f64).sin();
    }
    re * re + im * im
}

#[test]
fn the_highs_of_a_low_note_fade_as_fast_as_a_high_notes() {
    // How many dB the harmonic `n` of `key` loses between 0.03 and 0.4 seconds; both
    // harmonics below lie at 1.98 kHz.
    let fall = |key: u8, n: f32| {
        let f = frequency(key, 0.0) * n;
        let mut voice = StringVoice::new(RATE, 20.0, GuitarModel::Electric.strings());
        voice.pluck(pick(frequency(key, 0.0)));
        let out = render(&mut voice, 0.6, |_| {});
        let early = power_at(seconds(&out, 0.03, 0.13), f);
        let late = power_at(seconds(&out, 0.4, 0.5), f);
        10.0 * (early / late.max(1e-30)).log10()
    };
    let (low, high) = (fall(40, 24.0), fall(64, 6.0));
    // The electric's brilliance is 0.8 s: about 28 dB in 0.37 s.
    assert!(low > 15.0, "the low E's highs fade: {low:.1} dB");
    assert!(
        (low / high - 1.0).abs() < 0.5,
        "the same rate on both strings: low {low:.1} dB, high {high:.1} dB"
    );
}

#[test]
fn a_bridge_pickup_bites_and_a_neck_pickup_is_round() {
    // The open A string (110 Hz) on an electric guitar with one coil at the bridge, in
    // the middle and at the neck, and with no pickup: the share of the mid overtones (the
    // third to the fifth) against the fundamental is far larger at the bridge, the neck pickup
    // loses the octave (it sits about a quarter of the way along), and every one is about
    // as loud as the string heard at the bridge.
    let hz = frequency(45, 0.0);
    let play = |pickup: Pickup, position: f32| {
        let model = StringModel {
            pickup,
            pickup_position: position,
            ..GuitarModel::Electric.strings()
        };
        let mut voice = StringVoice::new(RATE, 20.0, model);
        voice.pluck(pick(hz));
        render(&mut voice, 1.0, |_| {})
    };
    let mids = |out: &[f32]| {
        let part = seconds(out, 0.1, 0.6);
        (3..=5).map(|n| power_at(part, hz * n as f32)).sum::<f64>() / power_at(part, hz)
    };
    let bridge = play(Pickup::SingleCoil, gscore_guitar::pickup_positions::BRIDGE);
    let middle = play(Pickup::SingleCoil, gscore_guitar::pickup_positions::MIDDLE);
    let neck = play(Pickup::SingleCoil, 0.25);
    let none = play(Pickup::None, 0.25);
    // (The middle pickup is not simply between them: it sits near the third overtone's
    // node, a comb has no single brightness.)
    assert!(
        mids(&bridge) > 2.0 * mids(&middle) && mids(&bridge) > 2.0 * mids(&neck),
        "{} {} {}",
        mids(&bridge),
        mids(&middle),
        mids(&neck)
    );
    let part = |out: &[f32]| seconds(out, 0.1, 0.6).to_vec();
    let octave = |out: &[f32]| power_at(&part(out), hz * 2.0) / power_at(&part(out), hz);
    assert!(
        octave(&neck) < 0.05 * octave(&none),
        "the neck pickup has no octave"
    );
    for (name, out) in [("bridge", &bridge), ("middle", &middle), ("neck", &neck)] {
        let ratio = rms(out) / rms(&none);
        assert!((0.4..2.5).contains(&ratio), "{name}: {ratio}");
    }
    // A humbucker plays too, and every pickup stays in tune.
    let humbucker = play(Pickup::Humbucker, gscore_guitar::pickup_positions::BRIDGE);
    assert!(humbucker.iter().all(|x| x.is_finite()) && rms(&humbucker) > 0.01);
    let off = cents_off(seconds(&neck, 0.2, 0.8), hz);
    assert!(off.abs() < 2.0, "{off:.2} cents");
}

/// The power above `from` Hz in `samples`, summed over harmonic-free bins every 50 Hz.
fn power_above(samples: &[f32], from: f32) -> f64 {
    let mut total = 0.0;
    let mut f = from;
    while f < 10_000.0 {
        total += power_at(samples, f);
        f += 50.0;
    }
    total
}

#[test]
fn a_palm_muted_note_keeps_its_strength_pitch_and_low_end() {
    // How fast the fundamental falls from 0.1 to 0.3 seconds, dB, per note.
    let mut falls = Vec::new();
    for (model, key) in [
        (GuitarModel::Electric, 40_u8),
        (GuitarModel::Electric, 52),
        (GuitarModel::Electric, 64),
        (GuitarModel::Steel, 40),
        (GuitarModel::Bass, 28),
    ] {
        let hz = frequency(key, 0.0);
        let play = |palm_mute: bool| {
            let mut voice = StringVoice::new(RATE, 20.0, model.strings());
            voice.pluck(Pluck {
                palm_mute,
                ..pick(hz)
            });
            render(&mut voice, 0.5, |_| {})
        };
        let (open, muted) = (play(false), play(true));
        let fundamental = |out: &[f32], from, to| power_at(seconds(out, from, to), hz);
        let db = |ratio: f64| 10.0 * ratio.log10();
        let case = format!("{model:?} key {key}");
        // As strong: the fundamental as loud as the open note's at first, the pick's
        // attack kept.
        let strength = db(fundamental(&muted, 0.0, 0.1) / fundamental(&open, 0.0, 0.1));
        assert!(strength > -6.0, "{case}: fundamental {strength:.1} dB");
        let attack = rms(seconds(&muted, 0.0, 0.03)) / rms(seconds(&open, 0.0, 0.03));
        assert!(
            attack > 0.6,
            "{case}: the attack {attack:.2} of the open note's"
        );
        // The pitch still sounds well past an eighth note (a quarter of a second at 120
        // BPM): not even 40 dB down by 0.3 seconds.
        let fall = db(fundamental(&muted, 0.1, 0.2) / fundamental(&muted, 0.3, 0.4));
        assert!(fall < 40.0, "{case}: the fundamental falls {fall:.1} dB");
        assert!(fall > 5.0, "{case}: but it is muted ({fall:.1} dB)");
        falls.push(fall);
        // The hand takes the highs: far less above 3 kHz than the open note.
        let highs = db(power_above(seconds(&muted, 0.02, 0.1), 3_000.0)
            / power_above(seconds(&open, 0.02, 0.1), 3_000.0));
        assert!(highs < -20.0, "{case}: highs {highs:.1} dB against open");
        // The same pitch: the damping does not detune it.
        let off = cents_off(seconds(&muted, 0.05, 0.35), hz);
        assert!(off.abs() < 2.0, "{case}: {off:.2} cents off");
    }
    // Low strings ring longer than high ones.
    assert!(falls[0] < falls[1] && falls[1] < falls[2], "{falls:?}");
}

#[test]
fn a_new_note_stops_a_palm_muted_one_on_the_same_string() {
    let (first, second) = (frequency(40, 0.0), frequency(43, 0.0));
    let mut voice = StringVoice::new(RATE, 20.0, GuitarModel::Electric.strings());
    let palm = |hz| Pluck {
        palm_mute: true,
        ..pick(hz)
    };
    voice.pluck(palm(first));
    let _ = render(&mut voice, 0.25, |_| {});
    voice.pluck(palm(second));
    let after = render(&mut voice, 0.3, |_| {});
    let window = seconds(&after, 0.05, 0.25);
    assert!(
        power_at(window, first) < 0.01 * power_at(window, second),
        "the string sounds the new note only"
    );
}
