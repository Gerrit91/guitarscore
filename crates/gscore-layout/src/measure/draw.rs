//! Drawing a laid-out measure: its notes and the ornaments that connect them.
//!
//! Kept apart from the column layout in the parent module: this file only positions and
//! paints, while `measure/mod.rs` decides the widths.

use gscore_model::Dynamic;

use super::{
    Column, GRACE_GAP, GRACE_SCALE, Grace, Item, MEASURE_NUMBER_RISE, MeasureLayout, Tops,
};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, Rect, TextAnchor, TextFont};
use crate::glyph::{Flag, Glyph};
use crate::notation::{self, BeamPlan, Chord, Head, NotationItem};
use crate::ornaments::{self, Fret};
use crate::place::{BeatPlace, MeasurePlace};

/// Draws the ties that continue a note across a barline, from `origin` to `target` (the next
/// measure). Within a system the tie runs from notehead to notehead; across a system break,
/// short fragments point out at the end of the line and in at the start of the next.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_barline_ties(
    origin: &MeasureLayout,
    origin_x: f32,
    origin_stretch: f32,
    origin_tops: Tops,
    target: &MeasureLayout,
    target_x: f32,
    target_stretch: f32,
    target_tops: Tops,
    same_system: bool,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let Some(first) = target.columns.first() else {
        return;
    };
    let (origin_xs, origin_right) = origin.column_positions(origin_x, origin_stretch);
    let (target_xs, _) = target.column_positions(target_x, target_stretch);
    let Some(&target_first_x) = target_xs.first() else {
        return;
    };
    let font = &ctx.fonts.music;
    let thickness = font.engraving().stem_thickness;

    if let (Some(origin_top), Some(target_top)) = (origin_tops.notation, target_tops.notation) {
        for voice in 0..first.notation.len() {
            let Some(NotationItem::Chord(dest)) =
                first.notation.get(voice).and_then(Option::as_ref)
            else {
                continue;
            };
            for head in dest.heads.iter().filter(|head| head.tied) {
                let Some((origin_index, origin_chord, origin_head)) =
                    last_head(origin, voice, head.position)
                else {
                    continue;
                };
                let Some(&ox) = origin_xs.get(origin_index) else {
                    continue;
                };
                let up = dest.stem_up();
                let width = font.advance(origin_chord.head_glyph(origin_head));
                let from_x = ox + width / 2.0 + 0.1;
                let to_x = target_first_x - font.advance(dest.head_glyph(head)) / 2.0 - 0.1;
                let from_y = notation::y_of(origin_top, origin_head.position);
                let to_y = notation::y_of(target_top, head.position);
                if same_system {
                    notation::draw_tie_curve(from_x, from_y, to_x, to_y, up, thickness, ops);
                } else {
                    let out = (from_x + 1.2).min(origin_right - 0.2);
                    notation::draw_tie_curve(from_x, from_y, out, from_y, up, thickness, ops);
                    let into = (to_x - 1.2).max(target_x);
                    notation::draw_tie_curve(into, to_y, to_x, to_y, up, thickness, ops);
                }
            }
        }
    }

    if origin_tops.show_tab && target_tops.show_tab {
        for item in &first.items {
            let Item::Fret(dest) = item else {
                continue;
            };
            if !dest.tie_destination {
                continue;
            }
            let Some((origin_index, source)) = last_fret(origin, dest.line) else {
                continue;
            };
            let Some(&ox) = origin_xs.get(origin_index) else {
                continue;
            };
            if same_system {
                ornaments::draw_tie(ox, target_first_x, origin_tops.tab, source, dest, ctx, ops);
            } else {
                let from_x = ox + source.half_width() + 0.05;
                let y = origin_tops.tab + ctx.staff.line_y(source.line);
                ornaments::draw_tie_fragment(from_x, from_x + 1.2, y, thickness, ops);
                let to_x = target_first_x;
                let target_y = target_tops.tab + ctx.staff.line_y(dest.line);
                let into = (to_x - 1.2).max(target_x);
                ornaments::draw_tie_fragment(into, to_x, target_y, thickness, ops);
            }
        }
    }
}

/// The beams a written value needs (8 = one flag, 16 = two, ...).
fn beams_for_value(value: u16) -> u8 {
    match value {
        8 => 1,
        16 => 2,
        32 => 3,
        64 => 4,
        128 => 5,
        _ => 0,
    }
}

