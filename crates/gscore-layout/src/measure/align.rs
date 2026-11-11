//! Lining up one measure across the tracks of a multi-track system: every track's columns
//! go on one time grid, so beats that start together stand one above the other, and the
//! key and time signatures and the first beat start at the same x in every track.
//!
//! Each track's [`MeasureLayout`] is still laid out (and cached) on its own; aligning only
//! changes the spaces between its columns and the room before them.

use std::collections::BTreeSet;
use std::sync::Arc;

use gscore_model::Duration;

use super::MeasureLayout;
use super::columns::as_f64;
use crate::options::Spacing;

/// The layouts of one measure in several tracks, with their columns on a common time grid:
/// a gap between two neighbouring start times is the larger of what its length asks for
/// and what any track's notes there need, and a column that spans several gaps (another
/// track has beats in between) gets at least its own natural space. One track (or none) is
/// returned unchanged.
pub(crate) fn align(measures: &[Arc<MeasureLayout>], spacing: &Spacing) -> Vec<Arc<MeasureLayout>> {
    if measures.len() < 2 {
        return measures.to_vec();
    }
    let key_width = |m: &MeasureLayout| m.key_width();
    let time_width = |m: &MeasureLayout| m.time_signature.as_ref().map_or(0.0, |t| t.width);
    let widest =
        |f: &dyn Fn(&MeasureLayout) -> f32| measures.iter().map(|m| f(m)).fold(0.0_f32, f32::max);
    let keys = widest(&key_width);
    let times = widest(&time_width);
    let lead = widest(&|m| m.lead);

    let mut grid: BTreeSet<Duration> = BTreeSet::new();
    for measure in measures {
        grid.extend(measure.columns.iter().map(|column| column.onset));
    }
    let end = measures
        .iter()
        .map(|m| m.end)
        .chain(grid.iter().next_back().copied())
        .max();
    let grid: Vec<Duration> = grid.into_iter().collect();
    let Some(end) = end else {
        return measures.to_vec();
    };
    let at = |onset: Duration| grid.binary_search(&onset).ok();
    // The length of each gap, from a start time to the next one (or to the end).
    let mut gaps: Vec<f32> = grid
        .iter()
        .enumerate()
        .map(|(index, onset)| {
            let next = grid.get(index.saturating_add(1)).copied().unwrap_or(end);
            spacing.after(next.checked_sub(*onset).map_or(0.0, as_f64))
        })
        .collect();
    // Every column's span on the grid: its gap index and the next column's (or the end).
    let spans: Vec<Vec<(usize, usize, f32)>> = measures
        .iter()
        .map(|measure| {
            let columns = &measure.columns;
            columns
                .iter()
                .enumerate()
                .filter_map(|(index, column)| {
                    let from = at(column.onset)?;
                    let to = columns
                        .get(index.saturating_add(1))
                        .and_then(|next| at(next.onset))
                        .unwrap_or(grid.len());
                    Some((from, to.max(from.saturating_add(1)), column.space))
                })
                .collect()
        })
        .collect();
    // Neighbouring columns of one track keep their natural distance...
    for &(from, to, space) in spans.iter().flatten() {
        if to == from.saturating_add(1)
            && let Some(gap) = gaps.get_mut(from)
        {
            *gap = gap.max(space);
        }
    }
    // ...and so does a column with other tracks' beats in between: the missing room goes
    // into the last gap it spans.
    for &(from, to, space) in spans.iter().flatten() {
        let sum: f32 = gaps.get(from..to).map_or(0.0, |gaps| gaps.iter().sum());
        if sum < space
            && let Some(last) = gaps.get_mut(to.saturating_sub(1))
        {
            *last += space - sum;
        }
    }

    measures
        .iter()
        .zip(&spans)
        .map(|(measure, spans)| {
            let mut aligned = MeasureLayout::clone(measure);
            aligned.signature_pad = keys - key_width(measure);
            aligned.lead = lead + (times - time_width(measure));
            for (column, &(from, to, _)) in aligned.columns.iter_mut().zip(spans) {
                column.space = gaps.get(from..to).map_or(0.0, |gaps| gaps.iter().sum());
                // Lined up across tracks, a column is not squeezed: the tracks' columns
                // would part.
                column.floor = column.space;
            }
            Arc::new(aligned)
        })
        .collect()
}
