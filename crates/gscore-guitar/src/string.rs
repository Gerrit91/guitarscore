//! One vibrating string as digital waveguides: a loop per direction of vibration (see
//! `string_loop`), the second slightly detuned and decaying faster, as a real string's
//! vertical vibration does: together they give the beating and two-stage decay that keep
//! a long note from sounding like a synthesizer.
//!
//! ```text
//!              ┌──► [ loop: across the top ]  ──┐
//! excitation ──┤                                ├──► bridge ──► low cut ──► tone ──► out
//!              └──► [ loop: toward the top ]  ──┘
//! ```
//!
//! Each loop is tuned exactly, so the note is in tune whatever its filters do. Every
//! technique changes parameters of the running loops: a bend, slide or vibrato changes
//! the delay while the string vibrates, a hammer-on changes it without a new pluck,
//! damping lowers the loop gain, and a new pluck adds to what is still vibrating. The
//! output is a blend of the string's swing and the force it puts on the bridge (its
//! velocity, which a bridge and a magnetic pickup pass): the force carries far less of the
//! fundamental, as a real guitar's low notes do.
//!
//! `render` neither allocates nor blocks: the delay lines are made once, long enough for
//! the lowest pitch the voice is set up for.

use std::f32::consts::{PI, TAU};

use crate::filters::{Biquad, DcBlocker, Noise};
use crate::model::StringModel;
use crate::string_loop::{Loop, Tuning};

pub(crate) use crate::string_loop::MAX_STIFFNESS_STAGES as MAX_STAGES;

/// A hammer-on or pull-off reaches its new pitch within this long, in seconds.
const LEGATO_GLIDE: f32 = 0.002;
/// How long damping changes take to settle (time constant, seconds).
const DAMPING_SETTLE: f32 = 0.004;
/// Below this level the string counts as silent and is no longer computed.
const SILENCE: f32 = 1e-5;
/// What is left of a vibrating string when the pick strikes it again.
const PICK_CONTACT: f32 = 0.35;
/// The strike's brightness: hardness 0 rounds it above this many Hz, hardness 1 above
/// [`HARDEST`].
const SOFTEST: f32 = 400.0;
const HARDEST: f32 = 12_000.0;
/// The second vibration's decay against the first's: its fundamental and its highs.
const SECOND_DECAY: f32 = 0.45;
const SECOND_HIGH_DECAY: f32 = 0.6;
/// The bridge force is scaled to the swing's level at this frequency, Hz.
const BRIDGE_AT: f32 = 1_000.0;
/// Quality of the low cut (Butterworth).
const LOW_CUT_Q: f32 = 0.707;

/// How a note is set going.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Attack {
    /// Picked: a fresh strike.
    #[default]
    Pick,
    /// Hammered on, pulled off or slid: the string keeps vibrating at a new length.
    Legato,
    /// Tapped (either hand): a strike at the fret, softer and rounder.
    Tap,
    /// Slapped with the thumb: hard and bright.
    Slap,
    /// Popped: pulled away and snapped back, very bright.
    Pop,
    /// The pick scraping along the string: noise, quickly damped.
    Scrape,
}

/// A note to start on the string.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pluck {
    /// The note's frequency in Hz.
    pub frequency: f32,
    /// How hard, 0 to 1.
    pub velocity: f32,
    /// How it is set going.
    pub attack: Attack,
    /// Palm muted: short and dull.
    pub palm_mute: bool,
    /// A dead note: only the strike, no pitch.
    pub dead: bool,
    /// A harmonic (the frequency is the harmonic's): a purer tone.
    pub harmonic: bool,
}

/// How a note ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Release {
    /// The fretting finger lets go: the string fades within a fraction of a second.
    Natural,
    /// Stopped dead (staccato, palm mute, the end of a dead note).
    Damped,
}

/// How strongly the string is damped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Damping {
    Ringing,
    PalmMuted,
    Released,
    Damped,
    Dead,
}

