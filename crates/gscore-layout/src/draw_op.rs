//! Draw operation definitions for headless rendering

use crate::types::{Position, Size};

/// Represents a positioned graphical operation in the layout
#[derive(Debug, Clone)]
pub enum DrawOp {
    /// Line operation
    Line {
        start: Position,
        end: Position,
    },
    /// Text operation
    Text {
        position: Position,
        text: String,
        font: String,
        size: u32,
    },
    /// Glyph operation using SMuFL names
    Glyph {
        position: Position,
        glyph_name: String,
        size: u32,
    },
    /// Rectangle operation
    Rectangle {
        position: Position,
        size: Size,
    },
}
