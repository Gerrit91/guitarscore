//! What kind of string a voice is: how long it sustains (low and high harmonics apart),
//! how stiff it is, where and how it is picked, and the tone of the pickup or body that
//! hears it. One preset per guitar model; every value is a knob the user can turn.

/// The guitar models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GuitarModel {
    /// Classical guitar: nylon strings, soft and short.
    Nylon,
    /// Acoustic guitar with steel strings: bright, long.
    Steel,
    /// Electric guitar: long sustain, read by a pickup (an amp comes in its chain).
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
    /// How long the harmonics around 2 kHz take to fall by 60 dB, in seconds, whatever the
    /// note: short is mellow, long is bright and metallic.
    pub brilliance: f32,
    /// Stiffness: the allpass coefficient (0 to -0.6; more negative is stiffer and more
    /// metallic).
    pub stiffness: f32,
    /// How many allpass stages make the stiffness (0 to 4).
    pub stiffness_stages: usize,
    /// Where the pick strikes, as a share of the string from the bridge (0.05 to 0.5):
    /// near the bridge is thin and twangy, toward the middle round and hollow.
    pub pick_position: f32,
    /// How hard the pick is (0 a soft thumb to 1 a hard pick): how bright the strike.
    pub hardness: f32,
    /// The shape of the strike (0 a sharp snap, buzzy; 1 a rounded pluck, soft).
    pub softness: f32,
    /// The pickup's or body's tone: a lowpass at this many Hz.
    pub tone: f32,
    /// Its resonance (0.5 flat to 4 a strong peak at the tone frequency).
    pub resonance: f32,
    /// Output level of the string.
    pub level: f32,
    /// How much of the output is the force on the bridge (the string's velocity, which a
    /// bridge or a magnetic pickup passes) rather than its swing: 0 full and round, 1 lean
    /// in the lows like a real guitar.
    pub bridge: f32,
    /// The body's or pickup's low cut, in Hz.
    pub low_cut: f32,
    /// The second direction of vibration's share (0 off: one perfectly periodic loop).
    pub second: f32,
    /// How far the second vibration is detuned, in cents: the beating of a long note.
    pub detune: f32,
    /// The magnetic pickup that hears the string (none: heard at the bridge), see
    /// [`crate::Pickup`]...
    pub pickup: crate::Pickup,
    /// ...and where it sits, as a share of the string from the bridge
    /// ([`crate::pickup_positions`]).
    pub pickup_position: f32,
    /// How hard a palm mutes (0 light to 1 heavy; only palm-muted notes): light lets the
    /// note ring on with some of its brightness, heavy is a short, dark chug.
    pub palm_mute: f32,
    /// How loud the pick's click and scrape are (0 none to 1 full; scaled further by how
    /// hard the strike is).
    pub pick_noise: f32,
    /// How sharp a full-strength pluck starts, in cents (it settles within about 50 ms;
    /// softer plucks start less sharp, by the square of their strength).
    pub attack_pitch: f32,
    /// How inharmonic the strings are: the inharmonicity coefficient `B` of an open
    /// string tuned to 110 Hz (a partial `n` sounds at `n f sqrt(1 + B n²)`). Lower open
    /// strings get more and higher ones less (a heavier gauge), and a fretted note more,
    /// with the square of how much shorter the string is; see
    /// [`StringModel::inharmonicity_at`]. Measured on an electric guitar (2026-11-21):
    /// about 1e-4. 0 leaves the stiffness to the plain `stiffness` allpasses.
    pub inharmonicity: f32,
    /// A fretted note's sustain as a share of an open string's at the same pitch (the
    /// fingertip on the fret takes energy out of the string): measured about 0.6.
    pub fretted_sustain: f32,
    /// Where the string's losses stop growing with the pitch, Hz: above it the treble rings
    /// about as long as the highs around 2 kHz (a real string's top does; measured about
    /// 1.2 kHz on an electric guitar, 2026-11-22). 0 for losses that keep growing (the
    /// treble dies much sooner than the highs).
    pub treble_corner: f32,
}

