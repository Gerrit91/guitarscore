//! Palette tiles: square buttons that show a music-font glyph, a drawn sign or a short text,
//! highlighted in the accent color when chosen (like the note-input tiles of score editors).

use egui::{
    Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect, Response, Sense, Stroke, vec2,
};

use super::style;
use crate::fonts::MUSIC;

/// The size of a tile.
pub(crate) const TILE: egui::Vec2 = egui::Vec2::new(27.0, 32.0);

/// What a tile shows.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Face<'a> {
    /// A SMuFL glyph: its code point, font size and how far below the tile's center its
    /// baseline sits (notes with stems sit low, a whole note in the middle).
    Glyph {
        text: &'a str,
        size: f32,
        baseline: f32,
    },
    /// Short text such as "3:2".
    Text(&'a str),
    /// A tie: an arc between two noteheads.
    Tie,
    /// A slur over a short line (a beat-level legato run).
    Slur,
    /// A bend: a curve rising to an arrow.
    Bend,
    /// A whammy dip: down and back up.
    Whammy,
    /// A fade-in hairpin ("<").
    Hairpin,
    /// A slide: a line from `from` to `to`, each -1 (low), 0 (the note) or 1 (high), with
    /// a dot for the note at the end that is 0.
    Slide { from: i8, to: i8 },
}

/// One tile. `label` is what screen readers (and tests) call it; `tip` the tooltip.
pub(crate) fn tile(
    ui: &mut egui::Ui,
    face: Face<'_>,
    selected: bool,
    enabled: bool,
    label: &str,
    tip: &str,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(TILE, sense);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, selected, label));
    let tokens = style::tokens(ui);
    let (fill, ink) = if selected {
        (tokens.accent, tokens.on_accent)
    } else if enabled && response.hovered() {
        (tokens.control_hover, tokens.text)
    } else {
        (tokens.control, tokens.text)
    };
    let ink = if enabled {
        ink
    } else {
        tokens.text_weak.gamma_multiply(0.6)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, fill);
    paint_face(painter, rect, face, ink);
    response.on_hover_text(tip)
}

