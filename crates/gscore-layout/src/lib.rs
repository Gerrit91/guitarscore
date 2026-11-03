//! Headless layout: turns the model plus settings into positioned draw operations (lines,
//! glyphs by SMuFL name, text, rectangles) with semantic color tokens. No GUI imports.
//!
//! The pipeline for one track ([`layout_track`]):
//! 1. [`header`]: title block, credits, tuning and tempo.
//! 2. [`measure`]: every measure on its own, as columns of beats with natural spacing.
//!    This is the unit [`LayoutCache`] keeps between edits.
//! 3. [`marks`]: section names, tempo changes, repeats, endings and dynamics, decided in
//!    order over the track (a tempo or dynamic is written where it changes).
//! 4. [`system`]: measures broken into lines and justified to the page width.
//! 5. Drawing: staff lines, clef, barlines and the measures' content, as [`DrawOp`]s.
//!
//! [`svg::to_svg`] renders the result for golden tests and the command line; the GUI draws
//! the same operations. One track is laid out at a time (standard notation, tab, or both;
//! a percussion staff for drums), on one continuous page (no page breaks yet).

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
mod place;
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
pub use metrics::{EngravingDefaults, GlyphBox, MusicFont, TextMetrics};
pub use options::{LayoutOptions, Spacing};
pub use place::{BeatPlace, Hit, MeasurePlace, Places, SystemPlace};
pub use svg::to_svg;
pub use theme::{Rgb, Theme};

use measure::{MeasureLayout, Tops};
use notation::Notation;
use staff::Staff;
use system::{MeasureWidth, break_systems};

/// Space between the header and the first system's top line (room for measure numbers).
const HEADER_GAP: f32 = 4.0;
/// How far a repeat count ("x3") sits above the staff (to its baseline), like measure
/// numbers.
const MEASURE_NUMBER_RISE: f32 = 0.8;
/// Gap between a repeat count and the end repeat's barline.
const REPEAT_COUNT_GAP: f32 = 0.6;

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
    /// Where the systems, measures and beats are (for the cursor and clicks).
    pub places: Places,
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

/// Lays out the tab staff of `track` in `song` as one continuous page.
pub fn layout_track(
    song: &Song,
    track: TrackIndex,
    options: &LayoutOptions,
    fonts: &Fonts,
) -> Result<Layout, LayoutError> {
    layout_with(song, track, options, fonts, &mut |measure, show, ctx| {
        MeasureLayout::new(measure, show, ctx).map(Arc::new)
    })
}

/// Lays out one measure: the measure, whether it shows its time signature, the context.
type MeasureSource<'s> =
    dyn FnMut(MeasureView<'_>, bool, &Context<'_>) -> Result<Arc<MeasureLayout>, LayoutError> + 's;

