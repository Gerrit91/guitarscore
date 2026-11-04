//! Marks by a notation chord's heads: accents and staccato opposite the stem, tremolo
//! strokes through the stem, an arrow before a brushed chord, slurs for hammer-ons, pull-offs
//! and beat-level legato, and lines for slides. Ghost-note parentheses are drawn with the
//! heads (`draw::draw_heads`), since they move the accidentals.

use gscore_model::{AccentKind, SlideKind, StrokeDirection};

use super::draw::{Stem, draw_tie_curve, y_of};
use super::{BeamPlan, Chord, Head, NotationItem};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point};
use crate::glyph::Glyph;

/// Distance from a notehead's middle to the first mark beside it.
const HEAD_CLEARANCE: f32 = 0.8;
/// Gap between two stacked marks.
const MARK_GAP: f32 = 0.25;
/// How far accents keep off the staff.
const STAFF_CLEARANCE: f32 = 0.5;
/// Thickness of slide lines and the brush arrow.
const LINE_THICKNESS: f32 = 0.13;
/// Brush arrow: gap to the chord, how far it reaches past the outer heads, its shortest
/// length, and the arrowhead's length and half width.
const BRUSH_GAP: f32 = 0.5;
const BRUSH_OVERHANG: f32 = 0.6;
const BRUSH_MIN: f32 = 2.0;
const ARROW: (f32, f32) = (0.7, 0.3);
/// Room the brush arrow takes left of the chord.
pub(super) const BRUSH_ROOM: f32 = BRUSH_GAP + ARROW.1 + 0.2;
/// Slides into and out of a note: length and drop of the line, and its gap to the head.
const SLIDE_REACH: f32 = 0.9;
const SLIDE_DROP: f32 = 0.5;
const SLIDE_GAP: f32 = 0.2;
/// Room a slide into or out of a note takes beside the head.
pub(super) const SLIDE_ROOM: f32 = SLIDE_REACH + SLIDE_GAP;
/// A beat-level legato slur: distance from the outer heads and extra arch over the run.
const LEGATO_CLEARANCE: f32 = 0.8;
const LEGATO_ARCH: f32 = 0.8;

/// Draws the chord's accent and staccato opposite its stem (above a stemless chord) and its
/// tremolo strokes. Returns the highest point drawn, for the lanes above the staff.
pub(super) fn draw_articulations(
    chord: &Chord,
    x: f32,
    stem: Option<Stem>,
    top: f32,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) -> f32 {
    let font = &ctx.fonts.music;
    let mut highest = f32::MAX;
    let below = stem.is_some_and(|stem| stem.up);
    let (Some(lowest), Some(high)) = (chord.heads.first(), chord.heads.last()) else {
        return highest;
    };
    let staff_bottom = top + 4.0;
    let mut put = |glyph: Glyph, y: f32, ops: &mut Vec<DrawOp>| {
        let bbox = font.bbox(glyph);
        ops.push(DrawOp::Glyph {
            glyph,
            at: Point::new(x - (bbox.x_min + bbox.x_max) / 2.0, y),
            scale: 1.0,
            color: ColorToken::Notation,
        });
        // Glyph boxes have y up; the drawing has y down.
        highest = highest.min(y - bbox.y_max);
    };
    if below {
        let mut cursor = y_of(top, lowest.position) + HEAD_CLEARANCE - 0.3;
        if chord.marks.staccato {
            let glyph = Glyph::StaccatoBelow;
            let height = font.bbox(glyph).y_max - font.bbox(glyph).y_min;
            let middle = y_of(top, lowest.position) + 1.0;
            let y = in_space(middle, top, true) - height / 2.0;
            put(glyph, y, ops);
            cursor = y + height + MARK_GAP;
        }
        if let Some(accent) = chord.marks.accent {
            let glyph = match accent {
                AccentKind::Normal => Glyph::AccentBelow,
                AccentKind::Heavy => Glyph::MarcatoBelow,
            };
            put(glyph, cursor.max(staff_bottom + STAFF_CLEARANCE), ops);
        }
    } else {
        let mut cursor = y_of(top, high.position) - HEAD_CLEARANCE + 0.3;
        if chord.marks.staccato {
            let glyph = Glyph::StaccatoAbove;
            let height = font.bbox(glyph).y_max - font.bbox(glyph).y_min;
            let middle = y_of(top, high.position) - 1.0;
            let y = in_space(middle, top, false) + height / 2.0;
            put(glyph, y, ops);
            cursor = y - height - MARK_GAP;
        }
        if let Some(accent) = chord.marks.accent {
            let glyph = match accent {
                AccentKind::Normal => Glyph::AccentAbove,
                AccentKind::Heavy => Glyph::MarcatoAbove,
            };
            put(glyph, cursor.min(top - STAFF_CLEARANCE), ops);
        }
    }
    if chord.marks.tremolo {
        let glyph = Glyph::Tremolo3;
        let bbox = font.bbox(glyph);
        // On the stem between the heads and the beam, or above a stemless head.
        let (stem_x, middle) = match stem {
            Some(stem) => (stem.x, stem.from + (stem.to - stem.from) * 0.5),
            None => (x, y_of(top, high.position) - 1.6),
        };
        let y = middle + (bbox.y_min + bbox.y_max) / 2.0;
        ops.push(DrawOp::Glyph {
            glyph,
            at: Point::new(stem_x - (bbox.x_min + bbox.x_max) / 2.0, y),
            scale: 1.0,
            color: ColorToken::Notation,
        });
        highest = highest.min(y - bbox.y_max);
    }
    highest
}

