//! Beat effects above the staff and beside a chord: pick strokes, tapping, slapping and
//! popping (`T`, `S`, `P`, as Guitar Pro writes them), a fade-in's opening hairpin, and the
//! brush arrow beside a strummed chord's fret numbers.

use gscore_model::{Fade, SlapKind, StrokeDirection};

use super::notes::draw_glyph_centered;
use super::symbols::{ARROW_LENGTH, draw_arrowhead, draw_vertical_wiggle};
use super::{ColumnMarks, Fret, LINE_THICKNESS};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
use crate::glyph::Glyph;
use crate::notation::draw_arrow;

/// Without notation: the bottom of the beat lane above the tab's top line, clear of the
/// accents, tremolo strokes and vibrato below it.
pub(crate) const TAB_BEAT_LANE: f32 = 4.8;
/// Size of the pick stroke signs.
const STROKE_SCALE: f32 = 0.65;
/// Size of the `T`, `S`, `P` letters.
const LETTER_SIZE: f32 = 1.1;
/// Fade-in hairpin: its length (at most), its opening, and the gap after a pick stroke.
const FADE_LENGTH: f32 = 2.2;
const FADE_OPENING: f32 = 0.7;
const FADE_GAP: f32 = 0.5;
/// Gap between the next beat mark and the one before it in the lane.
const MARK_STEP: f32 = 1.1;
/// Brush arrow: gap to the fret numbers, how far it reaches past the outer strings, and
/// its shortest length.
const BRUSH_GAP: f32 = 0.35;
const BRUSH_OVERHANG: f32 = 0.5;
const BRUSH_MIN: f32 = 2.0;
/// Room the brush arrow takes left of the numbers.
pub(crate) const BRUSH_ROOM: f32 = BRUSH_GAP + 0.5;
/// Arpeggio arrow: gap to the fret numbers, how far it reaches past the outer strings, its
/// shortest length, and the wiggle's amplitude.
const ARPEGGIO_GAP: f32 = 0.4;
const ARPEGGIO_OVERHANG: f32 = 0.5;
const ARPEGGIO_MIN: f32 = 2.0;
const ARPEGGIO_AMPLITUDE: f32 = 0.16;
const ARPEGGIO_STEP: f32 = 0.45;
/// Room the arpeggio arrow takes left of the numbers.
pub(crate) const ARPEGGIO_ROOM: f32 = ARPEGGIO_GAP + ARPEGGIO_AMPLITUDE + 0.4;
/// Size of the `rasg.` label.
const RASG_SIZE: f32 = 0.95;

/// Draws a column's pick stroke, tap/slap/pop letter and fade-in with their bottom at
/// `bottom`, side by side from `x`; the fade-in stops before `span_end`.
pub(crate) fn draw_beat_marks(
    x: f32,
    span_end: f32,
    bottom: f32,
    marks: &ColumnMarks,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let mut at = x;
    if let Some(direction) = marks.pick {
        let glyph = match direction {
            StrokeDirection::Down => Glyph::DownStroke,
            StrokeDirection::Up => Glyph::UpStroke,
        };
        draw_glyph_centered(glyph, at, bottom, STROKE_SCALE, ctx, ops);
        at += MARK_STEP;
    }
    if let Some(kind) = marks.slap {
        let letter = match kind {
            SlapKind::Tapping => "T",
            SlapKind::Slapping => "S",
            SlapKind::Popping => "P",
        };
        ops.push(DrawOp::Text {
            text: letter.to_owned(),
            at: Point::new(at, bottom),
            size: LETTER_SIZE,
            font: TextFont::Bold,
            anchor: TextAnchor::Middle,
            color: ColorToken::Notation,
        });
        at += MARK_STEP;
    }
    if let Some(fade) = marks.fade {
        let start = if at > x {
            at - MARK_STEP + FADE_GAP
        } else {
            x - 0.3
        };
        let end = (span_end - 0.6).min(start + FADE_LENGTH).max(start + 1.0);
        draw_fade(start, end, bottom, fade, ops);
        at += MARK_STEP;
    }
    if marks.rasgueado {
        ops.push(DrawOp::Text {
            text: "rasg.".to_owned(),
            at: Point::new(at, bottom),
            size: RASG_SIZE,
            font: TextFont::Regular,
            anchor: TextAnchor::Start,
            color: ColorToken::Notation,
        });
    }
}

