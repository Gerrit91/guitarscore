//! Painting a measure of standard notation: noteheads, ledger lines, accidentals, stems,
//! flags, beams, dots, rests, ties and tuplet numbers.

use super::beams::{beam_groups, draw_beams, draw_ties};
use super::{Chord, Clef, DOT_GAP, MIDDLE, NotationItem};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point};
use crate::glyph::{Flag, Glyph};

/// Normal stem length (one octave).
const STEM_LENGTH: f32 = 3.5;
/// Gap between an accidental and its notehead.
const ACCIDENTAL_GAP: f32 = 0.25;
/// Ledger lines start two positions outside the staff.
const LEDGER_BELOW: i32 = -2;
const LEDGER_ABOVE: i32 = 10;
/// Space between the system start and the clef, and after the clef.
const CLEF_GAPS: (f32, f32) = (0.6, 1.0);

/// A notation item at its column's center `x`; `beat` is the beat it starts in (beams
/// join notes within one beat).
#[derive(Debug, Clone, Copy)]
pub(crate) struct BeamPlan<'m> {
    pub(crate) x: f32,
    pub(crate) item: &'m NotationItem,
    pub(crate) beat: u64,
}

/// The y of staff position `position` on a notation staff whose top line is at `top`.
pub(super) fn y_of(top: f32, position: i32) -> f32 {
    top + 4.0 - position as f32 * 0.5
}

/// The room the clef takes at the start of a system.
pub(crate) fn prefix_width(clef: Clef, ctx: &Context<'_>) -> f32 {
    let (glyph, _) = clef.glyph();
    CLEF_GAPS.0 + ctx.fonts.music.advance(glyph) + CLEF_GAPS.1
}

/// Draws the clef at the start of a system whose left edge is `x`.
pub(crate) fn draw_staff_start(x: f32, top: f32, clef: Clef, ops: &mut Vec<DrawOp>) {
    let (glyph, position) = clef.glyph();
    ops.push(DrawOp::Glyph {
        glyph,
        at: Point::new(x + CLEF_GAPS.0, y_of(top, position)),
        scale: 1.0,
        color: ColorToken::Notation,
    });
}

/// Where a chord's stem goes.
#[derive(Debug, Clone, Copy)]
pub(super) struct Stem {
    pub(super) up: bool,
    pub(super) x: f32,
    /// The end at the noteheads.
    pub(super) from: f32,
    /// The free end (at the beam or flag).
    pub(super) to: f32,
}

