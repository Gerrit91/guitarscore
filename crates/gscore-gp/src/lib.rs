//! Guitar Pro files: the song data exactly as the formats store it ([`model`]), and reading
//! and writing GP3, GP4, GP5 (binary), GPX (GP6) and GP7+ (zip with GPIF XML).
//!
//! Read with [`Song::read_gp3`], [`Song::read_gp4`], [`Song::read_gp5`], [`Song::read_gpx`]
//! or [`Song::read_gp`]; write with [`Song::write`] (GP3-5), [`Song::write_gpx`] or
//! [`Song::write_gp`].
//!
//! Only `gscore-model` (which wraps [`Song`]) and `gscore-io` (the safe entry point:
//! format detection, size limits, `catch_unwind`) may depend on this crate; `cargo xtask
//! layers` enforces that. Everything else uses `gscore_model::Song`.
//!
//! Derived from the `guitarpro` crate by slundi (<https://github.com/slundi/guitarpro>,
//! MIT, see `LICENSE-guitarpro`). Origin and changes: `docs/guitarpro.md`.

// This code is not yet held to the workspace's no-panic rules (it can still panic on
// malformed input, which is why gscore-io calls it behind `catch_unwind`). Tightening it
// module by module is tracked in docs/gaps.md.
#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::restriction,
    dead_code,
    missing_debug_implementations,
    unreachable_pub,
    unused_imports,
    unused_qualifications
)]

mod io;
pub mod model;

pub use io::error::{GpError, GpResult};
pub use model::*;
