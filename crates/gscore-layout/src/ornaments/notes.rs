//! Painting ornaments that sit on or above one note: number, mask, slashes, accents,
//! vibrato, beats and bends.

use super::{Fret, LINE_THICKNESS, MASK_PAD, SLASH_REACH};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, Rect, TextAnchor, TextFont};
use crate::glyph::Glyph;
use gscore_model::{AccentKind, Bend, BendKind, HarmonicKind, Vibrato};

/// Baseline of an accent above the top staff line.
const ACCENT_LANE: f32 = 1.0;
/// Baseline of a trill or tremolo above the top staff line.
const ORNAMENT_LANE: f32 = 1.4;
/// Baseline of a vibrato wiggle above the staff.
const WIGGLE_BOTTOM: f32 = 1.6;

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
        let slash = |from: Point, to: Point, ops: &mut Vec<DrawOp>| {
            ops.push(DrawOp::Line {
                from,
                to,
                thickness: LINE_THICKNESS,
                color: ColorToken::Notation,
            });
        };
        if self.marks.slides_in_from_above() {
            slash(
                Point::new(x - half - SLASH_REACH, top),
                Point::new(x - half - 0.05, bottom),
                ops,
            );
        }
        if self.marks.slides_in_from_below() {
            slash(
                Point::new(x - half - SLASH_REACH, bottom),
                Point::new(x - half - 0.05, top),
                ops,
            );
        }
        if self.marks.slides_out_downwards() {
            slash(
                Point::new(x + half + 0.05, top),
                Point::new(x + half + SLASH_REACH, bottom),
                ops,
            );
        }
        if self.marks.slides_out_upwards() {
            slash(
                Point::new(x + half + 0.05, bottom),
                Point::new(x + half + SLASH_REACH, top),
                ops,
            );
        }
    }

    /// Draws the ornaments that sit above the staff (or above the note).
    pub(crate) fn draw_above(&self, x: f32, top: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
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
            draw_bend(x, top, self, bend, ctx, ops);
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

pub(crate) fn draw_vibrato(
    x: f32,
    top: f32,
    vibrato: Vibrato,
    _ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    // Drawn as vector strokes rather than SMuFL wiggle glyphs, so the golden SVG shows
    // them even where the music font is not installed.
    let center_y = top - WIGGLE_BOTTOM;
    match vibrato {
        Vibrato::Slight => {
            let width = 2.4;
            let amplitude = 0.18;
            let half_waves: u8 = 6;
            let step = width / f32::from(half_waves);
            let start = x - width / 2.0;
            for wave in 0..half_waves {
                let from_x = start + step * f32::from(wave);
                let to_x = from_x + step;
                let peak = if wave & 1 == 0 {
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
        Vibrato::Wide => {
            let width = 2.6;
            let amplitude = 0.42;
            let segments: u8 = 4;
            let step = width / f32::from(segments);
            let start = x - width / 2.0;
            for segment in 0..segments {
                let from_x = start + step * f32::from(segment);
                let to_x = from_x + step;
                let (from_y, to_y) = if segment & 1 == 0 {
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

fn draw_bend(x: f32, top: f32, fret: &Fret, bend: Bend, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
    let size = ctx.options.fret_size;
    let line_y = top + ctx.staff.line_y(fret.line);
    let cap = ctx.fonts.text.cap_height * size;
    let up = bend.kind == BendKind::Up;
    let start = Point::new(x + fret.width / 2.0 + 0.15, line_y - cap * 0.3);
    let right = ctx.options.page_width - ctx.options.margin_x;
    let end_x = (x + 3.8).min(right);
    if end_x <= start.x {
        return;
    }
    let end = Point::new(end_x, if up { top - 2.2 } else { line_y + 2.2 });
    let direction = if up { -1.0 } else { 1.0 };
    ops.push(DrawOp::Curve {
        from: start,
        to: end,
        control1: Point::new(start.x + 1.0, start.y + direction * 0.4),
        control2: Point::new(end.x - 1.2, end.y - direction * 0.4),
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

/// Draws an open arrowhead with its tip at `tip`. Vector strokes, so it is visible
/// wherever the SVG is opened, even without the music font.
pub(super) fn draw_arrowhead(tip: Point, pointing_up: bool, ops: &mut Vec<DrawOp>) {
    const LENGTH: f32 = 1.0;
    const HALF_WIDTH: f32 = 0.5;
    const THICKNESS: f32 = 0.18;
    let base_y = if pointing_up {
        tip.y + LENGTH
    } else {
        tip.y - LENGTH
    };
    for x in [tip.x - HALF_WIDTH, tip.x + HALF_WIDTH] {
        ops.push(DrawOp::Line {
            from: Point::new(x, base_y),
            to: tip,
            thickness: THICKNESS,
            color: ColorToken::Notation,
        });
    }
}
