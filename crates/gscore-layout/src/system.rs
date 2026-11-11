//! Systems: breaking the measures into lines and justifying each line to the page width.

use std::ops::Range;

/// Below this fill ratio the last system keeps its natural width instead of being stretched
/// to the full line.
const LAST_SYSTEM_STRETCH_FROM: f32 = 0.75;
/// How far a line may squeeze its measures to take one more: the room each column has
/// beyond what its notes need shrinks to this share at most (Guitar Pro squeezes lines
/// too, rather than leaving a bar alone on the next).
const MIN_SQUEEZE: f32 = 0.6;

/// The widths of one measure, as line breaking sees them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MeasureWidth {
    /// Width that never stretches.
    pub(crate) fixed: f32,
    /// Width that stretches when the system is justified.
    pub(crate) stretchable: f32,
    /// The least the stretchable width can be squeezed to (what its notes need).
    pub(crate) floor: f32,
}

impl MeasureWidth {
    fn natural(&self) -> f32 {
        self.fixed + self.stretchable
    }

    /// The room beyond the floor, which a squeezed line shrinks.
    fn squeezable(&self) -> f32 {
        (self.stretchable - self.floor.clamp(0.0, self.stretchable.max(0.0))).max(0.0)
    }

    /// The width squeezed as far as a line may.
    fn narrowest(&self) -> f32 {
        self.natural() - self.squeezable() * (1.0 - MIN_SQUEEZE)
    }
}

/// One line of measures and how much its measures stretch.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SystemPlan {
    /// The measures on this line (indices into the measure list).
    pub(crate) measures: Range<usize>,
    /// Factor for the stretchable widths: above 1 a justified line, below 1 a squeezed one
    /// (only the room beyond each column's floor shrinks, by this factor).
    pub(crate) stretch: f32,
}

/// Where lines must or may break, besides the width.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Breaks<'a> {
    /// `forced[i]`: measure `i` starts a new line (a line or page break).
    pub(crate) forced: &'a [bool],
    /// At most this many measures on a line.
    pub(crate) max_measures: Option<usize>,
    /// Put every measure on one line, however wide it gets.
    pub(crate) single_line: bool,
}

