//! The mixer's meters: the real level of every track, of the reverb bus and of the master,
//! measured in the audio thread after the faders and read by the user interface, kept in
//! atomics: the audio thread never waits for the interface.
//!
//! Each side of a meter has the two marks of a mixing desk's peak meter:
//!
//! - **the bar**, which jumps up to a peak at once, holds it for [`BAR_HOLD`] seconds and
//!   then falls slowly (20 dB in [`FALL_SECONDS`]), so it reads calmly instead of
//!   flickering with every note;
//! - **the peak mark**, a line at the highest level of the last [`MARK_HOLD`] seconds,
//!   which then drops quickly to the bar.

use std::sync::atomic::{AtomicU32, Ordering};

use rustysynth::PARTS;

use crate::chains::MAX_TONES;

/// How long the bar holds a peak, in seconds, before it falls.
const BAR_HOLD: f32 = 0.3;
/// How long the bar takes to fall by 20 dB, in seconds.
const FALL_SECONDS: f32 = 1.7;
/// How long the peak mark stays, in seconds, before it drops.
const MARK_HOLD: f32 = 1.5;
/// How long the peak mark takes to drop by 20 dB once it goes, in seconds.
const MARK_FALL_SECONDS: f32 = 0.3;
/// Below this a falling level is shown as silence (-120 dB).
const FLOOR: f32 = 1e-6;
/// How quickly a compressor's gain-reduction needle returns toward 0 dB (time constant,
/// seconds), as the needle of a studio compressor does.
const NEEDLE_RETURN: f32 = 0.3;

/// One meter's reading, as linear levels (1 is 0 dBFS; above 1 clips).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Level {
    /// The bars, left and right.
    pub left: f32,
    pub right: f32,
    /// The peak marks, left and right (never below the bars).
    pub mark_left: f32,
    pub mark_right: f32,
}

/// One meter as the interface reads it.
#[derive(Debug, Default)]
struct Meter {
    values: [AtomicU32; 4],
}

impl Meter {
    fn get(&self) -> Level {
        let [left, right, mark_left, mark_right] = self
            .values
            .each_ref()
            .map(|value| f32::from_bits(value.load(Ordering::Relaxed)));
        Level {
            left,
            right,
            mark_left,
            mark_right,
        }
    }

    fn set(&self, level: Level) {
        let values = [level.left, level.right, level.mark_left, level.mark_right];
        for (slot, value) in self.values.iter().zip(values) {
            slot.store(value.to_bits(), Ordering::Relaxed);
        }
    }
}

/// The levels of one engine, shared with whoever shows them.
#[derive(Debug)]
pub struct Meters {
    /// By the song's track index (tracks past [`MAX_TONES`] have none).
    tracks: Vec<Meter>,
    /// Each track's input, before its chain (and its trim): by the song's track index.
    inputs: Vec<Meter>,
    /// Each track's chain output, after the chain (and its output level), before the
    /// fader: by the song's track index.
    outputs: Vec<Meter>,
    /// A drum track's parts, by the track's index and then the part's.
    parts: Vec<Meter>,
    reverb: Meter,
    compressor: Meter,
    chorus: Meter,
    master: Meter,
    /// The gain reduction of each track's compressors and of the compressor bus, in dB
    /// (0 or below; `f32` bits).
    reductions: Vec<AtomicU32>,
    bus_reduction: AtomicU32,
}

impl Default for Meters {
    fn default() -> Self {
        Meters {
            tracks: (0..MAX_TONES).map(|_| Meter::default()).collect(),
            inputs: (0..MAX_TONES).map(|_| Meter::default()).collect(),
            outputs: (0..MAX_TONES).map(|_| Meter::default()).collect(),
            parts: (0..MAX_TONES.saturating_mul(PARTS))
                .map(|_| Meter::default())
                .collect(),
            reverb: Meter::default(),
            compressor: Meter::default(),
            chorus: Meter::default(),
            master: Meter::default(),
            reductions: (0..MAX_TONES).map(|_| AtomicU32::new(0)).collect(),
            bus_reduction: AtomicU32::new(0),
        }
    }
}

