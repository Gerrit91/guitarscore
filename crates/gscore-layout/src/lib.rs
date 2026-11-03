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
//! [`svg::to_svg`] renders the result for golden tests and the command line, and
//! [`pdf::to_pdf`] a layout on pages as a PDF with embedded fonts; the GUI draws the same
//! operations. One track is laid out at a time (standard notation, tab, or both;
//! a percussion staff for drums), on one endless page or on separate pages
//! ([`LayoutOptions::page_height`]): pages are stacked from top to bottom with a gap
//! between them, and a system that does not fit moves to the next page.

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
mod pdf;
mod place;
mod staff;
mod svg;
mod system;
mod theme;

use std::sync::Arc;

use gscore_model::{MeasureView, Song, SystemBreak, TrackIndex};

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
pub use pdf::{PdfFonts, to_pdf};
pub use place::{BeatPlace, Hit, MeasurePlace, Places, SystemPlace};
pub use svg::to_svg;
pub use theme::{Rgb, Theme};

use measure::{MeasureLayout, Tops};
use notation::Notation;
use staff::Staff;
use system::{Breaks, MeasureWidth, break_systems};

/// Space between the header and the first system's top line (room for measure numbers).
const HEADER_GAP: f32 = 4.0;
/// How far a repeat count ("x3") sits above the staff (to its baseline), like measure
/// numbers.
const MEASURE_NUMBER_RISE: f32 = 0.8;
/// Gap between a repeat count and the end repeat's barline.
const REPEAT_COUNT_GAP: f32 = 0.6;
/// Gap between two pages, in staff spaces.
pub const PAGE_GAP: f32 = 6.0;
/// Room kept free at the bottom of a page (above the bottom margin), so the last system
/// does not touch the footer.
const FOOTER_ROOM: f32 = 3.0;
/// Height of one copyright line at the bottom of the first page.
const FOOTER_LINE: f32 = 1.4;
/// Room for the running header below the top margin of pages after the first.
const RUNNING_HEADER_ROOM: f32 = 2.5;
/// The most space added between two systems when a page's systems are spread down to its
/// bottom.
const MAX_SPREAD: f32 = 6.0;
/// Room below a system's lowest staff line for what hangs under it (P.M. and let ring rows,
/// dynamics), counted when deciding whether a system still fits on a page.
const BELOW_SYSTEM: f32 = 3.0;

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
    layout_with(
        song,
        track,
        options,
        fonts,
        &mut |measure, show, key, prev, ctx| {
            MeasureLayout::new(measure, show, key, prev, ctx).map(Arc::new)
        },
    )
}