fn paint_face(painter: &Painter, rect: Rect, face: Face<'_>, ink: Color32) {
    match face {
        Face::Glyph {
            text,
            size,
            baseline,
        } => {
            let at = egui::pos2(rect.center().x, rect.center().y + baseline);
            music_glyph(painter, at, text, size, ink);
        }
        Face::Text(text) => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(12.0),
                ink,
            );
        }
        Face::Tie => {
            let left = rect.left_center() + vec2(7.0, 3.0);
            let right = rect.right_center() + vec2(-7.0, 3.0);
            for x in [left.x, right.x] {
                painter.circle_filled(egui::pos2(x, left.y + 3.0), 2.2, ink);
            }
            let points: Vec<Pos2> = (0..=12)
                .map(|step| {
                    let t = step as f32 / 12.0;
                    let x = left.x + (right.x - left.x) * t;
                    let y = left.y - 6.0 * (1.0 - (2.0 * t - 1.0).powi(2));
                    egui::pos2(x, y)
                })
                .collect();
            painter.add(egui::Shape::line(points, Stroke::new(1.6, ink)));
        }
        Face::Slur => {
            let left = rect.left_center() + vec2(6.0, 4.0);
            let right = rect.right_center() + vec2(-6.0, 4.0);
            let points: Vec<Pos2> = (0..=12)
                .map(|step| {
                    let t = step as f32 / 12.0;
                    let x = left.x + (right.x - left.x) * t;
                    let y = left.y - 8.0 * (1.0 - (2.0 * t - 1.0).powi(2));
                    egui::pos2(x, y)
                })
                .collect();
            painter.add(egui::Shape::line(points, Stroke::new(1.6, ink)));
        }
        Face::Bend => {
            // Like the score's bend: a quarter ellipse that leaves level and rises straight
            // into the base of a solid arrowhead.
            const QUARTER_ELLIPSE: f32 = 0.552_284_8;
            let start = rect.left_bottom() + vec2(6.0, -8.0);
            let tip = rect.right_top() + vec2(-9.0, 5.0);
            let base = tip + vec2(0.0, 6.0);
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [
                    start,
                    egui::pos2(start.x + (base.x - start.x) * QUARTER_ELLIPSE, start.y),
                    egui::pos2(base.x, base.y + (start.y - base.y) * QUARTER_ELLIPSE),
                    base,
                ],
                false,
                Color32::TRANSPARENT,
                Stroke::new(1.6, ink),
            ));
            painter.add(egui::Shape::convex_polygon(
                vec![tip, base + vec2(3.5, 0.0), base + vec2(-3.5, 0.0)],
                ink,
                Stroke::NONE,
            ));
        }
        Face::Whammy => {
            let y = rect.center().y - 3.0;
            let points = vec![
                egui::pos2(rect.left() + 5.0, y),
                egui::pos2(rect.left() + 10.0, y),
                egui::pos2(rect.center().x, y + 9.0),
                egui::pos2(rect.right() - 10.0, y),
                egui::pos2(rect.right() - 5.0, y),
            ];
            painter.add(egui::Shape::line(points, Stroke::new(1.6, ink)));
        }
        Face::Hairpin => {
            let tip = rect.left_center() + vec2(7.0, 0.0);
            let stroke = Stroke::new(1.5, ink);
            painter.line_segment([tip, rect.right_center() + vec2(-6.0, -6.0)], stroke);
            painter.line_segment([tip, rect.right_center() + vec2(-6.0, 6.0)], stroke);
        }
        Face::Slide { from, to } => {
            let y = |level: i8| rect.center().y - 6.0 * f32::from(level);
            let start = egui::pos2(rect.left() + 6.0, y(from));
            let end = egui::pos2(rect.right() - 6.0, y(to));
            painter.line_segment([start, end], Stroke::new(1.6, ink));
            for (level, at) in [(from, start), (to, end)] {
                if level == 0 {
                    painter.circle_filled(at, 2.8, ink);
                }
            }
        }
    }
}

/// Paints a music-font glyph with its baseline (not its box) centered at `at`.
pub(crate) fn music_glyph(painter: &Painter, at: Pos2, text: &str, size: f32, ink: Color32) {
    let galley = painter.layout_no_wrap(
        text.to_owned(),
        FontId::new(size, FontFamily::Name(MUSIC.into())),
        ink,
    );
    let baseline = galley
        .rows
        .first()
        .and_then(|row| row.row.glyphs.first().map(|glyph| row.pos.y + glyph.pos.y))
        .unwrap_or(0.0);
    let width = galley.size().x;
    painter.galley(egui::pos2(at.x - width / 2.0, at.y - baseline), galley, ink);
}

/// A row of segment buttons ("Voice 1 | Voice 2"): the chosen one in the accent color.
/// Returns the clicked index.
pub(crate) fn segments(
    ui: &mut egui::Ui,
    items: &[(&str, &str)],
    chosen: usize,
    tip: &str,
) -> Option<usize> {
    let tokens = style::tokens(ui);
    let mut clicked = None;
    egui::Frame::new()
        .fill(tokens.control)
        .corner_radius(5)
        .inner_margin(2)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (index, (text, label)) in items.iter().enumerate() {
                    let on = index == chosen;
                    let galley_width = 12.0 + 7.0 * text.chars().count() as f32;
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(galley_width.max(36.0), 22.0), Sense::click());
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::Button, true, on, *label)
                    });
                    let fill = if on {
                        tokens.accent
                    } else if response.hovered() {
                        tokens.control_hover
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, 4.0, fill);
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        *text,
                        FontId::proportional(12.0),
                        if on { tokens.on_accent } else { tokens.text },
                    );
                    if response.on_hover_text(tip).clicked() {
                        clicked = Some(index);
                    }
                }
            });
        });
    clicked
}
