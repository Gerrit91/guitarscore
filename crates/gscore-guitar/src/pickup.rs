//! A magnetic pickup: it hears the string at one spot, not at the bridge. A string's
//! vibration there is the wave running toward the bridge minus its reflection coming
//! back, which has travelled twice the distance to the bridge: a comb filter
//!
//! ```text
//!  string ─┬────────────────────────────────┬─(+)─► (coil resonance: the voice's tone)
//!          └─► delay 2·position·period ─(−)─┘
//! ```
//!
//! whose notches fall on the overtones that have a node under the pickup. That is the
//! difference between the pickups of one guitar: the bridge pickup (close to the bridge)
//! keeps the high overtones and thins the low ones, nasal and biting; the neck pickup (a
//! quarter of the way) loses every second overtone's neighbours and the octave, round
//! and warm. The delay follows the pitch, bends and slides included.
//!
//! A humbucker has two coils side by side, reading two spots about 18 mm apart (on a
//! 648 mm scale): the two combs averaged cancel more of the highest overtones, which is
//! its fatter, darker sound. The coil's own resonance (a peak at 2-5 kHz, then a steep
//! roll-off) is the voice's resonant tone filter after it.

use crate::delay::DelayLine;

/// The kind of pickup that hears the string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pickup {
    /// None: the string is heard at the bridge (an acoustic guitar's body).
    #[default]
    None,
    /// One coil: clear and bright, with a peak around 4-5 kHz.
    SingleCoil,
    /// Two coils side by side: fatter and darker, with a peak around 2-3 kHz.
    Humbucker,
}

/// Where the pickups of a typical electric guitar sit, as a share of the string from the
/// bridge.
pub mod positions {
    pub const BRIDGE: f32 = 0.09;
    pub const MIDDLE: f32 = 0.17;
    pub const NECK: f32 = 0.26;
}

/// The distance between a humbucker's two coils, as a share of the string.
const HUMBUCKER_GAP: f32 = 0.028;
/// Each comb's level, so a pickup is about as loud as the string heard at the bridge (a
/// comb doubles some overtones and cancels others: on average it gives sqrt(2) for one
/// coil, sqrt(1.5) for two).
const SINGLE_LEVEL: f32 = std::f32::consts::FRAC_1_SQRT_2;
const HUMBUCKER_LEVEL: f32 = 0.816_496_6;
/// How much of the way to a new pitch's delays the comb goes per block (a bend glides, a
/// new note jumps: see [`PickupComb::retune`]); within a block the delays hold, a fraction
/// of a sample apart at most while a string bends.
const FOLLOW: f32 = 0.5;

/// The comb of one string's pickup.
#[derive(Debug, Clone)]
pub(crate) struct PickupComb {
    line: DelayLine,
    /// The two coils' delays now and where they go, in samples.
    delays: [f32; 2],
    targets: [f32; 2],
}

impl PickupComb {
    /// A comb for a string whose longest period is `period` samples.
    pub(crate) fn new(period: usize) -> Self {
        // Twice the farthest coil from the bridge.
        let longest = (period as f32 * 2.0 * (0.5 + HUMBUCKER_GAP)).ceil() as usize;
        PickupComb {
            line: DelayLine::new(longest),
            delays: [2.0; 2],
            targets: [2.0; 2],
        }
    }

    /// Sets the delays for a note of `period` samples with the pickup at `position` (a
    /// share of the string from the bridge), once per block: at once for a new note
    /// (`jump`), otherwise gliding (a bend).
    pub(crate) fn retune(&mut self, period: f32, position: f32, jump: bool) {
        let position = position.clamp(0.02, 0.5);
        self.targets = [position, position + HUMBUCKER_GAP].map(|at| 2.0 * at * period);
        if jump {
            self.delays = self.targets;
        } else {
            for (delay, target) in self.delays.iter_mut().zip(self.targets) {
                *delay += (target - *delay) * FOLLOW;
            }
        }
    }

    /// One sample of the string through the pickup.
    pub(crate) fn process(&mut self, pickup: Pickup, x: f32) -> f32 {
        if pickup == Pickup::None {
            return x;
        }
        self.line.push(x);
        let [near, far] = self.delays;
        // The newest sample is one back, the delayed ones `delay` further. Linear
        // interpolation is enough for the reflection (it only dulls it a little at the
        // very top, where the coil's resonance rolls off anyway).
        let near_wave = self.line.read_linear(near + 1.0);
        if pickup == Pickup::SingleCoil {
            return (x - near_wave) * SINGLE_LEVEL;
        }
        let far_wave = self.line.read_linear(far + 1.0);
        (x - 0.5 * (near_wave + far_wave)) * HUMBUCKER_LEVEL
    }

    /// Silence in the delay.
    pub(crate) fn clear(&mut self) {
        self.line.clear();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use std::f32::consts::TAU;

    use super::*;

    const RATE: f32 = 48_000.0;

    /// The comb's gain for overtone `harmonic` of a 100 Hz note.
    fn gain(pickup: Pickup, position: f32, harmonic: f32) -> f32 {
        let period = RATE / 100.0;
        let mut comb = PickupComb::new(period as usize + 10);
        comb.retune(period, position, true);
        let frequency = 100.0 * harmonic;
        let frames = 9_600;
        let (mut sum_in, mut sum_out) = (0.0, 0.0);
        for i in 0..frames {
            let x = (TAU * frequency * i as f32 / RATE).sin();
            let y = comb.process(pickup, x);
            if i > 4_800 {
                sum_in += x * x;
                sum_out += y * y;
            }
        }
        (sum_out / sum_in).sqrt()
    }

    #[test]
    fn the_neck_pickup_hears_no_octave_and_the_bridge_pickup_keeps_the_highs() {
        // A quarter of the way along, the second overtone has a node under the pickup.
        let octave = gain(Pickup::SingleCoil, 0.25, 2.0);
        assert!(octave < 0.05, "{octave}");
        assert!(gain(Pickup::SingleCoil, 0.25, 1.0) > 1.2);
        // Close to the bridge the fundamental is thinned and the mid overtones (third,
        // fourth) come through; its first notch is far up, near the sixth.
        let low = gain(Pickup::SingleCoil, positions::BRIDGE, 1.0);
        let mid = gain(Pickup::SingleCoil, positions::BRIDGE, 3.0);
        assert!(mid > 1.5 * low, "{low} {mid}");
        assert!(
            gain(
                Pickup::SingleCoil,
                positions::BRIDGE,
                1.0 / (2.0 * positions::BRIDGE)
            ) < 0.1
        );
    }

    #[test]
    fn a_humbuckers_comb_is_shallower_in_the_highest_overtones() {
        // Overtones high enough that the two coils hear them out of step (for a 100 Hz
        // note around the ninth, where the coils are half a wave apart): a single coil's
        // comb swings between its notches and peaks, a humbucker's evens out (its darker
        // sound comes mostly from its coils' lower resonance, the tone filter after it).
        let spread = |pickup| {
            let gains: Vec<f32> = (7..12)
                .map(|n| gain(pickup, positions::BRIDGE, n as f32))
                .collect();
            let (low, high) = gains
                .iter()
                .fold((f32::MAX, 0.0_f32), |(lo, hi), &g| (lo.min(g), hi.max(g)));
            high - low
        };
        assert!(spread(Pickup::Humbucker) < 0.7 * spread(Pickup::SingleCoil));
    }

    #[test]
    fn without_a_pickup_nothing_changes() {
        for harmonic in [1.0, 3.0, 7.0] {
            assert!((gain(Pickup::None, 0.2, harmonic) - 1.0).abs() < 1e-3);
        }
    }
}
