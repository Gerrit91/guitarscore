//! Drawing a laid-out measure: its notes and the ornaments that connect them.
//!
//! Kept apart from the column layout in the parent module: this file only positions and
//! paints, while `measure/mod.rs` decides the widths.

use gscore_model::Dynamic;

use super::effects::LANE_CLEARANCE;
use super::{Column, DEAD_SLAP_HALF, Item, MEASURE_NUMBER_RISE, MeasureLayout, Tops};

use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
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
        let mut cursor = x + self.fixed_width();
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
        first_in_system: bool,
        ctx: &Context<'_>,
        ops: &mut Vec<DrawOp>,
    ) {
        let top = tops.tab;
        // The measure number goes above the topmost staff, as in Guitar Pro.
        if tops.numbers {
            ops.push(DrawOp::Text {
                text: self.index.get().saturating_add(1).to_string(),
                at: Point::new(x, tops.notation.unwrap_or(top) - MEASURE_NUMBER_RISE),
                size: ctx.options.measure_number_size,
                font: TextFont::Regular,
                anchor: TextAnchor::Start,
                color: ColorToken::Annotation,
            });
        }
        let mut signature_x = x;
        // A clef change is written before the key signature. On a system's first measure
        // the system-start clef already shows it.
        if self.clef_change
            && !first_in_system
            && let Some(notation_top) = tops.notation
        {
            notation::draw_clef_change(signature_x, notation_top, self.clef, ops);
            signature_x += self.clef_width;
        }
        if let Some(key_signature) = &self.key_signature {
            key_signature.draw_at(signature_x, tops.notation.unwrap_or(top), ctx, ops);
            signature_x += key_signature.width;
        }
        signature_x += self.signature_pad;
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
        // A dead slapped beat is a large cross across each staff instead of its notes.
        for (index, column) in self.columns.iter().enumerate() {
            if !column.marks.dead_slapped {
                continue;
            }
            let Some(&at) = xs.get(index) else {
                continue;
            };
            if tops.show_tab {
                draw_dead_slap(at, top, ctx.staff.height(), ops);
            }
            if let Some(notation_top) = tops.notation {
                draw_dead_slap(at, notation_top, notation::staff().height(), ops);
            }
        }
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
            // Without a notation staff, the tab carries the tuplet numbers itself: a
            // triplet must be visible there too, not only in the standard notation.
            if tops.notation.is_none() {
                self.draw_tab_tuplets(&xs, right, top, ops);
            }
        }
        // The slash-notation staff: the same rhythm, written as slashes.
        if let Some(slash_top) = tops.slash {
            let voices = self
                .columns
                .iter()
                .map(|column| column.slash.len())
                .max()
                .unwrap_or(0);
            for voice in 0..voices {
                let placed: Vec<BeamPlan<'_>> = self
                    .columns
                    .iter()
                    .zip(&xs)
                    .filter_map(|(column, &x)| {
                        Some(BeamPlan {
                            x,
                            item: column.slash.get(voice)?.as_ref()?,
                            beat: column.beat,
                        })
                    })
                    .collect();
                notation::draw_measure(&placed, slash_top, true, ctx, ops);
            }
        }
        // The highest point of each column's notation, so marks above the staff clear it.
        let mut ceilings = vec![f32::MAX; self.columns.len()];
        if let Some(notation_top) = tops.notation {
            let voices = self
                .columns
                .iter()
                .map(|column| column.notation.len())
                .max()
                .unwrap_or(0);
            for voice in 0..voices {
                let (indices, placed): (Vec<usize>, Vec<BeamPlan<'_>>) = self
                    .columns
                    .iter()
                    .zip(&xs)
                    .enumerate()
                    .filter_map(|(index, (column, &x))| {
                        let item = column.notation.get(voice)?.as_ref()?;
                        let shift = if voice > 0 {
                            column.notation_offset
                        } else {
                            0.0
                        };
                        Some((
                            index,
                            BeamPlan {
                                x: x + shift,
                                item,
                                beat: column.beat,
                            },
                        ))
                    })
                    .unzip();
                let highest =
                    notation::draw_measure(&placed, notation_top, !tops.show_tab, ctx, ops);
                for (index, y) in indices.into_iter().zip(highest) {
                    if let Some(slot) = ceilings.get_mut(index) {
                        *slot = slot.min(y);
                    }
                }
            }
        }
        // Grace notes sit just left of their column's main note, on both staves.
        for (index, column) in self.columns.iter().enumerate() {
            if let Some(&at) = xs.get(index) {
                super::graces::draw(at, column, tops, ctx, ops);
            }
        }
        // Without a tab, the ornaments that live above and below it go above and below
        // the notation staff instead (the notation draws its own slurs and slides).
        self.draw_beat_effects(&xs, right, &ceilings, tops, ctx, ops);
        if !tops.show_tab {
            if let Some(notation_top) = tops.notation {
                // Each column's lanes rise over its high notes.
                let lanes: Vec<f32> = ceilings
                    .iter()
                    .map(|&ceiling| tops.lanes.min(ceiling + LANE_CLEARANCE))
                    .collect();
                for (index, column) in self.columns.iter().enumerate() {
                    let (Some(&at), Some(&lane)) = (xs.get(index), lanes.get(index)) else {
                        continue;
                    };
                    for item in &column.items {
                        if let Item::Fret(fret) = item {
                            fret.draw_lane_marks(at, lane, ctx, ops);
                        }
                    }
                }
                self.draw_column_ornaments(&xs, right, &lanes, ctx, ops);
                // The rows below line up with the notation staff's bottom line.
                let under = notation_top + notation::staff().height() - ctx.staff.height();
                self.draw_under_staff(&xs, right, under, ctx, ops);
            }
            return;
        }
        self.draw_links(&xs, top, ctx, ops);
        for (index, column) in self.columns.iter().enumerate() {
            let Some(&at) = xs.get(index) else {
                continue;
            };
            if let Some(direction) = column.marks.brush {
                let frets = column.items.iter().filter_map(|item| match item {
                    Item::Fret(fret) => Some(fret),
                    Item::Rest { .. } => None,
                });
                ornaments::draw_tab_brush(at, frets, top, direction, ctx, ops);
            }
            if let Some(direction) = column.marks.arpeggio {
                let frets = column.items.iter().filter_map(|item| match item {
                    Item::Fret(fret) => Some(fret),
                    Item::Rest { .. } => None,
                });
                ornaments::draw_tab_arpeggio(at, frets, top, direction, ctx, ops);
            }
        }
        self.draw_column_ornaments(&xs, right, &vec![top; xs.len()], ctx, ops);
        self.draw_legato(&xs, top, ops);
        self.draw_under_staff(&xs, right, top, ctx, ops);
    }

    /// Tuplet brackets above a tab staff that has no notation staff to carry them: one
    /// number per beat's group, over the columns it spans.
    fn draw_tab_tuplets(&self, xs: &[f32], right: f32, top: f32, ops: &mut Vec<DrawOp>) {
        let mut index = 0;
        while index < self.columns.len() {
            let Some(column) = self.columns.get(index) else {
                break;
            };
            let Some((enters, _)) = column.tuplet else {
                index = index.saturating_add(1);
                continue;
            };
            let beat = column.beat;
            let start = xs.get(index).copied().unwrap_or(right);
            // The columns of this beat that share the tuplet.
            let mut end = index;
            while self
                .columns
                .get(end.saturating_add(1))
                .is_some_and(|next| next.beat == beat && next.tuplet == column.tuplet)
            {
                end = end.saturating_add(1);
            }
            let last = xs.get(end).copied().unwrap_or(start);
            let bracket = (start - 0.5, last + 0.5);
            let line_y = top - 1.0;
            let hook = 0.25;
            ops.push(DrawOp::Line {
                from: Point::new(bracket.0, line_y + hook),
                to: Point::new(bracket.0, line_y),
                thickness: 0.08,
                color: ColorToken::Notation,
            });
            ops.push(DrawOp::Line {
                from: Point::new(bracket.0, line_y),
                to: Point::new(bracket.1, line_y),
                thickness: 0.08,
                color: ColorToken::Notation,
            });
            ops.push(DrawOp::Line {
                from: Point::new(bracket.1, line_y),
                to: Point::new(bracket.1, line_y + hook),
                thickness: 0.08,
                color: ColorToken::Notation,
            });
            ops.push(DrawOp::Text {
                text: enters.to_string(),
                at: Point::new((bracket.0 + bracket.1) / 2.0, line_y - 0.35),
                size: 0.9,
                font: TextFont::Regular,
                anchor: TextAnchor::Middle,
                color: ColorToken::Notation,
            });
            index = end.saturating_add(1);
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
                } else if origin.marks.pick_slide.is_some() {
                    ornaments::draw_pick_slide(at, target_x, top, origin, target, ctx, ops);
                } else if origin.marks.hammer || origin.marks.direct_slide().is_some() {
                    ornaments::draw_link(at, target_x, top, origin, target, ctx, ops);
                }
            }
        }
    }

    /// Each column's dynamic and tempo change, in order.
    pub(crate) fn column_marks(&self) -> impl Iterator<Item = (Option<Dynamic>, Option<u32>)> {
        self.columns.iter().map(|c| (c.dynamic, c.tempo))
    }

    /// How far the notation reaches above its staff's top line and below its bottom line
    /// (noteheads, ledger lines and stems), in staff spaces; 0 within the staff.
    pub(crate) fn notation_reach(&self) -> (f32, f32) {
        // Staff positions: 0 the bottom line, 8 the top line; a stem is 3.5 spaces long.
        const TOP_LINE: i32 = 8;
        const STEM: f32 = 3.5;
        const HEAD: f32 = 0.5;
        let mut above = 0.0_f32;
        let mut below = 0.0_f32;
        let chords = self
            .columns
            .iter()
            .flat_map(|column| column.notation.iter().flatten())
            .filter_map(|item| match item {
                NotationItem::Chord(chord) => Some(chord),
                NotationItem::Rest { .. } => None,
            });
        for chord in chords {
            let (Some(low), Some(high)) = (chord.heads.first(), chord.heads.last()) else {
                continue;
            };
            let stem_up = chord.stem_up() && chord.value >= 2;
            let stem_down = !chord.stem_up() && chord.value >= 2;
            let top = high.position.saturating_sub(TOP_LINE) as f32 / 2.0
                + HEAD
                + if stem_up { STEM } else { 0.0 };
            let bottom = -(low.position as f32) / 2.0 + HEAD + if stem_down { STEM } else { 0.0 };
            above = above.max(top);
            below = below.max(bottom);
        }
        (above, below)
    }

    /// How far below the notation staff's bottom line the lowest ink of column `index`
    /// goes, in staff spaces (negative inside the staff): its lowest notehead (with an
    /// accent or staccato dot below it) or the end of a stem pointing down. `None` for a
    /// column without notes.
    pub(crate) fn column_depth(&self, index: usize) -> Option<f32> {
        const STEM: f32 = 3.5;
        const HEAD: f32 = 0.5;
        const ACCIDENTAL: f32 = 1.4;
        let head = self
            .lowest_head(index)
            .map(|position| -(position as f32) / 2.0 + HEAD);
        let stem = self
            .columns
            .get(index)?
            .notation
            .iter()
            .flatten()
            .filter_map(|item| match item {
                NotationItem::Chord(chord) if !chord.stem_up() && chord.value >= 2 => chord
                    .heads
                    .first()
                    .map(|low| -(low.position as f32) / 2.0 + STEM),
                _ => None,
            })
            .reduce(f32::max);
        // A sharp or natural hangs well below its note's head.
        let accidental = self
            .columns
            .get(index)?
            .notation
            .iter()
            .flatten()
            .filter_map(|item| match item {
                NotationItem::Chord(chord) => chord
                    .heads
                    .iter()
                    .filter(|head| head.accidental.is_some())
                    .map(|head| -(head.position as f32) / 2.0 + ACCIDENTAL)
                    .reduce(f32::max),
                NotationItem::Rest { .. } => None,
            })
            .reduce(f32::max);
        [head, stem, accidental]
            .into_iter()
            .flatten()
            .reduce(f32::max)
    }

    /// The lowest notehead's staff position in column `index` of the notation staff, lower
    /// by the room an accent or staccato dot below it takes.
    pub(crate) fn lowest_head(&self, index: usize) -> Option<i32> {
        self.columns
            .get(index)?
            .notation
            .iter()
            .flatten()
            .filter_map(|item| match item {
                NotationItem::Chord(chord) => chord.heads.first().map(|head| {
                    // Marks go below a chord whose stem points up.
                    let below = chord.stem_up();
                    let accent = if below && chord.marks.accent.is_some() {
                        3
                    } else {
                        0
                    };
                    let dot = if below && chord.marks.staccato { 2 } else { 0 };
                    head.position.saturating_sub(accent).saturating_sub(dot)
                }),
                NotationItem::Rest { .. } => None,
            })
            .min()
    }
}

/// A large cross across a staff for a dead slapped beat, centered on `x`.
fn draw_dead_slap(x: f32, top: f32, height: f32, ops: &mut Vec<DrawOp>) {
    let (left, right) = (x - DEAD_SLAP_HALF, x + DEAD_SLAP_HALF);
    let (high, low) = (top - 0.3, top + height + 0.3);
    for (from, to) in [
        (Point::new(left, high), Point::new(right, low)),
        (Point::new(left, low), Point::new(right, high)),
    ] {
        ops.push(DrawOp::Line {
            from,
            to,
            thickness: 0.22,
            color: ColorToken::Notation,
        });
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
