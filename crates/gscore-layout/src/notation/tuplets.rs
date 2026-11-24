//! Tuplet brackets and labels: a group of notes (and rests) that share a tuplet is written
//! with a number over its beam, or with a bracketed `n:m` when the group is not one beam.

use super::draw::{BeamPlan, Stem, y_of};
use super::{NotationItem, Tuplet};
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
use crate::glyph::Glyph;

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

/// The written length of an item (its value and dots, without the tuplet's ratio), as a
/// share of a whole note.
fn written_length(item: &NotationItem) -> f64 {
    let (value, dots) = match item {
        NotationItem::Chord(chord) => (u32::from(chord.value), chord.dots),
        NotationItem::Rest { glyph, dots, .. } => {
            let value = match glyph {
                Glyph::RestWhole => 1,
                Glyph::RestHalf => 2,
                Glyph::RestQuarter => 4,
                Glyph::Rest8th => 8,
                Glyph::Rest16th => 16,
                Glyph::Rest32nd => 32,
                Glyph::Rest64th => 64,
                _ => 128,
            };
            (value, *dots)
        }
    };
    let base = 1.0 / f64::from(value.max(1));
    // Each dot adds half of what the one before it added.
    base * (2.0 - 0.5_f64.powi(i32::from(dots)))
}

/// Splits a sequence of items into tuplet groups, given each item's tuplet (`None` for a
/// normal item) and its written length (a share of a whole note). A group is a run of
/// items with the same tuplet, and it ends where the run is broken (by a normal item or
/// another tuplet) or where it is complete: when its written lengths add up to `enters`
/// of one of its notes (three sixteenths for a sixteenth triplet), so four sixteenth
/// triplets in a row are four groups, not one. Returns each group's items.
pub(crate) fn tuplet_groups(items: &[(Option<(u8, u8)>, f64)]) -> Vec<(Vec<usize>, (u8, u8))> {
    const EPSILON: f64 = 1e-9;
    let mut groups: Vec<(Vec<usize>, (u8, u8))> = Vec::new();
    let mut open = false;
    let mut sum = 0.0_f64;
    let mut units: Vec<f64> = Vec::new();
    for (index, &(tuplet, length)) in items.iter().enumerate() {
        let Some(tuplet) = tuplet else {
            open = false;
            continue;
        };
        match groups.last_mut() {
            Some((indices, current)) if open && *current == tuplet => indices.push(index),
            _ => {
                groups.push((vec![index], tuplet));
                sum = 0.0;
                units.clear();
            }
        }
        open = true;
        sum += length;
        units.push(length);
        let whole = units
            .iter()
            .any(|unit| (sum - unit * f64::from(tuplet.0)).abs() < EPSILON);
        if whole {
            open = false;
        }
    }
    groups
}

/// The tuplet groups of a staff's items (see [`tuplet_groups`]).
fn groups(placed: &[BeamPlan<'_>]) -> Vec<Group> {
    let items: Vec<(Option<(u8, u8)>, f64)> = placed
        .iter()
        .map(|p| {
            (
                item_tuplet(p.item).map(|tuplet| (tuplet.enters, tuplet.times)),
                written_length(p.item),
            )
        })
        .collect();
    tuplet_groups(&items)
        .into_iter()
        .map(|(indices, (enters, times))| Group {
            indices,
            tuplet: Tuplet { enters, times },
        })
        .collect()
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
            draw_bracket(&group, placed, stems, top, &label, ops);
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
    stems: &[Option<Stem>],
    top: f32,
    label: &str,
    ops: &mut Vec<DrawOp>,
) {
    let (Some(first), Some(last)) = (placed.get(group.start()), placed.get(group.end())) else {
        return;
    };
    // The bracket sits a fixed distance above the staff unless a note (or the end of an
    // up-stem) is higher.
    let highest = group
        .indices
        .iter()
        .filter_map(|&index| {
            let p = placed.get(index)?;
            let head = match p.item {
                NotationItem::Chord(chord) => {
                    chord.heads.last().map(|head| y_of(top, head.position))
                }
                NotationItem::Rest { .. } => None,
            }?;
            let stem = stems
                .get(index)
                .copied()
                .flatten()
                .filter(|stem| stem.up)
                .map_or(head, |stem| stem.to.min(head));
            Some(stem)
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

    /// Four sixteenth triplets in a row are four groups; a normal note between two
    /// triplets keeps them apart; a triplet of an eighth and a sixteenth is whole too.
    #[test]
    fn a_tuplet_group_ends_when_it_is_whole_or_interrupted() {
        let triplet = Some((3, 2));
        let sixteenth = 1.0 / 16.0;
        let groups = |items: &[(Option<(u8, u8)>, f64)]| -> Vec<Vec<usize>> {
            tuplet_groups(items)
                .into_iter()
                .map(|(indices, _)| indices)
                .collect()
        };
        let twelve = vec![(triplet, sixteenth); 12];
        assert_eq!(
            groups(&twelve),
            vec![vec![0, 1, 2], vec![3, 4, 5], vec![6, 7, 8], vec![9, 10, 11]]
        );
        // Bar 78 of the owner's solo: quarter, triplet, quarter, triplet.
        let quarter = (None, 0.25);
        let interrupted = [
            quarter,
            (triplet, sixteenth),
            (triplet, sixteenth),
            (triplet, sixteenth),
            quarter,
            (triplet, sixteenth),
            (triplet, sixteenth),
            (triplet, sixteenth),
        ];
        assert_eq!(groups(&interrupted), vec![vec![1, 2, 3], vec![5, 6, 7]]);
        // An eighth and a sixteenth: three sixteenths' worth.
        let swung = [
            (triplet, 0.125),
            (triplet, sixteenth),
            (triplet, 0.125),
            (triplet, sixteenth),
        ];
        assert_eq!(groups(&swung), vec![vec![0, 1], vec![2, 3]]);
        // An unfinished group stays one group.
        assert_eq!(
            groups(&[(triplet, sixteenth), (triplet, sixteenth)]),
            vec![vec![0, 1]]
        );
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
