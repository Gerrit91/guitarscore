//! Painting ornaments that sit on or above one note: number, mask, slashes, accents,
//! vibrato, beats and bends.

use super::{Fret, LINE_THICKNESS, MASK_PAD, SLASH_GAP, SLASH_REACH};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, Rect, TextAnchor, TextFont};
use crate::glyph::Glyph;
use gscore_model::{AccentKind, Bend, BendKind, HarmonicKind, Vibrato};

// Rows above the staff, measured up from the top line. Measure numbers take the band up
// to about 1.6 staff spaces, so ornaments start above it and never collide with them.
/// Baseline of accents, staccato, trill, tremolo and "PH".
const ACCENT_LANE: f32 = 2.2;
/// Baseline of a trill or tremolo (the same row as accents).
const ORNAMENT_LANE: f32 = ACCENT_LANE;
/// Middle of a vibrato wiggle.
const WIGGLE_CENTER: f32 = 3.0;
/// Where a vibrato starts before its note and stops before the next one.
const WIGGLE_INSET: (f32, f32) = (0.3, 1.0);
/// Shortest vibrato line.
const WIGGLE_MIN: f32 = 1.2;
/// Length of one slight-vibrato half wave and one wide-vibrato stroke.
const WIGGLE_STEP: (f32, f32) = (0.45, 0.7);
/// Most wiggle segments drawn for one note.
const MAX_WIGGLES: f32 = 200.0;
/// Gap between a bend's arrow and the next note.
const BEND_END_GAP: f32 = 0.8;
/// Shortest bend, from the number to the arrow.
const BEND_MIN: f32 = 1.5;
/// How far a cubic Bézier's control points reach for a quarter ellipse (4/3·(√2−1)).
const QUARTER_ELLIPSE: f32 = 0.552_284_8;
/// An arrowhead's length (tip to base) and half its base width.
const ARROW_LENGTH: f32 = 0.9;
const ARROW_HALF_WIDTH: f32 = 0.42;

impl Fret {
    /// Draws the number (with its mask and inline slide slashes).
    pub(crate) fn draw(&self, x: f32, top: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
        if self.tie_destination {
            return;
        }
        let size = ctx.options.fret_size;
        let line_y = top + ctx.staff.line_y(self.line);
        let cap = ctx.fonts.text.cap_height * size;
        // Hide the staff line behind the number.
        ops.push(DrawOp::Rect {
            rect: Rect {
                origin: Point::new(
                    x - self.width / 2.0 - MASK_PAD.0,
                    line_y - cap / 2.0 - MASK_PAD.1,
                ),
                width: self.width + 2.0 * MASK_PAD.0,
                height: cap + 2.0 * MASK_PAD.1,
            },
            color: ColorToken::Paper,
        });
        ops.push(DrawOp::Text {
            text: self.text.clone(),
            at: Point::new(x, line_y + cap / 2.0),
            size,
            font: TextFont::Regular,
            anchor: TextAnchor::Middle,
            color: ColorToken::FretNumber,
        });
        self.draw_side_slashes(x, line_y, cap, ops);
    }

    /// The steep slashes of slides in and out, just outside the number.
    fn draw_side_slashes(&self, x: f32, line_y: f32, cap: f32, ops: &mut Vec<DrawOp>) {
        let half = self.width / 2.0;
        let top = line_y - cap / 2.0;
        let bottom = line_y + cap / 2.0;
        let near = half + SLASH_GAP;
        let far = near + SLASH_REACH;
        let slash = |from: Point, to: Point, ops: &mut Vec<DrawOp>| {
            ops.push(DrawOp::Line {
                from,
                to,
                thickness: LINE_THICKNESS,
                color: ColorToken::Notation,
            });
        };
        if self.marks.slides_in_from_above() {
            slash(Point::new(x - far, top), Point::new(x - near, bottom), ops);
        }
        if self.marks.slides_in_from_below() {
            slash(Point::new(x - far, bottom), Point::new(x - near, top), ops);
        }
        if self.marks.slides_out_downwards() {
            slash(Point::new(x + near, top), Point::new(x + far, bottom), ops);
        }
        if self.marks.slides_out_upwards() {
            slash(Point::new(x + near, bottom), Point::new(x + far, top), ops);
        }
    }

