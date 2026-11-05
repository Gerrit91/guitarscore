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
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
use crate::glyph::Glyph;
use crate::measure::{MeasureLayout, Tops};
use crate::staff::Staff;

/// Distance from the top staff line up to the baseline of the first row of marks.
const FIRST_ROW: f32 = 2.6;
/// Space between a tempo mark and the feel mark after it.
const FEEL_GAP: f32 = 1.0;
/// Distance between two rows of marks.
const ROW_HEIGHT: f32 = 2.4;
/// With notation: the row of beat effects (pick strokes, tapping, fade-ins) nearest the
/// notation staff, and how far above the band below it their bottom is. Without notation
/// their lane is above the tab's own ornament lanes, so the row is higher.
const BEAT_ROW: f32 = 2.4;
pub(crate) const BEAT_ABOVE: f32 = 1.8;
const TAB_BEAT_ROW: f32 = 3.8;
/// A notation staff without a tab: the band for the ornaments the tab would carry (trill,
/// vibrato, whammy line), and how far below the staff's top line their lanes are measured
/// from (the tab's lanes are measured from its top line).
const ORNAMENT_BAND: f32 = 3.6;
pub(crate) const LANES_DROP: f32 = 1.0;
/// Dynamics: their baseline at least this far below the notation staff's bottom line, their
/// top this far below the lowest ink of every note they span, and their baseline this far
/// above the tab staff below them (the gap between the staves grows for that).
const DYNAMIC_BELOW_STAFF: f32 = 3.2;
const DYNAMIC_CLEARANCE: f32 = 0.6;
pub(crate) const DYNAMIC_ABOVE_TAB: f32 = 2.2;
/// A column's ink reaches this far left of its x (accidentals) and right of it (the head).
const COLUMN_LEFT_REACH: f32 = 1.8;
const COLUMN_RIGHT_REACH: f32 = 0.8;
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
    /// Whether the measure is in free time ("Free time" over it, its time signature in
    /// parentheses).
    pub(crate) free_time: bool,
    /// The fermata over the measure's first note, if any.
    pub(crate) fermata: Option<Fermata>,
    /// The feel, written where it changes (and at the start unless straight).
    pub(crate) feel: Option<Feel>,
    /// The ending bracket over this measure, if it is an alternative ending.
    pub(crate) ending: Option<Ending>,
    /// Whether a column has beat effects above the staff (pick strokes, tapping, fade-ins).
    pub(crate) beat_effects: bool,
    /// Whether a column has ornaments in the tab's lanes (trill, vibrato, whammy line).
    pub(crate) lane_ornaments: bool,
    /// Whether a column carries Guitar Pro's beat text.
    pub(crate) beat_text: bool,
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
        self.tempo.is_some()
            || !self.column_tempos.is_empty()
            || self.feel.is_some()
            || self.free_time
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
                free_time: song.free_time(index).unwrap_or(false),
                fermata: song.fermata(index),
                beat_effects: measure.has_beat_effects(),
                lane_ornaments: measure.has_lane_ornaments(),
                beat_text: measure.has_beat_text(),
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

/// Which rows of marks a system has above its top staff, from the staff upwards: on a
/// notation staff the ornaments a missing tab would carry and the beat effects, then
/// fermatas, ending brackets, tempo marks, section names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Rows {
    ornaments: bool,
    beat_effects: bool,
    /// Whether the top staff is a tab (no notation), which puts the beat effects higher.
    tab_only: bool,
    beat_text: bool,
    fermata: bool,
    endings: bool,
    tempo: bool,
    section: bool,
    /// Whether these are the rows over a system's top track: only there go the marks that
    /// belong to the whole system (endings, tempo, sections).
    system: bool,
}

