//! Systems: breaking the measures into lines and justifying each line to the page width.

use std::ops::Range;

/// Below this fill ratio the last system keeps its natural width instead of being stretched
/// to the full line.
const LAST_SYSTEM_STRETCH_FROM: f32 = 0.75;

/// The widths of one measure, as line breaking sees them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MeasureWidth {
    /// Width that never stretches.
    pub(crate) fixed: f32,
    /// Width that stretches when the system is justified.
    pub(crate) stretchable: f32,
}

impl MeasureWidth {
    fn natural(&self) -> f32 {
        self.fixed + self.stretchable
    }
}

/// One line of measures and how much its measures stretch.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SystemPlan {
    /// The measures on this line (indices into the measure list).
    pub(crate) measures: Range<usize>,
    /// Factor for the stretchable widths (at least 1).
    pub(crate) stretch: f32,
}

/// Breaks measures into systems of at most `line_width`, each starting with `prefix`
/// (clef and gaps). A measure wider than a line gets a line of its own. Every system but the
/// last is stretched to the full width; the last only if it is already mostly full.
pub(crate) fn break_systems(
    widths: &[MeasureWidth],
    line_width: f32,
    prefix: f32,
) -> Vec<SystemPlan> {
    let mut plans = Vec::new();
    let mut start = 0;
    let mut used = prefix;
    for (index, width) in widths.iter().enumerate() {
        if index > start && used + width.natural() > line_width {
            plans.push(start..index);
            start = index;
            used = prefix;
        }
        used += width.natural();
    }
    if start < widths.len() {
        plans.push(start..widths.len());
    }

    let count = plans.len();
    plans
        .into_iter()
        .enumerate()
        .map(|(number, measures)| {
            let line: &[MeasureWidth] = widths.get(measures.clone()).unwrap_or_default();
            let fixed: f32 = prefix + line.iter().map(|w| w.fixed).sum::<f32>();
            let stretchable: f32 = line.iter().map(|w| w.stretchable).sum();
            let natural = fixed + stretchable;
            let is_last = number.saturating_add(1) == count;
            let stretch = if is_last && natural < line_width * LAST_SYSTEM_STRETCH_FROM {
                1.0
            } else {
                justify(line_width - fixed, stretchable)
            };
            SystemPlan { measures, stretch }
        })
        .collect()
}

/// The factor that makes `stretchable` fill `room`; never below 1 (an overfull line keeps
/// its natural width) and 1 for nonsense input.
fn justify(room: f32, stretchable: f32) -> f32 {
    let factor = room / stretchable;
    if factor.is_finite() && factor > 1.0 {
        factor
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(fixed: f32, stretchable: f32) -> MeasureWidth {
        MeasureWidth { fixed, stretchable }
    }

    #[test]
    fn measures_fill_lines_greedily() {
        let widths = [w(1.0, 9.0); 5];
        let plans = break_systems(&widths, 40.0, 2.0);
        let ranges: Vec<_> = plans.iter().map(|p| p.measures.clone()).collect();
        assert_eq!(ranges, [0..3, 3..5]);
        // 2 + 3 * 1 fixed and 27 stretchable, stretched to fill 40.
        assert!((plans[0].stretch - (40.0 - 5.0) / 27.0).abs() < 1e-5);
        // The last line is 2 + 20 = 22 of 40 (55%): natural width.
        assert_eq!(plans[1].stretch, 1.0);
    }

    #[test]
    fn a_mostly_full_last_line_is_justified() {
        let plans = break_systems(&[w(1.0, 25.0)], 32.0, 2.0);
        assert!(plans[0].stretch > 1.0);
    }

    #[test]
    fn a_measure_wider_than_the_line_gets_its_own_line() {
        let plans = break_systems(&[w(1.0, 5.0), w(1.0, 100.0), w(1.0, 5.0)], 32.0, 2.0);
        let ranges: Vec<_> = plans.iter().map(|p| p.measures.clone()).collect();
        assert_eq!(ranges, [0..1, 1..2, 2..3]);
        assert_eq!(plans[1].stretch, 1.0, "overfull lines are not squeezed");
    }

    #[test]
    fn nothing_to_break_gives_no_systems() {
        assert!(break_systems(&[], 32.0, 2.0).is_empty());
        assert_eq!(justify(10.0, 0.0), 1.0);
        assert_eq!(justify(f32::NAN, 1.0), 1.0);
    }
}
