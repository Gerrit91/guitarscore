//! The metronome's own click, rendered by the synthesizer instead of a SoundFont drum.
//!
//! General MIDI's "Metronome Click" (key 33) and "Metronome Bell" (key 34) on the
//! percussion channel trigger it. SoundFonts differ a lot there (and in the wood blocks):
//! some barely sound, others vanish under the guitars. Like Guitar Pro, the built-in
//! synthesizer plays a click of its own: a short wooden "tock", two damped partials as a
//! wood block rings, bright enough to cut through a mix without being shrill. The accent (the
//! bar's first beat) is a little higher and longer.

/// The General MIDI key of a plain click (Metronome Click).
pub(crate) const CLICK_KEY: u8 = 33;
/// The General MIDI key of the accented click (Metronome Bell).
pub(crate) const ACCENT_KEY: u8 = 34;

/// The fundamental of a plain click and of an accent, in hertz.
const CLICK_PITCH: f32 = 1_250.0;
const ACCENT_PITCH: f32 = 1_650.0;
/// A wood block's second mode, as a ratio to the first.
const OVERTONE: f32 = 2.76;
/// How fast the two partials die away (time constants in seconds); the accent rings a
/// little longer.
const BODY_DECAY: f32 = 0.022;
const ACCENT_DECAY: f32 = 0.032;
const OVERTONE_DECAY: f32 = 0.008;
/// The rise at the start, so the click has no hard edge (seconds).
const ATTACK: f32 = 0.0006;
/// The peak at full velocity; above the SoundFont's notes at the synthesizer's master
/// volume, as a metronome must be.
const PEAK: f32 = 0.5;
/// When the click has died away (seconds).
const LENGTH: f32 = 0.2;

/// One click sounding at a time (a new one replaces it).
#[derive(Debug, Clone)]
pub(crate) struct ClickVoice {
    sample_rate: f32,
    /// Samples since the click started; `None` when silent.
    age: Option<u32>,
    pitch: f32,
    decay: f32,
    gain: f32,
}

impl ClickVoice {
    /// A silent voice at `sample_rate`.
    pub(crate) fn new(sample_rate: u32) -> Self {
        ClickVoice {
            sample_rate: sample_rate.max(1) as f32,
            age: None,
            pitch: CLICK_PITCH,
            decay: BODY_DECAY,
            gain: 0.0,
        }
    }

    /// Whether `key` on the percussion channel is the metronome's (this voice plays it).
    pub(crate) fn plays(key: u8) -> bool {
        matches!(key, CLICK_KEY | ACCENT_KEY)
    }

    /// Starts a click: the accent for [`ACCENT_KEY`], at `velocity` (1-127).
    pub(crate) fn trigger(&mut self, key: u8, velocity: u8) {
        let accent = key == ACCENT_KEY;
        self.pitch = if accent { ACCENT_PITCH } else { CLICK_PITCH };
        self.decay = if accent { ACCENT_DECAY } else { BODY_DECAY };
        self.gain = PEAK * f32::from(velocity.min(127)) / 127.0;
        self.age = (velocity > 0).then_some(0);
    }

    /// Adds the click to both sides of the block.
    pub(crate) fn add(&mut self, left: &mut [f32], right: &mut [f32]) {
        let Some(mut age) = self.age else {
            return;
        };
        let end = (LENGTH * self.sample_rate) as u32;
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if age >= end {
                self.age = None;
                return;
            }
            let sample = self.sample(age);
            *l += sample;
            *r += sample;
            age = age.saturating_add(1);
        }
        self.age = Some(age);
    }

    /// The click `age` samples after its start.
    fn sample(&self, age: u32) -> f32 {
        let t = age as f32 / self.sample_rate;
        let rise = (t / ATTACK).min(1.0);
        let tau = std::f32::consts::TAU;
        let body = (tau * self.pitch * t).sin() * (-t / self.decay).exp();
        let overtone = (tau * self.pitch * OVERTONE * t).sin() * (-t / OVERTONE_DECAY).exp();
        self.gain * rise * (0.75 * body + 0.25 * overtone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(key: u8, velocity: u8) -> Vec<f32> {
        let mut voice = ClickVoice::new(44_100);
        voice.trigger(key, velocity);
        let mut left = vec![0.0; 44_100 / 4];
        let mut right = vec![0.0; 44_100 / 4];
        voice.add(&mut left, &mut right);
        assert_eq!(left, right, "the click is centered");
        left
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0_f32, |a, b| a.max(b.abs()))
    }

    #[test]
    fn a_click_is_short_clear_and_never_clips() {
        let click = render(CLICK_KEY, 127);
        let loud = peak(&click);
        assert!(loud > 0.3 && loud <= PEAK, "{loud}");
        // It starts softly (no hard edge) and has died away well before the next beat.
        assert!(click[0].abs() < 1e-6);
        let tail = &click[(0.1 * 44_100.0) as usize..];
        assert!(peak(tail) < 0.01 * loud);
    }

    #[test]
    fn the_velocity_sets_the_loudness_and_the_accent_differs() {
        let soft = peak(&render(CLICK_KEY, 40));
        let loud = peak(&render(CLICK_KEY, 120));
        assert!(soft < loud * 0.5);
        assert_ne!(render(ACCENT_KEY, 100), render(CLICK_KEY, 100));
        assert!(ClickVoice::plays(CLICK_KEY) && ClickVoice::plays(ACCENT_KEY));
        assert!(!ClickVoice::plays(76));
    }
}
