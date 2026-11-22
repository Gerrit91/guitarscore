//! The browser's smooth playing position for the screen, as [`crate::clock`] is the
//! desktop's. The worklet reports, every few blocks, the tick it has rendered up to, the
//! audio context's time at that point and how fast ticks go; the page reads, each frame,
//! the context time being heard now (the output's latency behind what is rendered: a
//! Bluetooth headset adds a few hundred milliseconds). The position shown is carried on
//! from the last report to that time, so the cursor glides and keeps to what is heard,
//! whenever the page's own thread happens to get the reports.

/// The longest a report is carried on: past it, the reports have stopped coming.
const MAX_CARRY: f64 = 0.5;
/// The longest the heard time may lag what is rendered (more is a broken timestamp).
const MAX_BEHIND: f64 = 0.5;

/// One report from the worklet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Report {
    /// The tick rendered up to.
    pub(crate) position: u64,
    /// The audio context's time at that tick, seconds.
    pub(crate) time: f64,
    /// Ticks per second (0 when not playing).
    pub(crate) rate: f64,
    /// Where this stretch of playing began (the last play, seek or stop): the position
    /// shown holds there until it is heard, never the beat before it.
    pub(crate) from: u64,
}

/// The last report.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct WebClock {
    last: Option<Report>,
}

impl WebClock {
    /// Takes a report.
    pub(crate) fn report(&mut self, report: Report) {
        self.last = Some(report);
    }

    /// Forgets the reports (a seek, a stop, a new start): until the next, the position is
    /// whatever the player says.
    pub(crate) fn forget(&mut self) {
        self.last = None;
    }

    /// The position heard at context time `heard` (seconds), if the last report came while
    /// playing.
    pub(crate) fn now(&self, heard: f64) -> Option<u64> {
        let last = self.last?;
        if !(last.rate.is_finite() && last.rate > 0.0 && heard.is_finite()) {
            return None;
        }
        let seconds = (heard - last.time).clamp(-MAX_BEHIND, MAX_CARRY);
        let ticks = (last.rate * seconds).round();
        let at = if ticks >= 0.0 {
            last.position.saturating_add(ticks as u64)
        } else {
            last.position.saturating_sub((-ticks) as u64)
        };
        Some(at.max(last.from.min(last.position)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 120 bpm: 1920 ticks a second.
    fn report(position: u64, time: f64, from: u64) -> Report {
        Report {
            position,
            time,
            rate: 1920.0,
            from,
        }
    }

    #[test]
    fn the_position_heard_lags_the_one_rendered() {
        let mut clock = WebClock::default();
        clock.report(report(1920, 11.0, 0));
        // Heard 0.2 s behind the rendering: 384 ticks back.
        assert_eq!(clock.now(10.8), Some(1536));
        // Carried on between reports.
        assert_eq!(clock.now(11.1), Some(2112));
        // Reports that stopped coming do not run it away.
        assert_eq!(clock.now(99.0), clock.now(11.0 + MAX_CARRY));
    }

    #[test]
    fn a_start_or_a_seek_holds_until_it_is_heard() {
        let mut clock = WebClock::default();
        // Playing from 4800; the first report a block in.
        clock.report(report(4805, 2.0, 4800));
        // Heard 0.3 s behind: still before the start, so held there.
        assert_eq!(clock.now(1.7), Some(4800));
        assert_eq!(clock.now(2.0), Some(4805));
        // A seek back to 960.
        clock.report(report(965, 2.1, 960));
        assert_eq!(clock.now(1.8), Some(960));
    }

    #[test]
    fn nothing_while_not_playing_or_without_reports() {
        let mut clock = WebClock::default();
        assert_eq!(clock.now(1.0), None);
        clock.report(Report {
            position: 960,
            time: 1.0,
            rate: 0.0,
            from: 960,
        });
        assert_eq!(clock.now(1.5), None);
        clock.report(report(960, 2.0, 960));
        assert_eq!(clock.now(f64::NAN), None);
        clock.forget();
        assert_eq!(clock.now(2.0), None);
    }
}
