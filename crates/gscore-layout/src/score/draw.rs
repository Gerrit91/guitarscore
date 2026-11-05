//! Drawing one track's staves in a system (lines, clef, measures, marks, barlines), and
//! the ties that cross barlines once every measure is placed.

use std::sync::Arc;

use super::part::Part;
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
use crate::marks::{self, MeasureMarks, Rows};
use crate::measure::{self, MeasureLayout, Tops};
use crate::notation;
use crate::ornaments;
use crate::place::{Places, SystemPlace};
use crate::staff::Staff;
use crate::{MEASURE_NUMBER_RISE, REPEAT_COUNT_GAP};

/// One system as one track draws it.
pub(super) struct System {
    pub(super) index: usize,
    pub(super) plan_measures: std::ops::Range<usize>,
    pub(super) stretch: f32,
    /// Where the staves start, and where the first measure starts (after the clef).
    pub(super) left: f32,
    pub(super) x: f32,
    /// The top line of the track's topmost staff.
    pub(super) top: f32,
    pub(super) rows: Rows,
    /// Whether the measure numbers are written (the system's top track).
    pub(super) numbers: bool,
    /// The key at the system's start.
    pub(super) key: i8,
    /// How many measures the score has (the last one ends with a final barline).
    pub(super) last_measure: usize,
}

/// What a track's drawing leaves for later: its places, and where each measure went (for
/// the ties across barlines).
pub(super) struct Drawn {
    pub(super) places: Places,
    pub(super) measure_x: Vec<f32>,
    pub(super) measure_stretch: Vec<f32>,
    pub(super) measure_tops: Vec<Option<Tops>>,
}

impl Drawn {
    /// A system the track is not drawn in (an empty staff hidden): a place at `y` without
    /// measures, which no click can hit.
    pub(super) fn skip(&mut self, y: f32) {
        self.places.systems.push(SystemPlace {
            top: y,
            tab_top: y,
            string_spacing: 0.0,
            strings: 1,
        });
    }

    pub(super) fn new(count: usize) -> Self {
        Drawn {
            places: Places::default(),
            measure_x: vec![0.0; count],
            measure_stretch: vec![1.0; count],
            measure_tops: vec![None; count],
        }
    }
}

/// Draws one track's staves of one system: lines, clef, measures, marks and barlines.
pub(super) fn draw_part(
    part: &Part<'_>,
    system: &System,
    drawn: &mut Drawn,
    ops: &mut Vec<DrawOp>,
) {
    let ctx = &part.ctx;
    let font = &ctx.fonts.music;
    let notation_staff = notation::staff();
    let show_tab = part.show_tab;
    let rows = system.rows;
    let top = system.top;
    let tab_top = top + part.tab_offset(system.plan_measures.clone());
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
pub(super) fn draw_barline_ties(part: &Part<'_>, drawn: &Drawn, ops: &mut Vec<DrawOp>) {
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
        // Measures of systems where the track is hidden have no place (nor tops).
        let system = |measure: usize| {
            places
                .measure(gscore_model::MeasureIndex::new(measure))
                .map(|place| place.system)
        };
        let same_system = system(index) == system(next);
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
