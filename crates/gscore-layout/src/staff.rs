//! The tab staff itself: line positions, staff lines, barlines and the tab clef.

use crate::draw::{ColorToken, DrawOp, Point};
use crate::glyph::Glyph;
use crate::metrics::MusicFont;

/// Gap between the start of a system and the tab clef.
const CLEF_LEFT_GAP: f32 = 0.6;
/// Gap between the tab clef and what follows it.
const CLEF_RIGHT_GAP: f32 = 1.0;
/// How much of the staff height the tab clef covers.
const CLEF_HEIGHT_RATIO: f32 = 0.8;

/// The geometry of a tab staff: one line per string, string 1 at the top.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Staff {
    /// Number of lines (strings), at least 1.
    lines: usize,
    /// Distance between two lines.
    spacing: f32,
}

impl Staff {
    /// A staff with `lines` lines (at least one) `spacing` apart.
    pub(crate) fn new(lines: usize, spacing: f32) -> Self {
        Staff {
            lines: lines.max(1),
            spacing,
        }
    }

    /// Number of lines.
    pub(crate) fn lines(&self) -> usize {
        self.lines
    }

    /// Distance from the top line to the bottom line.
    pub(crate) fn height(&self) -> f32 {
        self.spacing * self.lines.saturating_sub(1) as f32
    }

    /// Offset of line `line` (0 = top) below the top line.
    pub(crate) fn line_y(&self, line: usize) -> f32 {
        self.spacing * line as f32
    }

    /// Offset of the vertical middle below the top line.
    pub(crate) fn middle(&self) -> f32 {
        self.height() / 2.0
    }

    /// The staff lines from `x_from` to `x_to`, with the top line at `top`.
    pub(crate) fn draw_lines(
        &self,
        top: f32,
        x_from: f32,
        x_to: f32,
        font: &MusicFont,
        ops: &mut Vec<DrawOp>,
    ) {
        let thickness = font.engraving().staff_line_thickness;
        for line in 0..self.lines {
            let y = top + self.line_y(line);
            ops.push(DrawOp::Line {
                from: Point::new(x_from, y),
                to: Point::new(x_to, y),
                thickness,
                color: ColorToken::Staff,
            });
        }
    }

    /// A barline whose right edge is at `x`; `final` draws the thin-thick end barline.
    pub(crate) fn draw_barline(
        &self,
        x: f32,
        top: f32,
        is_final: bool,
        font: &MusicFont,
        ops: &mut Vec<DrawOp>,
    ) {
        let engraving = font.engraving();
        let line = |center: f32, thickness: f32| DrawOp::Line {
            from: Point::new(center, top),
            to: Point::new(center, top + self.height()),
            thickness,
            color: ColorToken::Barline,
        };
        let thin = engraving.thin_barline_thickness;
        if is_final {
            let thick = engraving.thick_barline_thickness;
            let thick_center = x - thick / 2.0;
            let thin_center = x - thick - engraving.thin_thick_barline_separation - thin / 2.0;
            ops.push(line(thin_center, thin));
            ops.push(line(thick_center, thick));
        } else {
            ops.push(line(x - thin / 2.0, thin));
        }
    }

    /// The thin line at the start of a system, its left edge at `x`.
    pub(crate) fn draw_start_line(
        &self,
        x: f32,
        top: f32,
        font: &MusicFont,
        ops: &mut Vec<DrawOp>,
    ) {
        let thin = font.engraving().thin_barline_thickness;
        self.draw_barline(x + thin, top, false, font, ops);
    }

    /// The tab clef glyph for this staff, and its scale.
    fn clef(&self, font: &MusicFont) -> (Glyph, f32) {
        let glyph = if self.lines >= 5 {
            Glyph::TabClef6
        } else {
            Glyph::TabClef4
        };
        let bbox = font.bbox(glyph);
        let glyph_height = bbox.y_max - bbox.y_min;
        let scale = if glyph_height > 0.0 && self.height() > 0.0 {
            self.height() * CLEF_HEIGHT_RATIO / glyph_height
        } else {
            1.0
        };
        (glyph, scale)
    }

    /// Room the system start takes before the first measure: clef and gaps.
    pub(crate) fn prefix_width(&self, font: &MusicFont) -> f32 {
        let (glyph, scale) = self.clef(font);
        CLEF_LEFT_GAP + font.advance(glyph) * scale + CLEF_RIGHT_GAP
    }

    /// The tab clef at the start of a system whose left edge is `x`.
    pub(crate) fn draw_clef(&self, x: f32, top: f32, font: &MusicFont, ops: &mut Vec<DrawOp>) {
        let (glyph, scale) = self.clef(font);
        let middle = top + self.middle();
        let at_y = middle + font.bbox(glyph).middle_above_origin() * scale;
        ops.push(DrawOp::Glyph {
            glyph,
            at: Point::new(x + CLEF_LEFT_GAP, at_y),
            scale,
            color: ColorToken::Notation,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_spaced_from_the_top() {
        let staff = Staff::new(6, 1.5);
        assert_eq!(staff.height(), 7.5);
        assert_eq!(staff.line_y(0), 0.0);
        assert_eq!(staff.line_y(5), 7.5);
        assert_eq!(staff.middle(), 3.75);
        assert_eq!(Staff::new(0, 1.5).lines(), 1, "never fewer than one line");
    }

    #[test]
    fn small_staves_get_the_four_line_clef() {
        let font = MusicFont::bravura().unwrap();
        assert_eq!(Staff::new(4, 1.5).clef(&font).0, Glyph::TabClef4);
        assert_eq!(Staff::new(7, 1.5).clef(&font).0, Glyph::TabClef6);
        let (_, big) = Staff::new(8, 1.5).clef(&font);
        let (_, normal) = Staff::new(6, 1.5).clef(&font);
        assert!(big > normal, "the clef grows with the staff");
    }
}
