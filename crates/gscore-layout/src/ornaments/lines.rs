//! Ornaments that connect notes or run along the staff: slurs, slides, legato runs, the
//! whammy line, and the palm-mute / let-ring rows below the staff.

use super::{Fret, LINE_THICKNESS};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
use gscore_model::{BEND_MAX_POSITION, BendPoint, SlideKind, WhammyShape};

/// How far a slur between two notes dips or rises.
const SLUR_CURVE: f32 = 0.9;
/// How far a slide line rises or falls on each side of the string line.
const SLIDE_SLANT: f32 = 0.35;
/// Height above the top line where a beat-level legato slur starts and ends; above the
/// measure numbers.
const LEGATO_BASE: f32 = 1.9;
/// How high a beat-level legato slur arches above its ends.
const LEGATO_ARCH: f32 = 1.0;
/// Whammy line heights above the top line: high and low end.
const WHAMMY_HEIGHTS: (f32, f32) = (3.4, 2.1);
/// Whammy line: start after the note, stop before the next one, shortest length.
const WHAMMY_INSET: (f32, f32, f32) = (0.8, 0.8, 1.5);
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
        // The line slants the way the pitch goes, crossing the string line, as in
        // Guitar Pro's tablature.
        let descending = matches!((origin.fret, target.fret), (Some(a), Some(b)) if b < a);
        let rise = if descending {
            SLIDE_SLANT
        } else {
            -SLIDE_SLANT
        };
        ops.push(DrawOp::Line {
            from: Point::new(from_x, y - rise),
            to: Point::new(to_x, y + rise),
            thickness: LINE_THICKNESS,
            color: ColorToken::Notation,
        });
        if slide == SlideKind::LegatoSlideTo {
            {
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
    draw_slur(
        start,
        top - LEGATO_BASE,
        end,
        top - LEGATO_BASE,
        -LEGATO_ARCH,
        ops,
    );
}

/// Draws the whammy-bar line of the column at `x`, within its space up to `span_end`, as
/// Guitar Pro does: a line that follows the shape over the beat (a dip goes down and back
/// up), with the depth in whole steps written above each turning point.
pub(crate) fn draw_whammy(
    x: f32,
    span_end: f32,
    top: f32,
    whammy: &WhammyShape,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let points = &whammy.points;
    let values = || points.iter().map(|p| f32::from(p.value));
    let down = values().fold(0.0_f32, |deepest, value| deepest.max(-value));
    let up = values().fold(0.0_f32, |highest, value| highest.max(value));
    if down + up <= 0.0 {
        return;
    }
    let right = ctx.options.page_width - ctx.options.margin_right;
    let start_x = x + WHAMMY_INSET.0;
    let end_x = (span_end - WHAMMY_INSET.1)
        .max(start_x + WHAMMY_INSET.2)
        .min(right);
    if end_x <= start_x {
        return;
    }
    // The deepest point sits at the band's low end, the highest at its high end.
    let (high, low) = (top - WHAMMY_HEIGHTS.0, top - WHAMMY_HEIGHTS.1);
    let per_quarter = (low - high) / (down + up);
    let at = |position: u8, value: i8| {
        Point::new(
            start_x + (end_x - start_x) * f32::from(position) / f32::from(BEND_MAX_POSITION),
            low - (f32::from(value) + down) * per_quarter,
        )
    };
    let mut corners: Vec<Point> = points.iter().map(|p| at(p.position, p.value)).collect();
    // Held level before the first point and after the last.
    if let Some(first) = corners.first().copied()
        && first.x > start_x
    {
        corners.insert(0, Point::new(start_x, first.y));
    }
    if let Some(last) = corners.last().copied()
        && last.x < end_x
    {
        corners.push(Point::new(end_x, last.y));
    }
    for pair in corners.windows(2) {
        let [from, to] = pair else { continue };
        ops.push(DrawOp::Line {
            from: *from,
            to: *to,
            thickness: LINE_THICKNESS,
            color: ColorToken::Annotation,
        });
    }
    for (index, point) in points.iter().enumerate() {
        if !is_turning_point(points, index) {
            continue;
        }
        // Labels at the ends of the line stay inside it, clear of the neighbours' labels.
        let anchor = match point.position {
            0 => TextAnchor::Start,
            BEND_MAX_POSITION => TextAnchor::End,
            _ => TextAnchor::Middle,
        };
        ops.push(DrawOp::Text {
            text: whammy_label(point.value),
            at: Point::new(at(point.position, point.value).x, high - 0.5),
            size: 0.9,
            font: TextFont::Regular,
            anchor,
            color: ColorToken::Annotation,
        });
    }
}

/// Whether point `index` of a whammy shape gets a label: it is away from 0 and the bar
/// stops there or turns (the first point of a level stretch; the start when it is held).
fn is_turning_point(points: &[BendPoint], index: usize) -> bool {
    let value = |i: usize| points.get(i).map(|p| p.value);
    let Some(here) = value(index) else {
        return false;
    };
    if here == 0 {
        return false;
    }
    let before = index.checked_sub(1).and_then(value);
    if before == Some(here) {
        return false;
    }
    let incoming = before.map(|before| here.cmp(&before));
    // The next point that differs, so a held level counts as a stop.
    let outgoing = points
        .iter()
        .skip(index.saturating_add(1))
        .map(|p| p.value)
        .find(|next| *next != here)
        .map(|next| next.cmp(&here));
    incoming.is_none() || outgoing != incoming
}

/// A whammy depth in whole steps, as Guitar Pro writes it: -1, -1/2, +1 1/2.
pub(crate) fn whammy_label(quarter_tones: i8) -> String {
    let sign = if quarter_tones < 0 { "-" } else { "+" };
    let quarters = quarter_tones.unsigned_abs();
    let whole = quarters / 4;
    let fraction = match quarters % 4 {
        1 => "1/4",
        2 => "1/2",
        3 => "3/4",
        _ => "",
    };
    match (whole, fraction) {
        (whole, "") => format!("{sign}{whole}"),
        (0, fraction) => format!("{sign}{fraction}"),
        (whole, fraction) => format!("{sign}{whole} {fraction}"),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn labelled(values: &[(u8, i8)]) -> Vec<String> {
        let points: Vec<BendPoint> = values
            .iter()
            .map(|&(position, value)| BendPoint { position, value })
            .collect();
        (0..points.len())
            .filter(|&index| is_turning_point(&points, index))
            .filter_map(|index| points.get(index))
            .map(|point| whammy_label(point.value))
            .collect()
    }

    #[test]
    fn whammy_depths_are_written_in_whole_steps() {
        let labels: Vec<_> = [-8, -4, -2, -6, 4, 6, -1].map(whammy_label).into();
        assert_eq!(
            labels,
            ["-2", "-1", "-1/2", "-1 1/2", "+1", "+1 1/2", "-1/4"]
        );
    }

    #[test]
    fn only_turning_points_away_from_zero_are_labelled() {
        // A dip is labelled at its bottom, a dive at its end, a release at its start.
        assert_eq!(labelled(&[(0, 0), (6, -8), (12, 0)]), ["-2"]);
        assert_eq!(labelled(&[(0, 0), (12, -8)]), ["-2"]);
        assert_eq!(labelled(&[(0, -8), (12, 0)]), ["-2"]);
        // A held level is labelled once; a point on the way down is not labelled.
        assert_eq!(labelled(&[(0, 0), (4, -8), (8, -8), (12, 0)]), ["-2"]);
        assert_eq!(labelled(&[(0, 0), (6, -4), (12, -8)]), ["-2"]);
        // Down, then up past zero: both extremes.
        assert_eq!(labelled(&[(0, 0), (4, -4), (8, 4), (12, 0)]), ["-1", "+1"]);
    }
}
