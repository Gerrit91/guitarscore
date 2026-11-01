//! Reading and writing Guitar Pro files: bytes in, `Song` out, and back.
//!
//! This crate never touches the file system (that is `gscore-session`'s job) so it also
//! builds for wasm. The format is detected from the content, not the file name. Every call
//! into the `guitarpro` crate runs inside `std::panic::catch_unwind`, so a parser bug
//! becomes an [`IoError`], not a crash.

#![warn(clippy::arithmetic_side_effects)]

pub mod error;
pub mod export;
pub mod format;
pub mod import;

pub use error::IoError;
pub use export::export_guitar_pro_to_bytes;
pub use format::{FileFormat, detect_format};
pub use import::{Imported, MAX_INPUT_BYTES, import_guitar_pro_from_bytes};