/// Draws one measure of notation on the staff whose top line is at `top`.
pub(crate) fn draw_measure(
    placed: &[BeamPlan<'_>],
    top: f32,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let groups = beam_groups(placed);
    let mut stems: Vec<Option<Stem>> = vec![None; placed.len()];
    let mut beamed = vec![false; placed.len()];

    // Beamed groups share a direction and a (horizontal) beam line.
    for group in &groups {
        let chords: Vec<(usize, &Chord)> = group
            .iter()
            .filter_map(|&i| chord_at(placed, i).map(|c| (i, c)))
            .collect();
        let up = chords
            .iter()
            .find_map(|(_, c)| c.stem_up)
            .unwrap_or_else(|| mean_position(chords.iter().map(|(_, c)| *c)) < MIDDLE as f32);
        let beam_y = chords.iter().map(|(_, c)| free_end(c, up, top)).fold(
            if up { f32::MAX } else { f32::MIN },
            |a, b| {
                if up { a.min(b) } else { a.max(b) }
            },
        );
        for &(i, chord) in &chords {
            if let Some(p) = placed.get(i) {
                let mut stem = stem_for(chord, p.x, up, top, ctx);
                stem.to = beam_y;
                if let Some(slot) = stems.get_mut(i) {
                    *slot = Some(stem);
                }
                if let Some(flag) = beamed.get_mut(i) {
                    *flag = true;
                }
            }
        }
    }
    // The other chords get their own stem (halves and shorter).
    for (i, p) in placed.iter().enumerate() {
        if let NotationItem::Chord(chord) = p.item
            && chord.value >= 2
            && stems.get(i).is_some_and(Option::is_none)
        {
            let up = chord
                .stem_up
                .unwrap_or_else(|| mean_position(std::iter::once(chord)) < MIDDLE as f32);
            let mut stem = stem_for(chord, p.x, up, top, ctx);
            stem.to = free_end(chord, up, top);
            if let Some(slot) = stems.get_mut(i) {
                *slot = Some(stem);
            }
        }
    }

    for (i, p) in placed.iter().enumerate() {
        let stem = stems.get(i).copied().flatten();
        match p.item {
            NotationItem::Rest { glyph, dots } => draw_rest(p.x, *glyph, *dots, top, ctx, ops),
            NotationItem::Chord(chord) => {
                let up = stem.is_none_or(|s| s.up);
                draw_heads(p.x, chord, up, top, ctx, ops);
                if let Some(stem) = stem {
                    ops.push(DrawOp::Line {
                        from: Point::new(stem.x, stem.from),
                        to: Point::new(stem.x, stem.to),
                        thickness: ctx.fonts.music.engraving().stem_thickness,
                        color: ColorToken::Notation,
                    });
                    let unbeamed = !beamed.get(i).copied().unwrap_or(false);
                    if unbeamed && let Some(flag) = Flag::new(chord.beams(), stem.up) {
                        let half = ctx.fonts.music.engraving().stem_thickness / 2.0;
                        ops.push(DrawOp::Glyph {
                            glyph: Glyph::Flag(flag),
                            at: Point::new(stem.x - half, stem.to),
                            scale: 1.0,
                            color: ColorToken::Notation,
                        });
                    }
                }
            }
        }
    }
    for group in &groups {
        draw_beams(group, placed, &stems, ctx, ops);
    }
    draw_ties(placed, &stems, top, ctx, ops);
}

pub(super) fn chord_at<'m>(placed: &[BeamPlan<'m>], index: usize) -> Option<&'m Chord> {
    match placed.get(index)?.item {
        NotationItem::Chord(chord) => Some(chord),
        NotationItem::Rest { .. } => None,
    }
}

/// The average staff position of the chords' noteheads.
fn mean_position<'c>(chords: impl Iterator<Item = &'c Chord>) -> f32 {
    let (sum, count) = chords
        .flat_map(|c| c.heads.iter())
        .fold((0.0_f32, 0.0_f32), |(sum, count), h| {
            (sum + h.position as f32, count + 1.0)
        });
    if count > 0.0 {
        sum / count
    } else {
        MIDDLE as f32
    }
}

/// Where a chord's stem would end on its own: an octave beyond the outermost head, but at
/// least at the middle line.
fn free_end(chord: &Chord, up: bool, top: f32) -> f32 {
    let middle = y_of(top, MIDDLE);
    if up {
        let highest = chord.heads.last().map_or(MIDDLE, |h| h.position);
        (y_of(top, highest) - STEM_LENGTH).min(middle)
    } else {
        let lowest = chord.heads.first().map_or(MIDDLE, |h| h.position);
        (y_of(top, lowest) + STEM_LENGTH).max(middle)
    }
}

/// The stem's x and its end at the noteheads, from the notehead's stem anchors.
fn stem_for(chord: &Chord, x: f32, up: bool, top: f32, ctx: &Context<'_>) -> Stem {
    let font = &ctx.fonts.music;
    let glyph = chord
        .heads
        .first()
        .map_or(Glyph::NoteheadBlack, |h| chord.head_glyph(h));
    let width = font.advance(glyph);
    let left = x - width / 2.0;
    let half = font.engraving().stem_thickness / 2.0;
    if up {
        let (ax, ay) = font.anchor(glyph, "stemUpSE").unwrap_or((width, 0.168));
        let lowest = chord.heads.first().map_or(MIDDLE, |h| h.position);
        Stem {
            up,
            x: left + ax - half,
            from: y_of(top, lowest) - ay,
            to: 0.0,
        }
    } else {
        let (ax, ay) = font.anchor(glyph, "stemDownNW").unwrap_or((0.0, -0.168));
        let highest = chord.heads.last().map_or(MIDDLE, |h| h.position);
        Stem {
            up,
            x: left + ax + half,
            from: y_of(top, highest) - ay,
            to: 0.0,
        }
    }
}

