//! A guitar amp, computed: a preamp of up to three clipping stages and a tone stack. It
//! plays direct, as an amp's line out would: a Cab block after it is its speaker. Nothing is captured from a real amp; the stages are what makes a tube preamp
//! sound the way it does, simplified:
//!
//! ```text
//!  in ─► tight ─► voice ─► drive ─┬─[ 4× oversampled ]────────────────────────┬─► tone ─► level
//!        (low cut) (mid push)     └► stage ─► couple ─► stage ─► … ─► smooth ─┘   stack
//! ```
//!
//! - **Tight** takes the lows out before the clipping, so palm mutes stay a chug instead
//!   of a fart; a higher-gain voice cuts higher.
//! - **Stages** clip softly and a little lopsided (a tube's bias), which adds the even
//!   harmonics that make a tube sound warm. Between them a coupling capacitor (a low cut)
//!   and the plate's roll-off (a high cut); the Gain knob sits before the first.
//! - **4× oversampling** runs the stages four times as fast, so the harmonics the clipping
//!   makes above half the sample rate are filtered away instead of folding back down as
//!   the harsh, inharmonic fizz cheap distortion has.
//! - **Tone stack** after the preamp, as on most amps: Bass, Mid and Treble shelves and a
//!   bell, each ±12 dB.

use crate::filters::{Biquad, DcBlocker};
use crate::oversample::{FACTOR as OVERSAMPLE, Oversampler};

/// At most this many clipping stages.
const STAGES: usize = 3;
/// The strings' level into the stages. A chord picked hard on the electric's bridge
/// humbucker sustains at about 0.035 RMS after the Tight cut (its attack peaks near 1.2).
/// At this pad a clean voice at half gain barely touches such a chord, a crunch at half
/// gain crunches (its peaks stand about twice its body, against seven times dry) and a
/// lead saturates. Until 2026-11-11 it was 0.15, calibrated on a mis-summed test chord
/// about 25 dB hotter than real strings: the clean and crunch voices played far too
/// clean, and the lead had 10 dB less gain than now.
pub(crate) const INPUT_PAD: f32 = 0.5;

/// The character of the amp: how many stages clip, how hard, and how tight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AmpVoice {
    /// One stage, little gain: clean until picked hard, then it breaks up a bit.
    Clean,
    /// Two stages: the crunch of a pushed British amp.
    #[default]
    Crunch,
    /// Three stages, a mid push before them and a tight low end: high-gain lead and
    /// rhythm.
    Lead,
}

impl AmpVoice {
    /// Every voice, in the order a user interface shows them.
    pub const ALL: [AmpVoice; 3] = [AmpVoice::Clean, AmpVoice::Crunch, AmpVoice::Lead];

    /// Its name.
    pub fn label(self) -> &'static str {
        match self {
            AmpVoice::Clean => "Clean",
            AmpVoice::Crunch => "Crunch",
            AmpVoice::Lead => "Lead",
        }
    }

    fn shape(self) -> VoiceShape {
        match self {
            AmpVoice::Clean => VoiceShape {
                tight: 40.0,
                push: (800.0, 0.0),
                drive_db: (0.0, 20.0),
                stage_gain: [1.0, 1.0, 1.0],
                stages: 1,
                bias: 0.05,
                plate: 9_000.0,
                // The pad given back (exactly, so clean is unity at no gain), less as the
                // gain rises (it clips by then).
                trim_db: 6.0,
                trim_follow: 0.7,
            },
            AmpVoice::Crunch => VoiceShape {
                tight: 90.0,
                push: (900.0, 4.0),
                drive_db: (6.0, 34.0),
                stage_gain: [1.0, 5.0, 1.0],
                stages: 2,
                bias: 0.15,
                plate: 7_000.0,
                trim_db: -8.4,
                trim_follow: 0.0,
            },
            AmpVoice::Lead => VoiceShape {
                tight: 160.0,
                push: (750.0, 7.0),
                drive_db: (14.0, 46.0),
                stage_gain: [1.0, 6.0, 4.0],
                stages: 3,
                bias: 0.2,
                plate: 6_000.0,
                trim_db: -3.6,
                trim_follow: 0.0,
            },
        }
    }
}

