//! A score of one or more tracks: every shown track's staves stacked in each system, with
//! the measures of all tracks on one time grid ([`crate::measure::align`]), broken into
//! systems together and put onto pages.
//!
//! The top track of a system carries what belongs to the whole system: measure numbers,
//! section names, tempo marks and alternative endings. Every track keeps its own staves,
//! barlines, repeat signs, dynamics, fermatas and beat effects. With several tracks, the
//! tracks' short names stand in the left margin before each system, one line joins all
//! their staves, and the song's multitrack line breaks apply instead of a track's own.

use std::sync::Arc;

use gscore_model::{Song, SystemBreak, TrackIndex, TrackView};

use crate::draw::{ColorToken, DrawOp, Point, Rect, TextAnchor, TextFont};
use crate::marks::{self, MeasureMarks, Rows};
use crate::measure::{self, MeasureLayout, Tops};
use crate::notation::{self, Notation};
use crate::ornaments;
use crate::pages::{PageDecor, place_systems};
use crate::place::{Places, SystemPlace};
use crate::staff::Staff;
use crate::system::{Breaks, MeasureWidth, break_systems};
use crate::{
    Context, Fonts, HEADER_GAP, Layout, LayoutError, LayoutOptions, MEASURE_NUMBER_RISE,
    MeasureSource, REPEAT_COUNT_GAP, TrackPlaces, header,
};

/// Space between the lowest staff of one track and the rows above the next track's staves
/// in a system (room for what hangs below a staff: P.M. and let ring rows, dynamics).
const TRACK_GAP: f32 = 5.0;
/// Gap between a track's name and the start of its staves.
const NAME_GAP: f32 = 1.2;
/// Font size of the track names before a system.
const NAME_SIZE: f32 = 1.2;
/// Room kept between a track's name and the page edge.
const NAME_MARGIN: f32 = 0.5;

/// One track of the score, set up for layout.
struct Part<'a> {
    track: TrackIndex,
    view: TrackView<'a>,
    ctx: Context<'a>,
    show_tab: bool,
    measures: Vec<Arc<MeasureLayout>>,
    marks: Vec<MeasureMarks>,
    /// From the top staff's top line to the tab's (0 without notation).
    tab_offset: f32,
    /// From the top staff's top line to the lowest staff's bottom line.
    staves_height: f32,
}

impl<'a> Part<'a> {
    fn new(
        song: &'a Song,
        track: TrackIndex,
        options: &'a LayoutOptions,
        fonts: &'a Fonts,
        measure_layout: &mut MeasureSource<'_>,
    ) -> Result<Self, LayoutError> {
        let view = song.track(track).ok_or(LayoutError::NoSuchTrack(track))?;
        let staves = view.staves();
        // Drums are always written on a percussion staff: their "frets" are drum sounds,
        // which a tab staff cannot show.
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
        let mut measures = Vec::with_capacity(view.measures().len());
        let mut previous_signature = None;
        let mut previous_key = None;
        for measure in view.measures() {
            let signature = measure.time_signature()?;
            let show = previous_signature != Some(signature);
            let key = song.key_signature(measure.index()).map_or(0, |key| key.key);
            measures.push(measure_layout(
                track,
                measure,
                show,
                key,
                previous_key,
                &ctx,
            )?);
            previous_signature = Some(signature);
            previous_key = Some(key);
        }
        let notation_staff = notation::staff();
        // With notation, the tab sits below the notation staff and the gap between them.
        let tab_offset = if ctx.notation.is_some() {
            notation_staff.height() + options.notation_gap
        } else {
            0.0
        };
        let staves_height = if show_tab {
            tab_offset + ctx.staff.height()
        } else {
            notation_staff.height()
        };
        Ok(Part {
            track,
            view,
            ctx,
            show_tab,
            measures,
            marks: Vec::new(),
            tab_offset,
            staves_height,
        })
    }

    /// The room a system starting with measure `index` needs before its first measure:
    /// the clef, and the key signature when the measure does not draw one of its own.
    fn prefix(&self, index: usize, key: i8) -> f32 {
        let ctx = &self.ctx;
        let drawn_by_measure = self
            .measures
            .get(index)
            .is_some_and(|measure| measure.has_key_signature());
        let notation_prefix = ctx.notation.map_or(0.0, |notation| {
            if drawn_by_measure {
                notation::key_start_width(notation.clef, ctx)
            } else {
                notation::prefix_width(notation.clef, key, false, ctx)
            }
        });
        let tab_prefix = if self.show_tab {
            if ctx.notation.is_some() {
                // With a notation staff, the key signature is written there only.
                ctx.staff.prefix_width(&ctx.fonts.music)
            } else {
                notation::tab_prefix_width(key, drawn_by_measure, ctx)
            }
        } else {
            0.0
        };
        notation_prefix.max(tab_prefix)
    }