impl Meters {
    /// The level of the song's track `track` after its fader. Silent for a track that does
    /// not play.
    pub fn track(&self, track: usize) -> Level {
        self.tracks.get(track).map_or(Level::default(), Meter::get)
    }

    /// The level of `part` of the song's drum track `track`, after its chain, level and
    /// mute. Silent for a track that does not play or a part that does not use the meter.
    pub fn part(&self, track: usize, part: usize) -> Level {
        track
            .checked_mul(PARTS)
            .and_then(|index| index.checked_add(part))
            .and_then(|index| self.parts.get(index))
            .map_or(Level::default(), Meter::get)
    }

    /// The level of the song's track `track` before its chain (after its input trim), for
    /// setting how hard an amp or compressor is driven.
    pub fn input(&self, track: usize) -> Level {
        self.inputs.get(track).map_or(Level::default(), Meter::get)
    }

    /// The level of the song's track `track` after its chain (and its output level),
    /// before the fader.
    pub fn chain_output(&self, track: usize) -> Level {
        self.outputs.get(track).map_or(Level::default(), Meter::get)
    }

    /// The level of the reverb bus after its fader.
    pub fn reverb(&self) -> Level {
        self.reverb.get()
    }

    /// The level of the compressor bus after its fader.
    pub fn compressor(&self) -> Level {
        self.compressor.get()
    }

    /// The level of the chorus bus after its fader.
    pub fn chorus(&self) -> Level {
        self.chorus.get()
    }

    /// The level of the whole mix (before it is clipped to full scale).
    pub fn master(&self) -> Level {
        self.master.get()
    }

    /// How far the compressors in the song's track `track`'s chain turn it down, in dB (0
    /// or below), moving like a compressor's needle.
    pub fn reduction(&self, track: usize) -> f32 {
        self.reductions
            .get(track)
            .map_or(0.0, |value| f32::from_bits(value.load(Ordering::Relaxed)))
    }

    /// How far the compressor bus turns its sum down, in dB (0 or below).
    pub fn bus_reduction(&self) -> f32 {
        f32::from_bits(self.bus_reduction.load(Ordering::Relaxed))
    }

    /// Whether any meter still shows something (an interface keeps redrawing until all
    /// have come to rest).
    pub fn moving(&self) -> bool {
        let shows = |meter: &Meter| meter.get().mark_left > 0.0 || meter.get().mark_right > 0.0;
        let reducing = |value: &AtomicU32| f32::from_bits(value.load(Ordering::Relaxed)) < 0.0;
        shows(&self.master)
            || shows(&self.reverb)
            || shows(&self.compressor)
            || shows(&self.chorus)
            || self.tracks.iter().any(shows)
            || self.parts.iter().any(shows)
            || reducing(&self.bus_reduction)
            || self.reductions.iter().any(reducing)
    }
}

/// How the audio thread moves a meter's marks block by block.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ballistics {
    bar_hold: u32,
    bar_fall: f32,
    mark_hold: u32,
    mark_fall: f32,
    needle_return: f32,
}

impl Ballistics {
    /// The ballistics for blocks of `block` frames at `sample_rate`.
    pub(crate) fn new(sample_rate: u32, block: usize) -> Self {
        let seconds = block as f32 / sample_rate.max(1) as f32;
        let blocks = |hold: f32| (hold / seconds.max(1e-6)).ceil() as u32;
        Ballistics {
            bar_hold: blocks(BAR_HOLD),
            bar_fall: 0.1_f32.powf(seconds / FALL_SECONDS),
            mark_hold: blocks(MARK_HOLD),
            mark_fall: 0.1_f32.powf(seconds / MARK_FALL_SECONDS),
            needle_return: (-seconds / NEEDLE_RETURN).exp(),
        }
    }
}

/// One side's falling value and how many more blocks it holds.
#[derive(Debug, Clone, Copy, Default)]
struct Held {
    value: f32,
    hold: u32,
}