/// What a voice is made of.
#[derive(Debug, Clone, Copy)]
struct VoiceShape {
    /// The low cut before the stages, Hz.
    tight: f32,
    /// A bell before the stages (Hz, dB): what they clip hardest.
    push: (f32, f32),
    /// The Gain knob's range, 0 to 1, in dB before the first stage.
    drive_db: (f32, f32),
    /// The gain before each stage after the first (the first takes the drive).
    stage_gain: [f32; STAGES],
    stages: usize,
    /// How lopsided the stages clip.
    bias: f32,
    /// The plate's high cut between the stages, Hz.
    plate: f32,
    /// The output's trim. Clean gives the pad back exactly (as loud as the strings without
    /// an amp); a crunch and a lead at half gain are as loud as they were before the input
    /// was recalibrated (2026-11-11: about +14 and +22 dB on a hard chord), so tones and
    /// mixes dialled in by ear keep their level...
    trim_db: f32,
    /// ...less this share of the drive (dB for dB), for a voice that stays clean long.
    trim_follow: f32,
}

/// The knobs of an amp block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmpKnobs {
    pub voice: AmpVoice,
    /// The preamp's gain, 0 to 1 (the voice decides how much that is).
    pub gain: f32,
    /// The tone stack, 0 to 1 each (0.5 flat).
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    /// The output level in dB.
    pub level_db: f32,
}

impl Default for AmpKnobs {
    fn default() -> Self {
        AmpKnobs {
            voice: AmpVoice::default(),
            gain: 0.5,
            bass: 0.5,
            mid: 0.5,
            treble: 0.5,
            level_db: 0.0,
        }
    }
}

