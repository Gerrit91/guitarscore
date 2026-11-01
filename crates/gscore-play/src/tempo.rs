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

    /// The time of `tick` in microseconds from the start.
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
        micros
    }

    /// The tick reached after `micros` microseconds.
    pub fn tick_at(&self, micros: u64) -> u64 {
        let mut elapsed: u64 = 0;
        for (i, &(start, rate)) in self.points.iter().enumerate() {
            let next = self.points.get(i.saturating_add(1)).map(|&(next, _)| next);
            let span = next.map(|next| span_micros(next.saturating_sub(start), rate));
            match span {
                Some(span) if elapsed.saturating_add(span) <= micros => {
                    elapsed = elapsed.saturating_add(span);
                }
                _ => {
                    let left = micros.saturating_sub(elapsed);
                    let ticks = u128::from(left)
                        .saturating_mul(u128::from(PPQ))
                        .checked_div(u128::from(rate))
                        .unwrap_or(0);
                    return start.saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX));
                }
            }
        }
        0
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
