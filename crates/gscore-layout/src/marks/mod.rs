//! Marks the score shows besides the notes: section names, tempo changes, alternative
//! endings and dynamics above and below the staves, and which measures carry repeat signs.
//!
//! Whether a tempo or a dynamic is written depends on the measures before it (only a
//! change is written), so [`plan`] decides everything in one pass over the track. Measure
//! layouts stay independent of their neighbours and can be cached; section markers and
//! repeats come from the measure headers, which the cache does not key on.

use std::sync::Arc;

use gscore_model::{Dynamic, Feel, Fermata, Repeat, Song};

mod feel;
mod text;

use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point};
use crate::glyph::Glyph;
use crate::measure::{MeasureLayout, Tops};
use crate::staff::Staff;

/// Distance from the top staff line up to the baseline of the first row of marks.
const FIRST_ROW: f32 = 2.6;
/// Space between a tempo mark and the feel mark after it.
const FEEL_GAP: f32 = 1.0;
/// Distance between two rows of marks.
const ROW_HEIGHT: f32 = 2.4;
/// Dynamics: at least this far below the notation staff's bottom line, this far below the
/// lowest notehead, and this far above the tab staff below it (all to the baseline).
const DYNAMIC_BELOW_STAFF: f32 = 3.2;
const DYNAMIC_BELOW_HEAD: f32 = 2.6;
const DYNAMIC_ABOVE_TAB: f32 = 2.6;
/// Below a tab staff: the first annotation lane and the gap between lanes (as the P.M. and
/// let ring rows use), plus a little more for the dynamic's height.
const TAB_FIRST_LANE: f32 = 2.2;
const TAB_LANE_GAP: f32 = 1.6;
const TAB_DYNAMIC_DROP: f32 = 0.8;

/// What one measure shows besides its notes.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct MeasureMarks {
    /// The section starting here.
    pub(crate) section: Option<String>,
    /// A tempo change written at the start of the measure.
    pub(crate) tempo: Option<u32>,
    /// Tempo changes on beats: column index and tempo.
    pub(crate) column_tempos: Vec<(usize, u32)>,
    /// Dynamics written under columns: column index and mark.
    pub(crate) dynamics: Vec<(usize, Dynamic)>,
    /// Repeat signs and alternative endings.
    pub(crate) repeat: Repeat,
    /// Whether the measure ends with a double barline.
    pub(crate) double_bar: bool,
    /// The fermata over the measure's first note, if any.
    pub(crate) fermata: Option<Fermata>,
    /// The feel, written where it changes (and at the start unless straight).
    pub(crate) feel: Option<Feel>,
    /// The ending bracket over this measure, if it is an alternative ending.
    pub(crate) ending: Option<Ending>,
}

/// An alternative-ending bracket over one measure.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Ending {
    /// "1." or "1., 2." where the bracket starts (with a hook); `None` where it goes on.
    pub(crate) label: Option<String>,
    /// Whether the bracket closes with a hook at the end of this measure.
    pub(crate) closed: bool,
}

impl MeasureMarks {
    /// Extra room before the first column (a start repeat sign) and after the last one
    /// (an end repeat sign).
    pub(crate) fn extra_width(&self, ctx: &Context<'_>) -> (f32, f32) {
        let sign = Staff::repeat_sign_width(&ctx.fonts.music);
        (
            if self.repeat.start { sign } else { 0.0 },
            if self.repeat.end_repeats.is_some() {
                sign
            } else {
                0.0
            },
        )
    }

    /// Whether the tempo row is needed: a tempo change or a feel mark.
    fn has_tempo(&self) -> bool {
        self.tempo.is_some() || !self.column_tempos.is_empty() || self.feel.is_some()
    }
}

