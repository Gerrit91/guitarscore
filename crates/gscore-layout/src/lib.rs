//! Headless layout: turns the model plus settings into positioned draw operations (lines,
//! glyphs by SMuFL name, text, rectangles) with semantic color tokens. No GUI imports.
//!
//! The pipeline (`score.rs`), per track and then together:
//! 1. [`header`]: title block, credits, tuning and tempo.
//! 2. [`measure`]: every measure on its own, as columns of beats with natural spacing.
//!    This is the unit [`LayoutCache`] keeps between edits. With several tracks, each
//!    measure's columns are then lined up across them (`measure::align`).
//! 3. [`marks`]: section names, tempo changes, repeats, endings and dynamics, decided in
//!    order over the track (a tempo or dynamic is written where it changes).
//! 4. [`system`]: measures broken into lines and justified to the page width.
//! 5. Drawing: staff lines, clef, barlines and the measures' content, as [`DrawOp`]s.
//!
//! [`svg::to_svg`] renders the result for golden tests and the command line, and
//! [`pdf::to_pdf`] a layout on pages as a PDF with embedded fonts; the GUI draws the same
//! operations. One track ([`layout_track`]) or several ([`layout_tracks`], each system
//! stacking every track's staves with their beats lined up, see `score.rs`) is laid out
//! (standard notation, tab, or both per track; a percussion staff for drums), on one
//! endless page or on separate pages ([`LayoutOptions::page_height`]): pages are stacked
//! from top to bottom with a gap between them, and a system that does not fit moves to
//! the next page.

#![warn(clippy::arithmetic_side_effects)]

mod cache;
mod draw;
mod error;
mod glyph;
mod header;
mod marks;
mod measure;
mod metrics;
mod notation;
mod options;
mod ornaments;
mod pages;
mod pdf;
mod place;
mod score;
mod staff;
mod svg;
mod system;
mod theme;

use std::sync::Arc;

use gscore_model::{MeasureView, Song, TrackIndex};

// The golden tests open the owner's files with `gscore-io`; this marks the test-only
// dependency for the library test target's unused-dependency check.
#[cfg(test)]
use gscore_io as _;

pub use cache::LayoutCache;
pub use draw::{ColorToken, DrawOp, Point, Rect, TextAnchor, TextFont};
pub use error::LayoutError;
pub use glyph::{Digit, Flag, Glyph};
use gscore_model::VoiceIndex;
pub use metrics::{EngravingDefaults, GlyphBox, MusicFont, TextMetrics};
pub use options::{EmptyStaves, LayoutOptions, Spacing};
pub use pdf::{PdfFonts, to_pdf};
pub use place::{BeatPlace, Hit, MeasurePlace, Places, SystemPlace};
pub use svg::to_svg;
pub use theme::{Rgb, Theme};

use measure::MeasureLayout;
use notation::Notation;
use staff::Staff;

/// Space between the header and the first system's top line (room for measure numbers).
pub(crate) const HEADER_GAP: f32 = 4.0;
/// How far a repeat count ("x3") sits above the staff (to its baseline), like measure
/// numbers.
pub(crate) const MEASURE_NUMBER_RISE: f32 = 0.8;
/// Gap between a repeat count and the end repeat's barline.
pub(crate) const REPEAT_COUNT_GAP: f32 = 0.6;
/// Gap between two pages, in staff spaces.
pub const PAGE_GAP: f32 = 6.0;

/// The fonts layout measures with.
#[derive(Debug, Clone)]
pub struct Fonts {
    /// The SMuFL music font.
    pub music: MusicFont,
    /// The text font's metrics (fret numbers, titles).
    pub text: TextMetrics,
}

impl Fonts {
    /// The bundled fonts: Bravura and Noto Sans.
    pub fn bundled() -> Result<Self, LayoutError> {
        Ok(Fonts {
            music: MusicFont::bravura()?,
            text: TextMetrics::NOTO_SANS,
        })
    }
}