/// `y` (a staccato dot's middle, a staff space from its head) moved on to the middle of a
/// staff space when it falls on a line inside the staff, away from the head (`down` when
/// the dot is below it), so it never sits on a line.
fn in_space(y: f32, top: f32, down: bool) -> f32 {
    if y < top - 0.25 || y > top + 4.25 {
        return y;
    }
    // Space middles are at top + 0.5, 1.5, 2.5, 3.5.
    let offset = y - top - 0.5;
    let index = if down { offset.ceil() } else { offset.floor() };
    let snapped = top + 0.5 + index;
    if (top..=top + 4.0).contains(&snapped) {
        snapped
    } else {
        // Just outside the staff: half a space past the outer line.
        if down { top + 4.5 } else { top - 0.5 }
    }
}

/// How far a chord's accidentals reach left of its heads.
fn accidental_room(chord: &Chord, ctx: &Context<'_>) -> f32 {
    chord
        .heads
        .iter()
        .filter_map(|head| head.accidental)
        .map(|glyph| ctx.fonts.music.advance(glyph) + 0.25)
        .fold(0.0, f32::max)
}

fn head_width(chord: &Chord, head: &Head, ctx: &Context<'_>) -> f32 {
    ctx.fonts.music.advance(chord.head_glyph(head))
}

/// The brush arrow before a chord: a vertical line over its heads. Guitar Pro writes a
/// brush down as an arrow pointing up, so the arrowhead is at the top for `Down`.
pub(super) fn draw_brush(
    chord: &Chord,
    x: f32,
    top: f32,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let Some(direction) = chord.marks.brush else {
        return;
    };
    let (Some(lowest), Some(highest)) = (chord.heads.first(), chord.heads.last()) else {
        return;
    };
    let width = head_width(chord, lowest, ctx);
    let line_x = x - width / 2.0 - accidental_room(chord, ctx) - BRUSH_GAP;
    let bottom = y_of(top, lowest.position) + BRUSH_OVERHANG;
    let high = y_of(top, highest.position) - BRUSH_OVERHANG;
    let high = high.min(bottom - BRUSH_MIN);
    draw_arrow(line_x, high, bottom, direction == StrokeDirection::Up, ops);
}

/// A vertical arrow from `high` to `low` at `x`, its head at the bottom when `down`.
pub(crate) fn draw_arrow(x: f32, high: f32, low: f32, down: bool, ops: &mut Vec<DrawOp>) {
    let (tip, base) = if down {
        (low, low - ARROW.0)
    } else {
        (high, high + ARROW.0)
    };
    let (from, to) = if down { (high, base) } else { (base, low) };
    ops.push(DrawOp::Line {
        from: Point::new(x, from),
        to: Point::new(x, to),
        thickness: LINE_THICKNESS,
        color: ColorToken::Notation,
    });
    ops.push(DrawOp::Polygon {
        points: vec![
            Point::new(x, tip),
            Point::new(x + ARROW.1, base),
            Point::new(x - ARROW.1, base),
        ],
        color: ColorToken::Notation,
    });
}

