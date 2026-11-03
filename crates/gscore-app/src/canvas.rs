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

/// How the layout's pages are shown: in rows of `columns` pages, left to right. The layout
/// stacks its pages in one column; this maps between its coordinates and the shown ones
/// (both in staff spaces). With one column (or the endless page) nothing moves.
#[derive(Debug, Clone)]
pub(crate) struct Arrangement {
    pages: Vec<gscore_layout::Rect>,
    columns: usize,
    page_width: f32,
    page_height: f32,
}

impl Arrangement {
    /// The pages of `layout` in rows of `columns` (at least one).
    pub(crate) fn new(layout: &Layout, columns: usize) -> Self {
        let first = layout.pages.first();
        Arrangement {
            columns: columns.clamp(1, layout.pages.len().max(1)),
            page_width: first.map_or(layout.width, |page| page.width),
            page_height: first.map_or(layout.height, |page| page.height),
            pages: layout.pages.clone(),
        }
    }

    /// As many pages side by side as fit `width` points at `zoom`.
    pub(crate) fn fitting(layout: &Layout, width: f32, zoom: f32) -> Self {
        let scale = POINTS_PER_SPACE * zoom;
        let page = layout.pages.first().map_or(layout.width, |page| page.width) * scale;
        let gap = gscore_layout::PAGE_GAP * scale;
        let fit = ((width + gap) / (page + gap)).floor();
        // A finite count from 1 up (checked just above), so the cast cannot truncate.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let columns = if fit.is_finite() && fit >= 1.0 {
            fit.min(64.0) as usize
        } else {
            1
        };
        Arrangement::new(layout, columns)
    }

    fn step(&self) -> (f32, f32) {
        (
            self.page_width + gscore_layout::PAGE_GAP,
            self.page_height + gscore_layout::PAGE_GAP,
        )
    }

    /// How far page `index` moves.
    fn shift(&self, index: usize) -> Vec2 {
        let Some(page) = self.pages.get(index) else {
            return Vec2::ZERO;
        };
        let (column, row) = (index % self.columns, index / self.columns);
        let (across, down) = self.step();
        Vec2::new(column as f32 * across, row as f32 * down - page.origin.y)
    }

    fn page_of(&self, y: f32) -> usize {
        self.pages
            .iter()
            .rposition(|page| page.origin.y <= y)
            .unwrap_or(0)
    }

    /// Where a layout point is shown.
    pub(crate) fn to_display(&self, point: Point) -> Vec2 {
        Vec2::new(point.x, point.y) + self.shift(self.page_of(point.y))
    }

    /// Where a layout rectangle is shown (it moves with the page of its top).
    pub(crate) fn rect(&self, rect: gscore_layout::Rect) -> gscore_layout::Rect {
        let shift = self.shift(self.page_of(rect.origin.y));
        gscore_layout::Rect {
            origin: Point::new(rect.origin.x + shift.x, rect.origin.y + shift.y),
            ..rect
        }
    }

    /// The layout point shown at `shown`.
    pub(crate) fn layout_point(&self, shown: Vec2) -> Point {
        let (across, down) = self.step();
        let column = (shown.x / across).floor().max(0.0);
        let row = (shown.y / down).floor().max(0.0);
        // Small, non-negative whole numbers (floored and clamped above).
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = (row as usize)
            .saturating_mul(self.columns)
            .saturating_add((column as usize).min(self.columns.saturating_sub(1)))
            .min(self.pages.len().saturating_sub(1));
        let point = shown - self.shift(index);
        Point::new(point.x, point.y)
    }

    /// The shown size in staff spaces.
    pub(crate) fn size(&self, layout: &Layout) -> Vec2 {
        if self.columns <= 1 {
            return Vec2::new(layout.width, layout.height);
        }
        let rows = self.pages.len().div_ceil(self.columns);
        let (across, down) = self.step();
        Vec2::new(
            self.columns as f32 * across - gscore_layout::PAGE_GAP,
            rows as f32 * down - gscore_layout::PAGE_GAP,
        )
    }
}

/// The shown pages' size on screen at `zoom`.
pub(crate) fn page_size(layout: &Layout, arrangement: &Arrangement, zoom: f32) -> Vec2 {
    arrangement.size(layout) * POINTS_PER_SPACE * zoom
}