impl Rows {
    /// The rows the measures of one system need; `notation` and `tab` are the staves shown.
    /// `system` is false for a track below a system's top one: it gets only its own rows
    /// (beat effects, beat text, fermatas).
    pub(crate) fn of(marks: &[MeasureMarks], notation: bool, tab: bool, system: bool) -> Rows {
        Rows {
            system,
            ornaments: notation && !tab && marks.iter().any(|m| m.lane_ornaments),
            beat_effects: marks.iter().any(|m| m.beat_effects),
            tab_only: !notation,
            beat_text: marks.iter().any(|m| m.beat_text),
            fermata: marks.iter().any(|m| m.fermata.is_some()),
            endings: system && marks.iter().any(|m| m.ending.is_some()),
            tempo: system && marks.iter().any(MeasureMarks::has_tempo),
            section: system && marks.iter().any(|m| m.section.is_some()),
        }
    }

    /// How much room the rows take above the system.
    pub(crate) fn height(self) -> f32 {
        let rows = [
            self.beat_text,
            self.fermata,
            self.endings,
            self.tempo,
            self.section,
        ]
        .into_iter()
        .filter(|&row| row)
        .count();
        rows as f32 * ROW_HEIGHT + self.band()
    }

    /// The ornament band of a notation staff without a tab, if the system needs it.
    pub(crate) fn ornament_band(self) -> f32 {
        if self.ornaments { ORNAMENT_BAND } else { 0.0 }
    }

    /// The ornament band and the beat effects' row, below the rows of text marks.
    fn band(self) -> f32 {
        let beat = match (self.beat_effects, self.tab_only) {
            (false, _) => 0.0,
            (true, false) => BEAT_ROW,
            (true, true) => TAB_BEAT_ROW,
        };
        self.ornament_band() + beat
    }

    /// The baseline of the `n`-th row (0 nearest the staff) above `top`.
    fn baseline(self, n: usize, top: f32) -> f32 {
        top - self.band() - FIRST_ROW - n as f32 * ROW_HEIGHT
    }

    /// The beat text sits nearest the staff, above the measure numbers.
    fn beat_text_row(self) -> usize {
        0
    }

    fn fermata_row(self) -> usize {
        usize::from(self.beat_text)
    }

    fn ending_row(self) -> usize {
        usize::from(self.beat_text).saturating_add(usize::from(self.fermata))
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
    let top = tops.top_line();
    let (xs, _) = placed
        .layout
        .column_positions(placed.content, placed.stretch);
    // Guitar Pro's beat text, left-aligned at its column.
    for (column, text) in placed.layout.beat_texts() {
        let Some(&x) = xs.get(column) else {
            continue;
        };
        ops.push(DrawOp::Text {
            text: text.to_owned(),
            at: Point::new(x, rows.baseline(rows.beat_text_row(), top)),
            size: 1.0,
            font: TextFont::Regular,
            anchor: TextAnchor::Start,
            color: ColorToken::Annotation,
        });
    }
    if let Some(fermata) = marks.fermata {
        let x = xs.first().copied().unwrap_or(placed.content);
        let glyph = Glyph::Fermata(fermata);
        let bbox = ctx.fonts.music.bbox(glyph);
        ops.push(DrawOp::Glyph {
            glyph,
            at: Point::new(
                x - (bbox.x_min + bbox.x_max) / 2.0,
                rows.baseline(rows.fermata_row(), top),
            ),
            scale: 1.0,
            color: ColorToken::Notation,
        });
    }
    if !rows.system {
        draw_dynamics(placed, &xs, tops, ctx, ops);
        return;
    }
    if let Some(ending) = &marks.ending {
        text::draw_ending(
            placed.left,
            placed.right,
            ending,
            rows.baseline(rows.ending_row(), top),
            ops,
        );
    }
    let tempo_y = rows.baseline(rows.tempo_row(), top);
    let mut start = placed.left + 0.3;
    if marks.free_time {
        // Guitar Pro writes "Free time" above a measure whose metronome is off.
        ops.push(DrawOp::Text {
            text: "Free time".to_owned(),
            at: Point::new(start, tempo_y),
            size: 1.0,
            font: TextFont::Regular,
            anchor: TextAnchor::Start,
            color: ColorToken::Annotation,
        });
        start += 5.2;
    }
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
            rows.baseline(rows.section_row(), top),
            title,
            ctx,
            ops,
        );
    }
    draw_dynamics(placed, &xs, tops, ctx, ops);
}

