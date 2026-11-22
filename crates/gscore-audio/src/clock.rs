//! A smooth playing position for the screen. The audio callback reports where each buffer
//! starts, when it will be heard (the output's latency later), and how fast ticks go;
//! between two callbacks the position is carried on from that, so a cursor drawn every
//! frame glides instead of moving once per buffer, and shows what is heard, not what was
//! just computed.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gscore_play::PPQ;

/// The longest a report is carried on: past it, the callbacks have stopped coming.
const MAX_CARRY: f64 = 0.25;
/// The longest output latency taken into account (more is a broken timestamp).
const MAX_AHEAD: Duration = Duration::from_millis(500);
/// A step back smaller than this (in seconds of playing) is the callbacks' jitter, not a
/// jump: the position holds instead of moving back.
const JITTER: f64 = 0.1;

/// What the last audio callback reported, shared with the UI thread.
#[derive(Debug)]
pub(crate) struct Clock {
    epoch: Instant,
    /// The tick the last buffer started at.
    start: AtomicU64,
    /// When that buffer will be heard, in microseconds since `epoch`.
    stamp: AtomicU64,
    /// Ticks per second then (`f64` bits; 0 when not playing).
    rate: AtomicU64,
    /// The position last read, so the screen never sees it step back by jitter.
    shown: AtomicU64,
    /// Where playing last started or jumped to: until that is heard, the position holds
    /// there (it is not carried back before it, onto the beat before a start or a seek).
    floor: AtomicU64,
}

impl Default for Clock {
    fn default() -> Self {
        Clock {
            epoch: Instant::now(),
            start: AtomicU64::new(0),
            stamp: AtomicU64::new(0),
            rate: AtomicU64::new(0),
            shown: AtomicU64::new(0),
            floor: AtomicU64::new(0),
        }
    }
}

impl Clock {
    /// Reports a buffer starting at `start`, played at `bpm` (`None` or 0: not moving),
    /// that will be heard `ahead` from now.
    pub(crate) fn publish(&self, start: u64, bpm: Option<f32>, ahead: Duration) {
        let ahead = ahead.min(MAX_AHEAD);
        let micros = u64::try_from((self.epoch.elapsed() + ahead).as_micros()).unwrap_or(u64::MAX);
        let rate = ticks_per_second(bpm);
        let (last, last_rate) = (
            self.start.load(Ordering::Acquire),
            f64::from_bits(self.rate.load(Ordering::Acquire)),
        );
        // A start (it was not moving), a step back (a seek or a loop) or a leap far ahead.
        let leap = (last_rate.max(rate) * MAX_CARRY * 4.0) as u64;
        let moving = last_rate.is_finite() && last_rate > 0.0;
        if !moving || start < last || start > last.saturating_add(leap) {
            self.floor.store(start, Ordering::Release);
        }
        self.start.store(start, Ordering::Release);
        self.stamp.store(micros, Ordering::Release);
        self.rate.store(rate.to_bits(), Ordering::Release);
    }

    /// The position now, carried on from the last report.
    pub(crate) fn now(&self) -> u64 {
        let micros = u64::try_from(self.epoch.elapsed().as_micros()).unwrap_or(u64::MAX);
        // Before the buffer is heard (its start still in the output's queue) this is
        // negative: the position heard is that much before its start.
        let stamp = self.stamp.load(Ordering::Acquire);
        let since = if micros >= stamp {
            (micros - stamp) as f64 / 1e6
        } else {
            -((stamp - micros) as f64 / 1e6)
        };
        let rate = f64::from_bits(self.rate.load(Ordering::Acquire));
        let now = carry(self.start.load(Ordering::Acquire), rate, since)
            .max(self.floor.load(Ordering::Acquire));
        let shown = steady(self.shown.load(Ordering::Acquire), now, rate);
        self.shown.store(shown, Ordering::Release);
        shown
    }
}

/// What to show after `last` when the clock says `now`: `last` again for a step back
/// within the jitter, `now` otherwise (moving on, or a real jump such as a seek).
fn steady(last: u64, now: u64, rate: f64) -> u64 {
    let back = last.saturating_sub(now);
    let jitter = if rate.is_finite() && rate > 0.0 {
        (rate * JITTER) as u64
    } else {
        0
    };
    if back > 0 && back <= jitter {
        last
    } else {
        now
    }
}

