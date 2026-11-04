//! Tuplet brackets and labels: a group of notes (and rests) that share a tuplet is written
//! with a number over its beam, or with a bracketed `n:m` when the group is not one beam.

use super::draw::{BeamPlan, Stem, y_of};
use super::{NotationItem, Tuplet};
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};

/// Size of a tuplet label.
const LABEL_SIZE: f32 = 1.3;
/// How far above the notes a bracket runs, and how long its end hooks are.
const BRACKET_RISE: f32 = 1.0;
const HOOK: f32 = 0.7;
/// How far the label sits above its bracket.
const LABEL_RISE: f32 = 0.4;
/// Bracket line thickness.
const THICKNESS: f32 = 0.15;

/// One run of neighbouring items that share a tuplet.
struct Group {
    indices: Vec<usize>,
    tuplet: Tuplet,
}

impl Group {
    fn start(&self) -> usize {
        self.indices.first().copied().unwrap_or(0)
    }

    fn end(&self) -> usize {
        self.indices.last().copied().unwrap_or(0)
    }
}

fn item_tuplet(item: &NotationItem) -> Option<Tuplet> {
    match item {
        NotationItem::Chord(chord) => chord.tuplet,
        NotationItem::Rest { tuplet, .. } => *tuplet,
    }
}

/// Consecutive items with the same tuplet.
fn groups(placed: &[BeamPlan<'_>]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (index, p) in placed.iter().enumerate() {
        let Some(tuplet) = item_tuplet(p.item) else {
            continue;
        };
        match groups.last_mut() {
            Some(group) if group.tuplet == tuplet => group.indices.push(index),
            _ => groups.push(Group {
                indices: vec![index],
                tuplet,
            }),
        }
    }
    groups
}

/// Draws each tuplet group's number over its beam, or a bracketed label when the group is
/// not exactly one beam group.
pub(super) fn draw_tuplets(
    placed: &[BeamPlan<'_>],
    stems: &[Option<Stem>],
    beams: &[Vec<usize>],
    top: f32,
    ops: &mut Vec<DrawOp>,
) {
    for group in groups(placed) {
        let beamed = beams
            .iter()
            .any(|beam| beam.as_slice() == group.indices.as_slice());
        let label = tuplet_label(group.tuplet);
        if beamed {
            draw_beam_label(&group, placed, stems, &label, ops);
        } else {
            draw_bracket(&group, placed, top, &label, ops);
        }
    }
}

/// The number over a beamed tuplet, just beyond the beam's free end.
fn draw_beam_label(
    group: &Group,
    placed: &[BeamPlan<'_>],
    stems: &[Option<Stem>],
    label: &str,
    ops: &mut Vec<DrawOp>,
) {
    let Some(stem) = stems.get(group.start()).copied().flatten() else {
        return;
    };
    let Some(first) = placed.get(group.start()) else {
        return;
    };
    let Some(last) = placed.get(group.end()) else {
        return;
    };
    let y = if stem.up {
        stem.to - 0.6
    } else {
        stem.to + 1.6
    };
    ops.push(DrawOp::Text {
        text: label.to_owned(),
        at: Point::new((first.x + last.x) / 2.0, y),
        size: LABEL_SIZE,
        font: TextFont::Regular,
        anchor: TextAnchor::Middle,
        color: ColorToken::Notation,
    });
}

/// A bracket over a tuplet group that the beams do not already label.
fn draw_bracket(
    group: &Group,
    placed: &[BeamPlan<'_>],
    top: f32,
    label: &str,
    ops: &mut Vec<DrawOp>,
) {
    let (Some(first), Some(last)) = (placed.get(group.start()), placed.get(group.end())) else {
        return;
    };
    // The bracket sits a fixed distance above the staff unless a note is higher.
    let highest = group
        .indices
        .iter()
        .filter_map(|&index| {
            let p = placed.get(index)?;
            match p.item {
                NotationItem::Chord(chord) => {
                    chord.heads.last().map(|head| y_of(top, head.position))
                }
                NotationItem::Rest { .. } => None,
            }
        })
        .fold(f32::MAX, f32::min);
    let y = if highest == f32::MAX {
        top - 2.5
    } else {
        (highest - BRACKET_RISE).min(top - 2.5)
    };
    let (start, end) = (first.x - 0.25, last.x + 0.25);
    ops.push(DrawOp::Line {
        from: Point::new(start, y),
        to: Point::new(end, y),
        thickness: THICKNESS,
        color: ColorToken::Notation,
    });
    for x in [start, end] {
        ops.push(DrawOp::Line {
            from: Point::new(x, y),
            to: Point::new(x, y + HOOK),
            thickness: THICKNESS,
            color: ColorToken::Notation,
        });
    }
    ops.push(DrawOp::Text {
        text: label.to_owned(),
        at: Point::new((start + end) / 2.0, y - LABEL_RISE),
        size: LABEL_SIZE,
        font: TextFont::Regular,
        anchor: TextAnchor::Middle,
        color: ColorToken::Notation,
    });
}

/// The largest power of two below `n` (1 for 2 and 3, 4 for 5 to 7, ...).
fn power_of_two_below(n: u8) -> u8 {
    let mut power = 1_u8;
    while let Some(next) = power.checked_mul(2)
        && next < n
    {
        power = next;
    }
    power
}

/// `n` when the tuplet takes the time of the largest power of two below `n` (a triplet in
/// the time of two), else `n:m` (11 in the time of 4).
pub(super) fn tuplet_label(tuplet: Tuplet) -> String {
    if tuplet.times == power_of_two_below(tuplet.enters) {
        tuplet.enters.to_string()
    } else {
        format!("{}:{}", tuplet.enters, tuplet.times)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(enters: u8, times: u8) -> String {
        tuplet_label(Tuplet { enters, times })
    }

    #[test]
    fn ordinary_tuplets_show_only_their_count() {
        assert_eq!(label(3, 2), "3");
        assert_eq!(label(6, 4), "6");
        assert_eq!(label(5, 4), "5");
        assert_eq!(label(7, 4), "7");
        assert_eq!(label(9, 8), "9");
    }

    #[test]
    fn unusual_tuplets_show_both_numbers() {
        assert_eq!(label(11, 4), "11:4");
        assert_eq!(label(2, 3), "2:3");
        assert_eq!(label(10, 8), "10");
        assert_eq!(label(3, 4), "3:4");
    }
}
