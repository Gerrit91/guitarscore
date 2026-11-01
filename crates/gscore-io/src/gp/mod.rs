//! Reading and writing Guitar Pro files: GP3/GP4/GP5 (binary), GPX (GP6) and GP7+ (zip with
//! GPIF XML), into and out of the song data in `gscore_model::gp`.
//!
//! Derived from the `guitarpro` crate by slundi (<https://github.com/slundi/guitarpro>,
//! MIT, see `LICENSE-guitarpro`). Changes since: `docs/guitarpro.md`. This code is not yet
//! held to the workspace's no-panic rules (it runs behind `catch_unwind` in
//! [`crate::import`]); see `docs/gaps.md`.

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

pub(crate) mod error;
pub(crate) mod gpif;
pub(crate) mod gpif_export;
pub(crate) mod gpif_import;
pub(crate) mod gpx;
pub(crate) mod primitive;

mod beat;
mod chord;
mod effects;
mod enums;
mod headers;
mod key_signature;
mod lyric;
mod measure;
mod midi;
mod mix_table;
mod note;
mod page;
mod rse;
mod song;
mod track;

pub(crate) use gscore_model::gp::*;

pub(crate) use self::beat::*;
pub(crate) use self::chord::*;
pub(crate) use self::effects::*;
pub(crate) use self::enums::*;
pub(crate) use self::error::*;
pub(crate) use self::gpif_export::SongGpifExportOps;
pub(crate) use self::gpif_import::SongGpifOps;
pub(crate) use self::headers::*;
pub(crate) use self::key_signature::*;
pub(crate) use self::lyric::*;
pub(crate) use self::measure::*;
pub(crate) use self::midi::*;
pub(crate) use self::mix_table::*;
pub(crate) use self::note::*;
pub(crate) use self::page::*;
pub(crate) use self::primitive::*;
pub(crate) use self::rse::*;
pub(crate) use self::song::*;
pub(crate) use self::track::*;
