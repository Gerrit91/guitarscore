//! The text-like marks above a system: tempo marks, framed section names and ending
//! brackets.

use super::Ending;
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
use crate::glyph::Glyph;

/// Font size of section names.
const SECTION_SIZE: f32 = 1.5;
/// Space between a section name and its frame.
const SECTION_PAD: f32 = 0.35;
/// Thickness of the section frame and the ending brackets.
const FRAME_THICKNESS: f32 = 0.12;
/// Font size of tempo marks ("= 120") and ending numbers.
const TEMPO_SIZE: f32 = 1.3;
const ENDING_SIZE: f32 = 1.1;
/// Scale of the quarter note in a tempo mark, and the gap before "= 120".
const TEMPO_NOTE_SCALE: f32 = 0.8;
const TEMPO_NOTE_GAP: f32 = 0.4;
/// How far an ending bracket's hooks reach down.
const ENDING_HOOK: f32 = 1.2;

/// A quarter note and "= 120" with the note's left edge at `x`; returns its width.
pub(super) fn draw_tempo(
    x: f32,
    baseline: f32,
    tempo: u32,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) -> f32 {
    let note = Glyph::MetronomeQuarter;
    ops.push(DrawOp::Glyph {
        glyph: note,
        at: Point::new(x, baseline),
        scale: TEMPO_NOTE_SCALE,
        color: ColorToken::Annotation,
    });
    let text_x = x + ctx.fonts.music.advance(note) * TEMPO_NOTE_SCALE + TEMPO_NOTE_GAP;
    let text = format!("= {tempo}");
    // Bold runs a little wider than the regular face the metrics are for.
    let text_width = ctx.fonts.text.width(&text, TEMPO_SIZE) * 1.08;
    ops.push(DrawOp::Text {
        text,
        at: Point::new(text_x, baseline),
        size: TEMPO_SIZE,
        font: TextFont::Bold,
        anchor: TextAnchor::Start,
        color: ColorToken::Annotation,
    });
    text_x + text_width - x
}

/// A section name in bold, framed, starting at `x`.
pub(super) fn draw_section(
    x: f32,
    baseline: f32,
    title: &str,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    // Bold runs a little wider than the regular face the metrics are for.
    let width = ctx.fonts.text.width(title, SECTION_SIZE) * 1.08 + 2.0 * SECTION_PAD;
    let frame_top = baseline - SECTION_SIZE * 0.72 - SECTION_PAD;
    let frame_bottom = baseline + SECTION_PAD;
    let right = x + width;
    for (from, to) in [
        ((x, frame_top), (right, frame_top)),
        ((x, frame_bottom), (right, frame_bottom)),
        ((x, frame_top), (x, frame_bottom)),
        ((right, frame_top), (right, frame_bottom)),
    ] {
        ops.push(DrawOp::Line {
            from: Point::new(from.0, from.1),
            to: Point::new(to.0, to.1),
            thickness: FRAME_THICKNESS,
            color: ColorToken::Title,
        });
    }
    ops.push(DrawOp::Text {
        text: title.to_owned(),
        at: Point::new(x + SECTION_PAD, baseline),
        size: SECTION_SIZE,
        font: TextFont::Bold,
        anchor: TextAnchor::Start,
        color: ColorToken::Title,
    });
}

/// An ending bracket from `left` to `right`, its line at the top of the row.
pub(super) fn draw_ending(
    left: f32,
    right: f32,
    ending: &Ending,
    baseline: f32,
    ops: &mut Vec<DrawOp>,
) {
    let y = baseline - ENDING_HOOK;
    let (start, end) = (left + 0.2, right - 0.2);
    let mut line = |from: Point, to: Point| {
        ops.push(DrawOp::Line {
            from,
            to,
            thickness: FRAME_THICKNESS,
            color: ColorToken::Annotation,
        });
    };
    line(Point::new(start, y), Point::new(end, y));
    if ending.label.is_some() {
        line(Point::new(start, y), Point::new(start, y + ENDING_HOOK));
    }
    if ending.closed {
        line(Point::new(end, y), Point::new(end, y + ENDING_HOOK));
    }
    if let Some(label) = &ending.label {
        ops.push(DrawOp::Text {
            text: label.clone(),
            at: Point::new(start + 0.4, y + ENDING_SIZE),
            size: ENDING_SIZE,
            font: TextFont::Regular,
            anchor: TextAnchor::Start,
            color: ColorToken::Annotation,
        });
    }
}
