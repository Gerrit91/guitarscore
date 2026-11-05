//! A score of one or more tracks: every shown track's staves stacked in each system, with
//! the measures of all tracks on one time grid ([`crate::measure::align`]), broken into
//! systems together and put onto pages.
//!
//! The top track of a system carries what belongs to the whole system: measure numbers,
//! section names, tempo marks and alternative endings. Every track keeps its own staves,
//! barlines, repeat signs, dynamics, fermatas and beat effects. With several tracks, the
//! tracks' short names stand in the left margin before each system, one line joins all
//! their staves, and the song's multitrack line breaks apply instead of a track's own.

mod draw;
mod part;

use std::sync::Arc;

use gscore_model::{Song, SystemBreak, TrackIndex, TrackView};

use self::draw::{Drawn, System, draw_barline_ties, draw_part};
use self::part::Part;
use crate::draw::{ColorToken, DrawOp, Point, Rect, TextAnchor, TextFont};
use crate::marks::{self, Rows};
use crate::measure::MeasureLayout;
use crate::pages::{PageDecor, place_systems};
use crate::system::{Breaks, MeasureWidth, break_systems};
use crate::{
    AlignSource, Fonts, HEADER_GAP, Layout, LayoutError, LayoutOptions, MeasureSource, TrackPlaces,
    header,
};

/// Space between the lowest staff of one track and the rows above the next track's staves
/// in a system (room for what hangs below a staff: P.M. and let ring rows, dynamics).
const TRACK_GAP: f32 = 5.0;
/// Gap between a track's name and the start of its staves.
const NAME_GAP: f32 = 1.2;
/// Font size of the track names before a system.
pub(super) const NAME_SIZE: f32 = 1.2;
/// Room kept between a track's name and the page edge.
const NAME_MARGIN: f32 = 0.5;

/// Lays out `tracks` of `song` (top to bottom) in shared systems; the result's
/// [`Layout::places`] are those of `primary` (the first track if it is not shown).
pub(crate) fn layout_with(
    song: &Song,
    tracks: &[TrackIndex],
    primary: TrackIndex,
    options: &LayoutOptions,
    fonts: &Fonts,
    measure_layout: &mut MeasureSource<'_>,
    align: &mut AlignSource<'_>,
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
            for (part, aligned) in parts.iter_mut().zip(align(index, &column)) {
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
    // One track breaks where it does on its own. Several break where the song's
    // multitrack layout says, and start a page where any shown track has a page break.
    let score_breaks = if multi {
        song.score_line_breaks()
    } else {
        Vec::new()
    };
    let system_breaks: Vec<SystemBreak> = (0..count)
        .map(|index| {
            let own = |part: &Part<'_>| {
                part.view
                    .measure(gscore_model::MeasureIndex::new(index))
                    .map_or(SystemBreak::None, |m| m.system_break())
            };
            if !multi {
                return own(top);
            }
            if parts.iter().any(|part| own(part) == SystemBreak::Page) {
                SystemBreak::Page
            } else if score_breaks.get(index).copied().unwrap_or(false) {
                SystemBreak::Line
            } else {
                SystemBreak::None
            }
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