/// A fade hairpin: opening (`<`), closing (`>`), or opening and closing (`<>`).
fn draw_fade(start: f32, end: f32, bottom: f32, fade: Fade, ops: &mut Vec<DrawOp>) {
    let middle = bottom - FADE_OPENING / 2.0;
    let (top, low) = (middle - FADE_OPENING / 2.0, middle + FADE_OPENING / 2.0);
    let line = |from: Point, to: Point, ops: &mut Vec<DrawOp>| {
        ops.push(DrawOp::Line {
            from,
            to,
            thickness: LINE_THICKNESS,
            color: ColorToken::Notation,
        });
    };
    match fade {
        Fade::In => {
            for y in [top, low] {
                line(Point::new(start, middle), Point::new(end, y), ops);
            }
        }
        Fade::Out => {
            for y in [top, low] {
                line(Point::new(start, y), Point::new(end, middle), ops);
            }
        }
        Fade::Swell => {
            let mid = (start + end) / 2.0;
            for y in [top, low] {
                line(Point::new(start, middle), Point::new(mid, y), ops);
                line(Point::new(mid, y), Point::new(end, middle), ops);
            }
        }
    }
}

/// Draws the arpeggio's wavy arrow left of a column's fret numbers. As with the brush, a
/// roll down is drawn as an arrow pointing up.
pub(crate) fn draw_tab_arpeggio<'f>(
    x: f32,
    frets: impl Iterator<Item = &'f Fret>,
    top: f32,
    direction: StrokeDirection,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let mut lines: Option<(usize, usize)> = None;
    let mut half = 0.0_f32;
    for fret in frets.filter(|fret| !fret.tie_destination) {
        half = half.max(fret.half_width());
        lines = Some(match lines {
            Some((low, high)) => (low.min(fret.line), high.max(fret.line)),
            None => (fret.line, fret.line),
        });
    }
    let Some((first, last)) = lines else {
        return;
    };
    let high = top + ctx.staff.line_y(first) - ARPEGGIO_OVERHANG;
    let low = top + ctx.staff.line_y(last) + ARPEGGIO_OVERHANG;
    let middle = (high + low) / 2.0;
    let half_length = ((low - high) / 2.0).max(ARPEGGIO_MIN / 2.0);
    let (high, low) = (middle - half_length, middle + half_length);
    let line_x = x - half - ARPEGGIO_GAP;
    let head = if direction == StrokeDirection::Down {
        high
    } else {
        low
    };
    let up = direction == StrokeDirection::Down;
    let from = if up { head + ARROW_LENGTH } else { high };
    let to = if up { low } else { head - ARROW_LENGTH };
    draw_vertical_wiggle(line_x, from, to, ARPEGGIO_AMPLITUDE, ARPEGGIO_STEP, ops);
    draw_arrowhead(Point::new(line_x, head), up, ops);
}

/// Draws the brush arrow left of a column's fret numbers, over the strings they are on.
/// Guitar Pro writes a brush down as an arrow pointing up (the hand moves down, the arrow
/// points up), so the arrowhead is at the top for `Down`.
pub(crate) fn draw_tab_brush<'f>(
    x: f32,
    frets: impl Iterator<Item = &'f Fret>,
    top: f32,
    direction: StrokeDirection,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let mut lines: Option<(usize, usize)> = None;
    let mut half = 0.0_f32;
    for fret in frets.filter(|fret| !fret.tie_destination) {
        half = half.max(fret.half_width());
        lines = Some(match lines {
            Some((low, high)) => (low.min(fret.line), high.max(fret.line)),
            None => (fret.line, fret.line),
        });
    }
    let Some((first, last)) = lines else {
        return;
    };
    let high = top + ctx.staff.line_y(first) - BRUSH_OVERHANG;
    let low = top + ctx.staff.line_y(last) + BRUSH_OVERHANG;
    let middle = (high + low) / 2.0;
    let half_length = ((low - high) / 2.0).max(BRUSH_MIN / 2.0);
    draw_arrow(
        x - half - BRUSH_GAP,
        middle - half_length,
        middle + half_length,
        direction == StrokeDirection::Up,
        ops,
    );
}