/// Noteheads (seconds side by side), ledger lines, accidentals and dots.
fn draw_heads(x: f32, chord: &Chord, up: bool, top: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
    let font = &ctx.fonts.music;
    let engraving = font.engraving();
    // Which heads move beside the stem: in a second, the upper one with an up-stem, the
    // lower one with a down-stem.
    let mut shifted = vec![false; chord.heads.len()];
    let order: Vec<usize> = if up {
        (0..chord.heads.len()).collect()
    } else {
        (0..chord.heads.len()).rev().collect()
    };
    for pair in order.windows(2) {
        if let [a, b] = pair
            && let (Some(ha), Some(hb)) = (chord.heads.get(*a), chord.heads.get(*b))
            && ha.position.abs_diff(hb.position) == 1
            && !shifted.get(*a).copied().unwrap_or(false)
            && let Some(flag) = shifted.get_mut(*b)
        {
            *flag = true;
        }
    }
    for (index, head) in chord.heads.iter().enumerate() {
        let glyph = chord.head_glyph(head);
        let width = font.advance(glyph);
        let shift = if shifted.get(index).copied().unwrap_or(false) {
            if up { width } else { -width }
        } else {
            0.0
        };
        let left = x - width / 2.0 + shift;
        let y = y_of(top, head.position);
        // Ledger lines between the staff and the note.
        let ledgers: Vec<i32> = if head.position <= LEDGER_BELOW {
            (head.position..=LEDGER_BELOW)
                .filter(|p| p % 2 == 0)
                .collect()
        } else if head.position >= LEDGER_ABOVE {
            (LEDGER_ABOVE..=head.position)
                .filter(|p| p % 2 == 0)
                .collect()
        } else {
            Vec::new()
        };
        for position in ledgers {
            let ly = y_of(top, position);
            ops.push(DrawOp::Line {
                from: Point::new(left - engraving.leger_line_extension, ly),
                to: Point::new(left + width + engraving.leger_line_extension, ly),
                thickness: engraving.leger_line_thickness,
                color: ColorToken::Staff,
            });
        }
        ops.push(DrawOp::Glyph {
            glyph,
            at: Point::new(left, y),
            scale: 1.0,
            color: ColorToken::Notation,
        });
        if let Some(accidental) = head.accidental {
            // Left of the head, or of the column when the head moved right.
            let base = (x - width / 2.0).min(left);
            ops.push(DrawOp::Glyph {
                glyph: accidental,
                at: Point::new(base - ACCIDENTAL_GAP - font.advance(accidental), y),
                scale: 1.0,
                color: ColorToken::Notation,
            });
        }
        // Dots sit in a space: a note on a line gets its dot in the space above.
        let dot_y = if head.position % 2 == 0 {
            y_of(top, head.position.saturating_add(1))
        } else {
            y
        };
        let mut dot_x = x + width / 2.0 + shift.max(0.0) + DOT_GAP;
        for _ in 0..chord.dots {
            draw_dot(dot_x, dot_y, ctx, ops);
            dot_x += font.advance(Glyph::AugmentationDot) + DOT_GAP;
        }
    }
}

fn draw_dot(x: f32, y: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
    let dot = Glyph::AugmentationDot;
    ops.push(DrawOp::Glyph {
        glyph: dot,
        at: Point::new(x, y + ctx.fonts.music.bbox(dot).middle_above_origin()),
        scale: 1.0,
        color: ColorToken::Notation,
    });
}

/// A rest: the whole rest hangs from the fourth line, the half rest sits on the middle
/// line, the others are centered on the staff.
fn draw_rest(x: f32, glyph: Glyph, dots: u8, top: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
    let font = &ctx.fonts.music;
    let bbox = font.bbox(glyph);
    let y = match glyph {
        Glyph::RestWhole => y_of(top, MIDDLE + 2),
        Glyph::RestHalf => y_of(top, MIDDLE),
        _ => y_of(top, MIDDLE) + bbox.middle_above_origin(),
    };
    let left = x - (bbox.x_min + bbox.x_max) / 2.0;
    ops.push(DrawOp::Glyph {
        glyph,
        at: Point::new(left, y),
        scale: 1.0,
        color: ColorToken::Notation,
    });
    let mut dot_x = left + font.advance(glyph) + DOT_GAP;
    for _ in 0..dots {
        draw_dot(dot_x, y_of(top, MIDDLE + 1), ctx, ops);
        dot_x += font.advance(Glyph::AugmentationDot) + DOT_GAP;
    }
}