    /// How far the notation of `measures` reaches above the top staff and below the
    /// notation staff (0 without notation).
    fn reach(&self, measures: std::ops::Range<usize>) -> (f32, f32) {
        if self.ctx.notation.is_none() {
            return (0.0, 0.0);
        }
        self.measures
            .get(measures)
            .unwrap_or_default()
            .iter()
            .map(|measure| measure.notation_reach())
            .fold((0.0_f32, 0.0_f32), |(a, b), (above, below)| {
                (a.max(above), b.max(below))
            })
    }

    /// The name written in the margin before each system: the short name (else the
    /// name), cut with "…" to fit `room`; empty when not even one letter fits.
    fn margin_name(&self, room: f32) -> String {
        let short = self.view.short_name().trim();
        let name = if short.is_empty() {
            self.view.name().trim()
        } else {
            short
        };
        let width = |text: &str| self.ctx.fonts.text.width(text, NAME_SIZE);
        if width(name) <= room {
            return name.to_owned();
        }
        let chars: Vec<char> = name.chars().collect();
        (1..chars.len())
            .rev()
            .map(|keep| {
                let mut cut: String = chars.iter().take(keep).collect();
                cut.push('…');
                cut
            })
            .find(|cut| width(cut) <= room)
            .unwrap_or_default()
    }
}

