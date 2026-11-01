//! Beams and ties: what joins the notes of a measure.

use super::draw::{BeamPlan, Stem, chord_at, y_of};
use super::{Chord, NotationItem};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, Rect, TextAnchor, TextFont};

/// Length of a partial beam (a 16th next to an 8th, ...).
const STUB_LENGTH: f32 = 1.1;
/// Tie curves: distance from the notehead and height of the arch.
const TIE_OFFSET: f32 = 0.6;
const TIE_ARCH: f32 = 0.5;
/// Size of a tuplet number.
const TUPLET_SIZE: f32 = 1.3;

/// Runs of at least two neighbouring beamable chords (eighths and shorter) within one beat.
pub(super) fn beam_groups(placed: &[BeamPlan<'_>]) -> Vec<Vec<usize>> {
    let mut groups = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut current_beat = None;
    for (i, p) in placed.iter().enumerate() {
        let beamable = matches!(p.item, NotationItem::Chord(c) if c.beams() > 0);
        if beamable && current_beat == Some(p.beat) {
            current.push(i);
            continue;
        }
        if current.len() >= 2 {
            groups.push(std::mem::take(&mut current));
        }
        current.clear();
        current_beat = None;
        if beamable {
            current.push(i);
            current_beat = Some(p.beat);
        }
    }
    if current.len() >= 2 {
        groups.push(current);
    }
    groups
}

/// The beams of one group: full beams between neighbours that share a level, short
/// partial beams for a level only one note has, and the tuplet number.
pub(super) fn draw_beams(
    group: &[usize],
    placed: &[BeamPlan<'_>],
    stems: &[Option<Stem>],
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let engraving = ctx.fonts.music.engraving();
    let notes: Vec<(&Chord, Stem)> = group
        .iter()
        .filter_map(|&i| Some((chord_at(placed, i)?, stems.get(i).copied().flatten()?)))
        .collect();
    let Some(&(first_chord, first)) = notes.first() else {
        return;
    };
    let half = engraving.stem_thickness / 2.0;
    let levels = notes.iter().map(|(c, _)| c.beams()).max().unwrap_or(0);
    for level in 1..=levels {
        let offset = f32::from(level.saturating_sub(1))
            * (engraving.beam_thickness + engraving.beam_spacing);
        let y = if first.up {
            first.to + offset
        } else {
            first.to - offset - engraving.beam_thickness
        };
        let has = |i: usize| notes.get(i).is_some_and(|(c, _)| c.beams() >= level);
        for (i, (_, stem)) in notes.iter().enumerate() {
            if !has(i) {
                continue;
            }
            let next = i.saturating_add(1);
            let (from, to) = if has(next) {
                let Some((_, other)) = notes.get(next) else {
                    continue;
                };
                (stem.x - half, other.x + half)
            } else if i.checked_sub(1).is_some_and(has) {
                // Already joined to the previous note.
                continue;
            } else if next < notes.len() {
                (stem.x - half, stem.x + STUB_LENGTH)
            } else {
                (stem.x - STUB_LENGTH, stem.x + half)
            };
            ops.push(DrawOp::Rect {
                rect: Rect {
                    origin: Point::new(from, y),
                    width: to - from,
                    height: engraving.beam_thickness,
                },
                color: ColorToken::Notation,
            });
        }
    }
    if let (Some(n), Some(&(_, last))) = (first_chord.tuplet, notes.last()) {
        let y = if first.up {
            first.to - 0.6
        } else {
            first.to + 1.6
        };
        ops.push(DrawOp::Text {
            text: n.to_string(),
            at: Point::new((first.x + last.x) / 2.0, y),
            size: TUPLET_SIZE,
            font: TextFont::Regular,
            anchor: TextAnchor::Middle,
            color: ColorToken::Notation,
        });
    }
}

/// Tie curves from each tied notehead back to the same note in the previous chord.
pub(super) fn draw_ties(
    placed: &[BeamPlan<'_>],
    stems: &[Option<Stem>],
    top: f32,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let font = &ctx.fonts.music;
    let mut previous: Option<(usize, &Chord)> = None;
    for (i, p) in placed.iter().enumerate() {
        let NotationItem::Chord(chord) = p.item else {
            previous = None;
            continue;
        };
        if let Some((j, before)) = previous
            && let Some(origin) = placed.get(j)
        {
            let up = stems.get(i).copied().flatten().is_none_or(|s| s.up);
            let direction = if up { 1.0 } else { -1.0 };
            for head in chord.heads.iter().filter(|h| h.tied) {
                if !before.heads.iter().any(|h| h.position == head.position) {
                    continue;
                }
                let width = font.advance(chord.head_glyph(head));
                let from_x = origin.x + width / 2.0 + 0.1;
                let to_x = p.x - width / 2.0 - 0.1;
                if to_x <= from_x {
                    continue;
                }
                let y = y_of(top, head.position) + direction * TIE_OFFSET;
                let handle = (to_x - from_x) / 3.0;
                ops.push(DrawOp::Curve {
                    from: Point::new(from_x, y),
                    to: Point::new(to_x, y),
                    control1: Point::new(from_x + handle, y + direction * TIE_ARCH),
                    control2: Point::new(to_x - handle, y + direction * TIE_ARCH),
                    thickness: font.engraving().stem_thickness,
                    color: ColorToken::Notation,
                });
            }
        }
        previous = Some((i, chord));
    }
}
