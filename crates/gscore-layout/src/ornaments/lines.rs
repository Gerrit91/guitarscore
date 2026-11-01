//! Ornaments that connect notes or run along the staff: slurs, slides, legato runs, the
//! whammy line, and the palm-mute / let-ring rows below the staff.

use super::notes::draw_arrowhead;
use super::{Fret, LINE_THICKNESS};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
use gscore_model::{SlideKind, Whammy};

/// How far a slur between two notes dips or rises.
const SLUR_CURVE: f32 = 0.9;
/// Baseline of the first under-staff annotation row, below the bottom staff line.
const ANNOTATION_LANE: f32 = 2.2;
/// Distance between two under-staff annotation rows.
const ANNOTATION_ROW_GAP: f32 = 1.6;
/// Gap between an under-staff label and its dashed line.
const LABEL_GAP: f32 = 0.3;
/// Dash and gap lengths of the under-staff lines.
const DASH: (f32, f32) = (0.5, 0.3);
/// Thickness of the under-staff rows; heavier than slurs so they read as text rows.
const ANNOTATION_THICKNESS: f32 = 0.2;
/// Safety cap for the dash loop.
const MAX_DASHES: usize = 4096;

/// Draws the slur or line that connects two neighbouring notes.
pub(crate) fn draw_link(
    origin_x: f32,
    target_x: f32,
    top: f32,
    origin: &Fret,
    target: &Fret,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let from_x = origin_x + origin.half_width() + 0.05;
    let to_x = target_x - target.half_width() - 0.05;
    if to_x <= from_x {
        return;
    }
    let y = top + ctx.staff.line_y(origin.line);
    if let Some(slide) = origin.marks.direct_slide() {
        match slide {
            SlideKind::ShiftSlideTo => ops.push(DrawOp::Line {
                // Off the string line, or the staff line hides it.
                from: Point::new(from_x, y - 0.3),
                to: Point::new(to_x, y - 0.3),
                thickness: LINE_THICKNESS,
                color: ColorToken::Notation,
            }),
            SlideKind::LegatoSlideTo => {
                draw_slur(
                    from_x,
                    y,
                    to_x,
                    top + ctx.staff.line_y(target.line),
                    -SLUR_CURVE,
                    ops,
                );
                ops.push(DrawOp::Text {
                    text: "sl.".to_owned(),
                    at: Point::new((from_x + to_x) / 2.0, y - SLUR_CURVE - 0.3),
                    size: 0.9,
                    font: TextFont::Regular,
                    anchor: TextAnchor::Middle,
                    color: ColorToken::Annotation,
                });
            }
            _ => {}
        }
    } else if origin.marks.hammer {
        // A pull-off and a hammer-on are both written as a slur; the direction follows
        // from the two frets, and the letter makes it explicit.
        draw_slur(
            from_x,
            y,
            to_x,
            top + ctx.staff.line_y(target.line),
            SLUR_CURVE,
            ops,
        );
        let pull_off = matches!(
            (origin.fret, target.fret),
            (Some(from), Some(to)) if to < from
        );
        ops.push(DrawOp::Text {
            text: if pull_off { "P" } else { "H" }.to_owned(),
            at: Point::new((from_x + to_x) / 2.0, y - 0.45),
            size: 1.0,
            font: TextFont::Regular,
            anchor: TextAnchor::Middle,
            color: ColorToken::Notation,
        });
    }
}

/// Draws the slur of a tie from its origin to the hidden continuation.
pub(crate) fn draw_tie(
    origin_x: f32,
    target_x: f32,
    top: f32,
    origin: &Fret,
    target: &Fret,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let from_x = origin_x + origin.half_width() + 0.05;
    let to_x = target_x - target.half_width() - 0.05;
    if to_x <= from_x {
        return;
    }
    let y = top + ctx.staff.line_y(origin.line);
    draw_slur(
        from_x,
        y,
        to_x,
        top + ctx.staff.line_y(target.line),
        SLUR_CURVE,
        ops,
    );
}