/// Ticks per second at `bpm` quarter notes a minute.
fn ticks_per_second(bpm: Option<f32>) -> f64 {
    match bpm {
        Some(bpm) if bpm.is_finite() && bpm > 0.0 => f64::from(bpm) / 60.0 * f64::from(PPQ),
        _ => 0.0,
    }
}

/// `start` carried on for `seconds` at `rate` ticks a second (at most [`MAX_CARRY`]);
/// negative `seconds` (not heard yet) carry it back, as far as the output's latency.
fn carry(start: u64, rate: f64, seconds: f64) -> u64 {
    if !(rate.is_finite() && rate > 0.0 && seconds.is_finite()) {
        return start;
    }
    let back = MAX_AHEAD.as_secs_f64();
    let ticks = (rate * seconds.clamp(-back, MAX_CARRY)).round();
    if ticks >= 0.0 {
        start.saturating_add(ticks as u64)
    } else {
        start.saturating_sub((-ticks) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_position_runs_on_between_reports() {
        // 120 bpm: two quarters (1920 ticks) a second.
        let rate = ticks_per_second(Some(120.0));
        assert_eq!(rate, 1920.0);
        assert_eq!(carry(1000, rate, 0.0), 1000);
        assert_eq!(carry(1000, rate, 0.01), 1019);
        assert_eq!(carry(1000, rate, 0.1), 1192);
        // Callbacks that stopped coming do not run it away.
        assert_eq!(carry(1000, rate, 10.0), carry(1000, rate, MAX_CARRY));
    }

    #[test]
    fn jitter_never_moves_it_back_but_a_jump_does() {
        let rate = 1920.0;
        assert_eq!(steady(1000, 1010, rate), 1010);
        // A late callback: a few ticks back is held.
        assert_eq!(steady(1000, 990, rate), 1000);
        // A seek or a loop going back: followed.
        assert_eq!(steady(9000, 100, rate), 100);
        // Not moving: whatever the clock says.
        assert_eq!(steady(1000, 990, 0.0), 990);
    }

    #[test]
    fn a_stopped_or_odd_clock_stands_still() {
        assert_eq!(ticks_per_second(None), 0.0);
        assert_eq!(ticks_per_second(Some(f32::NAN)), 0.0);
        assert_eq!(ticks_per_second(Some(-5.0)), 0.0);
        assert_eq!(carry(500, 0.0, 1.0), 500);
        assert_eq!(carry(500, f64::NAN, 1.0), 500);
        // Not heard yet: back, by at most the longest latency.
        assert_eq!(carry(5000, 1920.0, -0.1), 4808);
        assert_eq!(carry(5000, 1920.0, -10.0), carry(5000, 1920.0, -0.5));
        assert_eq!(carry(10, 1920.0, -0.1), 0);
        assert_eq!(carry(u64::MAX, 1920.0, 0.1), u64::MAX);
    }

    #[test]
    fn a_published_clock_reads_back_at_least_its_start() {
        let clock = Clock::default();
        clock.publish(4800, Some(90.0), Duration::ZERO);
        assert!(clock.now() >= 4800);
        clock.publish(10, None, Duration::ZERO);
        assert_eq!(clock.now(), 10);
    }

    #[test]
    fn the_position_shown_is_the_one_heard() {
        // 120 bpm, a buffer starting at tick 9600 that will be heard in 0.1 s: now the
        // speakers play about 192 ticks before it.
        let clock = Clock::default();
        clock.publish(9600, Some(120.0), Duration::from_millis(100));
        clock.publish(9856, Some(120.0), Duration::from_millis(100));
        let now = clock.now();
        assert!((9640..=9680).contains(&now), "{now}");
    }

    #[test]
    fn a_start_holds_until_it_is_heard() {
        // Playing starts at 4800, heard in 0.1 s: not shown before it meanwhile.
        let clock = Clock::default();
        clock.publish(0, None, Duration::ZERO);
        clock.publish(4800, Some(120.0), Duration::from_millis(100));
        assert_eq!(clock.now(), 4800);
        // A seek back to 960 holds at 960 too.
        clock.publish(960, Some(120.0), Duration::from_millis(100));
        assert_eq!(clock.now(), 960);
    }
}
