//! Which bars form multi-bar rests: runs of at least two bars in which every shown staff
//! only rests, broken wherever something is written at a bar's start (a section, a tempo,
//! a time or key signature, a repeat or an ending, a jump or coda, a fermata, a page
//! break) or a bar inside the run ends with a double barline or a repeat. The run's first
//! bar may carry such marks itself. A line break inside a run is dropped: it is layout,
//! not music (Guitar Pro stores its bars-per-line layout as a break every few bars).

use crate::marks::MeasureMarks;
use crate::measure::{MeasureLayout, RestRun};

/// What one staff shows of a measure, as the runs need it.
pub(super) struct StaffBar<'a> {
    pub(super) layout: &'a MeasureLayout,
    pub(super) marks: &'a MeasureMarks,
}

/// Each measure's part in a multi-bar rest: `bars[measure][staff]` is every shown staff's
/// view of the measure; `pages[measure]` whether a page starts there.
pub(super) fn rest_runs(bars: &[Vec<StaffBar<'_>>], pages: &[bool]) -> Vec<RestRun> {
    let count = bars.len();
    let mut runs = vec![RestRun::None; count];
    let mut index = 0;
    while index < count {
        if !rests(bars.get(index)) {
            index = index.saturating_add(1);
            continue;
        }
        // Extend the run while the next bar rests, starts clean and this one ends plain.
        let mut end = index.saturating_add(1);
        while end < count
            && rests(bars.get(end))
            && starts_clean(bars.get(end))
            && !pages.get(end).copied().unwrap_or(false)
            && ends_plain(bars.get(end.saturating_sub(1)))
        {
            end = end.saturating_add(1);
        }
        let length = end.saturating_sub(index);
        if length >= 2 {
            if let Some(slot) = runs.get_mut(index) {
                *slot = RestRun::Leads(u32::try_from(length).unwrap_or(u32::MAX));
            }
            for slot in runs.iter_mut().take(end).skip(index.saturating_add(1)) {
                *slot = RestRun::Folded;
            }
        }
        index = end;
    }
    runs
}

/// Every staff of the bar rests: no note, no bar-repetition sign, no dynamic.
fn rests(staves: Option<&Vec<StaffBar<'_>>>) -> bool {
    staves.is_some_and(|staves| {
        !staves.is_empty()
            && staves
                .iter()
                .all(|bar| !bar.layout.has_notes() && bar.marks.dynamics.is_empty())
    })
}

/// Nothing is written at the bar's start on any staff.
fn starts_clean(staves: Option<&Vec<StaffBar<'_>>>) -> bool {
    staves.is_some_and(|staves| {
        staves.iter().all(|bar| {
            let marks = bar.marks;
            !bar.layout.shows_signature_change()
                && marks.section.is_none()
                && marks.tempo.is_none()
                && marks.column_tempos.is_empty()
                && !marks.repeat.start
                && marks.ending.is_none()
                && marks.fermata.is_none()
                && marks.directions.is_empty()
                && marks.feel.is_none()
                && !marks.free_time_starts
                && !marks.beat_effects
                && !marks.lane_ornaments
                && !marks.beat_text
                && !marks.chord_diagram
        })
    })
}

/// The bar ends with a plain barline on every staff (a run hides the barlines inside it).
fn ends_plain(staves: Option<&Vec<StaffBar<'_>>>) -> bool {
    staves.is_some_and(|staves| {
        staves
            .iter()
            .all(|bar| bar.marks.repeat.end_repeats.is_none() && !bar.marks.double_bar)
    })
}
