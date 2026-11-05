//! The metronome's own click, rendered by the synthesizer instead of a SoundFont drum.
//!
//! General MIDI's "Metronome Click" (key 33) and "Metronome Bell" (key 34) on the
//! percussion channel trigger it. SoundFonts differ a lot there (and in the wood blocks):
//! some barely sound, others vanish under the guitars. Like Guitar Pro, the built-in
//! synthesizer plays a click of its own, bright enough to cut through a mix without being
//! shrill. Controller [`CLICK_SOUND_CONTROLLER`] on the percussion channel chooses the sound
//! ([`ClickSound`]): a wooden "tock", an electronic beep or a cowbell, each two damped
//! partials. The accent (the bar's first beat) is a little higher and longer.

use gscore_play::{CLICK_SOUND_CONTROLLER, ClickSound};

/// The General MIDI key of a plain click (Metronome Click).
pub(crate) const CLICK_KEY: u8 = 33;
/// The General MIDI key of the accented click (Metronome Bell).
pub(crate) const ACCENT_KEY: u8 = 34;

/// The rise at the start, so the click has no hard edge (seconds).
const ATTACK: f32 = 0.0006;
/// The peak at full velocity; above the SoundFont's notes at the synthesizer's master
/// volume, as a metronome must be.
const PEAK: f32 = 0.5;
/// When the click has died away (seconds).
const LENGTH: f32 = 0.25;

/// How one sound is made: a body partial and a second one above it, each dying away.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Recipe {
    /// The body's pitch for a plain click and for an accent (hertz).
    pitch: f32,
    accent_pitch: f32,
    /// How fast the body dies away (time constant in seconds), plain and accented.
    decay: f32,
    accent_decay: f32,
    /// The second partial: its pitch as a ratio to the body's, its share and its decay.
    ratio: f32,
    share: f32,
    second_decay: f32,
}

impl Recipe {
    fn of(sound: ClickSound) -> Self {
        match sound {
            // A wood block: its second mode 2.76 times up, gone almost at once.
            ClickSound::Wood => Recipe {
                pitch: 1_250.0,
                accent_pitch: 1_650.0,
                decay: 0.022,
                accent_decay: 0.032,
                ratio: 2.76,
                share: 0.25,
                second_decay: 0.008,
            },
            // A clean sine with a faint octave, a little longer.
            ClickSound::Beep => Recipe {
                pitch: 1_000.0,
                accent_pitch: 1_500.0,
                decay: 0.035,
                accent_decay: 0.045,
                ratio: 2.0,
                share: 0.1,
                second_decay: 0.02,
            },
            // A cowbell's two inharmonic tones (as a drum machine's), lower and rounder.
            ClickSound::Cowbell => Recipe {
                pitch: 560.0,
                accent_pitch: 700.0,
                decay: 0.05,
                accent_decay: 0.07,
                ratio: 1.48,
                share: 0.6,
                second_decay: 0.04,
            },
        }
    }
}

/// One click sounding at a time (a new one replaces it).
#[derive(Debug, Clone)]
pub(crate) struct ClickVoice {
    sample_rate: f32,
    /// The chosen sound.
    sound: ClickSound,
    /// Samples since the click started; `None` when silent.
    age: Option<u32>,
    pitch: f32,
    decay: f32,
    gain: f32,
}

impl ClickVoice {
    /// A silent voice at `sample_rate`, with the default sound.
    pub(crate) fn new(sample_rate: u32) -> Self {
        let recipe = Recipe::of(ClickSound::default());
        ClickVoice {
            sample_rate: sample_rate.max(1) as f32,
            sound: ClickSound::default(),
            age: None,
            pitch: recipe.pitch,
            decay: recipe.decay,
            gain: 0.0,
        }
    }

    /// Whether `key` on the percussion channel is the metronome's (this voice plays it).
    pub(crate) fn plays(key: u8) -> bool {
        matches!(key, CLICK_KEY | ACCENT_KEY)
    }