/// Decides every measure's marks, in order: a tempo or dynamic is written only where it
/// differs from the one in effect (the song's tempo and mezzo-forte at the start, which
/// Guitar Pro does not print either).
pub(crate) fn plan(song: &Song, measures: &[Arc<MeasureLayout>]) -> Vec<MeasureMarks> {
    let mut tempo = song.tempo();
    let mut dynamic = Dynamic::DEFAULT;
    let mut previous_endings = 0;
    let mut previous_feel = Feel::None;
    measures
        .iter()
        .map(|measure| {
            let index = measure.index();
            let mut marks = MeasureMarks {
                section: song
                    .section(index)
                    .map(|section| section.title)
                    .filter(|title| !title.trim().is_empty()),
                repeat: song.repeat(index).unwrap_or_default(),
                double_bar: song.double_bar(index).unwrap_or(false),
                fermata: song.fermata(index),
                ..MeasureMarks::default()
            };
            let feel = song.feel(index).unwrap_or_default();
            if feel != previous_feel {
                marks.feel = Some(feel);
                previous_feel = feel;
            }
            if let Some(change) = song.tempo_change(index)
                && change != tempo
            {
                marks.tempo = Some(change);
                tempo = change;
            }
            for (column, (column_dynamic, column_tempo)) in measure.column_marks().enumerate() {
                if let Some(change) = column_tempo
                    && change != tempo
                {
                    marks.column_tempos.push((column, change));
                    tempo = change;
                }
                if let Some(change) = column_dynamic
                    && change != dynamic
                {
                    marks.dynamics.push((column, change));
                    dynamic = change;
                }
            }
            let endings = marks.repeat.endings;
            if endings != 0 {
                marks.ending = Some(Ending {
                    label: (endings != previous_endings).then(|| ending_label(endings)),
                    closed: marks.repeat.end_repeats.is_some(),
                });
            }
            previous_endings = endings;
            marks
        })
        .collect()
}

