//! Reading and writing: GP3/GP4/GP5 binary codec (the `Song*Ops` traits), GPIF XML
//! import/export, GPX (BCFZ/BCFS) and GP7 (zip) containers, byte primitives and errors.
//! The public entry points are methods on [`crate::Song`] (see `song.rs`).

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

pub(crate) use crate::model::*;

pub(crate) use self::beat::*;
pub(crate) use self::chord::*;
pub(crate) use self::effects::*;
pub(crate) use self::enums::*;
pub(crate) use self::error::*;
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
pub(crate) use self::track::*;

/// The item at `index`, or [`GpError::NoSuch`] naming `what`.
pub(crate) fn item<'a, T>(items: &'a [T], index: usize, what: &'static str) -> GpResult<&'a T> {
    items.get(index).ok_or(GpError::NoSuch { what, index })
}

/// The item at `index` to change, or [`GpError::NoSuch`] naming `what`.
pub(crate) fn item_mut<'a, T>(
    items: &'a mut [T],
    index: usize,
    what: &'static str,
) -> GpResult<&'a mut T> {
    items.get_mut(index).ok_or(GpError::NoSuch { what, index })
}
