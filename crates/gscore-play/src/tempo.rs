//! Tempo map: converts between ticks and microseconds.

/// Ticks per quarter note. 960 divides evenly by 2, 3, 4, 5, 6, 8, ... so most tuplets land
/// on exact ticks.
pub const PPQ: u32 = 960;

/// Microseconds per minute.
const MICROS_PER_MINUTE: u64 = 60_000_000;
/// Tempos are clamped to this range (beats per minute).
const TEMPO_RANGE: (u32, u32) = (10, 1000);

/// Tempo changes over time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TempoMap {
    /// `(tick, microseconds per quarter note)`, sorted by tick, starting at tick 0.
    points: Vec<(u64, u32)>,
    /// `(tick, microseconds)`: a fermata or similar hold just before that tick. Sorted by
    /// tick. Ticks do not move: the pause only lengthens the time taken to reach them.
    pauses: Vec<(u64, u64)>,
}

impl Default for TempoMap {
    /// 120 beats per minute, Guitar Pro's default tempo.
    fn default() -> Self {
        TempoMap::new(120)
    }
}

/// Microseconds per quarter note at `bpm` beats per minute (clamped to a sane range).
pub fn micros_per_quarter(bpm: u32) -> u32 {
    let bpm = u64::from(bpm.clamp(TEMPO_RANGE.0, TEMPO_RANGE.1));
    u32::try_from(
        MICROS_PER_MINUTE
            .checked_div(bpm)
            .unwrap_or(MICROS_PER_MINUTE),
    )
    .unwrap_or(u32::MAX)
}

impl TempoMap {
    /// A map that starts at `bpm`.
    pub fn new(bpm: u32) -> Self {
        TempoMap {
            points: vec![(0, micros_per_quarter(bpm))],
            pauses: Vec::new(),
        }
    }

    /// Sets the tempo to `bpm` from `tick` on. Changes must come in tick order; one at the
    /// same tick as the last replaces it.
    pub fn set(&mut self, tick: u64, bpm: u32) {
        let micros = micros_per_quarter(bpm);
        match self.points.last_mut() {
            Some(last) if last.0 == tick => last.1 = micros,
            Some(last) if last.0 > tick => {}
            Some(last) if last.1 == micros => {}
            _ => self.points.push((tick, micros)),
        }
    }

    /// The tempo points: `(tick, microseconds per quarter note)`.
    pub fn points(&self) -> &[(u64, u32)] {
        &self.points
    }

    /// The pauses: `(tick, microseconds)` holds just before that tick.
    pub fn pauses(&self) -> &[(u64, u64)] {
        &self.pauses
    }

    /// Holds `micros` microseconds just before reaching `tick` (a fermata). Ticks do not
    /// move; only the time taken to reach them grows. Several holds at one tick add up.
    pub fn pause(&mut self, tick: u64, micros: u64) {
        if micros == 0 {
            return;
        }
        let at = self.pauses.partition_point(|&(start, _)| start < tick);
        match self.pauses.get_mut(at) {
            Some(hold) if hold.0 == tick => hold.1 = hold.1.saturating_add(micros),
            _ => self.pauses.insert(at, (tick, micros)),
        }
    }

    /// Holds `ticks` of the tempo in effect just before `tick`, converted to time.
    pub fn hold_ticks(&mut self, tick: u64, ticks: u64) {
        let bpm = self.bpm_at(tick.saturating_sub(1));
        let micros = u128::from(ticks)
            .saturating_mul(u128::from(micros_per_quarter(bpm)))
            .checked_div(u128::from(PPQ))
            .unwrap_or(0);
        if let Ok(micros) = u64::try_from(micros) {
            self.pause(tick, micros);
        }
    }

