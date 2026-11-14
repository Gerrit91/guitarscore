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

/// What the score is painted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Surface {
    /// Sheets of paper floating on the canvas, each with its shadow and edge.
    Pages,
    /// Paper filling the whole view (the screen views), without page edges.
    Screen,
}

/// The box a horizontal or vertical line from `a` to `b` of `width` points fills on a
/// screen with `pixels_per_point`, and how opaque it is; `None` for a slanted line.
///
/// Every edge is rounded to the pixel grid from where the layout puts it: the ends, and
/// the side the line's thickness starts at. Two lines that share an edge in the layout
/// (the staff lines and the line that starts their system) then share it on the screen;
/// rounding each line's middle instead lets one stick out past the other by a pixel when
/// the score is zoomed out. The thickness is a whole number of pixels, at least one; a
/// line thinner than a pixel is drawn that much lighter instead.
fn crisp_line(a: Pos2, b: Pos2, width: f32, pixels_per_point: f32) -> Option<(Rect, f32)> {
    const ALIGNED: f32 = 1e-3;
    if !(pixels_per_point.is_finite() && pixels_per_point > 0.0 && width.is_finite()) {
        return None;
    }
    let pixel = 1.0 / pixels_per_point;
    // Two edges that are the same in the layout reach here a rounding error apart; the
    // small bias keeps them from parting at an exact half pixel, where pages often lie.
    const TIE_BIAS: f32 = 1e-3;
    let snap = |value: f32| (value * pixels_per_point + 0.5 + TIE_BIAS).floor() * pixel;
    // Across the line: from its near edge, a whole number of pixels thick.
    let across = |middle: f32| {
        let near = snap(middle - width / 2.0);
        (near, near + snap(width).max(pixel))
    };
    // Along the line: both ends on the grid; a short line keeps one pixel.
    let along = |from: f32, to: f32| {
        let (start, end) = (snap(from.min(to)), snap(from.max(to)));
        (start, end.max(start + pixel))
    };
    let (x, y) = if (a.y - b.y).abs() < ALIGNED {
        (along(a.x, b.x), across(a.y))
    } else if (a.x - b.x).abs() < ALIGNED {
        (across(a.x), along(a.y, b.y))
    } else {
        return None;
    };
    let opacity = (width * pixels_per_point).clamp(0.0, 1.0);
    Some((
        Rect::from_min_max(Pos2::new(x.0, y.0), Pos2::new(x.1, y.1)),
        opacity,
    ))
}