    /// Draws the ornaments that sit above the staff (or above the note). `span_end` is
    /// where the note's space ends: the next column, or the end of the measure.
    pub(crate) fn draw_above(
        &self,
        x: f32,
        span_end: f32,
        top: f32,
        ctx: &Context<'_>,
        ops: &mut Vec<DrawOp>,
    ) {
        if self.tie_destination {
            return;
        }
        if let Some(accent) = self.marks.accent {
            draw_accent(x, top, accent, ops);
        }
        if self.marks.staccato {
            draw_staccato(x, top, ctx, ops);
        }
        if self.marks.trill {
            draw_glyph_centered(Glyph::Trill, x, top - ORNAMENT_LANE, 1.0, ctx, ops);
        }
        if self.marks.tremolo_picking {
            draw_glyph_centered(Glyph::Tremolo3, x, top - ORNAMENT_LANE, 1.0, ctx, ops);
        }
        if self
            .marks
            .harmonic
            .is_some_and(|harmonic| harmonic.kind == HarmonicKind::Pinch)
        {
            // Guitar Pro labels a pinch harmonic "PH" above the staff.
            ops.push(DrawOp::Text {
                text: "PH".to_owned(),
                at: Point::new(x, top - ACCENT_LANE),
                size: 1.0,
                font: TextFont::Regular,
                anchor: TextAnchor::Middle,
                color: ColorToken::Notation,
            });
        }
        if let Some(bend) = self.marks.bend {
            draw_bend(x, span_end, top, self, bend, ctx, ops);
        }
    }
}

fn draw_accent(x: f32, top: f32, accent: AccentKind, ops: &mut Vec<DrawOp>) {
    let (text, size, font) = match accent {
        AccentKind::Normal => (">", 1.2, TextFont::Regular),
        // Guitar Pro writes a heavy accent as a downward wedge.
        AccentKind::Heavy => ("v", 1.4, TextFont::Bold),
    };
    ops.push(DrawOp::Text {
        text: text.to_owned(),
        at: Point::new(x, top - ACCENT_LANE),
        size,
        font,
        anchor: TextAnchor::Middle,
        color: ColorToken::Notation,
    });
}

fn draw_staccato(x: f32, top: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
    let glyph = Glyph::AugmentationDot;
    let bbox = ctx.fonts.music.bbox(glyph);
    // Put the dot's middle at the accent lane.
    let at = Point::new(
        x - (bbox.x_min + bbox.x_max) / 2.0,
        top - ACCENT_LANE + bbox.middle_above_origin(),
    );
    ops.push(DrawOp::Glyph {
        glyph,
        at,
        scale: 1.0,
        color: ColorToken::Notation,
    });
}

