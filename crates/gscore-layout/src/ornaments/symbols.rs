//! Small vector symbols the ornaments share: wiggles, arrowheads, circles and tremolo
//! strokes. Drawn as strokes and shapes rather than music-font glyphs so the golden SVGs
//! show them even where the music font is not installed.

use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};

use super::LINE_THICKNESS;

/// Most wiggle segments drawn for one symbol.
pub(super) const MAX_WIGGLES: f32 = 200.0;
/// How far a cubic Bézier's control points reach for a quarter ellipse (4/3·(√2−1)).
pub(super) const QUARTER_ELLIPSE: f32 = 0.552_284_8;
/// An arrowhead's length (tip to base) and half its base width.
pub(crate) const ARROW_LENGTH: f32 = 0.9;
const ARROW_HALF_WIDTH: f32 = 0.42;
/// Radius of a circled symbol (a left-hand tap's T, a string number).
pub(crate) const CIRCLE_RADIUS: f32 = 0.55;

/// Draws a horizontal wiggle from `start_x` to `end_x` around `center_y`, as the vibrato
/// and the pick slide use it.
pub(crate) fn draw_horizontal_wiggle(
    start_x: f32,
    end_x: f32,
    center_y: f32,
    amplitude: f32,
    step_length: f32,
    ops: &mut Vec<DrawOp>,
) {
    let width = end_x - start_x;
    if width <= 0.0 || step_length <= 0.0 {
        return;
    }
    let pairs = (width / step_length / 2.0)
        .round()
        .clamp(1.0, MAX_WIGGLES / 2.0);
    let segments = (pairs * 2.0) as u16;
    let step = width / f32::from(segments);
    for segment in 0..segments {
        let from_x = start_x + step * f32::from(segment);
        let to_x = from_x + step;
        let peak = if segment % 2 == 0 {
            center_y - amplitude
        } else {
            center_y + amplitude
        };
        ops.push(DrawOp::Curve {
            from: Point::new(from_x, center_y),
            to: Point::new(to_x, center_y),
            control1: Point::new(from_x + step * 0.35, peak),
            control2: Point::new(to_x - step * 0.35, peak),
            thickness: LINE_THICKNESS,
            color: ColorToken::Notation,
        });
    }
}

/// Draws a wiggle from `start` to `end`, following the line between them but rising and
/// falling around it: a pick slide slants with the pitch while it stays a scrape.
pub(crate) fn draw_slanted_wiggle(
    start: Point,
    end: Point,
    amplitude: f32,
    step_length: f32,
    ops: &mut Vec<DrawOp>,
) {
    let width = end.x - start.x;
    if width <= 0.0 || step_length <= 0.0 {
        return;
    }
    let pairs = (width / step_length / 2.0)
        .round()
        .clamp(1.0, MAX_WIGGLES / 2.0);
    let segments = (pairs * 2.0) as u16;
    let step = width / f32::from(segments);
    let at = |x: f32| {
        let share = (x - start.x) / width;
        start.y + (end.y - start.y) * share
    };
    for segment in 0..segments {
        let from_x = start.x + step * f32::from(segment);
        let to_x = from_x + step;
        let middle = at(from_x + step / 2.0);
        let peak = if segment % 2 == 0 {
            middle - amplitude
        } else {
            middle + amplitude
        };
        ops.push(DrawOp::Curve {
            from: Point::new(from_x, at(from_x)),
            to: Point::new(to_x, at(to_x)),
            control1: Point::new(from_x + step * 0.35, peak),
            control2: Point::new(to_x - step * 0.35, peak),
            thickness: LINE_THICKNESS,
            color: ColorToken::Notation,
        });
    }
}

/// Draws a vertical wiggle from `high` to `low` at `x`, as the arpeggio arrow uses it.
pub(crate) fn draw_vertical_wiggle(
    x: f32,
    high: f32,
    low: f32,
    amplitude: f32,
    step_length: f32,
    ops: &mut Vec<DrawOp>,
) {
    let height = low - high;
    if height <= 0.0 || step_length <= 0.0 {
        return;
    }
    let pairs = (height / step_length / 2.0)
        .round()
        .clamp(1.0, MAX_WIGGLES / 2.0);
    let segments = (pairs * 2.0) as u16;
    let step = height / f32::from(segments);
    for segment in 0..segments {
        let from_y = high + step * f32::from(segment);
        let to_y = from_y + step;
        let peak = if segment % 2 == 0 {
            x - amplitude
        } else {
            x + amplitude
        };
        ops.push(DrawOp::Curve {
            from: Point::new(x, from_y),
            to: Point::new(x, to_y),
            control1: Point::new(peak, from_y + step * 0.35),
            control2: Point::new(peak, to_y - step * 0.35),
            thickness: LINE_THICKNESS,
            color: ColorToken::Notation,
        });
    }
}

