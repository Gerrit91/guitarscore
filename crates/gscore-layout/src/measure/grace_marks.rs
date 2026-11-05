//! The H/P of a hammer-on grace note and its slur, on the notation staff and the tab.

use super::graces::{Grace, tab_width};
use super::{Column, Item, Tops};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
use crate::notation::{self, NotationItem};

/// The H (hammer-on) or P (pull-off) of a grace note with a hammer transition, with a slur
/// from the grace note to its main note on both staves.
pub(super) fn draw_hammer(
    center: f32,
    main_x: f32,
    tops: Tops,
    column: &Column,
    notes: &[&Grace],
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let Some(grace) = notes.iter().find(|grace| grace.hammer) else {
        return;
    };
    let Some(line) = grace.line else {
        return;
    };
    let main = column.items.iter().find_map(|item| match item {
        Item::Fret(fret) if fret.line == line && !fret.tie_destination => Some(fret),
        _ => None,
    });
    let (Some(grace_fret), Some(main)) = (grace.fret, main) else {
        return;
    };
    let Some(main_fret) = main.fret else {
        return;
    };
    let letter = if main_fret >= grace_fret { "H" } else { "P" };
    let label = |y: f32, ops: &mut Vec<DrawOp>| {
        ops.push(DrawOp::Text {
            text: letter.to_owned(),
            at: Point::new(center, y),
            size: 1.0,
            font: TextFont::Regular,
            anchor: TextAnchor::Middle,
            color: ColorToken::Notation,
        });
    };
    if let Some(top) = tops.notation {
        label(top - 2.2, ops);
        // The slur dips from the grace head to the main note's head.
        let grace_position = notes.iter().find_map(|grace| grace.position);
        let main_position = column
            .notation
            .iter()
            .flatten()
            .find_map(|item| match item {
                NotationItem::Chord(chord) => chord
                    .heads
                    .iter()
                    .find(|head| head.string == u8::try_from(line.saturating_add(1)).ok())
                    .map(|head| head.position),
                NotationItem::Rest { .. } => None,
            });
        if let (Some(grace_position), Some(main_position)) = (grace_position, main_position) {
            draw_slur(
                center,
                notation::y_of(top, grace_position),
                main_x,
                notation::y_of(top, main_position),
                0.5,
                ops,
            );
        }
    }
    if tops.show_tab {
        label(tops.tab - 2.2, ops);
        let y = tops.tab + ctx.staff.line_y(line);
        let grace_half = tab_width(grace, ctx) / 2.0;
        draw_slur(
            center + grace_half,
            y,
            main_x - main.half_width(),
            y,
            -0.5,
            ops,
        );
    }
}

/// A small slur between two points, as the grace note's hammer-on uses it.
fn draw_slur(from_x: f32, from_y: f32, to_x: f32, to_y: f32, curve: f32, ops: &mut Vec<DrawOp>) {
    let distance = to_x - from_x;
    if distance <= 0.0 {
        return;
    }
    let handle = distance / 3.0;
    ops.push(DrawOp::Curve {
        from: Point::new(from_x, from_y),
        to: Point::new(to_x, to_y),
        control1: Point::new(from_x + handle, from_y + curve),
        control2: Point::new(to_x - handle, to_y + curve),
        thickness: 0.15,
        color: ColorToken::Notation,
    });
}
