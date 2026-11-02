//! The safe entry point for Guitar Pro files: bytes in, `gscore_model::Song` out, and back
//! (safe saving to files follows in milestone M3).
//!
//! This crate never touches the file system (that is `gscore-session`'s job) so it also
//! builds for wasm. The format is detected from the content, not the file name, and input
//! size is capped. Every call into the reader and writer in `gscore-gp` runs inside
//! `std::panic::catch_unwind`, so a parser bug becomes an [`IoError`], not a crash. (Since
//! `gscore-gp` follows the no-panic lints, this is the second line of defense.)

#![warn(clippy::arithmetic_side_effects)]

mod contain;
pub mod error;
pub mod export;
pub mod format;
pub mod import;

pub use error::IoError;
pub use export::export_to_bytes;
pub use format::{FileFormat, detect_format};
pub use import::{Imported, MAX_INPUT_BYTES, import_guitar_pro_from_bytes};