/// Breaks measures into systems of at most `line_width`, each starting with the prefix of
/// the measure that starts it (clef, and maybe a key signature), and before every forced
/// break and after `max_measures`. A measure wider than a line gets a line of its own. Every
/// system but the last is stretched to the full width; the last only if it is already mostly
/// full.
pub(crate) fn break_systems(
    widths: &[MeasureWidth],
    line_width: f32,
    prefixes: &[f32],
    breaks: Breaks<'_>,
) -> Vec<SystemPlan> {
    let prefix_at = |index: usize| prefixes.get(index).copied().unwrap_or_default();
    if breaks.single_line && !widths.is_empty() {
        return vec![SystemPlan {
            measures: 0..widths.len(),
            stretch: 1.0,
        }];
    }
    let mut plans = Vec::new();
    let mut start = 0;
    // The line so far squeezed as far as it may.
    let mut used = prefix_at(0);
    for (index, width) in widths.iter().enumerate() {
        let forced = breaks.forced.get(index).copied().unwrap_or(false);
        let full = breaks
            .max_measures
            .is_some_and(|max| index.saturating_sub(start) >= max.max(1));
        if index > start && (forced || full || used + width.narrowest() > line_width) {
            plans.push(start..index);
            start = index;
            used = prefix_at(start);
        }
        used += width.narrowest();
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
            let prefix = prefix_at(measures.start);
            let fixed: f32 = prefix + line.iter().map(|w| w.fixed).sum::<f32>();
            let stretchable: f32 = line.iter().map(|w| w.stretchable).sum();
            let squeezable: f32 = line.iter().map(MeasureWidth::squeezable).sum();
            let natural = fixed + stretchable;
            let is_last = number.saturating_add(1) == count;
            let stretch = if natural > line_width {
                squeeze(natural - line_width, squeezable)
            } else if is_last && natural < line_width * LAST_SYSTEM_STRETCH_FROM {
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

/// The factor that takes `excess` off a line whose columns have `squeezable` room beyond
/// their floors; never below [`MIN_SQUEEZE`] (a measure wider than the line stays
/// overfull) and 1 for nonsense input.
fn squeeze(excess: f32, squeezable: f32) -> f32 {
    let factor = 1.0 - excess / squeezable;
    if factor.is_finite() {
        factor.clamp(MIN_SQUEEZE, 1.0)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A constant system-start width for the tests.
    const PREFIX: [f32; 64] = [2.0; 64];

    fn w(fixed: f32, stretchable: f32) -> MeasureWidth {
        // No room to squeeze: what the notes need is all of it.
        MeasureWidth {
            fixed,
            stretchable,
            floor: stretchable,
        }
    }

    #[test]
    fn a_line_squeezes_to_take_one_more_measure() {
        // Three measures of 1 + 12 (floor 6) on a 36 line: 39 natural, 3 too wide; 18 of
        // room beyond the floors, so a factor of 1 - 3/18.
        let roomy = MeasureWidth {
            fixed: 1.0,
            stretchable: 12.0,
            floor: 6.0,
        };
        let plans = break_systems(&[roomy; 4], 36.0, &[0.0; 4], Breaks::default());
        assert_eq!(ranges(&plans), vec![0..3, 3..4]);
        assert!((plans[0].stretch - (1.0 - 3.0 / 18.0)).abs() < 1e-5);
        // Never below the floors' share: a fourth one would need more.
        let tight = break_systems(&[roomy; 4], 30.0, &[0.0; 4], Breaks::default());
        assert_eq!(ranges(&tight)[0], 0..2);
        assert_eq!(squeeze(100.0, 1.0), MIN_SQUEEZE);
        assert_eq!(squeeze(1.0, 0.0), 1.0);
        assert_eq!(squeeze(f32::NAN, 1.0), 1.0);
    }

    #[test]
    fn measures_fill_lines_greedily() {
        let widths = [w(1.0, 9.0); 5];
        let plans = break_systems(&widths, 40.0, &PREFIX, Breaks::default());
        let ranges: Vec<_> = plans.iter().map(|p| p.measures.clone()).collect();
        assert_eq!(ranges, [0..3, 3..5]);
        // 2 + 3 * 1 fixed and 27 stretchable, stretched to fill 40.
        assert!((plans[0].stretch - (40.0 - 5.0) / 27.0).abs() < 1e-5);
        // The last line is 2 + 20 = 22 of 40 (55%): natural width.
        assert_eq!(plans[1].stretch, 1.0);
    }

    #[test]
    fn a_mostly_full_last_line_is_justified() {
        let plans = break_systems(&[w(1.0, 25.0)], 32.0, &PREFIX, Breaks::default());
        assert!(plans[0].stretch > 1.0);
    }

    #[test]
    fn a_measure_wider_than_the_line_gets_its_own_line() {
        let plans = break_systems(
            &[w(1.0, 5.0), w(1.0, 100.0), w(1.0, 5.0)],
            32.0,
            &PREFIX,
            Breaks::default(),
        );
        let ranges: Vec<_> = plans.iter().map(|p| p.measures.clone()).collect();
        assert_eq!(ranges, [0..1, 1..2, 2..3]);
        assert_eq!(plans[1].stretch, 1.0, "overfull lines are not squeezed");
    }

    fn ranges(plans: &[SystemPlan]) -> Vec<Range<usize>> {
        plans.iter().map(|p| p.measures.clone()).collect()
    }

    #[test]
    fn forced_breaks_and_a_bar_limit_start_new_lines() {
        let widths = [w(1.0, 4.0); 8];
        // Room for 6 measures a line by width.
        let free = break_systems(&widths, 32.0, &PREFIX, Breaks::default());
        assert_eq!(ranges(&free), [0..6, 6..8]);
        let forced = [false, false, true, false, false, false, false, true];
        let breaks = Breaks {
            forced: &forced,
            max_measures: None,
            single_line: false,
        };
        assert_eq!(
            ranges(&break_systems(&widths, 32.0, &PREFIX, breaks)),
            [0..2, 2..7, 7..8]
        );
        let limited = Breaks {
            forced: &forced,
            max_measures: Some(3),
            single_line: false,
        };
        assert_eq!(
            ranges(&break_systems(&widths, 32.0, &PREFIX, limited)),
            [0..2, 2..5, 5..7, 7..8]
        );
        // A break on the first measure changes nothing; a limit of 0 means one a line.
        let first = [true];
        let odd = Breaks {
            forced: &first,
            max_measures: Some(0),
            single_line: false,
        };
        assert_eq!(
            ranges(&break_systems(&widths[..3], 32.0, &PREFIX, odd)),
            [0..1, 1..2, 2..3]
        );
    }

    #[test]
    fn nothing_to_break_gives_no_systems() {
        assert!(break_systems(&[], 32.0, &PREFIX, Breaks::default()).is_empty());
        assert_eq!(justify(10.0, 0.0), 1.0);
        assert_eq!(justify(f32::NAN, 1.0), 1.0);
    }
}