/// Paints the pages with the top-left corner at `origin`, arranged by `arrangement`,
/// skipping what lies outside `visible`. On a [`Surface::Screen`] the caller fills the view
/// with paper; no page edges are drawn.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint(
    painter: &Painter,
    origin: Pos2,
    layout: &Layout,
    arrangement: &Arrangement,
    theme: &Theme,
    zoom: f32,
    visible: Rect,
    surface: Surface,
) {
    let scale = POINTS_PER_SPACE * zoom;
    let pixels_per_point = painter.ctx().pixels_per_point();
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
        if surface == Surface::Screen || !visible.expand(CULL_MARGIN).intersects(page) {
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
                    let ink = color(theme.color(*token));
                    let width = thickness * scale;
                    // Staff lines, barlines and stems are snapped to the pixels as boxes,
                    // so lines that meet in the layout meet on the screen too.
                    match crisp_line(a, b, width, pixels_per_point) {
                        Some((rect, opacity)) => {
                            painter.rect_filled(rect, 0.0, ink.gamma_multiply(opacity));
                        }
                        None => {
                            painter.line_segment([a, b], Stroke::new(width, ink));
                        }
                    }
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

/// A very light tint over the cursor's bar, to see at a glance where the cursor (or the
/// playback) is.
pub(crate) fn paint_cursor_bar(painter: &Painter, bar: Rect, theme: &Theme) {
    painter.rect_filled(
        bar,
        2.0,
        color(theme.cursor).gamma_multiply(CURSOR_BAR_ALPHA),
    );
}

/// How strongly [`paint_cursor_bar`] tints the bar.
const CURSOR_BAR_ALPHA: f32 = 0.05;

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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// The staff lines start at the left edge of the line that starts their system, at
    /// every zoom and wherever the page lies on the pixel grid.
    #[test]
    fn staff_lines_start_where_the_system_line_does() {
        let (left, top, bottom, thin) = (12.0_f32, 30.0_f32, 37.5_f32, 0.16_f32);
        for pixels_per_point in [1.0_f32, 1.25, 2.0] {
            for zoom in [0.25_f32, 0.4, 0.5, 0.75, 1.0, 2.0] {
                for step in 0..40_u8 {
                    let scale = POINTS_PER_SPACE * zoom;
                    let origin = Pos2::new(100.0 + f32::from(step) * 0.025, 50.3);
                    let at = |x: f32, y: f32| origin + Vec2::new(x, y) * scale;
                    let width = thin * scale;
                    let (start, _) = crisp_line(
                        at(left + thin / 2.0, top),
                        at(left + thin / 2.0, bottom),
                        width,
                        pixels_per_point,
                    )
                    .unwrap();
                    let (staff, _) =
                        crisp_line(at(left, top), at(left + 80.0, top), width, pixels_per_point)
                            .unwrap();
                    let pixel = 1.0 / pixels_per_point;
                    assert!(
                        (staff.left() - start.left()).abs() < pixel / 4.0,
                        "zoom {zoom}, step {step}: {staff:?} vs {start:?}"
                    );
                    // The start line touches the top staff line (no gap between them).
                    assert!(start.top() <= staff.bottom() + pixel / 4.0);
                    assert!(start.width() >= pixel - 1e-4 && staff.height() >= pixel - 1e-4);
                }
            }
        }
    }

    #[test]
    fn crisp_lines_are_whole_pixels_and_slanted_lines_are_left_alone() {
        // A hairline keeps one pixel and is drawn lighter.
        let (rect, opacity) =
            crisp_line(Pos2::new(3.2, 7.4), Pos2::new(40.7, 7.4), 0.5, 1.0).unwrap();
        assert_eq!((rect.left(), rect.right()), (3.0, 41.0));
        assert_eq!(rect.height(), 1.0);
        assert!((opacity - 0.5).abs() < 1e-6);
        // A thick line is a whole number of pixels and fully opaque.
        let (rect, opacity) =
            crisp_line(Pos2::new(5.0, 2.0), Pos2::new(5.0, 20.0), 2.6, 1.0).unwrap();
        assert_eq!(rect.width(), 3.0);
        assert_eq!(opacity, 1.0);
        assert!(crisp_line(Pos2::new(0.0, 0.0), Pos2::new(5.0, 3.0), 1.0, 1.0).is_none());
        assert!(crisp_line(Pos2::new(0.0, 0.0), Pos2::new(5.0, 0.0), 1.0, 0.0).is_none());
    }

    /// Three A4 pages, as the layout stacks them.
    fn layout() -> Layout {
        let page = gscore_layout::Rect {
            origin: Point::new(0.0, 0.0),
            width: 120.0,
            height: 170.0,
        };
        Layout {
            width: 120.0,
            height: page.height * 3.0 + gscore_layout::PAGE_GAP * 2.0,
            pages: (0..3)
                .map(|index| gscore_layout::Rect {
                    origin: Point::new(0.0, index as f32 * (page.height + gscore_layout::PAGE_GAP)),
                    ..page
                })
                .collect(),
            ops: Vec::new(),
            places: gscore_layout::Places::default(),
            tracks: Vec::new(),
            header_texts: Vec::new(),
            marks: Vec::new(),
        }
    }

    #[test]
    fn vertical_pages_stay_in_one_column() {
        let arrangement = Arrangement::new(&layout(), 1);
        let size = arrangement.size(&layout());
        assert!((size.x - 120.0).abs() < 1e-3);
        assert!(size.y > 500.0);
    }

    #[test]
    fn horizontal_pages_go_in_one_row() {
        let arrangement = Arrangement::new(&layout(), 3);
        let size = arrangement.size(&layout());
        assert!(size.x > 360.0, "three pages side by side: {size:?}");
        assert!((size.y - 170.0).abs() < 1e-3, "one row: {size:?}");
    }

    #[test]
    fn grid_pages_wrap_to_the_next_row() {
        let arrangement = Arrangement::new(&layout(), 2);
        let size = arrangement.size(&layout());
        assert!(size.x > 240.0 && size.x < 260.0, "two columns: {size:?}");
        assert!(size.y > 340.0, "two rows: {size:?}");
    }
}
