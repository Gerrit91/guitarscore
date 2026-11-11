//! A measure's ornaments that are not tied to one note: the whammy line and vibrato in the
//! lanes above the tab (or a notation-only staff), the beat effects (pick strokes, tapping,
//! fade-ins), beat-level legato slurs, and the palm-mute and let-ring rows below the staff.

use super::{Column, Item, MeasureLayout, Tops};
use crate::Context;
use crate::draw::DrawOp;
use crate::ornaments;

/// Gap between a column's highest note (or mark) and a beat effect above it.
const BEAT_CLEARANCE: f32 = 0.6;
/// How far below a column's highest point a notation-only staff's ornament lanes start
/// (the lowest lane, the trill's, is 2.2 above it).
pub(super) const LANE_CLEARANCE: f32 = 1.4;

impl MeasureLayout {
    /// The whammy line and the vibrato of each column, in the lanes above `lanes` (one line
    /// per column).
    pub(super) fn draw_column_ornaments(
        &self,
        xs: &[f32],
        right: f32,
        lanes: &[f32],
        ctx: &Context<'_>,
        ops: &mut Vec<DrawOp>,
    ) {
        let span_end = |index: usize| xs.get(index.saturating_add(1)).copied().unwrap_or(right);
        for (index, column) in self.columns.iter().enumerate() {
            let (Some(&at), Some(&top)) = (xs.get(index), lanes.get(index)) else {
                continue;
            };
            if let Some(whammy) = &column.marks.whammy {
                ornaments::draw_whammy(at, span_end(index), top, whammy, ctx, ops);
            }
            if let Some(vibrato) = column.marks.vibrato {
                ornaments::draw_vibrato(at, span_end(index), top, vibrato, ops);
            }
        }
    }

    /// Pick strokes, tapping, slapping, popping and fade-ins: in their row above the
    /// notation staff (higher over a high note), or in their lane above the tab.
    pub(super) fn draw_beat_effects(
        &self,
        xs: &[f32],
        right: f32,
        ceilings: &[f32],
        tops: Tops,
        ctx: &Context<'_>,
        ops: &mut Vec<DrawOp>,
    ) {
        if tops.notation.is_none() && !tops.show_tab {
            return;
        }
        let span_end = |index: usize| xs.get(index.saturating_add(1)).copied().unwrap_or(right);
        for (index, column) in self.columns.iter().enumerate() {
            let Some(&at) = xs.get(index) else {
                continue;
            };
            if !column.marks.has_beat_lane() {
                continue;
            }
            let ceiling = ceilings.get(index).copied().unwrap_or(f32::MAX);
            let bottom = tops.beat_bottom.min(ceiling - BEAT_CLEARANCE);
            ornaments::draw_beat_marks(at, span_end(index), bottom, &column.marks, ctx, ops);
        }
    }

    /// Whether a column has beat effects drawn above the staff.
    pub(crate) fn has_beat_effects(&self) -> bool {
        self.columns.iter().any(|c| c.marks.has_beat_lane())
    }

    /// Whether a column has ornaments in the tab's lanes: a trill, a pinch harmonic's
    /// label, a vibrato or a whammy line.
    pub(crate) fn has_lane_ornaments(&self) -> bool {
        self.columns.iter().any(|column| {
            column.marks.vibrato.is_some()
                || column.marks.whammy.is_some()
                || column.items.iter().any(|item| match item {
                    Item::Fret(fret) => {
                        fret.marks.trill.is_some()
                            || fret
                                .marks
                                .harmonic
                                .is_some_and(|h| h.kind == gscore_model::HarmonicKind::Pinch)
                    }
                    Item::Rest { .. } => false,
                })
        })
    }

    /// One slur over each contiguous beat-level legato run.
    pub(super) fn draw_legato(&self, xs: &[f32], top: f32, ops: &mut Vec<DrawOp>) {
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

    /// How many rows of marks (tuplets on a tab without notation when `tuplets`, palm
    /// mute, let ring) this measure has below the tab staff.
    pub(crate) fn under_staff_lanes(&self, tuplets: bool) -> usize {
        let marks = [UnderRow::PalmMute, UnderRow::LetRing]
            .into_iter()
            .filter(|row| self.columns.iter().any(|column| row.matches(column)))
            .count();
        marks.saturating_add(usize::from(tuplets && self.has_tuplets()))
    }

    /// Whether a beat of this measure is in a tuplet.
    fn has_tuplets(&self) -> bool {
        self.columns.iter().any(|column| column.tuplet.is_some())
    }

    /// The rows below the staff: tuplets first (when `tuplets`: a tab without a notation
    /// staff to carry them), then palm mute and let ring.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_under_staff(
        &self,
        xs: &[f32],
        right: f32,
        top: f32,
        tuplets: bool,
        ctx: &Context<'_>,
        ops: &mut Vec<DrawOp>,
    ) {
        let mut lane = 0;
        if tuplets && self.has_tuplets() {
            self.draw_tab_tuplets(
                xs,
                right,
                ornaments::under_staff_baseline(0, top, ctx),
                ctx,
                ops,
            );
            lane = 1;
        }
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