/// A soft clip (a rational approximation of tanh), reaching ±1 at ±3 and staying there.
pub(crate) fn soft(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// One clipping stage in the oversampled domain: its coupling low cut and plate high cut.
#[derive(Debug, Clone, Copy, Default)]
struct Stage {
    couple: Biquad,
    plate: Biquad,
}

/// The stages on one sample at the fast rate.
fn clip(stages: &mut [Stage; STAGES], shape: VoiceShape, x: f32) -> f32 {
    let mut value = x;
    let used = shape.stages.min(STAGES);
    for (index, (stage, gain)) in stages
        .iter_mut()
        .zip(shape.stage_gain)
        .take(used)
        .enumerate()
    {
        let input = if index == 0 {
            value
        } else {
            stage.couple.process(value) * gain
        };
        let clipped = soft(input + shape.bias) - soft(shape.bias);
        value = stage.plate.process(clipped);
    }
    value
}

/// One channel of the amp.
#[derive(Debug, Clone)]
pub(crate) struct Amp {
    rate: f32,
    knobs: AmpKnobs,
    shape: VoiceShape,
    drive: f32,
    trim: f32,
    tight: Biquad,
    push: Biquad,
    oversampler: Oversampler,
    stages: [Stage; STAGES],
    dc: DcBlocker,
    tone: [Biquad; 3],
}

impl Amp {
    /// An amp for `sample_rate` with `knobs`. Allocates nothing.
    pub(crate) fn new(sample_rate: f32, knobs: AmpKnobs) -> Self {
        let mut amp = Amp {
            rate: sample_rate.max(1.0),
            knobs,
            shape: knobs.voice.shape(),
            drive: 1.0,
            trim: 1.0,
            tight: Biquad::default(),
            push: Biquad::default(),
            oversampler: Oversampler::new(sample_rate.max(1.0)),
            stages: [Stage::default(); STAGES],
            dc: DcBlocker::new(sample_rate),
            tone: [Biquad::default(); 3],
        };
        amp.set_knobs(knobs);
        amp
    }

    /// New knob values, keeping what it is playing (no allocation).
    pub(crate) fn set_knobs(&mut self, knobs: AmpKnobs) {
        let rate = self.rate;
        let fast = rate * OVERSAMPLE as f32;
        let shape = knobs.voice.shape();
        let db = shape.drive_db.0 + (shape.drive_db.1 - shape.drive_db.0) * knobs.gain;
        self.drive = 10.0_f32.powf(db / 20.0);
        let trim = shape.trim_db - shape.trim_follow * db + knobs.level_db;
        self.trim = 10.0_f32.powf(trim / 20.0);
        self.tight = self
            .tight
            .retuned(Biquad::highpass(rate, shape.tight, 0.707));
        self.push = self
            .push
            .retuned(Biquad::peaking(rate, shape.push.0, 0.7, shape.push.1));
        for stage in &mut self.stages {
            stage.couple = stage.couple.retuned(Biquad::highpass(fast, 30.0, 0.5));
            stage.plate = stage.plate.retuned(Biquad::lowpass(fast, shape.plate, 0.6));
        }
        let [bass, mid, treble] = &mut self.tone;
        let tone_db = |knob: f32| (knob.clamp(0.0, 1.0) - 0.5) * 24.0;
        *bass = bass.retuned(Biquad::shelf(rate, 120.0, tone_db(knobs.bass), false));
        *mid = mid.retuned(Biquad::peaking(rate, 700.0, 0.7, tone_db(knobs.mid)));
        *treble = treble.retuned(Biquad::shelf(rate, 3_000.0, tone_db(knobs.treble), true));
        self.shape = shape;
        self.knobs = knobs;
    }

    /// Silence in every filter.
    pub(crate) fn reset(&mut self) {
        let filters = [&mut self.tight, &mut self.push]
            .into_iter()
            .chain(
                self.stages
                    .iter_mut()
                    .flat_map(|stage| [&mut stage.couple, &mut stage.plate]),
            )
            .chain(&mut self.tone);
        for filter in filters {
            filter.reset();
        }
        self.dc.reset();
        self.oversampler.reset();
    }

    /// One sample through the amp.
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let driven = self.push.process(self.tight.process(x * INPUT_PAD)) * self.drive;
        let (stages, shape) = (&mut self.stages, self.shape);
        let out = self
            .oversampler
            .run(driven, |value| clip(stages, shape, value));
        let toned = self
            .tone
            .iter_mut()
            .fold(self.dc.process(out), |value, filter| filter.process(value));
        toned * self.trim
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use std::f32::consts::TAU;

    use super::*;

    const RATE: f32 = 48_000.0;

    fn sine(frequency: f32, level: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| level * (TAU * frequency * i as f32 / RATE).sin())
            .collect()
    }

    fn rms(signal: &[f32]) -> f32 {
        (signal.iter().map(|x| x * x).sum::<f32>() / signal.len().max(1) as f32).sqrt()
    }

    /// The level of `frequency` in `signal` (one DFT bin).
    fn tone(signal: &[f32], frequency: f32) -> f32 {
        let (mut re, mut im) = (0.0_f32, 0.0_f32);
        for (i, x) in signal.iter().enumerate() {
            let phase = TAU * frequency * i as f32 / RATE;
            re += x * phase.cos();
            im += x * phase.sin();
        }
        2.0 * re.hypot(im) / signal.len() as f32
    }

    fn run(knobs: AmpKnobs, input: &[f32]) -> Vec<f32> {
        let mut amp = Amp::new(RATE, knobs);
        input.iter().map(|&x| amp.process(x)).collect()
    }

    #[test]
    fn more_gain_more_harmonics() {
        // 220 Hz for a whole number of periods in the measured part (0.25 s), quiet
        // enough that the voices differ before all of them clip to a square.
        let input = sine(220.0, 0.009, 24_000);
        let direct = AmpKnobs {
            ..AmpKnobs::default()
        };
        let mut previous = 0.0;
        for voice in AmpVoice::ALL {
            let out = run(AmpKnobs { voice, ..direct }, &input);
            let tail = &out[12_000..];
            let fundamental = tone(tail, 220.0);
            let harmonics: f32 = (2..8).map(|h| tone(tail, 220.0 * h as f32)).sum();
            let ratio = harmonics / fundamental.max(1e-9);
            assert!(ratio > previous, "{voice:?}: {ratio} after {previous}");
            previous = ratio;
        }
        assert!(previous > 0.3, "the lead voice distorts: {previous}");
        let clean = run(
            AmpKnobs {
                voice: AmpVoice::Clean,
                ..direct
            },
            &input,
        );
        let tail = &clean[12_000..];
        let harmonics: f32 = (2..8).map(|h| tone(tail, 220.0 * h as f32)).sum();
        assert!(harmonics < 0.02 * tone(tail, 220.0), "clean stays clean");
    }

    #[test]
    fn little_aliasing() {
        // A high note clipped hard: its harmonics past 24 kHz must not fold back. 3 kHz
        // has harmonics at multiples of 3 kHz only; anything off that grid is aliasing.
        let input = sine(3_000.0, 0.15, 48_000);
        let knobs = AmpKnobs {
            voice: AmpVoice::Lead,
            gain: 1.0,
            treble: 0.5,
            ..AmpKnobs::default()
        };
        let out = run(knobs, &input);
        let tail = &out[24_000..];
        let wanted = tone(tail, 3_000.0);
        // 24 kHz - 3 kHz * 9 folds to 3 kHz too; take bins between the harmonics.
        let folded = [1_500.0, 4_500.0, 7_500.0, 10_500.0]
            .map(|f| tone(tail, f))
            .into_iter()
            .fold(0.0_f32, f32::max);
        assert!(folded < wanted * 0.03, "{folded} against {wanted}");
    }

    #[test]
    fn the_voices_keep_their_levels_and_silence_stays_silent() {
        // A string's level. Clean comes out as loud as it goes in; a crunch and a lead
        // louder, as they always were on real strings (their trims keep the levels tones
        // were dialled in with; a pure sine squares up in the lead, so higher than a chord).
        let input = sine(330.0, 0.06, 24_000);
        let levels = AmpVoice::ALL.map(|voice| {
            rms(&run(
                AmpKnobs {
                    voice,
                    gain: 0.0,
                    ..AmpKnobs::default()
                },
                &input,
            )[12_000..])
        });
        let db = |level: f32| 20.0 * (level / rms(&input[12_000..])).log10();
        let [clean, crunch, lead] = levels.map(db);
        assert!(clean.abs() < 3.0, "clean is unity: {clean:.1} dB");
        assert!(
            clean < crunch && crunch < lead && lead < 36.0,
            "{clean:.1} {crunch:.1} {lead:.1} dB"
        );
        let out = run(AmpKnobs::default(), &[0.0; 4_800]);
        assert!(out.iter().all(|x| x.abs() < 1e-6));
    }

    #[test]
    fn the_tone_stack_moves_its_band() {
        let flat = Amp::new(RATE, AmpKnobs::default());
        let [bass, mid, treble] = flat.tone;
        assert!((bass.gain_at(RATE, 1_000.0) - 1.0).abs() < 0.05);
        assert!((mid.gain_at(RATE, 700.0) - 1.0).abs() < 0.01);
        let full = Amp::new(
            RATE,
            AmpKnobs {
                bass: 1.0,
                mid: 0.0,
                treble: 1.0,
                ..AmpKnobs::default()
            },
        );
        let [bass, mid, treble2] = full.tone;
        let db = |g: f32| 20.0 * g.log10();
        assert!((db(bass.gain_at(RATE, 30.0)) - 12.0).abs() < 1.0);
        assert!((db(mid.gain_at(RATE, 700.0)) + 12.0).abs() < 0.5);
        assert!((db(treble2.gain_at(RATE, 15_000.0)) - 12.0).abs() < 1.0);
        assert!((treble.gain_at(RATE, 100.0) - 1.0).abs() < 0.05);
    }
}