/// [`layout_track`] with the measures' layouts coming from `measure_layout` (a cache, or a
/// fresh layout).
fn layout_with(
    song: &Song,
    track: TrackIndex,
    options: &LayoutOptions,
    fonts: &Fonts,
    measure_layout: &mut MeasureSource<'_>,
) -> Result<Layout, LayoutError> {
    let view = song.track(track).ok_or(LayoutError::NoSuchTrack(track))?;
    let staves = view.staves();
    // Drums are always written on a percussion staff: their "frets" are drum sounds, which
    // a tab staff cannot show.
    let drums = view.is_percussion();
    let notation = if drums || (options.notation && staves.notation()) {
        Notation::for_track(view)?
    } else {
        None
    };
    // A track without notation (none possible, or switched off) always shows its tab.
    let show_tab = !drums && (staves.tablature() || notation.is_none());
    let ctx = Context {
        options,
        fonts,
        staff: Staff::new(view.string_count(), options.tab_line_spacing),
        notation,
    };

    let mut ops = Vec::new();
    let header_end = header::draw(song, view, &ctx, &mut ops)?;

    let mut measures = Vec::with_capacity(view.measures().len());
    let mut previous_signature = None;
    for measure in view.measures() {
        let signature = measure.time_signature()?;
        let show = previous_signature != Some(signature);
        measures.push(measure_layout(measure, show, &ctx)?);
        previous_signature = Some(signature);
    }

    let marks = marks::plan(song, &measures);
    let extras: Vec<(f32, f32)> = marks.iter().map(|m| m.extra_width(&ctx)).collect();
    let widths: Vec<MeasureWidth> = measures
        .iter()
        .zip(&extras)
        .map(|(m, (before, after))| MeasureWidth {
            fixed: m.fixed_width() + before + after,
            stretchable: m.stretchable_width(),
        })
        .collect();
    let left = options.margin_x;
    let line_width = options.page_width - 2.0 * options.margin_x;
    let prefix = ctx
        .notation
        .map_or(0.0, |n| notation::prefix_width(n.clef, &ctx))
        .max(ctx.staff.prefix_width(&fonts.music));
    let notation_staff = notation::staff();
    // With notation, the tab sits below the notation staff and the gap between them.
    let tab_offset = if ctx.notation.is_some() {
        notation_staff.height() + options.notation_gap
    } else {
        0.0
    };
    let plans = break_systems(&widths, line_width, prefix);

    let font = &fonts.music;
    let mut places = Places::default();
    let mut top = header_end + HEADER_GAP;
    let mut bottom = header_end;
    for plan in &plans {
        let line: &[Arc<MeasureLayout>] = measures.get(plan.measures.clone()).unwrap_or_default();
        let line_marks: &[marks::MeasureMarks] =
            marks.get(plan.measures.clone()).unwrap_or_default();
        let rows = marks::Rows::of(line_marks);
        // Section names, tempo marks and endings need room above the system.
        top += rows.height();
        let tops = Tops {
            tab: top + tab_offset,
            notation: ctx.notation.map(|_| top),
            show_tab,
        };
        let tab_top = tops.tab;
        let system = places.systems.len();
        places.systems.push(if show_tab {
            SystemPlace {
                top,
                tab_top,
                string_spacing: ctx.options.tab_line_spacing,
                strings: view.string_count(),
            }
        } else {
            // Notation only: the cursor's string rows are spread over the notation staff.
            let strings = view.string_count();
            let gaps = strings.saturating_sub(1).max(1) as f32;
            SystemPlace {
                top,
                tab_top: top,
                string_spacing: notation_staff.height() / gaps,
                strings,
            }
        });
        let mut x = left + prefix;
        let mut content = Vec::new();
        let default_marks = marks::MeasureMarks::default();
        for (offset, measure) in line.iter().enumerate() {
            let measure_marks = line_marks.get(offset).unwrap_or(&default_marks);
            let (before, after) = measure_marks.extra_width(&ctx);
            let left = x;
            let content_x = x + before;
            let mut place = measure.place(content_x, plan.stretch, system);
            place.left = left;
            places.measures.push(place);
            measure.draw(content_x, tops, plan.stretch, &ctx, &mut content);
            x = content_x + measure.width(plan.stretch) + after;
            marks::draw(
                marks::Placed {
                    layout: measure,
                    marks: measure_marks,
                    left,
                    right: x,
                    content: content_x,
                    stretch: plan.stretch,
                },
                rows,
                tops,
                &ctx,
                &mut content,
            );
            let is_final =
                plan.measures.start.saturating_add(offset).saturating_add(1) == measures.len();
            let end_repeat = measure_marks.repeat.end_repeats.is_some();
            let mut staves: Vec<(&Staff, f32)> = Vec::with_capacity(2);
            if show_tab {
                staves.push((&ctx.staff, tab_top));
            }
            if let Some(notation_top) = tops.notation {
                staves.push((&notation_staff, notation_top));
            }
            for (staff, staff_top) in staves {
                if measure_marks.repeat.start {
                    staff.draw_repeat(left, staff_top, true, font, &mut content);
                }
                if end_repeat {
                    staff.draw_repeat(x, staff_top, false, font, &mut content);
                } else {
                    staff.draw_barline(x, staff_top, is_final, font, &mut content);
                }
            }
            if let Some(count) = marks::repeat_count(measure_marks) {
                content.push(DrawOp::Text {
                    text: count,
                    // Left of the barline, clear of the next measure's number.
                    at: Point::new(
                        x - REPEAT_COUNT_GAP,
                        tops.notation.unwrap_or(tab_top) - MEASURE_NUMBER_RISE,
                    ),
                    size: options.measure_number_size,
                    font: TextFont::Regular,
                    anchor: TextAnchor::End,
                    color: ColorToken::Annotation,
                });
            }
        }
        // Staff lines first, so masks and fret numbers paint over them.
        if show_tab {
            ctx.staff.draw_lines(tab_top, left, x, font, &mut ops);
            ctx.staff.draw_clef(left, tab_top, font, &mut ops);
        }
        if let (Some(notation), Some(notation_top)) = (ctx.notation, tops.notation) {
            notation_staff.draw_lines(notation_top, left, x, font, &mut ops);
            notation::draw_staff_start(left, notation_top, notation.clef, &mut ops);
            // One line joins the staves of a system at its start.
            let thin = font.engraving().thin_barline_thickness;
            let end = if show_tab {
                tab_top + ctx.staff.height()
            } else {
                notation_top + notation_staff.height()
            };
            ops.push(DrawOp::Line {
                from: Point::new(left + thin / 2.0, notation_top),
                to: Point::new(left + thin / 2.0, end),
                thickness: thin,
                color: ColorToken::Barline,
            });
        } else {
            ctx.staff.draw_start_line(left, tab_top, font, &mut ops);
        }
        ops.append(&mut content);
        bottom = match tops.notation {
            Some(notation_top) if !show_tab => notation_top + notation_staff.height(),
            _ => tab_top + ctx.staff.height(),
        };
        top = bottom + options.system_gap + HEADER_GAP;
    }

    Ok(Layout {
        width: options.page_width,
        height: bottom + options.margin_y,
        ops,
        places,
    })
}