/// Slurs and slide lines from each head to the same string's head in the voice's next
/// chord, slides into and out of single heads, and one slur over each beat-level legato run.
pub(super) fn draw_links(
    placed: &[BeamPlan<'_>],
    stems: &[Option<Stem>],
    top: f32,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let thickness = ctx.fonts.music.engraving().stem_thickness;
    let chords: Vec<(usize, f32, &Chord)> = placed
        .iter()
        .enumerate()
        .filter_map(|(i, p)| match p.item {
            NotationItem::Chord(chord) => Some((i, p.x, chord)),
            NotationItem::Rest { .. } => None,
        })
        .collect();
    for (n, &(i, x, chord)) in chords.iter().enumerate() {
        let up = stems.get(i).copied().flatten().is_none_or(|s| s.up);
        for head in &chord.heads {
            draw_slides_in_out(chord, head, x, top, ctx, ops);
            let slide = head
                .slides
                .iter()
                .copied()
                .find(|s| matches!(s, SlideKind::ShiftSlideTo | SlideKind::LegatoSlideTo));
            if !head.hammer && slide.is_none() {
                continue;
            }
            let Some(string) = head.string else { continue };
            let target = chords
                .iter()
                .skip(n.saturating_add(1))
                .find_map(|&(_, tx, tc)| {
                    tc.heads
                        .iter()
                        .find(|h| h.string == Some(string))
                        .map(|h| (tx, tc, h))
                });
            let Some((target_x, target_chord, target_head)) = target else {
                continue;
            };
            let from_y = y_of(top, head.position);
            let to_y = y_of(top, target_head.position);
            if head.hammer || slide == Some(SlideKind::LegatoSlideTo) {
                draw_tie_curve(x + 0.15, from_y, target_x - 0.15, to_y, up, thickness, ops);
            }
            if slide.is_some() {
                let from_x = x + head_width(chord, head, ctx) / 2.0 + SLIDE_GAP;
                let to_x = target_x
                    - head_width(target_chord, target_head, ctx) / 2.0
                    - accidental_room(target_chord, ctx)
                    - SLIDE_GAP;
                if to_x > from_x {
                    let rise = (to_y - from_y).clamp(-0.4, 0.4);
                    line(from_x, from_y - rise / 2.0, to_x, to_y + rise / 2.0, ops);
                }
            }
        }
    }
    draw_legato_runs(&chords, stems, top, ops);
}

/// The short lines of slides into and out of a note.
fn draw_slides_in_out(
    chord: &Chord,
    head: &Head,
    x: f32,
    top: f32,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let y = y_of(top, head.position);
    let half = head_width(chord, head, ctx) / 2.0;
    let left = x - half - accidental_room(chord, ctx) - SLIDE_GAP;
    let dots = f32::from(chord.dots) * 0.6;
    let right = x + half + dots + SLIDE_GAP;
    for slide in &head.slides {
        match slide {
            SlideKind::IntoFromBelow => line(left - SLIDE_REACH, y + SLIDE_DROP, left, y, ops),
            SlideKind::IntoFromAbove => line(left - SLIDE_REACH, y - SLIDE_DROP, left, y, ops),
            SlideKind::OutDownwards => line(right, y, right + SLIDE_REACH, y + SLIDE_DROP, ops),
            SlideKind::OutUpwards => line(right, y, right + SLIDE_REACH, y - SLIDE_DROP, ops),
            SlideKind::ShiftSlideTo
            | SlideKind::LegatoSlideTo
            | SlideKind::PickSlideDown
            | SlideKind::PickSlideUp => {}
        }
    }
}

fn line(from_x: f32, from_y: f32, to_x: f32, to_y: f32, ops: &mut Vec<DrawOp>) {
    ops.push(DrawOp::Line {
        from: Point::new(from_x, from_y),
        to: Point::new(to_x, to_y),
        thickness: LINE_THICKNESS,
        color: ColorToken::Notation,
    });
}

/// One slur over each run of chords in a beat-level legato, opposite the first chord's
/// stem. A run whose last chord starts a legato reaches on to the next chord.
fn draw_legato_runs(
    chords: &[(usize, f32, &Chord)],
    stems: &[Option<Stem>],
    top: f32,
    ops: &mut Vec<DrawOp>,
) {
    let active = |chord: &Chord| chord.marks.legato_origin || chord.marks.legato_destination;
    let mut n = 0;
    while let Some(&(i, _, chord)) = chords.get(n) {
        if !active(chord) {
            n = n.saturating_add(1);
            continue;
        }
        let start = n;
        let mut end = n;
        while chords
            .get(end.saturating_add(1))
            .is_some_and(|&(_, _, c)| active(c))
        {
            end = end.saturating_add(1);
        }
        let reach = match chords.get(end) {
            Some(&(_, _, last)) if last.marks.legato_origin => {
                end.saturating_add(1).min(chords.len().saturating_sub(1))
            }
            _ => end,
        };
        if reach > start {
            let below = stems.get(i).copied().flatten().is_some_and(|s| s.up);
            let run = chords.get(start..=reach).unwrap_or_default();
            let edge = |chord: &Chord| {
                let head = if below {
                    chord.heads.first()
                } else {
                    chord.heads.last()
                };
                let y = head.map_or(top + 2.0, |h| y_of(top, h.position));
                if below {
                    y + LEGATO_CLEARANCE
                } else {
                    y - LEGATO_CLEARANCE
                }
            };
            let outer = run.iter().map(|&(_, _, c)| edge(c)).fold(
                if below { f32::MIN } else { f32::MAX },
                |a, b| {
                    if below { a.max(b) } else { a.min(b) }
                },
            );
            if let (Some(&(_, from_x, first)), Some(&(_, to_x, last))) = (run.first(), run.last()) {
                let arch = if below { LEGATO_ARCH } else { -LEGATO_ARCH };
                let handle = (to_x - from_x) / 3.0;
                ops.push(DrawOp::Curve {
                    from: Point::new(from_x, edge(first)),
                    to: Point::new(to_x, edge(last)),
                    control1: Point::new(from_x + handle, outer + arch),
                    control2: Point::new(to_x - handle, outer + arch),
                    thickness: 0.15,
                    color: ColorToken::Notation,
                });
            }
        }
        n = end.saturating_add(1);
    }
}