impl Held {
    fn update(&mut self, now: f32, hold: u32, fall: f32) {
        let now = if now.is_finite() { now } else { 0.0 };
        if now >= self.value {
            self.value = now;
            self.hold = hold;
        } else if self.hold > 0 {
            self.hold -= 1;
        } else {
            let fallen = (self.value * fall).max(now);
            // Exactly zero once far below anything shown, so a silent meter rests.
            self.value = if fallen < FLOOR { 0.0 } else { fallen };
        }
    }
}

/// Where the audio thread keeps a meter between blocks.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Peak {
    bars: [Held; 2],
    marks: [Held; 2],
}

impl Peak {
    /// The meter after a block whose loudest samples were `left` and `right`.
    pub(crate) fn update(&mut self, left: f32, right: f32, ballistics: Ballistics) {
        for ((bar, mark), now) in self.bars.iter_mut().zip(&mut self.marks).zip([left, right]) {
            bar.update(now, ballistics.bar_hold, ballistics.bar_fall);
            mark.update(now, ballistics.mark_hold, ballistics.mark_fall);
            // The mark never shows below the bar.
            mark.value = mark.value.max(bar.value);
        }
    }

    fn level(self) -> Level {
        let [left, right] = self.bars.map(|bar| bar.value);
        let [mark_left, mark_right] = self.marks.map(|mark| mark.value);
        Level {
            left,
            right,
            mark_left,
            mark_right,
        }
    }
}

/// A gain-reduction needle: it jumps to the deepest reduction of a block and returns
/// toward 0 dB smoothly.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Needle {
    db: f32,
}

impl Needle {
    /// The needle after a block whose deepest reduction was `deepest` dB (0 or below).
    pub(crate) fn update(&mut self, deepest: f32, ballistics: Ballistics) {
        let deepest = if deepest.is_finite() {
            deepest.min(0.0)
        } else {
            0.0
        };
        let returned = self.db * ballistics.needle_return;
        self.db = deepest.min(returned);
        if self.db > -0.01 {
            self.db = 0.0;
        }
    }
}

/// What the audio thread writes: a track's, a bus's or the master's meter.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Which {
    Track(usize),
    /// A track's sound before its chain (after its input trim).
    Input(usize),
    /// A track's sound after its chain and its output level.
    ChainOutput(usize),
    /// One part of a drum track.
    Part(usize, usize),
    Reverb,
    Compressor,
    Chorus,
    Master,
}

impl Meters {
    /// Publishes `peak` for `which` (from the audio thread; never blocks).
    pub(crate) fn publish(&self, which: Which, peak: Peak) {
        let meter = match which {
            Which::Track(track) => self.tracks.get(track),
            Which::Input(track) => self.inputs.get(track),
            Which::ChainOutput(track) => self.outputs.get(track),
            Which::Part(track, part) => track
                .checked_mul(PARTS)
                .and_then(|index| index.checked_add(part))
                .and_then(|index| self.parts.get(index)),
            Which::Reverb => Some(&self.reverb),
            Which::Compressor => Some(&self.compressor),
            Which::Chorus => Some(&self.chorus),
            Which::Master => Some(&self.master),
        };
        if let Some(meter) = meter {
            meter.set(peak.level());
        }
    }

    /// Publishes the gain reduction of the song's track `track` (`None`: the compressor
    /// bus).
    pub(crate) fn publish_reduction(&self, track: Option<usize>, needle: Needle) {
        let slot = match track {
            Some(track) => self.reductions.get(track),
            None => Some(&self.bus_reduction),
        };
        if let Some(slot) = slot {
            slot.store(needle.db.to_bits(), Ordering::Relaxed);
        }
    }
}