/// Draws a whole beat-level legato run as one slur above the staff.
pub(crate) fn draw_legato_run(start_x: f32, end_x: f32, top: f32, ops: &mut Vec<DrawOp>) {
    let start = start_x - 0.6;
    let end = end_x + 0.6;
    if end <= start {
        return;
    }
    draw_slur(start, top - 0.6, end, top - 0.6, -1.4, ops);
}

/// Draws the whammy-bar line of one column.
pub(crate) fn draw_whammy(
    x: f32,
    top: f32,
    whammy: Whammy,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    if whammy.quarter_tones == 0 {
        return;
    }
    let down = whammy.quarter_tones < 0;
    let (start_y, end_y) = if down {
        (top - 2.1, top - 0.8)
    } else {
        (top - 2.1, top - 3.3)
    };
    let right = ctx.options.page_width - ctx.options.margin_x;
    let end_x = (x + 6.0).min(right);
    if end_x <= x + 1.0 {
        return;
    }
    let start = Point::new(x + 0.8, start_y);
    let end = Point::new(end_x, end_y);
    ops.push(DrawOp::Line {
        from: start,
        to: end,
        thickness: LINE_THICKNESS,
        color: ColorToken::Annotation,
    });
    draw_arrowhead(end, !down, ops);
    // Guitar Pro labels the dive in whole steps; four quarter tones make one.
    let whole_steps = whammy.quarter_tones.checked_div(4).unwrap_or(0);
    ops.push(DrawOp::Text {
        text: format!("{whole_steps:+}"),
        at: Point::new(end.x + 0.15, end.y + 0.35),
        size: 0.9,
        font: TextFont::Regular,
        anchor: TextAnchor::Start,
        color: ColorToken::Annotation,
    });
}

/// Draws a `P.M.` or `let ring` row below the staff from `start_x` to `end_x`. `lane` is
/// the row index below the staff (0 for the first present row).
pub(crate) fn draw_under_staff_run(
    start_x: f32,
    end_x: f32,
    label: &str,
    lane: usize,
    top: f32,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let baseline = top + ctx.staff.height() + ANNOTATION_LANE + lane as f32 * ANNOTATION_ROW_GAP;
    ops.push(DrawOp::Text {
        text: label.to_owned(),
        at: Point::new(start_x, baseline),
        size: 1.0,
        font: TextFont::Bold,
        anchor: TextAnchor::Start,
        color: ColorToken::Annotation,
    });
    let line_start = start_x + ctx.fonts.text.width(label, 1.0) + LABEL_GAP;
    if end_x <= line_start {
        return;
    }
    let y = baseline - 0.35;
    let mut x = line_start;
    for _ in 0..MAX_DASHES {
        if x >= end_x {
            break;
        }
        let next = (x + DASH.0).min(end_x);
        ops.push(DrawOp::Line {
            from: Point::new(x, y),
            to: Point::new(next, y),
            thickness: ANNOTATION_THICKNESS,
            color: ColorToken::Annotation,
        });
        x = next + DASH.1;
    }
    // A small closing tick, as in Guitar Pro's own tablature.
    ops.push(DrawOp::Line {
        from: Point::new(end_x, y - 0.3),
        to: Point::new(end_x, y + 0.3),
        thickness: ANNOTATION_THICKNESS,
        color: ColorToken::Annotation,
    });
}

/// Draws a slur from `from_x` to `to_x`; `curve` is positive for a dip below the notes and
/// negative for an arch above them.
fn draw_slur(from_x: f32, from_y: f32, to_x: f32, to_y: f32, curve: f32, ops: &mut Vec<DrawOp>) {
    let distance = to_x - from_x;
    let handle = distance / 3.0;
    ops.push(DrawOp::Curve {
        from: Point::new(from_x, from_y),
        to: Point::new(to_x, to_y),
        control1: Point::new(from_x + handle, from_y + curve),
        control2: Point::new(to_x - handle, to_y + curve),
        thickness: LINE_THICKNESS,
        color: ColorToken::Notation,
    });
}
