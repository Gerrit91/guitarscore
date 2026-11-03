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
/// Gap between a repeat sign's thin line and its dots (Bravura's
/// `repeatBarlineDotSeparation`).
const REPEAT_DOT_SEPARATION: f32 = 0.16;

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

    /// Where the two repeat dots sit: in the spaces on either side of the middle.
    fn repeat_dot_offsets(&self) -> [f32; 2] {
        let middle = self.middle();
        // With an odd number of lines the middle is a line, so the spaces are half a
        // spacing away; with an even number the middle is a space and the next ones a
        // whole spacing away.
        let away = if self.lines % 2 == 1 {
            self.spacing / 2.0
        } else {
            self.spacing
        };
        [middle - away, middle + away]
    }

    /// The width a repeat sign takes (thick and thin line, dots).
    pub(crate) fn repeat_sign_width(font: &MusicFont) -> f32 {
        let e = font.engraving();
        e.thick_barline_thickness
            + e.thin_thick_barline_separation
            + e.thin_barline_thickness
            + REPEAT_DOT_SEPARATION
            + font.advance(Glyph::RepeatDot)
    }

    /// A repeat sign: at a start (`start`, thick line at `x`, dots to the right) or an
    /// end (thick line ending at `x`, dots to the left).
    pub(crate) fn draw_repeat(
        &self,
        x: f32,
        top: f32,
        start: bool,
        font: &MusicFont,
        ops: &mut Vec<DrawOp>,
    ) {
        let e = font.engraving();
        let (thick, thin, gap) = (
            e.thick_barline_thickness,
            e.thin_barline_thickness,
            e.thin_thick_barline_separation,
        );
        let dot = font.advance(Glyph::RepeatDot);
        // Distances from `x` inwards: thick line center, thin line center, dots' near edge.
        let thick_center = thick / 2.0;
        let thin_center = thick + gap + thin / 2.0;
        let dots = thick + gap + thin + REPEAT_DOT_SEPARATION;
        let (thick_x, thin_x, dot_x) = if start {
            (x + thick_center, x + thin_center, x + dots)
        } else {
            (x - thick_center, x - thin_center, x - dots - dot)
        };
        for (center, thickness) in [(thick_x, thick), (thin_x, thin)] {
            ops.push(DrawOp::Line {
                from: Point::new(center, top),
                to: Point::new(center, top + self.height()),
                thickness,
                color: ColorToken::Barline,
            });
        }
        for offset in self.repeat_dot_offsets() {
            ops.push(DrawOp::Glyph {
                glyph: Glyph::RepeatDot,
                at: Point::new(dot_x, top + offset),
                scale: 1.0,
                color: ColorToken::Barline,
            });
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
    fn repeat_dots_sit_in_the_spaces_around_the_middle() {
        assert_eq!(Staff::new(5, 1.0).repeat_dot_offsets(), [1.5, 2.5]);
        assert_eq!(Staff::new(6, 1.5).repeat_dot_offsets(), [2.25, 5.25]);
        assert_eq!(Staff::new(4, 1.5).repeat_dot_offsets(), [0.75, 3.75]);
        assert_eq!(Staff::new(7, 1.5).repeat_dot_offsets(), [3.75, 5.25]);
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