/// Lays out `tracks` of `song` (top to bottom) in shared systems; the result's
/// [`Layout::places`] are those of `primary` (the first track if it is not shown).
pub(crate) fn layout_with(
    song: &Song,
    tracks: &[TrackIndex],
    primary: TrackIndex,
    options: &LayoutOptions,
    fonts: &Fonts,
    measure_layout: &mut MeasureSource<'_>,
) -> Result<Layout, LayoutError> {
    let first = *tracks.first().ok_or(LayoutError::NoSuchTrack(primary))?;
    let mut parts = tracks
        .iter()
        .map(|&track| Part::new(song, track, options, fonts, measure_layout))
        .collect::<Result<Vec<_>, _>>()?;
    let count = parts
        .iter()
        .map(|part| part.measures.len())
        .min()
        .unwrap_or(0);
    let multi = parts.len() > 1;
    if multi {
        for index in 0..count {
            let column: Vec<Arc<MeasureLayout>> = parts
                .iter()
                .filter_map(|part| part.measures.get(index).cloned())
                .collect();
            for (part, aligned) in parts
                .iter_mut()
                .zip(measure::align(&column, &options.spacing))
            {
                if let Some(slot) = part.measures.get_mut(index) {
                    *slot = aligned;
                }
            }
        }
    }
    for part in &mut parts {
        part.measures.truncate(count);
        part.marks = marks::plan(song, &part.measures);
    }
    let Some(top) = parts.first() else {
        return Err(LayoutError::NoSuchTrack(first));
    };

    let mut ops = Vec::new();
    let views: Vec<TrackView<'_>> = parts.iter().map(|part| part.view).collect();
    let header_end = header::draw(song, &views, &top.ctx, &mut ops)?;

    let keys: Vec<i8> = (0..count)
        .map(|index| {
            song.key_signature(gscore_model::MeasureIndex::new(index))
                .map_or(0, |key| key.key)
        })
        .collect();
    // After aligning, a measure is equally wide in every track; repeat signs are the
    // song's, so the top track's marks size them for all.
    let widths: Vec<MeasureWidth> = top
        .measures
        .iter()
        .zip(&top.marks)
        .map(|(m, marks)| {
            let (before, after) = marks.extra_width(&top.ctx);
            MeasureWidth {
                fixed: m.fixed_width() + before + after,
                stretchable: m.stretchable_width(),
            }
        })
        .collect();
    let prefixes: Vec<f32> = (0..count)
        .map(|index| {
            let key = keys.get(index).copied().unwrap_or_default();
            parts
                .iter()
                .map(|part| part.prefix(index, key))
                .fold(0.0, f32::max)
        })
        .collect();
    // The tracks' short names stand in the left margin before every system, cut to fit
    // it, as Guitar Pro writes them (the header lists the full names).
    let names: Vec<String> = if multi {
        let room = options.margin_left - NAME_GAP - NAME_MARGIN;
        parts.iter().map(|part| part.margin_name(room)).collect()
    } else {
        Vec::new()
    };
    // One track breaks where it does on its own; several where the song's multitrack
    // layout says (page breaks are the top track's in both).
    let score_breaks = if multi {
        song.score_line_breaks()
    } else {
        Vec::new()
    };
    let system_breaks: Vec<SystemBreak> = top
        .view
        .measures()
        .take(count)
        .map(|m| match m.system_break() {
            SystemBreak::Page => SystemBreak::Page,
            _ if multi => {
                if score_breaks.get(m.index().get()).copied().unwrap_or(false) {
                    SystemBreak::Line
                } else {
                    SystemBreak::None
                }
            }
            other => other,
        })
        .collect();
    let forced: Vec<bool> = system_breaks
        .iter()
        .map(|kind| *kind != SystemBreak::None)
        .collect();
    let plans = break_systems(
        &widths,
        options.line_width(),
        &prefixes,
        Breaks {
            forced: &forced,
            // A track's bar limit is for its own view.
            max_measures: top
                .view
                .bars_per_system()
                .filter(|_| !multi)
                .map(usize::from),
        },
    );
    // Per system, the rows above each track's staves.
    let rows: Vec<Vec<Rows>> = plans
        .iter()
        .map(|plan| {
            parts
                .iter()
                .enumerate()
                .map(|(index, part)| {
                    Rows::of(
                        part.marks.get(plan.measures.clone()).unwrap_or_default(),
                        part.ctx.notation.is_some(),
                        part.show_tab,
                        index == 0,
                    )
                })
                .collect()
        })
        .collect();
    // Per system and track: the room above its staves (its rows; for a track below the
    // top one also its notes that reach above the staff) and below them before the next
    // track (more when a notation staff is lowest and its notes hang below it).
    let rooms: Vec<Vec<(f32, f32)>> = plans
        .iter()
        .zip(&rows)
        .map(|(plan, rows)| {
            parts
                .iter()
                .zip(rows)
                .enumerate()
                .map(|(index, (part, rows))| {
                    let (reach_above, reach_below) = part.reach(plan.measures.clone());
                    let above = if index == 0 {
                        rows.height()
                    } else {
                        rows.height().max(reach_above)
                    };
                    let below = if index.saturating_add(1) == parts.len() {
                        0.0
                    } else if part.show_tab {
                        TRACK_GAP
                    } else {
                        TRACK_GAP + reach_below
                    };
                    (above, below)
                })
                .collect()
        })
        .collect();
    let heights: Vec<f32> = rooms
        .iter()
        .map(|rooms| {
            rooms
                .iter()
                .zip(&parts)
                .map(|((above, below), part)| above + part.staves_height + below)
                .sum()
        })
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

    let mut drawn: Vec<Drawn> = parts.iter().map(|_| Drawn::new(count)).collect();
    let mut bottom = header_end;
    for (index, plan) in plans.iter().enumerate() {
        let left = options.margin_left;
        let x = left
            + prefixes
                .get(plan.measures.start)
                .copied()
                .unwrap_or_default();
        let mut y = placement.tops.get(index).copied().unwrap_or(bottom);
        let mut system_top = None;
        for (part_index, (part, drawn)) in parts.iter().zip(&mut drawn).enumerate() {
            let part_rows = rows
                .get(index)
                .and_then(|rows| rows.get(part_index))
                .copied()
                .unwrap_or_default();
            let (above, below) = rooms
                .get(index)
                .and_then(|rooms| rooms.get(part_index))
                .copied()
                .unwrap_or_default();
            let top = y + above;
            system_top.get_or_insert(top);
            let system = System {
                index,
                plan_measures: plan.measures.clone(),
                stretch: plan.stretch,
                left,
                x,
                top,
                rows: part_rows,
                numbers: part_index == 0,
                key: keys.get(plan.measures.start).copied().unwrap_or_default(),
                last_measure: count,
            };
            draw_part(part, &system, drawn, &mut ops);
            if let Some(name) = names.get(part_index).filter(|name| !name.is_empty()) {
                ops.push(DrawOp::Text {
                    text: name.clone(),
                    at: Point::new(
                        left - NAME_GAP,
                        top + part.staves_height / 2.0 + NAME_SIZE * 0.35,
                    ),
                    size: NAME_SIZE,
                    font: TextFont::Regular,
                    anchor: TextAnchor::End,
                    color: ColorToken::Annotation,
                });
            }
            bottom = top + part.staves_height;
            y = bottom + below;
        }
        // One line joins the staves of all tracks at the system's start.
        if multi && let Some(system_top) = system_top {
            let thin = fonts.music.engraving().thin_barline_thickness;
            ops.push(DrawOp::Line {
                from: Point::new(left + thin / 2.0, system_top),
                to: Point::new(left + thin / 2.0, bottom),
                thickness: thin,
                color: ColorToken::Barline,
            });
        }
    }
    for (part, drawn) in parts.iter().zip(&drawn) {
        draw_barline_ties(part, drawn, &mut ops);
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

    let tracks: Vec<TrackPlaces> = parts
        .iter()
        .zip(drawn)
        .map(|(part, drawn)| TrackPlaces {
            track: part.track,
            places: drawn.places,
        })
        .collect();
    let places = tracks
        .iter()
        .find(|t| t.track == primary)
        .or(tracks.first())
        .map(|t| t.places.clone())
        .unwrap_or_default();
    Ok(Layout {
        width: options.page_width,
        height,
        pages,
        ops,
        places,
        tracks,
    })
}

/// One system as one track draws it.
struct System {
    index: usize,
    plan_measures: std::ops::Range<usize>,
    stretch: f32,
    /// Where the staves start, and where the first measure starts (after the clef).
    left: f32,
    x: f32,
    /// The top line of the track's topmost staff.
    top: f32,
    rows: Rows,
    /// Whether the measure numbers are written (the system's top track).
    numbers: bool,
    /// The key at the system's start.
    key: i8,
    /// How many measures the score has (the last one ends with a final barline).
    last_measure: usize,
}

/// What a track's drawing leaves for later: its places, and where each measure went (for
/// the ties across barlines).
struct Drawn {
    places: Places,
    measure_x: Vec<f32>,
    measure_stretch: Vec<f32>,
    measure_tops: Vec<Option<Tops>>,
}

impl Drawn {
    fn new(count: usize) -> Self {
        Drawn {
            places: Places::default(),
            measure_x: vec![0.0; count],
            measure_stretch: vec![1.0; count],
            measure_tops: vec![None; count],
        }
    }
}

/// Draws one track's staves of one system: lines, clef, measures, marks and barlines.
fn draw_part(part: &Part<'_>, system: &System, drawn: &mut Drawn, ops: &mut Vec<DrawOp>) {
    let ctx = &part.ctx;
    let font = &ctx.fonts.music;
    let notation_staff = notation::staff();
    let show_tab = part.show_tab;
    let rows = system.rows;
    let top = system.top;
    let tab_top = top + part.tab_offset;
    let tops = Tops {
        tab: tab_top,
        notation: ctx.notation.map(|_| top),
        show_tab,
        numbers: system.numbers,
        // Beat effects go above the notation staff, below the rows of text marks, or in a
        // lane above the tab when there is no notation.
        beat_bottom: if ctx.notation.is_some() {
            top - rows.ornament_band() - marks::BEAT_ABOVE
        } else {
            tab_top - ornaments::TAB_BEAT_LANE
        },
        lanes: if show_tab {
            tab_top
        } else {
            top + marks::LANES_DROP
        },
    };
    let place_index = drawn.places.systems.len();
    debug_assert_eq!(place_index, system.index);
    drawn.places.systems.push(if show_tab {
        SystemPlace {
            top,
            tab_top,
            string_spacing: ctx.options.tab_line_spacing,
            strings: part.view.string_count(),
        }
    } else {
        // Notation only: the cursor's string rows are spread over the notation staff.
        let strings = part.view.string_count();
        let gaps = strings.saturating_sub(1).max(1) as f32;
        SystemPlace {
            top,
            tab_top: top,
            string_spacing: notation_staff.height() / gaps,
            strings,
        }
    });
    let line: &[Arc<MeasureLayout>] = part
        .measures
        .get(system.plan_measures.clone())
        .unwrap_or_default();
    let line_marks: &[MeasureMarks] = part
        .marks
        .get(system.plan_measures.clone())
        .unwrap_or_default();
    let mut x = system.x;
    let mut content = Vec::new();
    let default_marks = MeasureMarks::default();
    for (offset, measure) in line.iter().enumerate() {
        let measure_marks = line_marks.get(offset).unwrap_or(&default_marks);
        let (before, after) = measure_marks.extra_width(ctx);
        let left = x;
        let content_x = x + before;
        let mut place = measure.place(content_x, system.stretch, place_index);
        place.left = left;
        drawn.places.measures.push(place);
        let index = measure.index().get();
        if let Some(slot) = drawn.measure_x.get_mut(index) {
            *slot = content_x;
        }
        if let Some(slot) = drawn.measure_stretch.get_mut(index) {
            *slot = system.stretch;
        }
        if let Some(slot) = drawn.measure_tops.get_mut(index) {
            *slot = Some(tops);
        }
        measure.draw(content_x, tops, system.stretch, ctx, &mut content);
        x = content_x + measure.width(system.stretch) + after;
        marks::draw(
            marks::Placed {
                layout: measure,
                marks: measure_marks,
                left,
                right: x,
                content: content_x,
                stretch: system.stretch,
            },
            rows,
            tops,
            ctx,
            &mut content,
        );
        let is_final = system
            .plan_measures
            .start
            .saturating_add(offset)
            .saturating_add(1)
            == system.last_measure;
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
        if system.numbers
            && let Some(count) = marks::repeat_count(measure_marks)
        {
            content.push(DrawOp::Text {
                text: count,
                // Left of the barline, clear of the next measure's number.
                at: Point::new(
                    x - REPEAT_COUNT_GAP,
                    tops.notation.unwrap_or(tab_top) - MEASURE_NUMBER_RISE,
                ),
                size: ctx.options.measure_number_size,
                font: TextFont::Regular,
                anchor: TextAnchor::End,
                color: ColorToken::Annotation,
            });
        }
    }
    // Staff lines first, so masks and fret numbers paint over them.
    let left = system.left;
    let key = system.key;
    let key_drawn_by_measure = line
        .first()
        .is_some_and(|measure| measure.has_key_signature());
    if show_tab {
        ctx.staff.draw_lines(tab_top, left, x, font, ops);
        notation::draw_tab_staff_start(
            left,
            tab_top,
            key,
            key_drawn_by_measure,
            ctx.notation.is_none(),
            ctx,
            ops,
        );
    }
    if let (Some(notation), Some(notation_top)) = (ctx.notation, tops.notation) {
        notation_staff.draw_lines(notation_top, left, x, font, ops);
        notation::draw_staff_start(
            left,
            notation_top,
            notation.clef,
            key,
            key_drawn_by_measure,
            ctx,
            ops,
        );
        // One line joins the staves of a track at the system's start.
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
        ctx.staff.draw_start_line(left, tab_top, font, ops);
    }
    ops.append(&mut content);
}

/// Ties that continue a note across a barline, drawn after the measures are placed so
/// both noteheads are known.
fn draw_barline_ties(part: &Part<'_>, drawn: &Drawn, ops: &mut Vec<DrawOp>) {
    let measures = &part.measures;
    let places = &drawn.places;
    for index in 0..measures.len().saturating_sub(1) {
        let next = index.saturating_add(1);
        let (Some(origin), Some(target)) = (measures.get(index), measures.get(next)) else {
            continue;
        };
        let (Some(&origin_x), Some(&target_x)) =
            (drawn.measure_x.get(index), drawn.measure_x.get(next))
        else {
            continue;
        };
        let (Some(Some(origin_tops)), Some(Some(target_tops))) = (
            drawn.measure_tops.get(index).copied(),
            drawn.measure_tops.get(next).copied(),
        ) else {
            continue;
        };
        let same_system = places.measures.get(index).map(|place| place.system)
            == places.measures.get(next).map(|place| place.system);
        measure::draw_barline_ties(
            origin,
            origin_x,
            drawn.measure_stretch.get(index).copied().unwrap_or(1.0),
            origin_tops,
            target,
            target_x,
            drawn.measure_stretch.get(next).copied().unwrap_or(1.0),
            target_tops,
            same_system,
            &part.ctx,
            ops,
        );
    }
}