/// Draws a vibrato above the note at `x`, along the note's space up to `span_end`.
pub(crate) fn draw_vibrato(
    x: f32,
    span_end: f32,
    top: f32,
    vibrato: Vibrato,
    ops: &mut Vec<DrawOp>,
) {
    // Drawn as vector strokes rather than SMuFL wiggle glyphs, so the golden SVG shows
    // them even where the music font is not installed.
    let center_y = top - WIGGLE_CENTER;
    let start = x - WIGGLE_INSET.0;
    let width = (span_end - WIGGLE_INSET.1 - start).max(WIGGLE_MIN);
    let (step_length, amplitude) = match vibrato {
        Vibrato::Slight => (WIGGLE_STEP.0, 0.18),
        Vibrato::Wide => (WIGGLE_STEP.1, 0.42),
    };
    // An even number of segments, so the line ends where it started.
    let pairs = (width / step_length / 2.0)
        .round()
        .clamp(1.0, MAX_WIGGLES / 2.0);
    let segments = (pairs * 2.0) as u16;
    let step = width / f32::from(segments);
    for segment in 0..segments {
        let from_x = start + step * f32::from(segment);
        let to_x = from_x + step;
        let rising = segment % 2 == 0;
        match vibrato {
            Vibrato::Slight => {
                let peak = if rising {
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
            Vibrato::Wide => {
                let (from_y, to_y) = if rising {
                    (center_y + amplitude, center_y - amplitude)
                } else {
                    (center_y - amplitude, center_y + amplitude)
                };
                ops.push(DrawOp::Line {
                    from: Point::new(from_x, from_y),
                    to: Point::new(to_x, to_y),
                    thickness: LINE_THICKNESS,
                    color: ColorToken::Notation,
                });
            }
        }
    }
}

/// Draws a bend from the number at `x` to an arrow just before `span_end`, as Guitar Pro
/// does: the curve spans the note's length and the arrow rises above the staff.
fn draw_bend(
    x: f32,
    span_end: f32,
    top: f32,
    fret: &Fret,
    bend: Bend,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let size = ctx.options.fret_size;
    let line_y = top + ctx.staff.line_y(fret.line);
    let cap = ctx.fonts.text.cap_height * size;
    let up = bend.kind == BendKind::Up;
    let start = Point::new(x + fret.width / 2.0 + 0.15, line_y - cap * 0.3);
    let right = ctx.options.page_width - ctx.options.margin_x;
    let end_x = (span_end - BEND_END_GAP).max(start.x + BEND_MIN).min(right);
    if end_x <= start.x {
        return;
    }
    let end = Point::new(end_x, if up { top - 2.2 } else { line_y + 2.2 });
    // A quarter ellipse: it leaves the number level and enters the arrowhead's base
    // straight up (or down), at a right angle to the base.
    let base = arrow_base(end, up);
    ops.push(DrawOp::Curve {
        from: start,
        to: base,
        control1: Point::new(start.x + (base.x - start.x) * QUARTER_ELLIPSE, start.y),
        control2: Point::new(base.x, base.y + (start.y - base.y) * QUARTER_ELLIPSE),
        thickness: LINE_THICKNESS,
        color: ColorToken::Notation,
    });
    draw_arrowhead(end, up, ops);
    let label = bend_label(bend.quarter_tones);
    ops.push(DrawOp::Text {
        text: label,
        at: Point::new(end.x, if up { end.y - 0.5 } else { end.y + 1.0 }),
        size: 0.9,
        font: TextFont::Regular,
        anchor: TextAnchor::Middle,
        color: ColorToken::Annotation,
    });
}

/// A short human-readable name for how far a bend reaches.
pub(crate) fn bend_label(quarter_tones: i16) -> String {
    match quarter_tones {
        2 => "1/2".to_owned(),
        4 => "full".to_owned(),
        6 => "1 1/2".to_owned(),
        8 => "2".to_owned(),
        value if value > 0 => {
            let whole = value.checked_div(2).unwrap_or(0);
            let odd = value.checked_rem(2).unwrap_or(0) != 0;
            if odd {
                format!("{whole}.5")
            } else {
                whole.to_string()
            }
        }
        _ => "bend".to_owned(),
    }
}

/// Draws a music glyph horizontally centered on `x` with its bottom at `bottom`.
fn draw_glyph_centered(
    glyph: Glyph,
    x: f32,
    bottom: f32,
    scale: f32,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let bbox = ctx.fonts.music.bbox(glyph);
    let width = (bbox.x_max - bbox.x_min) * scale;
    ops.push(DrawOp::Glyph {
        glyph,
        at: Point::new(
            x - width / 2.0 - bbox.x_min * scale,
            bottom + bbox.y_min * scale,
        ),
        scale,
        color: ColorToken::Notation,
    });
}

/// The middle of the base of an arrowhead with its tip at `tip`: where the line into it
/// ends.
fn arrow_base(tip: Point, pointing_up: bool) -> Point {
    let direction = if pointing_up { 1.0 } else { -1.0 };
    Point::new(tip.x, tip.y + direction * ARROW_LENGTH)
}

/// Draws a solid arrowhead with its tip at `tip`. Vector shapes, so it is visible wherever
/// the SVG is opened, even without the music font.
fn draw_arrowhead(tip: Point, pointing_up: bool, ops: &mut Vec<DrawOp>) {
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
