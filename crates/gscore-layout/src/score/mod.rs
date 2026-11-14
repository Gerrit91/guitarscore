//! A score of one or more tracks: every staff of every shown track stacked in each
//! system, with the measures of all of them on one time grid ([`crate::measure::align`]),
//! broken into systems together and put onto pages.
//!
//! One row of a system is one staff of one track: a track with several staves (a keyboard's
//! treble and bass) takes a row per staff, its staves one under the other, joined by one
//! line and a brace and named once. The top staff of a system carries what belongs to the
//! whole system: measure numbers, section names, tempo marks and alternative endings. Every
//! staff keeps its own clef, barlines, repeat signs, dynamics, fermatas and beat effects.
//! With several tracks, the tracks' short names stand in the left margin before each system,
//! one line joins all their staves, and the song's multitrack line breaks apply instead of a
//! track's own.

mod draw;
mod part;

use std::sync::Arc;

use gscore_model::{Song, StaffIndex, SystemBreak, TrackIndex, TrackView};

use self::draw::{Drawn, Group, System, draw_barline_ties, draw_group, draw_part};
use self::part::Part;
use crate::draw::{ColorToken, DrawOp, Point, Rect};
use crate::marks::{self, Rows};
use crate::measure::MeasureLayout;
use crate::options::EmptyStaves;
use crate::pages::{PageDecor, place_systems};
use crate::system::{Breaks, MeasureWidth, break_systems};
use crate::{
    AlignSource, Fonts, HEADER_GAP, Layout, LayoutError, LayoutOptions, MeasureSource, Places,
    TrackPlaces, header,
};

/// Space between the lowest staff of one track and the rows above the next track's staves
/// in a system (room for what hangs below a staff: P.M. and let ring rows, dynamics).
const TRACK_GAP: f32 = 5.0;
/// Space between two staves of one track (a keyboard's treble and bass): less than between
/// two tracks, more than the lines of one staff.
const STAFF_GAP: f32 = 4.0;
/// Gap between a track's name and the start of its staves.
const NAME_GAP: f32 = 1.2;
/// Font size of the track names before a system.
pub(super) const NAME_SIZE: f32 = 1.2;
/// Room kept between a track's name and the page edge.
const NAME_MARGIN: f32 = 0.5;

