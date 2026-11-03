//! The output of layout: positioned drawing operations with semantic colors.
//!
//! Coordinates are in **staff spaces** (the distance between two lines of a five-line staff
//! in the music font, a quarter of its em), x to the right and y **down**, with the origin
//! at the top-left corner of the page. Renderers scale them (see
//! [`crate::LayoutOptions::staff_space_mm`]).

use crate::glyph::Glyph;

/// A point in staff spaces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// Distance from the left edge.
    pub x: f32,
    /// Distance from the top edge (y grows downwards).
    pub y: f32,
}

impl Point {
    /// The point at `x`, `y`.
    pub const fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }
}

/// An axis-aligned rectangle in staff spaces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// The top-left corner.
    pub origin: Point,
    /// Width (not negative).
    pub width: f32,
    /// Height (not negative).
    pub height: f32,
}

/// What a drawn element is, so the renderer can pick its color from a theme. Layout never
/// chooses colors itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorToken {
    /// The page background. Also used to mask staff lines behind fret numbers.
    Paper,
    /// Staff (string) lines.
    Staff,
    /// Barlines.
    Barline,
    /// Fret numbers and other note text on the staff.
    FretNumber,
    /// Music symbols: clefs, time signatures, rests.
    Notation,
    /// Title and other song information.
    Title,
    /// Small annotations: measure numbers, tuning, tempo.
    Annotation,
    /// The edit cursor.
    Cursor,
}

/// Which text font a [`DrawOp::Text`] uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextFont {
    /// The regular text face.
    Regular,
    /// The bold text face.
    Bold,
}

/// Where a text's position is, horizontally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextAnchor {
    /// The position is the start (left end) of the text.
    Start,
    /// The position is the middle of the text.
    Middle,
    /// The position is the end (right end) of the text.
    End,
}

/// One drawing operation.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawOp {
    /// A straight line.
    Line {
        /// Start point.
        from: Point,
        /// End point.
        to: Point,
        /// Line thickness in staff spaces.
        thickness: f32,
        /// What the line is.
        color: ColorToken,
    },
    /// A filled rectangle.
    Rect {
        /// The rectangle.
        rect: Rect,
        /// What the rectangle is.
        color: ColorToken,
    },
    /// A filled polygon (convex), for solid arrowheads.
    Polygon {
        /// The corners, in order.
        points: Vec<Point>,
        /// What the polygon is.
        color: ColorToken,
    },
    /// A cubic Bézier curve, for slurs, ties and bends.
    Curve {
        /// Start point.
        from: Point,
        /// End point.
        to: Point,
        /// First control point, next to `from`.
        control1: Point,
        /// Second control point, next to `to`.
        control2: Point,
        /// Line thickness in staff spaces.
        thickness: f32,
        /// What the curve is.
        color: ColorToken,
    },
    /// A music-font glyph, by SMuFL name. The renderer resolves the code point.
    Glyph {
        /// The glyph.
        glyph: Glyph,
        /// The glyph's origin (on its baseline).
        at: Point,
        /// Scale relative to the normal size (1.0: one em is four staff spaces).
        scale: f32,
        /// What the glyph is.
        color: ColorToken,
    },
    /// A run of text in the text font.
    Text {
        /// The text.
        text: String,
        /// Position of the baseline; see `anchor` for the horizontal meaning.
        at: Point,
        /// Font size (em) in staff spaces.
        size: f32,
        /// Regular or bold.
        font: TextFont,
        /// Horizontal anchoring of `at`.
        anchor: TextAnchor,
        /// What the text is.
        color: ColorToken,
    },
}