/// "1." for the first ending, "1., 3." for an ending taken on passes 1 and 3.
fn ending_label(endings: u8) -> String {
    (0..8_u8)
        .filter(|bit| endings & (1 << bit) != 0)
        .map(|bit| format!("{}.", bit.saturating_add(1)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Which rows of marks a system has above its top staff, from the staff upwards: fermatas,
/// ending brackets, tempo marks, section names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Rows {
    fermata: bool,
    endings: bool,
    tempo: bool,
    section: bool,
}

impl Rows {
    /// The rows the measures of one system need.
    pub(crate) fn of(marks: &[MeasureMarks]) -> Rows {
        Rows {
            fermata: marks.iter().any(|m| m.fermata.is_some()),
            endings: marks.iter().any(|m| m.ending.is_some()),
            tempo: marks.iter().any(MeasureMarks::has_tempo),
            section: marks.iter().any(|m| m.section.is_some()),
        }
    }

    /// How much room the rows take above the system.
    pub(crate) fn height(self) -> f32 {
        let rows = [self.fermata, self.endings, self.tempo, self.section]
            .into_iter()
            .filter(|&row| row)
            .count();
        rows as f32 * ROW_HEIGHT
    }

    /// The baseline of the `n`-th row (0 nearest the staff) above `top`.
    fn baseline(n: usize, top: f32) -> f32 {
        top - FIRST_ROW - n as f32 * ROW_HEIGHT
    }

    fn ending_row(self) -> usize {
        usize::from(self.fermata)
    }

    fn tempo_row(self) -> usize {
        self.ending_row().saturating_add(usize::from(self.endings))
    }

    fn section_row(self) -> usize {
        self.tempo_row().saturating_add(usize::from(self.tempo))
    }
}

/// A measure as placed in its system.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Placed<'a> {
    pub(crate) layout: &'a MeasureLayout,
    pub(crate) marks: &'a MeasureMarks,
    /// The measure's left edge (its barline) and right edge.
    pub(crate) left: f32,
    pub(crate) right: f32,
    /// Where the columns start (after a start repeat sign) and their stretch.
    pub(crate) content: f32,
    pub(crate) stretch: f32,
}

/// Draws a measure's marks above its top staff (`top`) and its dynamics.
pub(crate) fn draw(
    placed: Placed<'_>,
    rows: Rows,
    tops: Tops,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let marks = placed.marks;
    let top = tops.notation.unwrap_or(tops.tab);
    let (xs, _) = placed
        .layout
        .column_positions(placed.content, placed.stretch);
    if let Some(fermata) = marks.fermata {
        let x = xs.first().copied().unwrap_or(placed.content);
        let glyph = Glyph::Fermata(fermata);
        let bbox = ctx.fonts.music.bbox(glyph);
        ops.push(DrawOp::Glyph {
            glyph,
            at: Point::new(x - (bbox.x_min + bbox.x_max) / 2.0, Rows::baseline(0, top)),
            scale: 1.0,
            color: ColorToken::Notation,
        });
    }
    if let Some(ending) = &marks.ending {
        text::draw_ending(
            placed.left,
            placed.right,
            ending,
            Rows::baseline(rows.ending_row(), top),
            ops,
        );
    }
    let tempo_y = Rows::baseline(rows.tempo_row(), top);
    let start = placed.left + 0.3;
    // The feel mark follows a tempo mark at the measure start.
    let mut feel_x = start;
    if let Some(tempo) = marks.tempo {
        feel_x += text::draw_tempo(start, tempo_y, tempo, ctx, ops) + FEEL_GAP;
    }
    for &(column, tempo) in &marks.column_tempos {
        // A beat's change at the measure start is written where a measure's would be.
        if column == 0 && marks.tempo.is_none() {
            feel_x += text::draw_tempo(start, tempo_y, tempo, ctx, ops) + FEEL_GAP;
        } else {
            let x = xs.get(column).copied().unwrap_or(placed.left) - 0.5;
            text::draw_tempo(x, tempo_y, tempo, ctx, ops);
        }
    }
    if let Some(feel) = marks.feel {
        feel::draw_feel(feel_x, tempo_y, feel, ctx, ops);
    }
    if let Some(title) = &marks.section {
        text::draw_section(
            placed.left,
            Rows::baseline(rows.section_row(), top),
            title,
            ctx,
            ops,
        );
    }
    for &(column, dynamic) in &marks.dynamics {
        let Some(&x) = xs.get(column) else {
            continue;
        };
        let y = dynamic_baseline(placed.layout, column, tops, ctx);
        let glyph = Glyph::Dynamic(dynamic);
        let bbox = ctx.fonts.music.bbox(glyph);
        ops.push(DrawOp::Glyph {
            glyph,
            at: Point::new(x - (bbox.x_min + bbox.x_max) / 2.0, y),
            scale: 1.0,
            color: ColorToken::Notation,
        });
    }
}

/// Where a dynamic under `column` goes: below the notation staff (clear of low notes and
/// of the tab below), or below the tab and its P.M./let ring rows when there is no notation.
fn dynamic_baseline(layout: &MeasureLayout, column: usize, tops: Tops, ctx: &Context<'_>) -> f32 {
    match tops.notation {
        Some(notation_top) => {
            let bottom = notation_top + crate::notation::staff().height();
            let below_head = layout.lowest_head(column).map_or(f32::MIN, |position| {
                notation_top + 4.0 - position as f32 * 0.5 + DYNAMIC_BELOW_HEAD
            });
            let y = (bottom + DYNAMIC_BELOW_STAFF).max(below_head);
            if tops.show_tab {
                y.min(tops.tab - DYNAMIC_ABOVE_TAB)
            } else {
                y
            }
        }
        None => {
            let lanes = layout.under_staff_lanes() as f32;
            tops.tab + ctx.staff.height() + TAB_FIRST_LANE + lanes * TAB_LANE_GAP + TAB_DYNAMIC_DROP
        }
    }
}

/// The text over an end repeat that is played more than twice ("x3").
pub(crate) fn repeat_count(marks: &MeasureMarks) -> Option<String> {
    marks
        .repeat
        .end_repeats
        .filter(|&jumps| jumps > 1)
        .map(|jumps| format!("x{}", u16::from(jumps).saturating_add(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endings_are_labelled_by_their_passes() {
        assert_eq!(ending_label(0b1), "1.");
        assert_eq!(ending_label(0b101), "1., 3.");
        assert_eq!(ending_label(0b1000_0000), "8.");
    }

    #[test]
    fn rows_stack_from_the_staff_up() {
        let rows = Rows {
            fermata: false,
            endings: true,
            tempo: false,
            section: true,
        };
        assert_eq!((rows.tempo_row(), rows.section_row()), (1, 1));
        assert_eq!(rows.height(), 2.0 * ROW_HEIGHT);
        assert_eq!(Rows::default().height(), 0.0);
        assert!(Rows::baseline(1, 10.0) < Rows::baseline(0, 10.0));
        let with_fermata = Rows {
            fermata: true,
            ..rows
        };
        assert_eq!(
            (with_fermata.tempo_row(), with_fermata.section_row()),
            (2, 2)
        );
        assert_eq!(with_fermata.height(), 3.0 * ROW_HEIGHT);
    }

    #[test]
    fn a_repeat_count_is_shown_from_three_passes() {
        let mut marks = MeasureMarks::default();
        assert_eq!(repeat_count(&marks), None);
        marks.repeat.end_repeats = Some(1);
        assert_eq!(repeat_count(&marks), None);
        marks.repeat.end_repeats = Some(3);
        assert_eq!(repeat_count(&marks), Some("x4".to_owned()));
    }
}
