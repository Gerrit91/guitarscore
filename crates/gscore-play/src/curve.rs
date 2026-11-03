//! A bend or whammy curve as Guitar Pro draws it: points from 0 to 12 over the note's
//! length, each with a pitch in quarter tones, joined by straight lines.

/// Most points a curve keeps (as many as the model allows).
const MAX_POINTS: usize = 16;
/// The last position of a curve (the note's end).
const END: f64 = 12.0;

/// A bend or whammy curve. Fixed-size, so the shapes holding it stay `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Curve {
    points: [(u8, i8); MAX_POINTS],
    len: usize,
}

impl Curve {
    /// The curve through `points` (position 0-12, quarter tones), in order; `None` when
    /// there are none or the curve stays at 0 throughout.
    pub(crate) fn new(points: impl IntoIterator<Item = (u8, i8)>) -> Option<Self> {
        let mut curve = Curve {
            points: [(0, 0); MAX_POINTS],
            len: 0,
        };
        for point in points.into_iter().take(MAX_POINTS) {
            if let Some(slot) = curve.points.get_mut(curve.len) {
                *slot = point;
                curve.len = curve.len.saturating_add(1);
            }
        }
        curve.points().iter().any(|&(_, value)| value != 0).then_some(curve)
    }

    fn points(&self) -> &[(u8, i8)] {
        self.points.get(..self.len).unwrap_or_default()
    }

    /// The pitch in quarter tones at `share` (0 at the note's start, 1 at its end): before
    /// the first point it holds the first value, after the last the last.
    pub(crate) fn at(&self, share: f64) -> f64 {
        let position = share.clamp(0.0, 1.0) * END;
        let points = self.points();
        let Some(&(first_at, first)) = points.first() else {
            return 0.0;
        };
        if position <= f64::from(first_at) {
            return f64::from(first);
        }
        for pair in points.windows(2) {
            let [(a_at, a), (b_at, b)] = pair else {
                continue;
            };
            let (a_at, b_at) = (f64::from(*a_at), f64::from(*b_at));
            if position <= b_at {
                let t = if b_at > a_at {
                    (position - a_at) / (b_at - a_at)
                } else {
                    1.0
                };
                return f64::from(*a) + (f64::from(*b) - f64::from(*a)) * t;
            }
        }
        points.last().map_or(0.0, |&(_, value)| f64::from(value))
    }

    /// The farthest the curve goes from 0, in quarter tones.
    pub(crate) fn reach(&self) -> u32 {
        self.points()
            .iter()
            .map(|&(_, value)| u32::from(value.unsigned_abs()))
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_curve_joins_its_points_with_lines() {
        // A bend up a whole step by the middle, released by the end.
        let curve = Curve::new([(0, 0), (6, 4), (12, 0)]).unwrap();
        assert!((curve.at(0.0)).abs() < 1e-9);
        assert!((curve.at(0.25) - 2.0).abs() < 1e-9);
        assert!((curve.at(0.5) - 4.0).abs() < 1e-9);
        assert!((curve.at(1.0)).abs() < 1e-9);
        assert_eq!(curve.reach(), 4);
        // A prebend holds its first value; a flat curve is no curve.
        let prebend = Curve::new([(3, 4), (12, 4)]).unwrap();
        assert!((prebend.at(0.0) - 4.0).abs() < 1e-9);
        assert!(Curve::new([(0, 0), (12, 0)]).is_none());
    }
}