/// A small arc left of a left-hand-tapped fret number.
pub(super) fn draw_tap_arc(x: f32, line_y: f32, cap: f32, ops: &mut Vec<DrawOp>) {
    let top = line_y - cap / 2.0;
    let bottom = line_y + cap / 2.0;
    ops.push(DrawOp::Curve {
        from: Point::new(x, top),
        to: Point::new(x, bottom),
        control1: Point::new(x - 0.45, top + cap * 0.2),
        control2: Point::new(x - 0.45, bottom - cap * 0.2),
        thickness: LINE_THICKNESS,
        color: ColorToken::Notation,
    });
}

/// Tremolo-picking strokes under a tab number: one to three short slanted bars.
pub(super) fn draw_tremolo_strokes(x: f32, below: f32, strokes: u8, ops: &mut Vec<DrawOp>) {
    for stroke in 0..strokes {
        let y = below + 0.3 + f32::from(stroke) * 0.3;
        ops.push(DrawOp::Line {
            from: Point::new(x - 0.38, y + 0.05),
            to: Point::new(x + 0.38, y - 0.05),
            thickness: 0.15,
            color: ColorToken::Notation,
        });
    }
}

/// Draws `text` inside a circle centered on `(x, center_y)`: the left-hand tap's T and the
/// circled string numbers.
pub(crate) fn draw_circled_symbol(
    text: &str,
    x: f32,
    center_y: f32,
    radius: f32,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let kappa = radius * QUARTER_ELLIPSE;
    let (left, right) = (x - radius, x + radius);
    let (top, bottom) = (center_y - radius, center_y + radius);
    for (from, to, c1, c2) in [
        (
            Point::new(x, top),
            Point::new(right, center_y),
            Point::new(x + kappa, top),
            Point::new(right, center_y - kappa),
        ),
        (
            Point::new(right, center_y),
            Point::new(x, bottom),
            Point::new(right, center_y + kappa),
            Point::new(x + kappa, bottom),
        ),
        (
            Point::new(x, bottom),
            Point::new(left, center_y),
            Point::new(x - kappa, bottom),
            Point::new(left, center_y + kappa),
        ),
        (
            Point::new(left, center_y),
            Point::new(x, top),
            Point::new(left, center_y - kappa),
            Point::new(x - kappa, top),
        ),
    ] {
        ops.push(DrawOp::Curve {
            from,
            to,
            control1: c1,
            control2: c2,
            thickness: LINE_THICKNESS,
            color: ColorToken::Notation,
        });
    }
    let cap = ctx.fonts.text.cap_height * 0.9;
    ops.push(DrawOp::Text {
        text: text.to_owned(),
        at: Point::new(x, center_y + cap / 2.0),
        size: 0.9,
        font: TextFont::Regular,
        anchor: TextAnchor::Middle,
        color: ColorToken::Notation,
    });
}

/// The middle of the base of an arrowhead with its tip at `tip`: where the line into it
/// ends.
pub(super) fn arrow_base(tip: Point, pointing_up: bool) -> Point {
    let direction = if pointing_up { 1.0 } else { -1.0 };
    Point::new(tip.x, tip.y + direction * ARROW_LENGTH)
}

/// Draws a solid arrowhead with its tip at `tip`. Vector shapes, so it is visible wherever
/// the SVG is opened, even without the music font.
pub(crate) fn draw_arrowhead(tip: Point, pointing_up: bool, ops: &mut Vec<DrawOp>) {
    let base = arrow_base(tip, pointing_up);
    ops.push(DrawOp::Polygon {
        points: vec![
            tip,
            Point::new(base.x + ARROW_HALF_WIDTH, base.y),
            Point::new(base.x - ARROW_HALF_WIDTH, base.y),
        ],
        color: ColorToken::Notation,
    });
}
