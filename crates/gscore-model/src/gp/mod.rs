//! Guitar Pro song data: the song, tracks, measures, beats, notes and effects exactly as
//! Guitar Pro files store them.
//!
//! [`crate::Song`] wraps these types, so a loaded file keeps every field. Only `gscore-io`
//! (which reads and writes the bytes) may use this module directly; everything else goes
//! through the wrapper. `cargo xtask layers` enforces that.
//!
//! Derived from the `guitarpro` crate by slundi (<https://github.com/slundi/guitarpro>,
//! MIT, see `LICENSE-guitarpro`). Changes since: `docs/guitarpro.md`. This code is not yet
//! held to the workspace's no-panic rules; see `docs/gaps.md`.

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

pub mod beat;
pub mod chord;
pub mod effects;
pub mod enums;
pub mod headers;
pub mod key_signature;
pub mod lyric;
pub mod measure;
pub mod midi;
pub mod mix_table;
pub mod note;
pub mod page;
pub mod rse;
pub mod song;
pub mod track;

pub use beat::*;
pub use chord::*;
pub use effects::*;
pub use enums::*;
pub use headers::*;
pub use key_signature::*;
pub use lyric::*;
pub use measure::*;
pub use midi::*;
pub use mix_table::*;
pub use note::*;
pub use page::*;
pub use rse::*;
pub use song::*;
pub use track::*;