/// A small grace-note head with a stem, flag and (unless it is an appoggiatura) the slash
/// of an acciaccatura.
fn draw_grace_head(
    x: f32,
    top: f32,
    position: i32,
    grace: &Grace,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let font = &ctx.fonts.music;
    let glyph = if grace.dead {
        Glyph::NoteheadX
    } else {
        Glyph::NoteheadBlack
    };
    let width = font.advance(glyph) * GRACE_SCALE;
    let left = x - width / 2.0;
    let y = notation::y_of(top, position);
    ops.push(DrawOp::Glyph {
        glyph,
        at: Point::new(left, y),
        scale: GRACE_SCALE,
        color: ColorToken::Notation,
    });
    let up = position < notation::MIDDLE;
    let thickness = font.engraving().stem_thickness * GRACE_SCALE;
    let stem_x = if up {
        left + width - thickness / 2.0
    } else {
        left + thickness / 2.0
    };
    let length = 2.8 * GRACE_SCALE;
    let end = if up { y - length } else { y + length };
    ops.push(DrawOp::Line {
        from: Point::new(stem_x, y),
        to: Point::new(stem_x, end),
        thickness,
        color: ColorToken::Notation,
    });
    if let Some(flag) = Flag::new(beams_for_value(grace.value), up) {
        ops.push(DrawOp::Glyph {
            glyph: Glyph::Flag(flag),
            at: Point::new(stem_x - thickness / 2.0, end),
            scale: GRACE_SCALE,
            color: ColorToken::Notation,
        });
    }
    if !grace.on_beat {
        // The slash of an acciaccatura crosses the stem.
        ops.push(DrawOp::Line {
            from: Point::new(left - 0.15, y + 0.75),
            to: Point::new(left + width + 0.35, y - 0.75),
            thickness: 0.1,
            color: ColorToken::Notation,
        });
    }
}

/// A small grace fret number on the tab, with the acciaccatura slash.
fn draw_grace_text(
    x: f32,
    top: f32,
    line: usize,
    grace: &Grace,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let size = ctx.options.fret_size * GRACE_SCALE;
    let line_y = top + ctx.staff.line_y(line);
    let width = ctx.fonts.text.width(&grace.text, size);
    let cap = ctx.fonts.text.cap_height * size;
    ops.push(DrawOp::Rect {
        rect: Rect {
            origin: Point::new(x - width / 2.0 - 0.1, line_y - cap / 2.0 - 0.1),
            width: width + 0.2,
            height: cap + 0.2,
        },
        color: ColorToken::Paper,
    });
    ops.push(DrawOp::Text {
        text: grace.text.clone(),
        at: Point::new(x, line_y + cap / 2.0),
        size,
        font: TextFont::Regular,
        anchor: TextAnchor::Middle,
        color: ColorToken::FretNumber,
    });
    if !grace.on_beat {
        ops.push(DrawOp::Line {
            from: Point::new(x - width / 2.0 - 0.2, line_y + cap / 2.0 + 0.2),
            to: Point::new(x + width / 2.0 + 0.2, line_y - cap / 2.0 - 0.2),
            thickness: 0.1,
            color: ColorToken::Notation,
        });
    }
}

/// The last chord of `voice` in `measure` with a notehead at `position`.
fn last_head(
    measure: &MeasureLayout,
    voice: usize,
    position: i32,
) -> Option<(usize, &Chord, &Head)> {
    measure
        .columns
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, column)| {
            let item = column.notation.get(voice)?.as_ref()?;
            let NotationItem::Chord(chord) = item else {
                return None;
            };
            chord
                .heads
                .iter()
                .find(|head| head.position == position)
                .map(|head| (index, chord, head))
        })
}

/// The last fret number on `line` in `measure`, ignoring hidden tie continuations.
fn last_fret(measure: &MeasureLayout, line: usize) -> Option<(usize, &Fret)> {
    measure
        .columns
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, column)| {
            column.items.iter().find_map(|item| match item {
                Item::Fret(fret) if fret.line == line && !fret.tie_destination => {
                    Some((index, fret))
                }
                _ => None,
            })
        })
}