/// Paints the pages with the top-left corner at `origin`, arranged by `arrangement`,
/// skipping what lies outside `visible`.
pub(crate) fn paint(
    painter: &Painter,
    origin: Pos2,
    layout: &Layout,
    arrangement: &Arrangement,
    theme: &Theme,
    zoom: f32,
    visible: Rect,
) {
    let scale = POINTS_PER_SPACE * zoom;
    let to_screen = |p: Point| origin + arrangement.to_display(p) * scale;
    // Each page floats on the canvas: a soft shadow, the paper, and a hairline edge so it
    // stands out even when paper and canvas are both dark.
    let shadow = egui::Shadow {
        offset: [0, 3],
        blur: 14,
        spread: 0,
        color: Color32::from_black_alpha(70),
    };
    let edge = Stroke::new(1.0, Color32::from_black_alpha(28));
    let paper = color(theme.paper);
    for page in &layout.pages {
        let page = arrangement.rect(*page);
        let page = Rect::from_min_size(
            origin + Vec2::new(page.origin.x, page.origin.y) * scale,
            Vec2::new(page.width, page.height) * scale,
        );
        if !visible.expand(CULL_MARGIN).intersects(page) {
            continue;
        }
        painter.add(shadow.as_shape(page, 2));
        painter.rect_filled(page, 2.0, paper);
        painter.rect_stroke(page, 2.0, edge, egui::StrokeKind::Outside);
    }
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
            DrawOp::Polygon {
                points,
                color: token,
            } => {
                let points: Vec<Pos2> = points.iter().map(|p| to_screen(*p)).collect();
                if visible.intersects(Rect::from_points(&points)) {
                    painter.add(egui::Shape::convex_polygon(
                        points,
                        color(theme.color(*token)),
                        Stroke::NONE,
                    ));
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

#[cfg(test)]
mod arrangement_tests {
    use super::*;
    use gscore_layout::{Fonts, LayoutOptions, layout_track};
    use gscore_model::TrackIndex;
    use gscore_model::testing::{self, GUITAR_6};

    fn paged(measures: usize) -> Layout {
        let song = testing::song(&[GUITAR_6], measures).unwrap();
        let options = LayoutOptions::default().on_pages();
        layout_track(
            &song,
            TrackIndex::new(0),
            &options,
            &Fonts::bundled().unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn pages_sit_side_by_side_and_points_map_back() {
        let layout = paged(200);
        let pages = layout.pages.len();
        assert!(pages >= 3, "{pages} pages");
        let page_width = layout.pages[0].width * POINTS_PER_SPACE;
        // Room for two pages at zoom 1, for one at zoom 2.
        let width = page_width * 2.0 + 100.0;
        assert_eq!(Arrangement::fitting(&layout, width, 1.0).columns, 2);
        assert_eq!(Arrangement::fitting(&layout, width, 2.0).columns, 1);
        assert_eq!(Arrangement::fitting(&layout, f32::NAN, 1.0).columns, 1);

        let two = Arrangement::new(&layout, 2);
        // The second page sits to the right of the first, at the same height.
        let second = layout.pages[1].origin;
        let shown = two.to_display(Point::new(second.x + 3.0, second.y + 4.0));
        assert!((shown.y - 4.0).abs() < 1e-3, "{shown:?}");
        assert!(shown.x > layout.pages[0].width, "{shown:?}");
        // Every system's top maps there and back.
        for system in &layout.places.systems {
            let point = Point::new(20.0, system.tab_top);
            let back = two.layout_point(two.to_display(point));
            assert!((back.x - point.x).abs() < 1e-3 && (back.y - point.y).abs() < 1e-3);
        }
        let size = two.size(&layout);
        assert!(size.x < layout.width * 2.0 + gscore_layout::PAGE_GAP + 1e-3);
        assert!(size.y < layout.height);
    }

    #[test]
    fn one_column_changes_nothing() {
        let layout = paged(40);
        let one = Arrangement::new(&layout, 1);
        let point = Point::new(12.0, layout.height - 10.0);
        let shown = one.to_display(point);
        assert!((shown.x - point.x).abs() < 1e-4 && (shown.y - point.y).abs() < 1e-4);
        assert_eq!(one.size(&layout), Vec2::new(layout.width, layout.height));
    }
}
