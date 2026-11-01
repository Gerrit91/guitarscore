//! Headless layout: turns the model plus settings into positioned draw operations (lines,
//! glyphs by SMuFL name, text, rectangles) with semantic color tokens. No GUI imports.
//!
//! The pipeline for one track ([`layout_track`]):
//! 1. [`header`]: title block, credits, tuning and tempo.
//! 2. [`measure`]: every measure on its own, as columns of beats with natural spacing.
//!    This is the unit to cache once editing needs incremental relayout.
//! 3. [`system`]: measures broken into lines and justified to the page width.
//! 4. Drawing: staff lines, clef, barlines and the measures' content, as [`DrawOp`]s.
//!
//! [`svg::to_svg`] renders the result for golden tests and the command line; the GUI draws
//! the same operations. Only the tab staff of one track is laid out so far, on one
//! continuous page (no page breaks yet).

#![warn(clippy::arithmetic_side_effects)]

mod draw;
mod error;
mod glyph;
mod header;
mod measure;
mod metrics;
mod options;
mod ornaments;
mod staff;
mod svg;
mod system;
mod theme;

use gscore_model::{Song, TrackIndex};

// The golden tests open the owner's files with `gscore-io`; this marks the test-only
// dependency for the library test target's unused-dependency check.
#[cfg(test)]
use gscore_io as _;

pub use draw::{ColorToken, DrawOp, Point, Rect, TextAnchor, TextFont};
pub use error::LayoutError;
pub use glyph::{Digit, Glyph};
pub use metrics::{EngravingDefaults, GlyphBox, MusicFont, TextMetrics};
pub use options::{LayoutOptions, Spacing};
pub use svg::to_svg;
pub use theme::{Rgb, Theme};

use measure::MeasureLayout;
use staff::Staff;
use system::{MeasureWidth, break_systems};

/// Space between the header and the first system's top line (room for measure numbers).
const HEADER_GAP: f32 = 4.0;

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
    /// Page height in staff spaces.
    pub height: f32,
    /// What to draw, in painting order.
    pub ops: Vec<DrawOp>,
}

/// What every layout step needs: settings, fonts and the staff's geometry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Context<'a> {
    pub(crate) options: &'a LayoutOptions,
    pub(crate) fonts: &'a Fonts,
    pub(crate) staff: Staff,
}

/// Lays out the tab staff of `track` in `song` as one continuous page.
pub fn layout_track(
    song: &Song,
    track: TrackIndex,
    options: &LayoutOptions,
    fonts: &Fonts,
) -> Result<Layout, LayoutError> {
    let view = song.track(track).ok_or(LayoutError::NoSuchTrack(track))?;
    if view.is_percussion() {
        return Err(LayoutError::Unsupported(format!(
            "{track} is a drum track; drum notation is not implemented"
        )));
    }
    let ctx = Context {
        options,
        fonts,
        staff: Staff::new(view.string_count(), options.tab_line_spacing),
    };

    let mut ops = Vec::new();
    let header_end = header::draw(song, view, &ctx, &mut ops)?;

    let mut measures = Vec::with_capacity(view.measures().len());
    let mut previous_signature = None;
    for measure in view.measures() {
        let signature = measure.time_signature()?;
        let show = previous_signature != Some(signature);
        measures.push(MeasureLayout::new(measure, show, &ctx)?);
        previous_signature = Some(signature);
    }

    let widths: Vec<MeasureWidth> = measures
        .iter()
        .map(|m| MeasureWidth {
            fixed: m.fixed_width(),
            stretchable: m.stretchable_width(),
        })
        .collect();
    let left = options.margin_x;
    let line_width = options.page_width - 2.0 * options.margin_x;
    let prefix = ctx.staff.prefix_width(&fonts.music);
    let plans = break_systems(&widths, line_width, prefix);

    let font = &fonts.music;
    let mut top = header_end + HEADER_GAP;
    let mut bottom = header_end;
    for plan in &plans {
        let line: &[MeasureLayout] = measures.get(plan.measures.clone()).unwrap_or_default();
        let mut x = left + prefix;
        let mut content = Vec::new();
        for (offset, measure) in line.iter().enumerate() {
            measure.draw(x, top, plan.stretch, &ctx, &mut content);
            x += measure.width(plan.stretch);
            let is_final =
                plan.measures.start.saturating_add(offset).saturating_add(1) == measures.len();
            ctx.staff.draw_barline(x, top, is_final, font, &mut content);
        }
        // Staff lines first, so masks and fret numbers paint over them.
        ctx.staff.draw_lines(top, left, x, font, &mut ops);
        ctx.staff.draw_start_line(left, top, font, &mut ops);
        ctx.staff.draw_clef(left, top, font, &mut ops);
        ops.append(&mut content);
        bottom = top + ctx.staff.height();
        top = bottom + options.system_gap + HEADER_GAP;
    }

    Ok(Layout {
        width: options.page_width,
        height: bottom + options.margin_y,
        ops,
    })
}
