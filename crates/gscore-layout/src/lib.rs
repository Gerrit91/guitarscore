//! Headless layout engine that converts a document model into positioned draw operations.

//!
//! This crate provides:
//! - Conversion of musical elements to positioned draw operations
//! - Semantic color tokens (Staff, NoteText, Cursor, Selection)
//! - Layout algorithms for tablature rendering
//! - No GUI dependencies (headless)

#![warn(clippy::arithmetic_side_effects)]

pub mod draw_op;
pub mod layout;
pub mod types;

pub use draw_op::DrawOp;
pub use layout::layout;
pub use types::{Position, Size};
