//! Paints a [`Layout`]'s draw operations with egui. Layout works in staff spaces; here they
//! become screen points (`POINTS_PER_SPACE` times the zoom). Only operations near the
//! visible area are painted, so long scores stay cheap per frame.

use egui::{Color32, FontFamily, FontId, Painter, Pos2, Rect, Stroke, Vec2};
use gscore_layout::{DrawOp, Layout, Point, Rgb, TextAnchor, TextFont, Theme};

use crate::fonts;

/// Screen points per staff space at zoom 1 (about MuseScore's default 1.75 mm on screen).
pub(crate) const POINTS_PER_SPACE: f32 = 7.0;
/// Operations this far (in points) outside the visible area are still painted, so glyphs
/// that extend beyond their anchor do not pop in at the edges.
const CULL_MARGIN: f32 = 60.0;

/// The page's size on screen at `zoom`.
pub(crate) fn page_size(layout: &Layout, zoom: f32) -> Vec2 {
    Vec2::new(layout.width, layout.height) * POINTS_PER_SPACE * zoom
}

/// Paints the page with its top-left corner at `origin`, skipping what lies outside
/// `visible`.
pub(crate) fn paint(
    painter: &Painter,
    origin: Pos2,
    layout: &Layout,
    theme: &Theme,
    zoom: f32,
    visible: Rect,
) {
    let scale = POINTS_PER_SPACE * zoom;
    let to_screen = |p: Point| origin + Vec2::new(p.x, p.y) * scale;
    let page = Rect::from_min_size(origin, page_size(layout, zoom));
    painter.rect_filled(page, 0.0, color(theme.paper));
    // A thin edge, so the page stands out from the window even when both are dark.
    let edge = Stroke::new(1.0, color(theme.staff).gamma_multiply(0.5));
    painter.rect_stroke(page, 0.0, edge, egui::StrokeKind::Outside);
    let visible = visible.expand(CULL_MARGIN);

    for op in &layout.ops {
        match op {
            DrawOp::Line {
                from,
                to,
                thickness,
                color: token,
            } => {
                let (a, b) = (to_screen(*from), to_screen(*to));
                if visible.intersects(Rect::from_two_pos(a, b)) {
                    let stroke = Stroke::new(thickness * scale, color(theme.color(*token)));
                    painter.line_segment([a, b], stroke);
                }
            }
            DrawOp::Rect { rect, color: token } => {
                let min = to_screen(rect.origin);
                let screen = Rect::from_min_size(min, Vec2::new(rect.width, rect.height) * scale);
                if visible.intersects(screen) {
                    painter.rect_filled(screen, 0.0, color(theme.color(*token)));
                }
            }
            DrawOp::Curve {
                from,
                to,
                control1,
                control2,
                thickness,
                color: token,
            } => {
                let points = [
                    to_screen(*from),
                    to_screen(*control1),
                    to_screen(*control2),
                    to_screen(*to),
                ];
                let bounds = Rect::from_points(&points);
                if visible.intersects(bounds) {
                    let stroke = Stroke::new(thickness * scale, color(theme.color(*token)));
                    painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                        points,
                        false,
                        Color32::TRANSPARENT,
                        stroke,
                    ));
                }
            }
            DrawOp::Glyph {
                glyph,
                at,
                scale: glyph_scale,
                color: token,
            } => {
                let at = to_screen(*at);
                if visible.contains(at) {
                    // One em of a SMuFL font is four staff spaces.
                    let font = FontId::new(
                        4.0 * glyph_scale * scale,
                        FontFamily::Name(fonts::MUSIC.into()),
                    );
                    let text = glyph.codepoint().to_string();
                    text_at_baseline(
                        painter,
                        at,
                        text,
                        font,
                        TextAnchor::Start,
                        theme.color(*token),
                    );
                }
            }
            DrawOp::Text {
                text,
                at,
                size,
                font,
                anchor,
                color: token,
            } => {
                let at = to_screen(*at);
                if visible.contains(at) {
                    let family = match font {
                        TextFont::Regular => FontFamily::Proportional,
                        TextFont::Bold => FontFamily::Name(fonts::BOLD.into()),
                    };
                    let font = FontId::new(size * scale, family);
                    text_at_baseline(
                        painter,
                        at,
                        text.clone(),
                        font,
                        *anchor,
                        theme.color(*token),
                    );
                }
            }
        }
    }
}

/// Paints `text` so that its baseline (not its top) is at `at.y`, anchored horizontally.
fn text_at_baseline(
    painter: &Painter,
    at: Pos2,
    text: String,
    font: FontId,
    anchor: TextAnchor,
    rgb: Rgb,
) {
    let ink = color(rgb);
    let galley = painter.layout_no_wrap(text, font, ink);
    // Glyph positions are baselines relative to their row; rows are relative to the galley.
    let baseline = galley
        .rows
        .first()
        .and_then(|row| row.row.glyphs.first().map(|glyph| row.pos.y + glyph.pos.y))
        .unwrap_or(0.0);
    let width = galley.size().x;
    let x = match anchor {
        TextAnchor::Start => at.x,
        TextAnchor::Middle => at.x - width / 2.0,
        TextAnchor::End => at.x - width,
    };
    painter.galley(Pos2::new(x, at.y - baseline), galley, ink);
}

/// Paints the edit cursor: a faint band over the beat's column and a box around the
/// string, each if given, in the theme's cursor color.
pub(crate) fn paint_cursor(
    painter: &Painter,
    column: Option<Rect>,
    string: Option<Rect>,
    theme: &Theme,
) {
    let ink = color(theme.cursor);
    if let Some(column) = column {
        painter.rect_filled(column, 2.0, ink.gamma_multiply(0.12));
    }
    if let Some(string) = string {
        painter.rect_stroke(
            string,
            2.0,
            Stroke::new(1.5, ink),
            egui::StrokeKind::Outside,
        );
    }
}

pub(crate) fn color(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}