/// A laid-out page.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// Page width in staff spaces.
    pub width: f32,
    /// Height in staff spaces: of the endless page, or of all pages with the gaps between.
    pub height: f32,
    /// The pages, from top to bottom (one for the endless page).
    pub pages: Vec<Rect>,
    /// What to draw, in painting order.
    pub ops: Vec<DrawOp>,
    /// Where the systems, measures and beats of the primary track are (for the cursor and
    /// clicks).
    pub places: Places,
    /// The same for every track shown, top to bottom (one entry for a single track).
    pub tracks: Vec<TrackPlaces>,
}

/// Where one track's systems, measures and beats are.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackPlaces {
    /// The track.
    pub track: TrackIndex,
    /// Its places.
    pub places: Places,
}

impl Layout {
    /// The places of `track`, if it is shown.
    pub fn places_of(&self, track: TrackIndex) -> Option<&Places> {
        self.tracks
            .iter()
            .find(|t| t.track == track)
            .map(|t| &t.places)
    }

    /// The track and the beat and string under `point`: on the staves the point is on, or
    /// else the nearest track's whose system it is near.
    pub fn hit(&self, point: Point) -> Option<(TrackIndex, Hit)> {
        self.hit_where(point, |places| places.hit(point))
    }

    /// Like [`Layout::hit`], but within `track` only and among beats of `voice` (a drag
    /// that selects beats).
    pub fn hit_in_voice(&self, point: Point, track: TrackIndex, voice: VoiceIndex) -> Option<Hit> {
        self.places_of(track)?.hit_in_voice(point, voice)
    }

    fn hit_where(
        &self,
        point: Point,
        hit: impl Fn(&Places) -> Option<Hit>,
    ) -> Option<(TrackIndex, Hit)> {
        self.tracks
            .iter()
            .filter_map(|t| Some((t.track, t.places.distance(point)?, hit(&t.places)?)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(track, _, hit)| (track, hit))
    }
}

/// What every layout step needs: settings, fonts and the staff's geometry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Context<'a> {
    pub(crate) options: &'a LayoutOptions,
    pub(crate) fonts: &'a Fonts,
    /// The tab staff.
    pub(crate) staff: Staff,
    /// Standard notation above the tab, if shown for this track.
    pub(crate) notation: Option<Notation<'a>>,
}

/// Lays out `track` of `song` on its own (its notation and tab staves, as the track shows
/// them).
pub fn layout_track(
    song: &Song,
    track: TrackIndex,
    options: &LayoutOptions,
    fonts: &Fonts,
) -> Result<Layout, LayoutError> {
    layout_tracks(song, &[track], track, options, fonts)
}

/// Lays out several tracks of `song` together: each system stacks the staves of every
/// track in `tracks` (top to bottom), with their beats lined up. [`Layout::places`] are
/// `primary`'s (the first track's if it is not among them); [`Layout::tracks`] has every
/// track's.
pub fn layout_tracks(
    song: &Song,
    tracks: &[TrackIndex],
    primary: TrackIndex,
    options: &LayoutOptions,
    fonts: &Fonts,
) -> Result<Layout, LayoutError> {
    score::layout_with(
        song,
        tracks,
        primary,
        options,
        fonts,
        &mut |_, measure, show, key, prev, ctx| {
            MeasureLayout::new(measure, show, key, prev, ctx).map(Arc::new)
        },
        &mut |_, column| measure::align(column, &options.spacing),
    )
}

/// Lines up measure `index` (its layout in each shown track, top to bottom) across the
/// tracks: [`measure::align`], or a cache of it.
type AlignSource<'s> = dyn FnMut(usize, &[Arc<MeasureLayout>]) -> Vec<Arc<MeasureLayout>> + 's;

/// Lays out one measure: its track, the measure, whether it shows its time signature, its
/// key, the key before it, and the context.
type MeasureSource<'s> = dyn FnMut(
        TrackIndex,
        MeasureView<'_>,
        bool,
        i8,
        Option<i8>,
        &Context<'_>,
    ) -> Result<Arc<MeasureLayout>, LayoutError>
    + 's;
