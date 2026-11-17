//! The rhythm under a tab without a notation staff, as Guitar Pro writes it: a stem down
//! from just under the beat's lowest fret number (across the staff lines) to below the
//! tab, beams joining the eighths and shorter of one beat, flags on the others, and dots.
//! Half notes take a short stem below the tab, whole notes none; rests are written in the
//! tab itself.

use super::NotationItem;
use super::beams::{beam_groups, draw_beams};
use super::draw::{BeamPlan, Stem, chord_at};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point};
use crate::glyph::{Flag, Glyph};

/// Gap between the tab's bottom line and the stems' top (clear of a low fret number).
const STEM_GAP: f32 = 1.0;
/// A stem's length, from its top to the beam or flag; a flag's stem is longer when the
/// flag needs it.
const STEM_LENGTH: f32 = 2.5;
/// Room between a flag's top and its stem's top.
const FLAG_CLEARANCE: f32 = 0.3;
/// Room below the beams and flags before the rows under the tab.
const BELOW: f32 = 0.3;
/// How far right of the stem a dot sits, and how far above the stem's end.
const DOT_OFFSET: (f32, f32) = (0.55, 0.9);
/// Gap between two dots.
const DOT_GAP: f32 = 0.45;

/// The stems of `placed` under a tab whose bottom line is at `bottom`, and whether each
/// item is beamed (an unbeamed eighth or shorter has a flag). `reach` is where each
/// item's stem starts at the top (under its lowest fret number), if it reaches into the
/// tab.
fn stems(
    placed: &[BeamPlan<'_>],
    reach: &[Option<f32>],
    bottom: f32,
    ctx: &Context<'_>,
) -> (Vec<Option<Stem>>, Vec<bool>, Vec<Vec<usize>>) {
    let from = bottom + STEM_GAP;
    let groups = beam_groups(placed);
    let beamed: Vec<bool> = (0..placed.len())
        .map(|i| groups.iter().any(|group| group.contains(&i)))
        .collect();
    let stems = placed
        .iter()
        .zip(&beamed)
        .enumerate()
        .map(|(i, (p, &beamed))| match p.item {
            NotationItem::Chord(chord) if chord.value >= 2 => {
                let flag = Flag::new(chord.beams(), false).filter(|_| !beamed);
                let length = flag.map_or(STEM_LENGTH, |flag| {
                    let rise = ctx.fonts.music.bbox(Glyph::Flag(flag)).y_max;
                    STEM_LENGTH.max(rise + FLAG_CLEARANCE)
                });
                Some(Stem {
                    up: false,
                    x: p.x,
                    // A half note's stem is half as long, below the tab and ending with
                    // the others; the others reach up to their lowest fret number.
                    from: if chord.value == 2 {
                        from + STEM_LENGTH / 2.0
                    } else {
                        reach
                            .get(i)
                            .copied()
                            .flatten()
                            .map_or(from, |y| y.min(from))
                    },
                    to: from + length,
                })
            }
            _ => None,
        })
        .collect();
    (stems, beamed, groups)
}

/// How far the rhythm of `placed` reaches below the tab's bottom line: what the rows
/// under the tab (tuplets, P.M., let ring, dynamics) move down by. The same for every
/// measure unless a long flag needs more, so the rows line up.
pub(crate) fn rhythm_depth(placed: &[BeamPlan<'_>], ctx: &Context<'_>) -> f32 {
    let (stems, _, _) = stems(placed, &[], 0.0, ctx);
    stems
        .iter()
        .flatten()
        .map(|stem| stem.to)
        .fold(STEM_GAP + STEM_LENGTH, f32::max)
        + BELOW
}

/// Draws the rhythm of `placed` (one voice's items at their columns' x) under the tab
/// staff whose bottom line is at `bottom`; `reach` is where each stem starts (just under
/// its lowest fret number), if anywhere above the plain stem's top.
pub(crate) fn draw_tab_rhythm(
    placed: &[BeamPlan<'_>],
    reach: &[Option<f32>],
    bottom: f32,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let (stems, beamed, groups) = stems(placed, reach, bottom, ctx);
    let engraving = ctx.fonts.music.engraving();
    let half = engraving.stem_thickness / 2.0;
    for (i, stem) in stems.iter().enumerate() {
        let Some(stem) = stem else {
            continue;
        };
        ops.push(DrawOp::Line {
            from: Point::new(stem.x, stem.from),
            to: Point::new(stem.x, stem.to),
            thickness: engraving.stem_thickness,
            color: ColorToken::Notation,
        });
        let Some(chord) = chord_at(placed, i) else {
            continue;
        };
        // A dot goes right of the stem, past a flag.
        let mut dot_x = stem.x + DOT_OFFSET.0;
        if !beamed.get(i).copied().unwrap_or(false)
            && let Some(flag) = Flag::new(chord.beams(), false)
        {
            let glyph = Glyph::Flag(flag);
            ops.push(DrawOp::Glyph {
                glyph,
                at: Point::new(stem.x - half, stem.to),
                scale: 1.0,
                color: ColorToken::Notation,
            });
            dot_x += ctx.fonts.music.advance(glyph);
        }
        let dot = Glyph::AugmentationDot;
        let dot_y = stem.to - DOT_OFFSET.1 + ctx.fonts.music.bbox(dot).middle_above_origin();
        for n in 0..chord.dots {
            ops.push(DrawOp::Glyph {
                glyph: dot,
                at: Point::new(dot_x + f32::from(n) * DOT_GAP, dot_y),
                scale: 1.0,
                color: ColorToken::Notation,
            });
        }
    }
    for group in &groups {
        draw_beams(group, placed, &stems, ctx, ops);
    }
}
