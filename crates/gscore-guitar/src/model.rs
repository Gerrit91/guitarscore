//! What kind of string a voice is: how long it sustains, how bright it is, how stiff, and
//! where and how hard it is picked. One preset per guitar model.

/// The guitar models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GuitarModel {
    /// Classical guitar: nylon strings, soft and short.
    Nylon,
    /// Acoustic guitar with steel strings: bright, long.
    Steel,
    /// Electric guitar: long sustain, read by a pickup (the amp comes later).
    Electric,
    /// Electric bass: long, round, stiff wound strings.
    Bass,
}

/// The parameters of one string model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StringModel {
    /// How long a ringing note takes to fall by 60 dB at 110 Hz, in seconds.
    pub sustain: f32,
    /// How the sustain changes with pitch: `sustain * (110 / f) ^ exponent`.
    pub sustain_exponent: f32,
    /// The loss filter's coefficient while ringing (0 bright to 0.9 dull).
    pub darkness: f32,
    /// Stiffness: the allpass coefficient (0 to -0.6; more negative is stiffer).
    pub stiffness: f32,
    /// How many allpass stages make the stiffness (0 to 4).
    pub stiffness_stages: usize,
    /// Where the pick strikes, as a share of the string from the bridge (0.05 to 0.5).
    pub pick_position: f32,
    /// How sharp the pick's attack is (0.05 soft finger to 1 hard pick).
    pub hardness: f32,
    /// Output level of the string.
    pub level: f32,
}

impl GuitarModel {
    /// The string model of this guitar.
    pub fn strings(self) -> StringModel {
        match self {
            GuitarModel::Nylon => StringModel {
                sustain: 3.0,
                sustain_exponent: 0.6,
                darkness: 0.35,
                stiffness: -0.05,
                stiffness_stages: 1,
                pick_position: 0.18,
                hardness: 0.25,
                level: 0.35,
            },
            GuitarModel::Steel => StringModel {
                sustain: 5.0,
                sustain_exponent: 0.5,
                darkness: 0.12,
                stiffness: -0.15,
                stiffness_stages: 2,
                pick_position: 0.13,
                hardness: 0.6,
                level: 0.3,
            },
            GuitarModel::Electric => StringModel {
                sustain: 8.0,
                sustain_exponent: 0.4,
                darkness: 0.18,
                stiffness: -0.12,
                stiffness_stages: 2,
                pick_position: 0.12,
                hardness: 0.7,
                level: 0.3,
            },
            GuitarModel::Bass => StringModel {
                sustain: 7.0,
                sustain_exponent: 0.3,
                darkness: 0.3,
                stiffness: -0.3,
                stiffness_stages: 4,
                pick_position: 0.2,
                hardness: 0.35,
                level: 0.45,
            },
        }
    }
}

impl StringModel {
    /// The ringing note's 60 dB decay time at `frequency`, in seconds.
    pub(crate) fn sustain_at(&self, frequency: f32) -> f32 {
        let ratio = (110.0 / frequency.max(1.0)).powf(self.sustain_exponent);
        (self.sustain * ratio).clamp(0.2, 30.0)
    }

    /// This model with every value in its usable range (a broken parameter must not break
    /// the sound).
    pub(crate) fn sanitized(self) -> Self {
        let finite = |value: f32, fallback: f32| if value.is_finite() { value } else { fallback };
        StringModel {
            sustain: finite(self.sustain, 4.0).clamp(0.05, 30.0),
            sustain_exponent: finite(self.sustain_exponent, 0.5).clamp(0.0, 2.0),
            darkness: finite(self.darkness, 0.2).clamp(0.0, 0.95),
            stiffness: finite(self.stiffness, 0.0).clamp(-0.6, 0.0),
            stiffness_stages: self
                .stiffness_stages
                .min(crate::string::MAX_STIFFNESS_STAGES),
            pick_position: finite(self.pick_position, 0.13).clamp(0.05, 0.5),
            hardness: finite(self.hardness, 0.5).clamp(0.05, 1.0),
            level: finite(self.level, 0.3).clamp(0.0, 2.0),
        }
    }
}