/// One string.
#[derive(Debug, Clone)]
pub struct StringVoice {
    sample_rate: f32,
    model: StringModel,
    /// The two directions of vibration.
    loops: [Loop; 2],
    dc: DcBlocker,
    /// The bridge's last input, for its force (the swing's change), and the force's scale.
    last: f32,
    bridge_scale: f32,
    /// The body's or pickup's low cut and tone on the output.
    low_cut: Biquad,
    tone: Biquad,
    noise: Noise,
    /// The note's frequency and the bend on top of it (cents).
    frequency: f32,
    bend: f32,
    /// Damping now and where it is going: the 60 dB decay times of the fundamental and of
    /// the harmonics around 2 kHz.
    damping: Damping,
    decay: f32,
    high_decay: f32,
    /// The output gain (a fade) now and where it goes.
    level: f32,
    level_target: f32,
    /// The loudest recent output, to stop computing a silent string.
    peak: f32,
    sounding: bool,
}

impl StringVoice {
    /// A string at `sample_rate` that can sound down to `lowest` Hz (bends and slides
    /// below it are held at it).
    pub fn new(sample_rate: f32, lowest: f32, model: StringModel) -> Self {
        let sample_rate = if sample_rate.is_finite() {
            sample_rate.clamp(8_000.0, 384_000.0)
        } else {
            48_000.0
        };
        let lowest = if lowest.is_finite() {
            lowest.clamp(8.0, 2_000.0)
        } else {
            20.0
        };
        let model = model.sanitized();
        // The second vibration may be tuned a little lower than the note.
        let samples = (sample_rate / lowest * 1.01).ceil() as usize;
        StringVoice {
            sample_rate,
            model,
            loops: [Loop::new(samples), Loop::new(samples)],
            dc: DcBlocker::new(sample_rate),
            last: 0.0,
            bridge_scale: 1.0 / (2.0 * (PI * BRIDGE_AT / sample_rate).sin()).max(1e-6),
            low_cut: Biquad::highpass(sample_rate, model.low_cut, LOW_CUT_Q),
            tone: Biquad::lowpass(sample_rate, model.tone, model.resonance),
            noise: Noise::new(0x9E37_79B9),
            frequency: 110.0,
            bend: 0.0,
            damping: Damping::Released,
            decay: model.sustain_at(110.0),
            high_decay: model.brilliance,
            level: 1.0,
            level_target: 1.0,
            peak: 0.0,
            sounding: false,
        }
    }

    /// Changes the string model (the vibration goes on).
    pub fn set_model(&mut self, model: StringModel) {
        self.model = model.sanitized();
        self.tone = self.tone.retuned(Biquad::lowpass(
            self.sample_rate,
            self.model.tone,
            self.model.resonance,
        ));
        self.low_cut = self.low_cut.retuned(Biquad::highpass(
            self.sample_rate,
            self.model.low_cut,
            LOW_CUT_Q,
        ));
        // The loss filters follow on the next block.
        for string in &mut self.loops {
            string.redesign();
        }
    }

    /// Whether the string still sounds.
    pub fn is_sounding(&self) -> bool {
        self.sounding
    }