    /// Whether `controller` on the percussion channel chooses the sound (this voice
    /// takes it).
    pub(crate) fn chooses(controller: u8) -> bool {
        controller == CLICK_SOUND_CONTROLLER
    }

    /// Chooses the sound by its controller `value`, for the next clicks.
    pub(crate) fn choose(&mut self, value: u8) {
        self.sound = ClickSound::of_value(value);
    }

    /// Starts a click: the accent for [`ACCENT_KEY`], at `velocity` (1-127).
    pub(crate) fn trigger(&mut self, key: u8, velocity: u8) {
        let accent = key == ACCENT_KEY;
        let recipe = Recipe::of(self.sound);
        self.pitch = if accent {
            recipe.accent_pitch
        } else {
            recipe.pitch
        };
        self.decay = if accent {
            recipe.accent_decay
        } else {
            recipe.decay
        };
        self.gain = PEAK * f32::from(velocity.min(127)) / 127.0;
        self.age = (velocity > 0).then_some(0);
    }

    /// Adds the click to both sides of the block.
    pub(crate) fn add(&mut self, left: &mut [f32], right: &mut [f32]) {
        let Some(mut age) = self.age else {
            return;
        };
        let end = (LENGTH * self.sample_rate) as u32;
        let recipe = Recipe::of(self.sound);
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if age >= end {
                self.age = None;
                return;
            }
            let sample = self.sample(age, &recipe);
            *l += sample;
            *r += sample;
            age = age.saturating_add(1);
        }
        self.age = Some(age);
    }

    /// The click `age` samples after its start.
    fn sample(&self, age: u32, recipe: &Recipe) -> f32 {
        let t = age as f32 / self.sample_rate;
        let rise = (t / ATTACK).min(1.0);
        let tau = std::f32::consts::TAU;
        let body = (tau * self.pitch * t).sin() * (-t / self.decay).exp();
        let second = (tau * self.pitch * recipe.ratio * t).sin() * (-t / recipe.second_decay).exp();
        // The two shares add up to 1, so no sound is louder than the peak.
        let share = recipe.share.clamp(0.0, 1.0);
        self.gain * rise * ((1.0 - share) * body + share * second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(sound: ClickSound, key: u8, velocity: u8) -> Vec<f32> {
        let mut voice = ClickVoice::new(44_100);
        voice.choose(sound.value());
        voice.trigger(key, velocity);
        let mut left = vec![0.0; 44_100 / 2];
        let mut right = vec![0.0; 44_100 / 2];
        voice.add(&mut left, &mut right);
        assert_eq!(left, right, "the click is centered");
        left
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0_f32, |a, b| a.max(b.abs()))
    }

    #[test]
    fn every_click_is_short_clear_and_never_clips() {
        for sound in ClickSound::ALL {
            let click = render(sound, CLICK_KEY, 127);
            let loud = peak(&click);
            assert!(loud > 0.3 && loud <= PEAK, "{sound:?}: {loud}");
            // It starts softly (no hard edge) and has died away before a fast beat.
            assert!(click[0].abs() < 1e-6);
            let tail = &click[(0.2 * 44_100.0) as usize..];
            assert!(peak(tail) < 0.05 * loud, "{sound:?}");
        }
    }

    #[test]
    fn the_velocity_sets_the_loudness_and_sounds_and_accents_differ() {
        let soft = peak(&render(ClickSound::Wood, CLICK_KEY, 40));
        let loud = peak(&render(ClickSound::Wood, CLICK_KEY, 120));
        assert!(soft < loud * 0.5);
        assert_ne!(
            render(ClickSound::Wood, ACCENT_KEY, 100),
            render(ClickSound::Wood, CLICK_KEY, 100)
        );
        assert_ne!(
            render(ClickSound::Beep, CLICK_KEY, 100),
            render(ClickSound::Cowbell, CLICK_KEY, 100)
        );
        assert!(ClickVoice::plays(CLICK_KEY) && ClickVoice::plays(ACCENT_KEY));
        assert!(!ClickVoice::plays(76));
        assert!(ClickVoice::chooses(CLICK_SOUND_CONTROLLER));
    }
}
