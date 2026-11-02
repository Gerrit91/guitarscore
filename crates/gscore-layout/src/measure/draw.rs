//! Drawing a laid-out measure: its notes and the ornaments that connect them.
//!
//! Kept apart from the column layout in the parent module: this file only positions and
//! paints, while `measure/mod.rs` decides the widths.

use super::{Column, Item, MEASURE_NUMBER_RISE, MeasureLayout, Tops};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
use crate::notation::{self, BeamPlan};
use crate::ornaments::{self, Fret};
use crate::place::{BeatPlace, MeasurePlace};

impl MeasureLayout {
    /// Where each column's center lies when the measure starts at `x` and its columns are
    /// stretched by `stretch`, and where the measure ends.
    pub(crate) fn column_positions(&self, x: f32, stretch: f32) -> (Vec<f32>, f32) {
        let mut cursor = x + self.time_signature.as_ref().map_or(0.0, |t| t.width) + self.lead;
        let mut xs = Vec::with_capacity(self.columns.len());
        for column in &self.columns {
            xs.push(cursor);
            cursor += column.space * stretch;
        }
        (xs, cursor)
    }

    /// Where the measure's beats are, for the cursor and clicks.
    pub(crate) fn place(&self, x: f32, stretch: f32, system: usize) -> MeasurePlace {
        let (xs, right) = self.column_positions(x, stretch);
        let beats = self
            .columns
            .iter()
            .zip(&xs)
            .flat_map(|(column, &x)| {
                column
                    .beats
                    .iter()
                    .map(move |&(voice, beat)| BeatPlace { voice, beat, x })
            })
            .collect();
        MeasurePlace {
            measure: self.index,
            system,
            left: x,
            right,
            content: xs.first().copied().unwrap_or(right),
            beats,
        }
    }

    /// Draws the measure's content (not the staff lines or barline) starting at `x`, on
    /// the notation staff and the tab staff below it, each if shown.
    pub(crate) fn draw(
        &self,
        x: f32,
        tops: Tops,
        stretch: f32,
        ctx: &Context<'_>,
        ops: &mut Vec<DrawOp>,
    ) {
        let top = tops.tab;
        // The measure number goes above the topmost staff, as in Guitar Pro.
        ops.push(DrawOp::Text {
            text: self.index.get().saturating_add(1).to_string(),
            at: Point::new(x, tops.notation.unwrap_or(top) - MEASURE_NUMBER_RISE),
            size: ctx.options.measure_number_size,
            font: TextFont::Regular,
            anchor: TextAnchor::Start,
            color: ColorToken::Annotation,
        });
        if let Some(time_signature) = &self.time_signature {
            // With notation, the time signature is written there only, as in Guitar Pro.
            match tops.notation {
                Some(notation_top) => {
                    let middle = notation_top + notation::staff().middle();
                    time_signature.draw_at(x, middle, 1.0, ctx, ops);
                }
                None => time_signature.draw(x, top, ctx, ops),
            }
        }
        // Positions first: ornaments connect neighbouring columns.
        let (xs, right) = self.column_positions(x, stretch);
        // Where each column's space ends: the next column, or the end of the measure.
        let span_end = |index: usize| xs.get(index.saturating_add(1)).copied().unwrap_or(right);
        if tops.show_tab {
            for (index, column) in self.columns.iter().enumerate() {
                let at = xs.get(index).copied().unwrap_or(right);
                for item in &column.items {
                    match item {
                        Item::Fret(fret) => {
                            fret.draw(at, top, ctx, ops);
                            fret.draw_above(at, span_end(index), top, ctx, ops);
                        }
                        Item::Rest { .. } => item.draw(at, top, ctx, ops),
                    }
                }
            }
        }
        if let Some(notation_top) = tops.notation {
            let placed: Vec<BeamPlan<'_>> = self
                .columns
                .iter()
                .zip(&xs)
                .filter_map(|(column, &x)| {
                    column.notation.as_ref().map(|item| BeamPlan {
                        x,
                        item,
                        beat: column.beat,
                    })
                })
                .collect();
            notation::draw_measure(&placed, notation_top, ctx, ops);
        }
        // Everything below belongs to the tab staff.
        if !tops.show_tab {
            return;
        }
        self.draw_links(&xs, top, ctx, ops);
        for (index, column) in self.columns.iter().enumerate() {
            let Some(&at) = xs.get(index) else {
                continue;
            };
            if let Some(whammy) = column.marks.whammy {
                ornaments::draw_whammy(at, span_end(index), top, whammy, ctx, ops);
            }
            if let Some(vibrato) = column.marks.vibrato {
                ornaments::draw_vibrato(at, span_end(index), top, vibrato, ops);
            }
        }
        self.draw_legato(&xs, top, ops);
        self.draw_under_staff(&xs, right, top, ctx, ops);
    }

