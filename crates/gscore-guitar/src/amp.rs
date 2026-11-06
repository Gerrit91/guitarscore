//! A guitar amp, computed: a preamp of up to three clipping stages, a tone stack and a
//! speaker. Nothing is captured from a real amp; the stages are what makes a tube preamp
//! sound the way it does, simplified:
//!
//! ```text
//!  in ─► tight ─► voice ─► drive ─┬─[ 4× oversampled ]────────────────────────┬─► tone ─► speaker ─► level
//!        (low cut) (mid push)     └► stage ─► couple ─► stage ─► … ─► smooth ─┘   stack   (or direct)
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
//! - **Speaker:** the response of a closed 4×12 cabinet (a low cut, the cone's resonance,
//!   the presence peak, and the steep roll-off above 5 kHz that keeps distortion from
//!   sounding like a bee). Turn it down to 0 when a cabinet's impulse response follows in
//!   a Cab block; it then plays direct, as an amp's line out would.

use crate::filters::{Biquad, DcBlocker};

/// The oversampling factor of the clipping stages.
const OVERSAMPLE: usize = 4;
/// The anti-aliasing lowpass (three Butterworth sections: sixth order) on the way up and
/// on the way down, as a share of the base rate.
const ALIAS_CUT: f32 = 0.42;
const BUTTERWORTH_6: [f32; 3] = [0.517_638, std::f32::consts::FRAC_1_SQRT_2, 1.931_852];
/// The fourth-order Butterworth of the speaker's roll-off.
const BUTTERWORTH_4: [f32; 2] = [0.541_196, 1.306_563];
/// At most this many clipping stages.
const STAGES: usize = 3;
/// The strings come in hot (a chord picked hard peaks near 4): this brings them to about
/// the level a pickup gives an amp's input, where the stages start to clip.
const INPUT_PAD: f32 = 0.15;

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
                // The pad given back, less as the gain rises (it clips by then).
                trim_db: 16.5,
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
                trim_db: -2.0,
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
                trim_db: -3.0,
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
    /// The output's trim, so the voices come out about as loud as one another and as the
    /// strings without an amp...
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
    /// The built-in speaker, 0 direct (for a cabinet's response after it) to 1 through
    /// the speaker.
    pub speaker: f32,
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
            speaker: 1.0,
        }
    }
}