/// Lays out one measure: the measure, whether it shows its time signature, its key, the
/// key before it, and the context.
type MeasureSource<'s> = dyn FnMut(
        MeasureView<'_>,
        bool,
        i8,
        Option<i8>,
        &Context<'_>,
    ) -> Result<Arc<MeasureLayout>, LayoutError>
    + 's;

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
    let mut keys: Vec<i8> = Vec::with_capacity(view.measures().len());
    let mut previous_signature = None;
    let mut previous_key = None;
    for measure in view.measures() {
        let signature = measure.time_signature()?;
        let show = previous_signature != Some(signature);
        let key = song.key_signature(measure.index()).map_or(0, |key| key.key);
        measures.push(measure_layout(measure, show, key, previous_key, &ctx)?);
        keys.push(key);
        previous_signature = Some(signature);
        previous_key = Some(key);
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
    let left = options.margin_left;
    let line_width = options.line_width();
    // Every measure's system start: the clef, and the key signature when the measure does
    // not draw one of its own (a key change at the measure start).
    let prefixes: Vec<f32> = measures
        .iter()
        .zip(&keys)
        .map(|(measure, &key)| {
            let drawn_by_measure = measure.has_key_signature();
            let notation_prefix = ctx.notation.map_or(0.0, |notation| {
                if drawn_by_measure {
                    notation::key_start_width(notation.clef, &ctx)
                } else {
                    notation::prefix_width(notation.clef, key, false, &ctx)
                }
            });
            let tab_prefix = if show_tab {
                if ctx.notation.is_some() {
                    // With a notation staff, the key signature is written there only.
                    ctx.staff.prefix_width(&fonts.music)
                } else {
                    notation::tab_prefix_width(key, drawn_by_measure, &ctx)
                }
            } else {
                0.0
            };
            notation_prefix.max(tab_prefix)
        })
        .collect();
    let notation_staff = notation::staff();
    // With notation, the tab sits below the notation staff and the gap between them.
    let tab_offset = if ctx.notation.is_some() {
        notation_staff.height() + options.notation_gap
    } else {
        0.0
    };
    let system_breaks: Vec<SystemBreak> = view.measures().map(|m| m.system_break()).collect();
    let forced: Vec<bool> = system_breaks
        .iter()
        .map(|kind| *kind != SystemBreak::None)
        .collect();
    let plans = break_systems(
        &widths,
        line_width,
        &prefixes,
        Breaks {
            forced: &forced,
            max_measures: view.bars_per_system().map(usize::from),
        },
    );
    // From a system's top line (notation or tab) to its lowest line.
    let staves_height = if show_tab {
        tab_offset + ctx.staff.height()
    } else {
        notation_staff.height()
    };
    let rows: Vec<marks::Rows> = plans
        .iter()
        .map(|plan| marks::Rows::of(marks.get(plan.measures.clone()).unwrap_or_default()))
        .collect();
    let heights: Vec<f32> = rows
        .iter()
        .map(|rows| rows.height() + staves_height)
        .collect();
    let page_starts: Vec<bool> = plans
        .iter()
        .map(|plan| system_breaks.get(plan.measures.start) == Some(&SystemBreak::Page))
        .collect();
    let decor = PageDecor::new(song, options);
    let placement = place_systems(
        &heights,
        &page_starts,
        header_end + HEADER_GAP,
        options,
        &decor,
    );

    let font = &fonts.music;
    let mut places = Places::default();
    let mut bottom = header_end;
    // Where each measure was drawn, for the ties that cross barlines.
    let mut measure_x = vec![0.0_f32; measures.len()];
    let mut measure_stretch = vec![1.0_f32; measures.len()];
    let mut measure_tops: Vec<Option<Tops>> = vec![None; measures.len()];
    for (index, plan) in plans.iter().enumerate() {
        let line: &[Arc<MeasureLayout>] = measures.get(plan.measures.clone()).unwrap_or_default();
        let line_marks: &[marks::MeasureMarks] =
            marks.get(plan.measures.clone()).unwrap_or_default();
        let rows = rows.get(index).copied().unwrap_or_default();
        // Section names, tempo marks and endings need room above the system.
        let top = placement.tops.get(index).copied().unwrap_or(bottom) + rows.height();
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
        let mut x = left
            + prefixes
                .get(plan.measures.start)
                .copied()
                .unwrap_or_default();
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
            if let Some(slot) = measure_x.get_mut(measure.index().get()) {
                *slot = content_x;
            }
            if let Some(slot) = measure_stretch.get_mut(measure.index().get()) {
                *slot = plan.stretch;
            }
            if let Some(slot) = measure_tops.get_mut(measure.index().get()) {
                *slot = Some(tops);
            }
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
                } else if is_final {
                    staff.draw_barline(x, staff_top, true, font, &mut content);
                } else if measure_marks.double_bar {
                    staff.draw_double_barline(x, staff_top, font, &mut content);
                } else {
                    staff.draw_barline(x, staff_top, false, font, &mut content);
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
        let first = plan.measures.start;
        let key = keys.get(first).copied().unwrap_or_default();
        let key_drawn_by_measure = measures
            .get(first)
            .is_some_and(|measure| measure.has_key_signature());
        if show_tab {
            ctx.staff.draw_lines(tab_top, left, x, font, &mut ops);
            notation::draw_tab_staff_start(
                left,
                tab_top,
                key,
                key_drawn_by_measure,
                ctx.notation.is_none(),
                &ctx,
                &mut ops,
            );
        }
        if let (Some(notation), Some(notation_top)) = (ctx.notation, tops.notation) {
            notation_staff.draw_lines(notation_top, left, x, font, &mut ops);
            notation::draw_staff_start(
                left,
                notation_top,
                notation.clef,
                key,
                key_drawn_by_measure,
                &ctx,
                &mut ops,
            );
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
    }

    // Ties that continue a note across a barline, after the measures are placed so both
    // noteheads are known.
    for index in 0..measures.len().saturating_sub(1) {
        let (Some(origin), Some(target)) =
            (measures.get(index), measures.get(index.saturating_add(1)))
        else {
            continue;
        };
        let (Some(&origin_x), Some(&target_x)) =
            (measure_x.get(index), measure_x.get(index.saturating_add(1)))
        else {
            continue;
        };
        let (Some(Some(origin_tops)), Some(Some(target_tops))) = (
            measure_tops.get(index).copied(),
            measure_tops.get(index.saturating_add(1)).copied(),
        ) else {
            continue;
        };
        let same_system = places.measures.get(index).map(|place| place.system)
            == places
                .measures
                .get(index.saturating_add(1))
                .map(|place| place.system);
        measure::draw_barline_ties(
            origin,
            origin_x,
            measure_stretch.get(index).copied().unwrap_or(1.0),
            origin_tops,
            target,
            target_x,
            measure_stretch
                .get(index.saturating_add(1))
                .copied()
                .unwrap_or(1.0),
            target_tops,
            same_system,
            &ctx,
            &mut ops,
        );
    }

    let (pages, height) = if placement.pages.is_empty() {
        let height = bottom + options.margin_bottom;
        (
            vec![Rect {
                origin: Point::new(0.0, 0.0),
                width: options.page_width,
                height,
            }],
            height,
        )
    } else {
        decor.draw(&placement.pages, options, &mut ops);
        let height = placement
            .pages
            .last()
            .map_or(0.0, |page| page.origin.y + page.height);
        (placement.pages, height)
    };

    Ok(Layout {
        width: options.page_width,
        height,
        pages,
        ops,
        places,
    })
}

/// Where the systems go: the top of each system's block (the rows above its staves come
/// first), and the pages (none for the endless page).
struct Placement {
    tops: Vec<f32>,
    pages: Vec<Rect>,
}

/// Puts systems of `heights` (rows above plus staves) one below the other from `first`.
/// With pages, a system that does not fit (or that starts with a page break) goes to the
/// next page, and the systems of a page that ended full are spread down to its bottom.
fn place_systems(
    heights: &[f32],
    page_starts: &[bool],
    first: f32,
    options: &LayoutOptions,
    decor: &PageDecor,
) -> Placement {
    let step = |height: f32| height + options.system_gap + HEADER_GAP;
    let page_height = options.page_height.filter(|height| {
        height.is_finite() && *height > options.margin_top + options.margin_bottom
    });
    let Some(page_height) = page_height else {
        let mut tops = Vec::with_capacity(heights.len());
        let mut cursor = first;
        for height in heights {
            tops.push(cursor);
            cursor += step(*height);
        }
        return Placement {
            tops,
            pages: Vec::new(),
        };
    };

    let mut tops = Vec::with_capacity(heights.len());
    let mut pages: Vec<Rect> = Vec::new();
    // Per page: its systems, and whether it ended because the next one did not fit.
    let mut page_systems: Vec<(std::ops::Range<usize>, bool)> = Vec::new();
    let mut page_top = 0.0_f32;
    let mut page_first = 0_usize;
    let mut cursor = first;
    let limit = |page: usize, page_top: f32| {
        page_top + page_height - options.margin_bottom - decor.footer_room(page)
    };
    for (index, height) in heights.iter().enumerate() {
        let page = pages.len();
        if index > page_first {
            let forced = page_starts.get(index).copied().unwrap_or(false);
            let overflow = cursor + height + BELOW_SYSTEM > limit(page, page_top);
            if forced || overflow {
                pages.push(Rect {
                    origin: Point::new(0.0, page_top),
                    width: options.page_width,
                    height: page_height,
                });
                page_systems.push((page_first..index, overflow && !forced));
                page_top += page_height + PAGE_GAP;
                page_first = index;
                cursor = page_top
                    + options.margin_top
                    + decor.header_room(page.saturating_add(1))
                    + HEADER_GAP;
            }
        }
        tops.push(cursor);
        cursor += step(*height);
    }
    pages.push(Rect {
        origin: Point::new(0.0, page_top),
        width: options.page_width,
        height: page_height,
    });
    page_systems.push((page_first..heights.len(), false));

    // Spread the systems of every full page down to its bottom, but never more than
    // MAX_SPREAD between two systems (a page of two systems stays readable).
    for (page, (systems, full)) in page_systems.iter().enumerate() {
        let count = systems.len();
        if !*full || count < 2 {
            continue;
        }
        let last = systems.end.saturating_sub(1);
        let (Some(top), Some(height), Some(rect)) =
            (tops.get(last), heights.get(last), pages.get(page))
        else {
            continue;
        };
        let free = limit(page, rect.origin.y) - (top + height + BELOW_SYSTEM);
        if !(free.is_finite() && free > 0.0) {
            continue;
        }
        let extra = (free / count.saturating_sub(1) as f32).min(MAX_SPREAD);
        for (offset, index) in systems.clone().enumerate() {
            if let Some(top) = tops.get_mut(index) {
                *top += extra * offset as f32;
            }
        }
    }
    Placement { tops, pages }
}

/// What the pages carry besides the music: page numbers, the copyright on the first page,
/// and a running header on the others.
struct PageDecor {
    page_numbers: bool,
    copyright: Vec<String>,
    header: Option<String>,
}

impl PageDecor {
    fn new(song: &Song, options: &LayoutOptions) -> Self {
        let copyright = if options.copyright {
            song.copyright()
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect()
        } else {
            Vec::new()
        };
        let header = options
            .running_header
            .then(|| match (song.title().trim(), song.artist().trim()) {
                ("", _) => None,
                (title, "") => Some(title.to_owned()),
                (title, artist) => Some(format!("{title} by {artist}")),
            })
            .flatten();
        PageDecor {
            page_numbers: options.page_numbers,
            copyright,
            header,
        }
    }

    /// Room kept above page `page`'s bottom margin.
    fn footer_room(&self, page: usize) -> f32 {
        let copyright = if page == 0 {
            self.copyright.len() as f32 * FOOTER_LINE
        } else {
            0.0
        };
        FOOTER_ROOM + copyright
    }

    /// Room kept below page `page`'s top margin (the first page has the title block).
    fn header_room(&self, page: usize) -> f32 {
        if page > 0 && self.header.is_some() {
            RUNNING_HEADER_ROOM
        } else {
            0.0
        }
    }

    /// Page numbers at the bottom right ("Page 2/5", as Guitar Pro), the copyright at the
    /// bottom of the first page, the running header atop the others.
    fn draw(&self, pages: &[Rect], options: &LayoutOptions, ops: &mut Vec<DrawOp>) {
        let count = pages.len();
        let text = |text: String, at: Point, anchor: TextAnchor| DrawOp::Text {
            text,
            at,
            size: options.measure_number_size,
            font: TextFont::Regular,
            anchor,
            color: ColorToken::Annotation,
        };
        let center = (options.margin_left + options.page_width - options.margin_right) / 2.0;
        for (index, page) in pages.iter().enumerate() {
            let bottom = page.origin.y + page.height;
            if self.page_numbers {
                ops.push(text(
                    format!("Page {}/{count}", index.saturating_add(1)),
                    Point::new(
                        page.origin.x + page.width - options.margin_right,
                        bottom - options.margin_bottom / 2.0,
                    ),
                    TextAnchor::End,
                ));
            }
            if index == 0 {
                let lines = self.copyright.len();
                for (line, copyright) in self.copyright.iter().enumerate() {
                    let up = lines.saturating_sub(line).saturating_sub(1) as f32;
                    ops.push(text(
                        copyright.clone(),
                        Point::new(center, bottom - options.margin_bottom - up * FOOTER_LINE),
                        TextAnchor::Middle,
                    ));
                }
            } else if let Some(header) = &self.header {
                ops.push(text(
                    header.clone(),
                    Point::new(center, page.origin.y + options.margin_top + 1.0),
                    TextAnchor::Middle,
                ));
            }
        }
    }
}