    /// A map from decoded points and pauses, both checked; `None` if they are damaged.
    pub(crate) fn from_parts(points: Vec<(u64, u32)>, pauses: Vec<(u64, u64)>) -> Option<Self> {
        let first = points.first()?;
        if first.0 != 0 || first.1 == 0 {
            return None;
        }
        if points.windows(2).any(|pair| match pair {
            [a, b] => a.0 >= b.0,
            _ => true,
        }) {
            return None;
        }
        if pauses.iter().any(|&(_, micros)| micros == 0) {
            return None;
        }
        if pauses.windows(2).any(|pair| match pair {
            [a, b] => a.0 >= b.0,
            _ => true,
        }) {
            return None;
        }
        Some(TempoMap { points, pauses })
    }

    /// The time of `tick` in microseconds from the start, including the pauses before it.
    pub fn micros_at(&self, tick: u64) -> u64 {
        let mut micros: u64 = 0;
        for (i, &(start, rate)) in self.points.iter().enumerate() {
            if tick <= start {
                break;
            }
            let end = self
                .points
                .get(i.saturating_add(1))
                .map_or(tick, |&(next, _)| next.min(tick));
            micros = micros.saturating_add(span_micros(end.saturating_sub(start), rate));
        }
        for &(pause_tick, pause) in &self.pauses {
            if pause_tick > tick {
                break;
            }
            micros = micros.saturating_add(pause);
        }
        micros
    }

    /// The tempo at `tick` in beats per minute (rounded): the last change at or before it.
    pub fn bpm_at(&self, tick: u64) -> u32 {
        let after = self.points.partition_point(|&(start, _)| start <= tick);
        let rate = after
            .checked_sub(1)
            .and_then(|index| self.points.get(index))
            .or(self.points.first())
            .map_or(0, |&(_, rate)| u64::from(rate));
        // Rounded: 60 000 000 / rate, plus half the divisor first.
        MICROS_PER_MINUTE
            .checked_add(rate / 2)
            .and_then(|minute| minute.checked_div(rate))
            .and_then(|bpm| u32::try_from(bpm).ok())
            .unwrap_or(0)
    }

    /// The tick reached after `micros` microseconds. A pause holds the result just before
    /// its tick, so the events at that tick do not come due until the hold is over.
    pub fn tick_at(&self, micros: u64) -> u64 {
        let mut elapsed: u64 = 0;
        let mut point: usize = 0;
        let mut current = self.points.first().map_or(0, |&(tick, _)| tick);
        let mut rate = self.points.first().map_or(1, |&(_, rate)| rate.max(1));
        let mut pause: usize = 0;
        loop {
            let next_point = self
                .points
                .get(point.saturating_add(1))
                .map(|&(tick, _)| tick);
            let next_pause = self.pauses.get(pause).map(|&(tick, _)| tick);
            let boundary = match (next_point, next_pause) {
                (Some(a), Some(b)) => a.min(b),
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => u64::MAX,
            };
            if boundary == u64::MAX {
                let left = micros.saturating_sub(elapsed);
                return current.saturating_add(ticks_for(left, rate));
            }
            let span = boundary.saturating_sub(current);
            let cost = span_micros(span, rate);
            if elapsed.saturating_add(cost) > micros {
                let left = micros.saturating_sub(elapsed);
                return current.saturating_add(ticks_for(left, rate));
            }
            elapsed = elapsed.saturating_add(cost);
            current = boundary;
            let mut advanced = false;
            if next_pause == Some(boundary) {
                let hold = self.pauses.get(pause).map_or(0, |&(_, micros)| micros);
                if elapsed.saturating_add(hold) > micros {
                    // Still holding: stay just before the tick whose events wait.
                    return current.saturating_sub(1);
                }
                elapsed = elapsed.saturating_add(hold);
                pause = pause.saturating_add(1);
                advanced = true;
            }
            if next_point == Some(boundary) {
                point = point.saturating_add(1);
                if let Some(&(tick, new_rate)) = self.points.get(point) {
                    current = tick;
                    rate = new_rate.max(1);
                }
                advanced = true;
            }
            if !advanced {
                break;
            }
        }
        current
    }
}

/// How long `ticks` last at `rate` microseconds per quarter.
fn span_micros(ticks: u64, rate: u32) -> u64 {
    let micros = u128::from(ticks)
        .saturating_mul(u128::from(rate))
        .checked_div(u128::from(PPQ))
        .unwrap_or(0);
    u64::try_from(micros).unwrap_or(u64::MAX)
}

