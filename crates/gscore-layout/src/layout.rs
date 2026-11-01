//! Layout: converts the musical model into positioned draw operations.

use gscore_model::Song;

use crate::draw_op::DrawOp;

/// Converts a song into draw operations for rendering.
///
/// Placeholder until milestone M3: it returns no draw operations.
pub fn layout(_song: &Song) -> Vec<DrawOp> {
    Vec::new()
}