impl GuitarModel {
    /// The string model of this guitar.
    pub fn strings(self) -> StringModel {
        match self {
            GuitarModel::Nylon => StringModel {
                sustain: 3.0,
                sustain_exponent: 0.6,
                brilliance: 0.3,
                stiffness: -0.03,
                stiffness_stages: 1,
                pick_position: 0.18,
                hardness: 0.25,
                softness: 0.3,
                tone: 3_500.0,
                resonance: 0.7,
                level: 0.4,
                bridge: 0.5,
                low_cut: 70.0,
                second: 0.4,
                detune: 1.0,
                pickup: crate::Pickup::None,
                pickup_position: crate::pickup_positions::BRIDGE,
                palm_mute: PALM_MUTE,
                pick_noise: PICK_NOISE,
                attack_pitch: ATTACK_PITCH,
                inharmonicity: 0.3e-4,
                fretted_sustain: 0.7,
                treble_corner: 0.0,
            },
            GuitarModel::Steel => StringModel {
                sustain: 9.0,
                sustain_exponent: 0.2,
                brilliance: 1.8,
                stiffness: -0.5,
                stiffness_stages: 3,
                pick_position: 0.2,
                hardness: 0.9,
                softness: 0.0,
                tone: 16_000.0,
                resonance: 1.8,
                level: 0.4,
                bridge: 0.85,
                low_cut: 110.0,
                second: 0.5,
                detune: 1.5,
                pickup: crate::Pickup::None,
                pickup_position: crate::pickup_positions::BRIDGE,
                palm_mute: PALM_MUTE,
                pick_noise: PICK_NOISE,
                attack_pitch: ATTACK_PITCH,
                inharmonicity: 0.8e-4,
                fretted_sustain: 0.6,
                treble_corner: 0.0,
            },
            // Fitted to the owner's reference recordings (2026-11-21: a Schecter with a
            // high-output bridge humbucker, its DI against these strings note by note:
            // the spectrum and decay of each partial, open, fretted and palm-muted).
            GuitarModel::Electric => StringModel {
                sustain: 15.3,
                sustain_exponent: 0.43,
                brilliance: 2.7,
                stiffness: -0.06,
                stiffness_stages: 2,
                pick_position: 0.087,
                hardness: 0.68,
                softness: 0.27,
                // A loaded high-output humbucker's coil resonance: a soft peak near
                // 1.4 kHz, then a steep roll-off.
                tone: 1_400.0,
                resonance: 1.43,
                // The level the amps' input is calibrated for. (The owner's DI came in
                // 8 dB hotter, 2026-11-22: its preamp's setting; the tones compared with
                // it raise their string level to 0.88.)
                level: 0.35,
                bridge: 0.93,
                low_cut: 68.0,
                second: 0.05,
                detune: 1.0,
                // A humbucker at the bridge, as most rock guitars are played.
                pickup: crate::Pickup::Humbucker,
                // The fit put it at 0.10; the bridge's standard place is as close.
                pickup_position: crate::pickup_positions::BRIDGE,
                palm_mute: PALM_MUTE,
                pick_noise: PICK_NOISE,
                attack_pitch: ATTACK_PITCH,
                inharmonicity: INHARMONICITY,
                fretted_sustain: FRETTED_SUSTAIN,
                treble_corner: TREBLE_CORNER,
            },
            GuitarModel::Bass => StringModel {
                sustain: 7.0,
                sustain_exponent: 0.3,
                brilliance: 0.35,
                stiffness: -0.12,
                stiffness_stages: 3,
                pick_position: 0.2,
                hardness: 0.3,
                softness: 0.3,
                tone: 2_500.0,
                resonance: 0.9,
                level: 0.5,
                bridge: 0.5,
                low_cut: 30.0,
                second: 0.3,
                detune: 0.8,
                pickup: crate::Pickup::None,
                pickup_position: crate::pickup_positions::BRIDGE,
                palm_mute: PALM_MUTE,
                pick_noise: PICK_NOISE,
                attack_pitch: ATTACK_PITCH,
                inharmonicity: 1.5e-4,
                fretted_sustain: 0.6,
                treble_corner: 0.0,
            },
        }
    }
}

/// The palm mute of every preset: the middle of the knob.
const PALM_MUTE: f32 = 0.5;
/// The pick's click and the sharp start of every preset (a sound saved before these knobs
/// gets them too, whatever its kind; the model's hardness already makes a nylon's or a
/// bass's click softer and duller).
const PICK_NOISE: f32 = 0.5;
const ATTACK_PITCH: f32 = 4.0;
/// The electric guitar's inharmonicity and fretted sustain, measured on the owner's
/// reference recordings (2026-11-21, `docs/reference-recordings.md`).
const INHARMONICITY: f32 = 1.0e-4;
const FRETTED_SUSTAIN: f32 = 0.58;
/// The electric guitar's treble corner, Hz (fitted 2026-11-22).
const TREBLE_CORNER: f32 = 1_120.0;
/// How the inharmonicity of an open string follows its pitch: `(110 / f) ^ this` (a
/// lower string is a heavier gauge). Measured: low E 1.5e-4, high E 0.2e-4 (2026-11-21).
const INHARMONICITY_EXPONENT: f32 = 1.4;

