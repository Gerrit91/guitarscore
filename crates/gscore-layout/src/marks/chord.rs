//! Chord diagrams above the staff, as Guitar Pro draws them: the chord's name, a marker
//! row (× for a string not played, ○ for an open one), the fretboard grid with a dot on
//! each fretted string, and the starting fret beside a grid that does not begin at the nut.

use gscore_model::ChordDiagram;

use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};

/// Distance between two strings of the grid.
const STRING_GAP: f32 = 0.75;
/// Distance between two frets of the grid.
const FRET_GAP: f32 = 0.85;
/// The radius of a fretted string's dot.
const DOT_RADIUS: f32 = 0.27;
/// The size of an open string's circle and a muted string's cross (half their width).
const MARKER_HALF: f32 = 0.2;
/// The marker row above the grid.
const MARKER_ROW: f32 = 0.65;
/// The chord name's size, and the gap from its baseline down to the marker row.
const NAME_SIZE: f32 = 1.35;
const NAME_GAP: f32 = 0.35;
/// Grid lines, and the nut's thick line.
const LINE: f32 = 0.08;
const NUT: f32 = 0.24;
/// Room under the grid, down to the row's bottom.
const BELOW: f32 = 0.6;
/// Frets drawn when the diagram has none (an invalid file).
const MIN_FRETS: usize = 4;

/// The height of the chord diagrams' row: name, markers, a five-fret grid and the gap below.
pub(super) const ROW: f32 = NAME_SIZE + NAME_GAP + MARKER_ROW + 5.0 * FRET_GAP + BELOW;

/// Draws `diagram` centered on `x`, its row ending at `bottom`.
pub(super) fn draw(x: f32, bottom: f32, diagram: &ChordDiagram, ops: &mut Vec<DrawOp>) {
    let strings = usize::from(diagram.string_count.max(1));
    let frets = usize::from(diagram.fret_count).max(MIN_FRETS);
    let width = strings.saturating_sub(1) as f32 * STRING_GAP;
    let left = x - width / 2.0;
    let grid_bottom = bottom - BELOW;
    // A grid of other than five frets stays on the same baseline, so names line up.
    let grid_top = grid_bottom - frets as f32 * FRET_GAP;
    let marker_y = grid_top - MARKER_ROW / 2.0 - 0.05;
    let name_y = grid_top - MARKER_ROW - NAME_GAP;
    let line = |from: Point, to: Point, thickness: f32| DrawOp::Line {
        from,
        to,
        thickness,
        color: ColorToken::Notation,
    };
    if !diagram.name.is_empty() {
        ops.push(DrawOp::Text {
            text: diagram.name.clone(),
            at: Point::new(x, name_y),
            size: NAME_SIZE,
            font: TextFont::Bold,
            anchor: TextAnchor::Middle,
            color: ColorToken::Notation,
        });
    }
    let string_x = |string: usize| left + string as f32 * STRING_GAP;
    for string in 0..strings {
        let sx = string_x(string);
        ops.push(line(
            Point::new(sx, grid_top),
            Point::new(sx, grid_bottom),
            LINE,
        ));
    }
    let at_nut = diagram.base_fret <= 1;
    for fret in 0..=frets {
        let fy = grid_top + fret as f32 * FRET_GAP;
        let thickness = if fret == 0 && at_nut { NUT } else { LINE };
        // The nut grows upwards, so the first fret space keeps its size.
        let fy = if fret == 0 && at_nut {
            fy - (NUT - LINE) / 2.0
        } else {
            fy
        };
        ops.push(line(
            Point::new(left - LINE / 2.0, fy),
            Point::new(left + width + LINE / 2.0, fy),
            thickness,
        ));
    }
    if !at_nut {
        ops.push(DrawOp::Text {
            text: format!("{}fr", diagram.base_fret),
            at: Point::new(left + width + 0.35, grid_top + FRET_GAP * 0.8),
            size: 0.85,
            font: TextFont::Regular,
            anchor: TextAnchor::Start,
            color: ColorToken::Notation,
        });
    }
    let first = i32::from(diagram.base_fret.max(1));
    for string in 0..strings {
        let sx = string_x(string);
        let fret = diagram
            .frets
            .iter()
            .find(|(number, _)| usize::from(*number) == string)
            .map(|&(_, fret)| fret);
        match fret {
            // A string the diagram leaves out is not played, as in Guitar Pro.
            None | Some(..0) => {
                let half = MARKER_HALF;
                for (dx, dy) in [(half, half), (half, -half)] {
                    ops.push(line(
                        Point::new(sx - dx, marker_y - dy),
                        Point::new(sx + dx, marker_y + dy),
                        LINE,
                    ));
                }
            }
            Some(0) => circle_outline(sx, marker_y, MARKER_HALF, ops),
            Some(fret) => {
                let row = i32::from(fret).saturating_sub(first);
                if row < 0 || row >= i32::try_from(frets).unwrap_or(i32::MAX) {
                    continue;
                }
                let fy = grid_top + (row as f32 + 0.5) * FRET_GAP;
                ops.push(DrawOp::Polygon {
                    points: circle(sx, fy, DOT_RADIUS),
                    color: ColorToken::Notation,
                });
            }
        }
    }
}

/// The corners of a circle around (`x`, `y`), fine enough to look round at any zoom.
fn circle(x: f32, y: f32, radius: f32) -> Vec<Point> {
    const SIDES: usize = 20;
    (0..SIDES)
        .map(|side| {
            let angle = std::f32::consts::TAU * side as f32 / SIDES as f32;
            Point::new(x + radius * angle.cos(), y + radius * angle.sin())
        })
        .collect()
}

/// An open string's small circle.
fn circle_outline(x: f32, y: f32, radius: f32, ops: &mut Vec<DrawOp>) {
    let points = circle(x, y, radius);
    let Some(&start) = points.first() else {
        return;
    };
    for (index, &from) in points.iter().enumerate() {
        let to = points
            .get(index.saturating_add(1))
            .copied()
            .unwrap_or(start);
        ops.push(DrawOp::Line {
            from,
            to,
            thickness: LINE,
            color: ColorToken::Notation,
        });
    }
}
