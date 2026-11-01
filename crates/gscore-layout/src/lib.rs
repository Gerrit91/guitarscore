//! Headless layout: turns the model plus viewport settings into positioned draw operations
//! (lines, glyphs by SMuFL name, text, rectangles) with semantic color tokens. No GUI
//! imports.
//!
//! Not implemented yet (milestone M5; AGENTS.md section 5, decision 3, and "Notation
//! fonts").

#![warn(clippy::arithmetic_side_effects)]