/// How many ticks fit in `micros` at `rate` microseconds per quarter.
fn ticks_for(micros: u64, rate: u32) -> u64 {
    let ticks = u128::from(micros)
        .saturating_mul(u128::from(PPQ))
        .checked_div(u128::from(rate.max(1)))
        .unwrap_or(0);
    u64::try_from(ticks).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_and_time_convert_both_ways_across_tempo_changes() {
        let mut map = TempoMap::new(120); // a quarter is 0.5 s
        map.set(u64::from(PPQ) * 4, 60); // after one 4/4 bar, a quarter is 1 s
        assert_eq!(map.micros_at(u64::from(PPQ)), 500_000);
        assert_eq!(map.micros_at(u64::from(PPQ) * 4), 2_000_000);
        assert_eq!(map.micros_at(u64::from(PPQ) * 5), 3_000_000);
        for tick in [0, 1, 480, 3840, 4800, 10_000] {
            let back = map.tick_at(map.micros_at(tick));
            assert!(back.abs_diff(tick) <= 1, "{tick} -> {back}");
        }
    }

    #[test]
    fn the_tempo_at_a_tick_is_the_last_change_before_it() {
        let mut map = TempoMap::new(120);
        map.set(u64::from(PPQ) * 4, 60);
        map.set(u64::from(PPQ) * 6, 133);
        assert_eq!(map.bpm_at(0), 120);
        assert_eq!(map.bpm_at(u64::from(PPQ) * 4 - 1), 120);
        assert_eq!(map.bpm_at(u64::from(PPQ) * 4), 60);
        assert_eq!(map.bpm_at(u64::from(PPQ) * 6), 133);
        assert_eq!(map.bpm_at(u64::MAX), 133);
    }

    #[test]
    fn a_pause_holds_the_clock_just_before_its_tick() {
        let mut map = TempoMap::new(120); // a quarter is 0.5 s
        map.pause(u64::from(PPQ) * 4, 1_000_000);
        assert_eq!(map.micros_at(0), 0);
        assert_eq!(map.micros_at(u64::from(PPQ) * 4 - 1), 1_999_479);
        // Reaching the end of the fourth quarter includes the hold.
        assert_eq!(map.micros_at(u64::from(PPQ) * 4), 3_000_000);
        // The clock waits just before it, then crosses it.
        assert_eq!(map.tick_at(2_000_000), u64::from(PPQ) * 4 - 1);
        assert_eq!(map.tick_at(2_500_000), u64::from(PPQ) * 4 - 1);
        assert_eq!(map.tick_at(3_000_000), u64::from(PPQ) * 4);
        assert_eq!(map.tick_at(3_500_000), u64::from(PPQ) * 5);
        // The tempo itself does not change.
        assert_eq!(map.bpm_at(u64::from(PPQ) * 4), 120);
        // Holds at the same tick add up, and they are kept in tick order.
        map.pause(u64::from(PPQ) * 2, 100_000);
        map.pause(u64::from(PPQ) * 4, 500_000);
        assert_eq!(
            map.pauses(),
            [
                (u64::from(PPQ) * 2, 100_000),
                (u64::from(PPQ) * 4, 1_500_000)
            ]
        );
    }

    #[test]
    fn odd_tempos_are_clamped_and_duplicates_dropped() {
        assert_eq!(micros_per_quarter(0), micros_per_quarter(10));
        assert_eq!(micros_per_quarter(100_000), micros_per_quarter(1000));
        let mut map = TempoMap::new(120);
        map.set(100, 120); // no change: not stored
        assert_eq!(map.points().len(), 1);
        map.set(200, 90);
        map.set(150, 60); // before the last change: ignored
        map.set(200, 100); // same tick: replaces
        assert_eq!(
            map.points(),
            [(0, micros_per_quarter(120)), (200, micros_per_quarter(100))]
        );
    }
}
