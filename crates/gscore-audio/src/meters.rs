//! The mixer's meters: the real level of every track, of the reverb bus and of the master,
//! measured in the audio thread after the faders and read by the user interface. Each is
//! a peak level per side that falls back slowly (20 dB in 1.7 s, as a mixing desk's
//! peak meter does), kept in atomics: the audio thread never waits for the interface.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::chains::MAX_TONES;

/// How long a peak takes to fall by 20 dB, in seconds.
const FALL_SECONDS: f32 = 1.7;

/// One meter: left and right peak, as linear levels (1 is 0 dBFS).
#[derive(Debug, Default)]
struct Meter {
    left: AtomicU32,
    right: AtomicU32,
}

impl Meter {
    fn get(&self) -> (f32, f32) {
        (
            f32::from_bits(self.left.load(Ordering::Relaxed)),
            f32::from_bits(self.right.load(Ordering::Relaxed)),
        )
    }

    fn set(&self, (left, right): (f32, f32)) {
        self.left.store(left.to_bits(), Ordering::Relaxed);
        self.right.store(right.to_bits(), Ordering::Relaxed);
    }
}

/// The levels of one engine, shared with whoever shows them.
#[derive(Debug)]
pub struct Meters {
    /// By the song's track index (tracks past [`MAX_TONES`] have none).
    tracks: Vec<Meter>,
    reverb: Meter,
    master: Meter,
}

impl Default for Meters {
    fn default() -> Self {
        Meters {
            tracks: (0..MAX_TONES).map(|_| Meter::default()).collect(),
            reverb: Meter::default(),
            master: Meter::default(),
        }
    }
}

impl Meters {
    /// The level of the song's track `track` after its fader: left and right peak, linear
    /// (1 is full scale; above 1 clips). Silent for a track that does not play.
    pub fn track(&self, track: usize) -> (f32, f32) {
        self.tracks.get(track).map_or((0.0, 0.0), Meter::get)
    }

    /// The level of the reverb bus after its fader.
    pub fn reverb(&self) -> (f32, f32) {
        self.reverb.get()
    }

    /// The level of the whole mix (before it is clipped to full scale).
    pub fn master(&self) -> (f32, f32) {
        self.master.get()
    }
}

/// Where the audio thread keeps a meter's falling peak between blocks.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Peak {
    left: f32,
    right: f32,
}

/// The per-block factor that makes a peak fall by 20 dB in [`FALL_SECONDS`].
pub(crate) fn fall_per_block(sample_rate: u32, block: usize) -> f32 {
    let seconds = block as f32 / sample_rate.max(1) as f32;
    0.1_f32.powf(seconds / FALL_SECONDS)
}

impl Peak {
    /// The peak after a block whose loudest samples were `left` and `right`.
    pub(crate) fn update(&mut self, left: f32, right: f32, fall: f32) {
        let next = |held: f32, now: f32| {
            let fallen = held * fall;
            // Exactly zero once far below anything shown, so a silent meter rests.
            let fallen = if fallen < 1e-6 { 0.0 } else { fallen };
            if now.is_finite() {
                now.max(fallen)
            } else {
                fallen
            }
        };
        self.left = next(self.left, left);
        self.right = next(self.right, right);
    }

    fn pair(self) -> (f32, f32) {
        (self.left, self.right)
    }
}

/// What the audio thread writes: a track's, the reverb bus's or the master's peak.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Which {
    Track(usize),
    Reverb,
    Master,
}

impl Meters {
    /// Publishes `peak` for `which` (from the audio thread; never blocks).
    pub(crate) fn publish(&self, which: Which, peak: Peak) {
        let meter = match which {
            Which::Track(track) => self.tracks.get(track),
            Which::Reverb => Some(&self.reverb),
            Which::Master => Some(&self.master),
        };
        if let Some(meter) = meter {
            meter.set(peak.pair());
        }
    }
}

/// The loudest sample of `samples` (magnitude).
pub(crate) fn peak_of(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |peak, x| peak.max(x.abs()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peak_holds_the_loudest_and_falls_twenty_db_in_its_time() {
        let fall = fall_per_block(48_000, 64);
        let mut peak = Peak::default();
        peak.update(0.5, 0.25, fall);
        assert_eq!(peak.pair(), (0.5, 0.25));
        // A second of silence: 20 dB in 1.7 s, so about 11.8 dB down.
        for _ in 0..750 {
            peak.update(0.0, 0.0, fall);
        }
        let db = 20.0 * (peak.pair().0 / 0.5).log10();
        assert!((db + 11.76).abs() < 0.2, "{db}");
        // A louder block takes over at once; NaN is ignored.
        peak.update(0.9, f32::NAN, fall);
        assert_eq!(peak.pair().0, 0.9);
        assert!(peak.pair().1.is_finite());
    }

    #[test]
    fn meters_are_read_where_they_are_written() {
        let meters = Meters::default();
        let mut peak = Peak::default();
        peak.update(0.7, 0.3, 1.0);
        meters.publish(Which::Track(3), peak);
        meters.publish(Which::Master, peak);
        assert_eq!(meters.track(3), (0.7, 0.3));
        assert_eq!(meters.track(2), (0.0, 0.0));
        assert_eq!(meters.track(MAX_TONES + 5), (0.0, 0.0));
        assert_eq!(meters.master(), (0.7, 0.3));
        assert_eq!(meters.reverb(), (0.0, 0.0));
    }
}
