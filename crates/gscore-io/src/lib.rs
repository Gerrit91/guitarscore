// gscore-io crate - GP file I/O operations

//! This crate handles reading and writing Guitar Pro files.
//! It uses the guitarpro crate internally for parsing and serializing,
//! while providing safe error handling with catch_unwind.

pub mod error;
pub mod import;
pub mod export;

pub use error::IoError;
pub use import::{import_guitar_pro_file, import_guitar_pro_from_bytes};
pub use export::{export_guitar_pro_file, export_guitar_pro_to_bytes};