/// The dynamics under the measure's columns at `xs`.
fn draw_dynamics(
    placed: Placed<'_>,
    xs: &[f32],
    tops: Tops,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    for &(column, dynamic) in &placed.marks.dynamics {
        let Some(&x) = xs.get(column) else {
            continue;
        };
        let y = dynamic_baseline(placed.layout, column, dynamic, xs, tops, ctx);
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

/// How far below the notation staff's bottom line the baseline of `dynamic` under
/// `column` goes (columns at `xs`): below the lowest ink of every column the mark spans,
/// accidentals and downward stems included, and at least [`DYNAMIC_BELOW_STAFF`].
fn dynamic_depth(
    layout: &MeasureLayout,
    column: usize,
    dynamic: Dynamic,
    xs: &[f32],
    ctx: &Context<'_>,
) -> f32 {
    let bbox = ctx.fonts.music.bbox(Glyph::Dynamic(dynamic));
    let Some(&x) = xs.get(column) else {
        return DYNAMIC_BELOW_STAFF;
    };
    let half = (bbox.x_max - bbox.x_min) / 2.0;
    let (left, right) = (x - half, x + half);
    let deepest = xs
        .iter()
        .enumerate()
        .filter(|&(_, &at)| at + COLUMN_RIGHT_REACH > left && at - COLUMN_LEFT_REACH < right)
        .filter_map(|(index, _)| layout.column_depth(index))
        .fold(f32::MIN, f32::max);
    DYNAMIC_BELOW_STAFF.max(deepest + DYNAMIC_CLEARANCE + bbox.y_max)
}

/// The deepest dynamic baseline of a measure below its notation staff (0 without
/// dynamics), at the measure's natural spacing: a system stretched wider only moves its
/// columns apart, so this is what the gap above the tab must leave room for.
pub(crate) fn dynamics_depth(
    layout: &MeasureLayout,
    marks: &MeasureMarks,
    ctx: &Context<'_>,
) -> f32 {
    if ctx.notation.is_none() {
        return 0.0;
    }
    let (xs, _) = layout.column_positions(0.0, 1.0);
    marks
        .dynamics
        .iter()
        .map(|&(column, dynamic)| dynamic_depth(layout, column, dynamic, &xs, ctx))
        .fold(0.0, f32::max)
}

/// Where a dynamic under `column` goes: below the notation staff, clear of every note it
/// spans (the system leaves room for it above the tab), or below the tab and its P.M./let
/// ring rows when there is no notation.
fn dynamic_baseline(
    layout: &MeasureLayout,
    column: usize,
    dynamic: Dynamic,
    xs: &[f32],
    tops: Tops,
    ctx: &Context<'_>,
) -> f32 {
    match tops.notation {
        Some(notation_top) => {
            let bottom = notation_top + crate::notation::staff().height();
            let y = bottom + dynamic_depth(layout, column, dynamic, xs, ctx);
            if tops.show_tab {
                y
            } else {
                // Without a tab, the P.M. and let ring rows are under the notation staff.
                y + layout.under_staff_lanes() as f32 * TAB_LANE_GAP
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
            ..Rows::default()
        };
        assert_eq!((rows.tempo_row(), rows.section_row()), (1, 1));
        assert_eq!(rows.height(), 2.0 * ROW_HEIGHT);
        assert_eq!(Rows::default().height(), 0.0);
        assert!(rows.baseline(1, 10.0) < rows.baseline(0, 10.0));
        // Beat effects and a notation-only staff's ornaments sit below the text rows.
        let effects = Rows {
            beat_effects: true,
            ornaments: true,
            ..rows
        };
        assert_eq!(effects.height(), rows.height() + BEAT_ROW + ORNAMENT_BAND);
        assert!(effects.baseline(0, 10.0) < rows.baseline(0, 10.0));
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