    /// Starts a note. A legato attack on a sounding string only changes its length (with
    /// a light tap of the finger); otherwise the string is struck, and what still vibrates
    /// is mostly stopped by the pick.
    pub fn pluck(&mut self, pluck: Pluck) {
        let frequency = if pluck.frequency.is_finite() {
            pluck.frequency.clamp(8.0, self.sample_rate * 0.45)
        } else {
            return;
        };
        let velocity = if pluck.velocity.is_finite() {
            pluck.velocity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.bend = 0.0;
        let damping = if pluck.dead {
            Damping::Dead
        } else if pluck.palm_mute {
            Damping::PalmMuted
        } else {
            Damping::Ringing
        };
        if pluck.attack == Attack::Legato && self.sounding {
            self.frequency = frequency;
            self.set_damping(damping, false);
            self.update(self.glide_samples());
            // The finger hitting the fret (or leaving it) adds a little energy.
            self.excite(0.15 * velocity, 0.35, 1.0, 0.0);
            return;
        }
        self.frequency = frequency;
        self.set_damping(damping, true);
        self.level = self.level_target;
        self.update(0);
        let (amplitude, hardness, noise) = match pluck.attack {
            Attack::Pick => (1.0, self.model.hardness, 0.08),
            Attack::Legato => (0.7, self.model.hardness * 0.5, 0.02),
            Attack::Tap => (0.8, self.model.hardness * 0.6, 0.03),
            Attack::Slap => (1.2, 1.0, 0.15),
            Attack::Pop => (1.3, 1.0, 0.1),
            Attack::Scrape => (0.6, 1.0, 1.0),
        };
        let hardness = if pluck.harmonic {
            hardness * 0.3
        } else if pluck.dead || pluck.palm_mute {
            hardness * 0.5
        } else {
            hardness
        };
        // Harder strikes are brighter; a soft one rounds the edge of the pluck.
        let hardness = (hardness * (0.4 + 0.6 * velocity)).clamp(0.02, 1.0);
        let amplitude = amplitude * velocity.powf(1.3);
        self.excite(amplitude, hardness, PICK_CONTACT, noise);
    }

    /// Bends the sounding note by `cents` (bends, slides, vibrato, whammy). The pitch
    /// glides there over the next rendered block.
    pub fn set_bend(&mut self, cents: f32) {
        if cents.is_finite() {
            self.bend = cents.clamp(-4_800.0, 4_800.0);
        }
    }

    /// Ends the note.
    pub fn release(&mut self, release: Release) {
        let damping = match release {
            Release::Natural => Damping::Released,
            Release::Damped => Damping::Damped,
        };
        // A dead note stays dead.
        if self.damping != Damping::Dead {
            self.set_damping(damping, false);
        }
    }

    /// Stops the string quickly (playback stopped or jumped).
    pub fn silence(&mut self) {
        self.set_damping(Damping::Damped, false);
    }

    /// Sets the output gain (a fade); it glides there over the next block.
    pub fn set_level(&mut self, level: f32) {
        if level.is_finite() {
            self.level_target = level.clamp(0.0, 4.0);
        }
    }

    /// Adds the next `out.len()` samples of the string to `out`.
    pub fn render(&mut self, out: &mut [f32]) {
        if !self.sounding || out.is_empty() {
            return;
        }
        self.settle(out.len());
        self.update(out.len());
        let level_step = (self.level_target - self.level) / out.len() as f32;
        let second = self.model.second > 0.0;
        let bridge = self.model.bridge;
        let mut peak = 0.0_f32;
        for sample in out.iter_mut() {
            let [first_loop, second_loop] = &mut self.loops;
            let Some(mut swing) = first_loop.step() else {
                // A broken value mutes the string instead of breaking playback.
                self.mute();
                return;
            };
            if second {
                let Some(other) = second_loop.step() else {
                    self.mute();
                    return;
                };
                swing += other;
            }
            let force = (swing - self.last) * self.bridge_scale;
            self.last = swing;
            let heard = swing * (1.0 - bridge) + force * bridge;
            self.level += level_step;
            let out_sample = self
                .tone
                .process(self.low_cut.process(self.dc.process(heard)))
                * self.level
                * self.model.level;
            peak = peak.max(out_sample.abs());
            *sample += out_sample;
        }
        self.level = self.level_target;
        self.peak = peak.max(self.peak * 0.5);
        if self.peak < SILENCE {
            self.mute();
        }
    }

    /// Silence at once, nothing left in the loops.
    fn mute(&mut self) {
        for string in &mut self.loops {
            string.clear();
        }
        self.dc.reset();
        self.low_cut.reset();
        self.tone.reset();
        self.last = 0.0;
        self.peak = 0.0;
        self.sounding = false;
    }

    fn glide_samples(&self) -> usize {
        (LEGATO_GLIDE * self.sample_rate) as usize
    }

    /// The 60 dB decay times (fundamental, harmonics around 2 kHz) of `damping` at the
    /// current pitch.
    fn damping_values(&self, damping: Damping) -> (f32, f32) {
        match damping {
            Damping::Ringing => (self.model.sustain_at(self.frequency), self.model.brilliance),
            Damping::PalmMuted => (0.18, 0.03),
            Damping::Released => (0.3, 0.06),
            Damping::Damped => (0.04, 0.01),
            Damping::Dead => (0.015, 0.005),
        }
    }

    /// Sets how the string is damped, at once or settling over a few milliseconds.
    fn set_damping(&mut self, damping: Damping, now: bool) {
        self.damping = damping;
        if now {
            let (decay, high_decay) = self.damping_values(damping);
            self.decay = decay;
            self.high_decay = high_decay;
        }
    }

    /// Moves the damping toward its target over `samples`.
    fn settle(&mut self, samples: usize) {
        let (decay, high_decay) = self.damping_values(self.damping);
        let share = 1.0 - (-(samples as f32) / (DAMPING_SETTLE * self.sample_rate)).exp();
        // Decay times move on a log scale, so long and short meet evenly; they snap to
        // the target once close, so the loss filter is not designed again and again.
        let glide = |now: f32, target: f32| {
            let next = (now.ln() + (target.ln() - now.ln()) * share).exp();
            if (next / target - 1.0).abs() < 0.01 {
                target
            } else {
                next
            }
        };
        self.decay = glide(self.decay.max(1e-4), decay);
        self.high_decay = glide(self.high_decay.max(1e-4), high_decay);
    }

    /// Retunes both loops for the current pitch and damping; their delays glide to the new
    /// values over `glide` samples (at once if 0).
    fn update(&mut self, glide: usize) {
        let frequency = (self.frequency * (self.bend / 1200.0).exp2()).max(8.0);
        let first = Tuning {
            frequency,
            decay: self.decay,
            high_decay: self.high_decay,
            stiffness: self.model.stiffness,
            stages: self.model.stiffness_stages,
        };
        // The second vibration: a little detuned (half each way would move the heard
        // pitch; the first stays exact), and it gives its energy to the bridge sooner.
        let second = Tuning {
            frequency: frequency * (self.model.detune / 1200.0).exp2(),
            decay: self.decay * SECOND_DECAY,
            high_decay: self.high_decay * SECOND_HIGH_DECAY,
            ..first
        };
        let [first_loop, second_loop] = &mut self.loops;
        first_loop.update(self.sample_rate, first, glide);
        second_loop.update(self.sample_rate, second, glide);
    }

    /// Adds one period of excitation to the string, after scaling what vibrates by
    /// `contact`: the picking hand's strike at the pick position, a blend of a sharp pulse
    /// and a rounded triangle (the model's softness), rounded above a frequency set by
    /// `hardness` (0 to 1), with `noise` mixed in. The second vibration gets its share.
    fn excite(&mut self, amplitude: f32, hardness: f32, contact: f32, noise: f32) {
        let split = self.model.pick_position;
        let after = -split / (1.0 - split);
        let softness = self.model.softness;
        let cutoff = SOFTEST * (HARDEST / SOFTEST).powf(hardness.clamp(0.0, 1.0));
        let rounding = 1.0 - (-TAU * cutoff / self.sample_rate).exp();
        let shares = [1.0 - 0.5 * self.model.second, self.model.second];
        let mut source = self.noise;
        for (string, share) in self.loops.iter_mut().zip(shares) {
            let period = (string.period().round() as usize).max(2);
            let mut smooth = 0.0_f32;
            let strength = amplitude * share;
            string.reshape(period, |position, old| {
                let x = position as f32 / period as f32;
                // Both shapes have no offset: the pulse's two levels balance, the triangle
                // (the string's shape when the pick lets go) is centred.
                let pulse = if x < split { 1.0 } else { after };
                let triangle = if x < split {
                    x / split
                } else {
                    (1.0 - x) / (1.0 - split)
                };
                let shape = pulse * (1.0 - softness) + (2.0 * triangle - 1.0) * softness;
                let raw = shape * (1.0 - noise) + source.next() * noise;
                smooth += (raw - smooth) * rounding;
                old * contact + smooth * strength
            });
        }
        self.noise = source;
        self.peak = self.peak.max(amplitude);
        self.sounding = amplitude > 0.0 || self.sounding;
    }
}
