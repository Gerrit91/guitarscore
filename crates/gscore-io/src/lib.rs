//! The safe entry point for Guitar Pro files: bytes in, `gscore_model::Song` out (writing
//! follows in milestone M3).
//!
//! This crate never touches the file system (that is `gscore-session`'s job) so it also
//! builds for wasm. The format is detected from the content, not the file name, and input
//! size is capped. Every call into the reader in `gscore-gp` runs inside
//! `std::panic::catch_unwind`, so a parser bug becomes an [`IoError`], not a crash.

#![warn(clippy::arithmetic_side_effects)]

pub mod error;
pub mod format;
pub mod import;

pub use error::IoError;
pub use format::{FileFormat, detect_format};
pub use import::{Imported, MAX_INPUT_BYTES, import_guitar_pro_from_bytes};