impl MeasureLayout {
    /// Where each column's center lies when the measure starts at `x` and its columns are
    /// stretched by `stretch`, and where the measure ends.
    pub(crate) fn column_positions(&self, x: f32, stretch: f32) -> (Vec<f32>, f32) {
        let mut cursor = x
            + self.key_signature.as_ref().map_or(0.0, |k| k.width)
            + self.time_signature.as_ref().map_or(0.0, |t| t.width)
            + self.lead;
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
        let mut signature_x = x;
        if let Some(key_signature) = &self.key_signature {
            key_signature.draw_at(signature_x, tops.notation.unwrap_or(top), ctx, ops);
            signature_x += key_signature.width;
        }
        if let Some(time_signature) = &self.time_signature {
            // With notation, the signature is written there only, as in Guitar Pro.
            match tops.notation {
                Some(notation_top) => {
                    let middle = notation_top + notation::staff().middle();
                    time_signature.draw_at(signature_x, middle, 1.0, ctx, ops);
                }
                None => time_signature.draw(signature_x, top, ctx, ops),
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
            let voices = self
                .columns
                .iter()
                .map(|column| column.notation.len())
                .max()
                .unwrap_or(0);
            for voice in 0..voices {
                let placed: Vec<BeamPlan<'_>> = self
                    .columns
                    .iter()
                    .zip(&xs)
                    .filter_map(|(column, &x)| {
                        let item = column.notation.get(voice)?.as_ref()?;
                        let shift = if voice > 0 {
                            column.notation_offset
                        } else {
                            0.0
                        };
                        Some(BeamPlan {
                            x: x + shift,
                            item,
                            beat: column.beat,
                        })
                    })
                    .collect();
                notation::draw_measure(&placed, notation_top, ctx, ops);
            }
        }
        // Grace notes sit just left of their column's main note, on both staves.
        for (index, column) in self.columns.iter().enumerate() {
            if let Some(&at) = xs.get(index) {
                self.draw_graces(at, column, tops, ctx, ops);
            }
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
            if let Some(whammy) = &column.marks.whammy {
                ornaments::draw_whammy(at, span_end(index), top, whammy, ctx, ops);
            }
            if let Some(vibrato) = column.marks.vibrato {
                ornaments::draw_vibrato(at, span_end(index), top, vibrato, ops);
            }
        }
        self.draw_legato(&xs, top, ops);
        self.draw_under_staff(&xs, right, top, ctx, ops);
    }

    /// Grace notes of one column, just left of its main note, on the notation staff and the
    /// tab. Notes of one grace beat share a stem position.
    fn draw_graces(
        &self,
        x: f32,
        column: &Column,
        tops: Tops,
        ctx: &Context<'_>,
        ops: &mut Vec<DrawOp>,
    ) {
        if column.graces.is_empty() {
            return;
        }
        let mut groups: Vec<usize> = Vec::new();
        for grace in &column.graces {
            if !groups.contains(&grace.group) {
                groups.push(grace.group);
            }
        }
        let round = |graces: &[&Grace], notation: bool| -> f32 {
            graces
                .iter()
                .filter(|grace| {
                    if notation {
                        grace.position.is_some()
                    } else {
                        grace.line.is_some()
                    }
                })
                .map(|grace| {
                    if notation {
                        ctx.fonts.music.advance(Glyph::NoteheadBlack) * GRACE_SCALE
                    } else {
                        ctx.fonts
                            .text
                            .width(&grace.text, ctx.options.fret_size * GRACE_SCALE)
                    }
                })
                .fold(0.0, f32::max)
        };
        let mut at = x - column.grace_width;
        for group in groups {
            let notes: Vec<&Grace> = column
                .graces
                .iter()
                .filter(|grace| grace.group == group)
                .collect();
            let width = round(&notes, true).max(round(&notes, false));
            let center = at + width / 2.0;
            if let Some(notation_top) = tops.notation {
                for grace in &notes {
                    if let Some(position) = grace.position {
                        draw_grace_head(center, notation_top, position, grace, ctx, ops);
                    }
                }
            }
            if tops.show_tab {
                for grace in &notes {
                    if let Some(line) = grace.line {
                        draw_grace_text(center, tops.tab, line, grace, ctx, ops);
                    }
                }
            }
            at += width + GRACE_GAP;
        }
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

    /// Each column's dynamic and tempo change, in order.
    pub(crate) fn column_marks(&self) -> impl Iterator<Item = (Option<Dynamic>, Option<u32>)> {
        self.columns.iter().map(|c| (c.dynamic, c.tempo))
    }

    /// The lowest notehead's staff position in column `index` of the notation staff.
    pub(crate) fn lowest_head(&self, index: usize) -> Option<i32> {
        self.columns
            .get(index)?
            .notation
            .iter()
            .flatten()
            .filter_map(|item| match item {
                NotationItem::Chord(chord) => chord.heads.first().map(|head| head.position),
                NotationItem::Rest { .. } => None,
            })
            .min()
    }

    /// How many rows of marks (palm mute, let ring) this measure has below the tab staff.
    pub(crate) fn under_staff_lanes(&self) -> usize {
        [UnderRow::PalmMute, UnderRow::LetRing]
            .into_iter()
            .filter(|row| self.columns.iter().any(|column| row.matches(column)))
            .count()
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
