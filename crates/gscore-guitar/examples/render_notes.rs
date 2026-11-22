//! Renders single notes on one string voice, for fitting the string model to recordings.
//!
//! ```text
//! cargo run --release -p gscore-guitar --example render_notes -- NOTES OUT [key=value ...]
//! ```
//!
//! `NOTES` has one note per line: `open_hz fret velocity palm start_s length_s` (palm 0 or
//! 1; the note is damped after its length). `OUT` gets mono 48 kHz f32 samples (little
//! endian, no header). The model starts as the electric guitar's; `key=value` pairs change
//! its fields (`pickup=humbucker|single|none`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use gscore_guitar::{Attack, GuitarModel, Pickup, Pluck, Release, StringModel, StringVoice};

const RATE: f32 = 48_000.0;
const BLOCK: usize = 64;

fn set(model: &mut StringModel, key: &str, value: &str) {
    let number = || {
        value
            .parse::<f32>()
            .unwrap_or_else(|_| panic!("{key}={value}"))
    };
    match key {
        "sustain" => model.sustain = number(),
        "sustain_exponent" => model.sustain_exponent = number(),
        "brilliance" => model.brilliance = number(),
        "stiffness" => model.stiffness = number(),
        "stiffness_stages" => model.stiffness_stages = number() as usize,
        "pick_position" => model.pick_position = number(),
        "hardness" => model.hardness = number(),
        "softness" => model.softness = number(),
        "tone" => model.tone = number(),
        "resonance" => model.resonance = number(),
        "level" => model.level = number(),
        "bridge" => model.bridge = number(),
        "low_cut" => model.low_cut = number(),
        "second" => model.second = number(),
        "detune" => model.detune = number(),
        "pickup_position" => model.pickup_position = number(),
        "palm_mute" => model.palm_mute = number(),
        "pick_noise" => model.pick_noise = number(),
        "attack_pitch" => model.attack_pitch = number(),
        "pickup" => {
            model.pickup = match value {
                "humbucker" => Pickup::Humbucker,
                "single" => Pickup::SingleCoil,
                _ => Pickup::None,
            }
        }
        "inharmonicity" => model.inharmonicity = number(),
        "fretted_sustain" => model.fretted_sustain = number(),
        "treble_corner" => model.treble_corner = number(),
        _ => extra(model, key, number()),
    }
}

/// Fields added while fitting (kept apart so the list above stays the model's own).
fn extra(model: &mut StringModel, key: &str, value: f32) {
    let _ = (model, value);
    panic!("unknown model field {key}");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let notes = std::fs::read_to_string(&args[1]).unwrap();
    let mut model = GuitarModel::Electric.strings();
    for pair in &args[3..] {
        let (key, value) = pair.split_once('=').expect("key=value");
        set(&mut model, key, value);
    }
    let notes: Vec<[f32; 6]> = notes
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let v: Vec<f32> = line
                .split_whitespace()
                .map(|x| x.parse().unwrap())
                .collect();
            [v[0], v[1], v[2], v[3], v[4], v[5]]
        })
        .collect();
    let end = notes
        .iter()
        .map(|n| n[4] + n[5] + 1.0)
        .fold(0.0_f32, f32::max);
    let frames = (end * RATE) as usize;
    let mut out = vec![0.0_f32; frames];
    // One voice per note (the notes are far apart; a fresh string each time).
    for note in &notes {
        let [open, fret, velocity, palm, start, length] = *note;
        let mut voice = StringVoice::new(RATE, 30.0, model);
        voice.set_open(open);
        let begin = (start * RATE) as usize;
        let stop = ((start + length) * RATE) as usize;
        voice.pluck(Pluck {
            frequency: open * (fret / 12.0).exp2(),
            velocity,
            attack: Attack::Pick,
            palm_mute: palm > 0.5,
            dead: false,
            harmonic: false,
        });
        let mut at = begin;
        let mut released = false;
        let mut block = [0.0_f32; BLOCK];
        while at < frames && (voice.is_sounding() || !released) {
            if !released && at >= stop {
                voice.release(Release::Damped);
                released = true;
            }
            block.fill(0.0);
            let n = BLOCK.min(frames - at);
            voice.render(&mut block[..n]);
            for (o, x) in out[at..at + n].iter_mut().zip(&block[..n]) {
                *o += x;
            }
            at += n;
        }
    }
    let bytes: Vec<u8> = out.iter().flat_map(|x| x.to_le_bytes()).collect();
    std::fs::write(&args[2], bytes).unwrap();
}