/// A soft clip (a rational approximation of tanh), reaching ±1 at ±3 and staying there.
fn soft(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// One clipping stage in the oversampled domain: its coupling low cut and plate high cut.
#[derive(Debug, Clone, Copy, Default)]
struct Stage {
    couple: Biquad,
    plate: Biquad,
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
    up: [Biquad; 3],
    down: [Biquad; 3],
    stages: [Stage; STAGES],
    dc: DcBlocker,
    tone: [Biquad; 3],
    speaker: [Biquad; 6],
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
            up: [Biquad::default(); 3],
            down: [Biquad::default(); 3],
            stages: [Stage::default(); STAGES],
            dc: DcBlocker::new(sample_rate),
            tone: [Biquad::default(); 3],
            speaker: [Biquad::default(); 6],
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
        let cut = (rate * ALIAS_CUT).min(20_000.0);
        for (filter, q) in [&mut self.up, &mut self.down]
            .into_iter()
            .flat_map(|filters| filters.iter_mut().zip(BUTTERWORTH_6))
        {
            *filter = filter.retuned(Biquad::lowpass(fast, cut, q));
        }
        for stage in &mut self.stages {
            stage.couple = stage.couple.retuned(Biquad::highpass(fast, 30.0, 0.5));
            stage.plate = stage.plate.retuned(Biquad::lowpass(fast, shape.plate, 0.6));
        }
        let [bass, mid, treble] = &mut self.tone;
        let tone_db = |knob: f32| (knob.clamp(0.0, 1.0) - 0.5) * 24.0;
        *bass = bass.retuned(Biquad::shelf(rate, 120.0, tone_db(knobs.bass), false));
        *mid = mid.retuned(Biquad::peaking(rate, 700.0, 0.7, tone_db(knobs.mid)));
        *treble = treble.retuned(Biquad::shelf(rate, 3_000.0, tone_db(knobs.treble), true));
        let [low, body, boxy, presence, roll_a, roll_b] = &mut self.speaker;
        *low = low.retuned(Biquad::highpass(rate, 75.0, 0.8));
        *body = body.retuned(Biquad::peaking(rate, 110.0, 1.4, 3.5));
        *boxy = boxy.retuned(Biquad::peaking(rate, 450.0, 1.0, -2.5));
        *presence = presence.retuned(Biquad::peaking(rate, 2_400.0, 1.5, 3.0));
        let [qa, qb] = BUTTERWORTH_4;
        *roll_a = roll_a.retuned(Biquad::lowpass(rate, 5_000.0, qa));
        *roll_b = roll_b.retuned(Biquad::lowpass(rate, 5_000.0, qb));
        self.shape = shape;
        self.knobs = knobs;
    }

    /// Silence in every filter.
    pub(crate) fn reset(&mut self) {
        let filters = [&mut self.tight, &mut self.push]
            .into_iter()
            .chain(&mut self.up)
            .chain(&mut self.down)
            .chain(
                self.stages
                    .iter_mut()
                    .flat_map(|stage| [&mut stage.couple, &mut stage.plate]),
            )
            .chain(&mut self.tone)
            .chain(&mut self.speaker);
        for filter in filters {
            filter.reset();
        }
        self.dc.reset();
    }

    /// The stages on one sample at the fast rate.
    fn clip(&mut self, x: f32) -> f32 {
        let shape = self.shape;
        let mut value = x;
        let used = shape.stages.min(STAGES);
        for (index, (stage, gain)) in self
            .stages
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

    /// One sample through the amp.
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let driven = self.push.process(self.tight.process(x * INPUT_PAD)) * self.drive;
        let mut out = 0.0;
        for step in 0..OVERSAMPLE {
            // Zero-stuffing: one sample in four, four times as loud, then the lowpass.
            let stuffed = if step == 0 {
                driven * OVERSAMPLE as f32
            } else {
                0.0
            };
            let up = self
                .up
                .iter_mut()
                .fold(stuffed, |value, filter| filter.process(value));
            let clipped = self.clip(up);
            out = self
                .down
                .iter_mut()
                .fold(clipped, |value, filter| filter.process(value));
        }
        let toned = self
            .tone
            .iter_mut()
            .fold(self.dc.process(out), |value, filter| filter.process(value));
        let speaker = self.knobs.speaker.clamp(0.0, 1.0);
        let through = self
            .speaker
            .iter_mut()
            .fold(toned, |value, filter| filter.process(value));
        (toned * (1.0 - speaker) + through * speaker) * self.trim
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
        let input = sine(220.0, 0.03, 24_000);
        let direct = AmpKnobs {
            speaker: 0.0,
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
        let input = sine(3_000.0, 0.5, 48_000);
        let knobs = AmpKnobs {
            voice: AmpVoice::Lead,
            gain: 1.0,
            speaker: 0.0,
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
    fn the_voices_are_about_as_loud_and_silence_stays_silent() {
        let input = sine(330.0, 0.2, 24_000);
        let levels = AmpVoice::ALL.map(|voice| {
            rms(&run(
                AmpKnobs {
                    voice,
                    ..AmpKnobs::default()
                },
                &input,
            )[12_000..])
        });
        let (low, high) = levels
            .iter()
            .fold((f32::MAX, 0.0_f32), |(lo, hi), &l| (lo.min(l), hi.max(l)));
        assert!(high / low < 4.0, "within 12 dB: {levels:?}");
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

    #[test]
    fn the_speaker_rolls_off_the_fizz() {
        let amp = Amp::new(RATE, AmpKnobs::default());
        let gain = |f: f32| {
            amp.speaker
                .iter()
                .map(|s| s.gain_at(RATE, f))
                .product::<f32>()
        };
        assert!(gain(10_000.0) < 0.1 * gain(1_000.0), "-20 dB at 10 kHz");
        assert!(gain(40.0) < 0.5 * gain(1_000.0), "no deep boom");
    }
}