/// The range of every knob, for user interfaces and for [`StringModel::sanitized`]:
/// `(lowest, highest)`.
pub mod ranges {
    /// Seconds.
    pub const SUSTAIN: (f32, f32) = (0.3, 20.0);
    pub const SUSTAIN_EXPONENT: (f32, f32) = (0.0, 1.5);
    /// Seconds.
    pub const BRILLIANCE: (f32, f32) = (0.02, 5.0);
    pub const STIFFNESS: (f32, f32) = (-0.6, 0.0);
    pub const STIFFNESS_STAGES: (usize, usize) = (0, 4);
    pub const PICK_POSITION: (f32, f32) = (0.05, 0.5);
    pub const HARDNESS: (f32, f32) = (0.0, 1.0);
    pub const SOFTNESS: (f32, f32) = (0.0, 1.0);
    /// Hz.
    pub const TONE: (f32, f32) = (500.0, 16_000.0);
    pub const RESONANCE: (f32, f32) = (0.5, 4.0);
    pub const LEVEL: (f32, f32) = (0.0, 2.0);
    pub const BRIDGE: (f32, f32) = (0.0, 1.0);
    /// Hz.
    pub const LOW_CUT: (f32, f32) = (20.0, 400.0);
    pub const SECOND: (f32, f32) = (0.0, 1.0);
    /// Cents.
    pub const DETUNE: (f32, f32) = (0.0, 10.0);
    /// Share of the string from the bridge.
    pub const PICKUP_POSITION: (f32, f32) = (0.04, 0.35);
    /// Light to heavy.
    pub const PALM_MUTE: (f32, f32) = (0.0, 1.0);
    pub const PICK_NOISE: (f32, f32) = (0.0, 1.0);
    /// Cents.
    pub const ATTACK_PITCH: (f32, f32) = (0.0, 20.0);
    /// The coefficient `B` of an open string at 110 Hz.
    pub const INHARMONICITY: (f32, f32) = (0.0, 2.0e-3);
    /// Share of the open string's sustain.
    pub const FRETTED_SUSTAIN: (f32, f32) = (0.2, 1.0);
    pub const TREBLE_CORNER: (f32, f32) = (0.0, 1_900.0);
}

impl StringModel {
    /// The ringing note's 60 dB decay time at `frequency`, in seconds.
    pub(crate) fn sustain_at(&self, frequency: f32) -> f32 {
        let ratio = (110.0 / frequency.max(1.0)).powf(self.sustain_exponent);
        (self.sustain * ratio).clamp(0.2, 30.0)
    }

    /// The inharmonicity coefficient `B` of a note at `frequency` on a string whose open
    /// pitch is `open` (both Hz): the open string's, from its pitch, times the square of how
    /// much shorter the fretting makes it.
    pub fn inharmonicity_at(&self, open: f32, frequency: f32) -> f32 {
        let open = open.clamp(8.0, 5_000.0);
        let frequency = frequency.clamp(open, 20_000.0);
        let string = self.inharmonicity * (110.0 / open).powf(INHARMONICITY_EXPONENT);
        (string * (frequency / open).powi(2)).clamp(0.0, 0.05)
    }

    /// This model with every value in its usable range (a broken parameter must not break
    /// the sound).
    pub fn sanitized(self) -> Self {
        let fit = |value: f32, (low, high): (f32, f32), fallback: f32| {
            if value.is_finite() {
                value.clamp(low, high)
            } else {
                fallback
            }
        };
        StringModel {
            sustain: fit(self.sustain, ranges::SUSTAIN, 4.0),
            sustain_exponent: fit(self.sustain_exponent, ranges::SUSTAIN_EXPONENT, 0.5),
            brilliance: fit(self.brilliance, ranges::BRILLIANCE, 0.5),
            stiffness: fit(self.stiffness, ranges::STIFFNESS, 0.0),
            stiffness_stages: self.stiffness_stages.min(ranges::STIFFNESS_STAGES.1),
            pick_position: fit(self.pick_position, ranges::PICK_POSITION, 0.13),
            hardness: fit(self.hardness, ranges::HARDNESS, 0.5),
            softness: fit(self.softness, ranges::SOFTNESS, 0.5),
            tone: fit(self.tone, ranges::TONE, 5_000.0),
            resonance: fit(self.resonance, ranges::RESONANCE, 0.7),
            level: fit(self.level, ranges::LEVEL, 0.35),
            bridge: fit(self.bridge, ranges::BRIDGE, 0.5),
            low_cut: fit(self.low_cut, ranges::LOW_CUT, 60.0),
            second: fit(self.second, ranges::SECOND, 0.4),
            detune: fit(self.detune, ranges::DETUNE, 1.0),
            pickup: self.pickup,
            pickup_position: fit(
                self.pickup_position,
                ranges::PICKUP_POSITION,
                crate::pickup_positions::BRIDGE,
            ),
            palm_mute: fit(self.palm_mute, ranges::PALM_MUTE, PALM_MUTE),
            pick_noise: fit(self.pick_noise, ranges::PICK_NOISE, PICK_NOISE),
            attack_pitch: fit(self.attack_pitch, ranges::ATTACK_PITCH, ATTACK_PITCH),
            inharmonicity: fit(self.inharmonicity, ranges::INHARMONICITY, 0.0),
            fretted_sustain: fit(self.fretted_sustain, ranges::FRETTED_SUSTAIN, 1.0),
            treble_corner: fit(self.treble_corner, ranges::TREBLE_CORNER, 0.0),
        }
    }
}