/// The loudest sample of `samples` (magnitude).
pub(crate) fn peak_of(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |peak, x| peak.max(x.abs()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::arithmetic_side_effects)]

    use super::*;

    const RATE: u32 = 48_000;
    const BLOCK: usize = 64;

    fn blocks(seconds: f32) -> usize {
        (seconds * RATE as f32 / BLOCK as f32) as usize
    }

    #[test]
    fn the_bar_holds_a_peak_then_falls_slowly() {
        let ballistics = Ballistics::new(RATE, BLOCK);
        let mut peak = Peak::default();
        peak.update(0.5, 0.25, ballistics);
        assert_eq!((peak.level().left, peak.level().right), (0.5, 0.25));
        // Held for 0.3 s...
        for _ in 0..blocks(0.25) {
            peak.update(0.0, 0.0, ballistics);
        }
        assert_eq!(peak.level().left, 0.5, "held");
        // ...then 20 dB in 1.7 s: one second after the hold, about 11.8 dB down.
        for _ in 0..blocks(0.05 + 1.0) {
            peak.update(0.0, 0.0, ballistics);
        }
        let db = 20.0 * (peak.level().left / 0.5).log10();
        assert!((db + 11.76).abs() < 0.5, "{db}");
        // A louder block takes over at once; NaN is silence.
        peak.update(0.9, f32::NAN, ballistics);
        assert_eq!(peak.level().left, 0.9);
        assert!(peak.level().right.is_finite());
    }

    #[test]
    fn the_peak_mark_stays_longer_and_never_shows_below_the_bar() {
        let ballistics = Ballistics::new(RATE, BLOCK);
        let mut peak = Peak::default();
        peak.update(0.8, 0.8, ballistics);
        // Quieter notes go on: the bar follows them down, the mark stays for 1.5 s.
        for _ in 0..blocks(1.2) {
            peak.update(0.1, 0.1, ballistics);
        }
        let level = peak.level();
        assert_eq!(level.mark_left, 0.8, "the mark stays");
        assert!(level.left < 0.4, "the bar fell: {}", level.left);
        // After its hold it drops quickly to the bar.
        for _ in 0..blocks(1.0) {
            peak.update(0.1, 0.1, ballistics);
        }
        let level = peak.level();
        assert!(level.mark_left < 0.2, "{}", level.mark_left);
        assert!(level.mark_left >= level.left);
        // Silence: everything comes to rest at exactly zero.
        for _ in 0..blocks(30.0) {
            peak.update(0.0, 0.0, ballistics);
        }
        assert_eq!(peak.level(), Level::default());
    }

    #[test]
    fn a_needle_jumps_to_the_reduction_and_returns_smoothly() {
        let ballistics = Ballistics::new(RATE, BLOCK);
        let mut needle = Needle::default();
        needle.update(-9.0, ballistics);
        assert_eq!(needle.db, -9.0);
        // A third of a second later about a third is left (time constant 0.3 s).
        for _ in 0..blocks(0.3) {
            needle.update(0.0, ballistics);
        }
        assert!(
            (needle.db + 9.0 / std::f32::consts::E).abs() < 0.3,
            "{}",
            needle.db
        );
        for _ in 0..blocks(5.0) {
            needle.update(0.0, ballistics);
        }
        assert_eq!(needle.db, 0.0, "rests at 0 dB");
        needle.update(f32::NAN, ballistics);
        assert_eq!(needle.db, 0.0);
        let meters = Meters::default();
        needle.update(-4.0, ballistics);
        meters.publish_reduction(Some(2), needle);
        meters.publish_reduction(None, needle);
        assert_eq!(meters.reduction(2), -4.0);
        assert_eq!(meters.bus_reduction(), -4.0);
        assert_eq!(meters.reduction(MAX_TONES + 1), 0.0);
        assert!(meters.moving());
    }

    #[test]
    fn meters_are_read_where_they_are_written() {
        let meters = Meters::default();
        let mut peak = Peak::default();
        peak.update(0.7, 0.3, Ballistics::new(RATE, BLOCK));
        assert!(!meters.moving());
        meters.publish(Which::Track(3), peak);
        meters.publish(Which::Master, peak);
        assert_eq!(meters.track(3).left, 0.7);
        assert_eq!(meters.track(3).right, 0.3);
        assert_eq!(meters.track(2), Level::default());
        assert_eq!(meters.track(MAX_TONES + 5), Level::default());
        assert_eq!(meters.master().left, 0.7);
        assert_eq!(meters.reverb(), Level::default());
        assert!(meters.moving());
    }
}