/// Lays out `tracks` of `song` (top to bottom, every staff of each) in shared systems; the
/// result's [`Layout::places`] are those of `primary`'s first staff (the first track's if
/// it is not shown).
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
    // A row of a system is one staff of one track.
    let mut parts = Vec::new();
    for &track in tracks {
        let view = song.track(track).ok_or(LayoutError::NoSuchTrack(track))?;
        let staves = view.staff_count().max(1);
        for staff in 0..staves {
            parts.push(Part::new(
                song,
                track,
                StaffIndex::new(staff),
                options,
                fonts,
                measure_layout,
            )?);
        }
    }
    // Which rows belong to a track with several staves (they are joined and named as a
    // group).
    let several: Vec<bool> = parts
        .iter()
        .map(|part| {
            song.track(part.track)
                .is_some_and(|view| view.staff_count() > 1)
        })
        .collect();
    let count = parts
        .iter()
        .map(|part| part.measures.len())
        .min()
        .unwrap_or(0);
    // Several tracks share the song's breaks and carry their names in the margin; a single
    // track (however many staves) breaks where it does on its own.
    let multi = tracks.len() > 1;
    // Every staff's measures line up: a piano's bass notes fall under its treble's.
    if parts.len() > 1 {
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
    // The header names and tunes each track once, not once per staff.
    let views: Vec<TrackView<'_>> = parts
        .iter()
        .filter(|part| part.staff.get() == 0)
        .map(|part| part.view)
        .collect();
    let header_end = header::draw(song, &views, &top.ctx, &mut ops)?;

    let keys: Vec<i8> = (0..count)
        .map(|index| {
            song.key_signature(gscore_model::MeasureIndex::new(index))
                .map_or(0, |key| key.key)
        })
        .collect();
    // After aligning, a measure is equally wide on every staff; repeat signs are the
    // song's, so the top staff's marks size them for all.
    let widths: Vec<MeasureWidth> = top
        .measures
        .iter()
        .zip(&top.marks)
        .map(|(m, marks)| {
            let (before, after) = marks.extra_width(&top.ctx);
            MeasureWidth {
                // Guitar Pro's design mode: a moved column's room is fixed, not squeezed.
                fixed: m.fixed_width() + m.spacing_width() + before + after,
                stretchable: m.stretchable_width(),
                floor: m.floor_width() + m.spacing_floor(),
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
        parts
            .iter()
            .filter(|part| part.staff.get() == 0)
            .map(|part| part.margin_name(room))
            .collect()
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
            single_line: options.single_line,
        },
    );
    // Per system, which tracks are drawn: all, unless empty staves are hidden; then a
    // track without a note in the system's bars is left out, with all its staves, except
    // on the first system and the primary track (the cursor's, which must stay reachable).
    let shown_tracks: Vec<Vec<bool>> = plans
        .iter()
        .map(|plan| {
            tracks
                .iter()
                .map(|&track| {
                    let keep = match options.empty_staves {
                        EmptyStaves::Shown => true,
                        EmptyStaves::Hidden => false,
                        EmptyStaves::HiddenButPrimary => track == primary,
                    };
                    keep || !multi
                        || plan.measures.start == 0
                        || parts.iter().any(|part| {
                            part.track == track && part.has_notes(plan.measures.clone())
                        })
                })
                .collect()
        })
        .collect();
    let shown: Vec<Vec<bool>> = plans
        .iter()
        .enumerate()
        .map(|(system, _)| {
            parts
                .iter()
                .map(|part| {
                    tracks
                        .iter()
                        .position(|&track| track == part.track)
                        .and_then(|at| shown_tracks.get(system).and_then(|s| s.get(at)).copied())
                        .unwrap_or(true)
                })
                .collect()
        })
        .collect();
    let first_shown = |system: usize| {
        shown
            .get(system)
            .and_then(|shown| shown.iter().position(|&on| on))
            .unwrap_or(0)
    };
    let last_shown = |system: usize| {
        shown
            .get(system)
            .and_then(|shown| shown.iter().rposition(|&on| on))
            .unwrap_or(0)
    };
    // Per system, the rows above each staff; only a system's top staff carries the marks
    // that belong to the whole system (sections, tempo, endings).
    let rows: Vec<Vec<Rows>> = plans
        .iter()
        .enumerate()
        .map(|(system, plan)| {
            parts
                .iter()
                .enumerate()
                .map(|(index, part)| {
                    Rows::of(
                        part.marks.get(plan.measures.clone()).unwrap_or_default(),
                        part.ctx.notation.is_some(),
                        part.show_tab,
                        index == first_shown(system) && part.staff.get() == 0,
                    )
                })
                .collect()
        })
        .collect();
    // Per system and staff: the room above it (its rows; for a staff below the top one
    // also its notes that reach above) and below it before what comes next (more when a
    // notation staff is lowest and its notes hang below it; less between two staves of one
    // track than between two tracks). A track not shown takes no room.
    let rooms: Vec<Vec<Option<(f32, f32)>>> = plans
        .iter()
        .zip(&rows)
        .enumerate()
        .map(|(system, (plan, rows))| {
            parts
                .iter()
                .zip(rows)
                .enumerate()
                .map(|(index, (part, rows))| {
                    let on = shown
                        .get(system)
                        .and_then(|shown| shown.get(index))
                        .copied()
                        .unwrap_or(true);
                    if !on {
                        return None;
                    }
                    let (reach_above, reach_below) = part.reach(plan.measures.clone());
                    let above = if index == first_shown(system) {
                        rows.height()
                    } else {
                        rows.height().max(reach_above)
                    };
                    // The next staff of the same track stays close.
                    let same_track = parts.get(index.saturating_add(1)).is_some_and(|next| {
                        next.track == part.track && next.staff.get() > part.staff.get()
                    });
                    let gap = if same_track { STAFF_GAP } else { TRACK_GAP };
                    let below = if index == last_shown(system) {
                        0.0
                    } else if part.show_tab {
                        gap
                    } else {
                        gap + reach_below
                    };
                    Some((above, below))
                })
                .collect()
        })
        .collect();
    let heights: Vec<f32> = rooms
        .iter()
        .zip(&plans)
        .map(|(rooms, plan)| {
            rooms
                .iter()
                .zip(&parts)
                .filter_map(|(room, part)| {
                    room.map(|(above, below)| {
                        above + part.staves_height(plan.measures.clone()) + below
                    })
                })
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
    let mut rightmost = 0.0_f32;
    for (index, plan) in plans.iter().enumerate() {
        let left = options.margin_left;
        let x = left
            + prefixes
                .get(plan.measures.start)
                .copied()
                .unwrap_or_default();
        let mut y = placement.tops.get(index).copied().unwrap_or(bottom);
        let mut system_top = None;
        // Where the track being drawn (its first staff) starts in this system, to join and
        // name its staves as a group.
        let mut group_top: Option<f32> = None;
        for (part_index, (part, drawn)) in parts.iter().zip(&mut drawn).enumerate() {
            let part_rows = rows
                .get(index)
                .and_then(|rows| rows.get(part_index))
                .copied()
                .unwrap_or_default();
            let Some((above, below)) = rooms
                .get(index)
                .and_then(|rooms| rooms.get(part_index))
                .copied()
                .unwrap_or(Some((0.0, 0.0)))
            else {
                // Hidden in this system: it keeps a place without measures, so its
                // system numbers stay in step with the other staves'.
                drawn.skip(y);
                continue;
            };
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
                numbers: part_index == first_shown(index),
                key: keys.get(plan.measures.start).copied().unwrap_or_default(),
                clef: part
                    .measures
                    .get(plan.measures.start)
                    .map_or(crate::notation::Clef::Treble, |measure| measure.clef()),
                last_measure: count,
                // A track with several staves gets one line around them all, below.
                join: !several.get(part_index).copied().unwrap_or(false),
            };
            draw_part(part, &system, drawn, &mut ops);
            let tallest = part.staves_height(plan.measures.clone());
            bottom = top + tallest;
            group_top.get_or_insert(top);
            y = bottom + below;
            // The last staff of a track: join its staves and write its name once.
            let last_of_track = parts
                .get(part_index.saturating_add(1))
                .is_none_or(|next| next.track != part.track);
            if !last_of_track {
                continue;
            }
            let group_top = group_top.take().unwrap_or(top);
            let name = tracks
                .iter()
                .position(|&track| track == part.track)
                .and_then(|at| names.get(at))
                .filter(|name| !name.is_empty());
            draw_group(
                part,
                Group {
                    top: group_top,
                    last_top: top,
                    height: bottom - group_top,
                    joined: several.get(part_index).copied().unwrap_or(false),
                },
                plan.measures.clone(),
                name,
                left,
                fonts,
                &mut ops,
            );
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
        let line_width: f32 = plan
            .measures
            .clone()
            .filter_map(|index| widths.get(index))
            .map(|width| width.fixed + width.stretchable * plan.stretch)
            .sum();
        rightmost = rightmost.max(x + line_width);
    }
    for (part, drawn) in parts.iter().zip(&drawn) {
        draw_barline_ties(part, drawn, &mut ops);
    }

    let width = if options.single_line {
        rightmost + options.margin_right
    } else {
        options.page_width
    };
    let (pages, height) = if placement.pages.is_empty() {
        let height = bottom + options.margin_bottom;
        (
            vec![Rect {
                origin: Point::new(0.0, 0.0),
                width,
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
            staff: part.staff,
            places: Places {
                staff: part.staff,
                ..drawn.places
            },
        })
        .collect();
    let places = tracks
        .iter()
        .find(|t| t.track == primary && t.staff.get() == 0)
        .or_else(|| tracks.iter().find(|t| t.track == primary))
        .or(tracks.first())
        .map(|t| t.places.clone())
        .unwrap_or_default();
    Ok(Layout {
        width,
        height,
        pages,
        ops,
        places,
        tracks,
    })
}