    /// Slurs and lines between one note and the next one on the same string.
    fn draw_links(&self, xs: &[f32], top: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
        for (index, column) in self.columns.iter().enumerate() {
            let Some(&at) = xs.get(index) else {
                continue;
            };
            for item in &column.items {
                let Item::Fret(origin) = item else {
                    continue;
                };
                if origin.tie_destination {
                    continue;
                }
                let Some((target_index, target)) = next_on_line(&self.columns, index, origin.line)
                else {
                    continue;
                };
                let Some(&target_x) = xs.get(target_index) else {
                    continue;
                };
                if target.tie_destination {
                    ornaments::draw_tie(at, target_x, top, origin, target, ctx, ops);
                } else if origin.marks.hammer || origin.marks.direct_slide().is_some() {
                    ornaments::draw_link(at, target_x, top, origin, target, ctx, ops);
                }
            }
        }
    }

    /// One slur over each contiguous beat-level legato run.
    fn draw_legato(&self, xs: &[f32], top: f32, ops: &mut Vec<DrawOp>) {
        let active =
            |column: &Column| column.marks.legato_origin || column.marks.legato_destination;
        let mut index = 0;
        while let Some(column) = self.columns.get(index) {
            if !active(column) {
                index = index.saturating_add(1);
                continue;
            }
            let start = index;
            let mut end = index;
            while self.columns.get(end.saturating_add(1)).is_some_and(active) {
                end = end.saturating_add(1);
            }
            if let (Some(&start_x), Some(&end_x)) = (xs.get(start), xs.get(end)) {
                let next = xs.get(end.saturating_add(1)).copied();
                let run_end = next.map_or(end_x, |next| (end_x + next) / 2.0);
                ornaments::draw_legato_run(start_x, run_end, top, ops);
            }
            index = end.saturating_add(1);
        }
    }

    /// Palm-mute and let-ring rows below the staff.
    fn draw_under_staff(
        &self,
        xs: &[f32],
        right: f32,
        top: f32,
        ctx: &Context<'_>,
        ops: &mut Vec<DrawOp>,
    ) {
        let mut lane = 0;
        for row in [UnderRow::PalmMute, UnderRow::LetRing] {
            // Only rows that occur in this measure take a lane, so a measure with let ring
            // alone keeps it close to the staff.
            if !self.columns.iter().any(|column| row.matches(column)) {
                continue;
            }
            let mut index = 0;
            while let Some(column) = self.columns.get(index) {
                if !row.matches(column) {
                    index = index.saturating_add(1);
                    continue;
                }
                let start = index;
                let mut end = index;
                while self
                    .columns
                    .get(end.saturating_add(1))
                    .is_some_and(|column| row.matches(column))
                {
                    end = end.saturating_add(1);
                }
                if let Some(&start_x) = xs.get(start) {
                    let end_x = xs
                        .get(end.saturating_add(1))
                        .copied()
                        .map_or(right, |next| {
                            (next + xs.get(end).copied().unwrap_or(next)) / 2.0
                        });
                    ornaments::draw_under_staff_run(
                        start_x - 0.7,
                        end_x,
                        row.label(),
                        lane,
                        top,
                        ctx,
                        ops,
                    );
                }
                index = end.saturating_add(1);
            }
            lane = lane.saturating_add(1);
        }
    }
}

/// A row of marks below the staff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnderRow {
    PalmMute,
    LetRing,
}

impl UnderRow {
    fn label(self) -> &'static str {
        match self {
            UnderRow::PalmMute => "P.M.",
            UnderRow::LetRing => "let ring",
        }
    }

    fn matches(self, column: &Column) -> bool {
        column.items.iter().any(|item| {
            let Item::Fret(fret) = item else {
                return false;
            };
            match self {
                UnderRow::PalmMute => fret.marks.palm_mute,
                UnderRow::LetRing => fret.marks.let_ring,
            }
        })
    }
}

/// The next fret on `line` after column `from`, if there is one in this measure.
fn next_on_line(columns: &[Column], from: usize, line: usize) -> Option<(usize, &Fret)> {
    let mut index = from.saturating_add(1);
    while let Some(column) = columns.get(index) {
        for item in &column.items {
            if let Item::Fret(fret) = item
                && fret.line == line
            {
                return Some((index, fret));
            }
        }
        index = index.saturating_add(1);
    }
    None
}
