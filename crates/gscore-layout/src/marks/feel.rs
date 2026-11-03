//! The feel mark above a measure whose feel changes: the written pair, "=", and how it is
//! played ("♫ = ⌐3¬♩♪" for a triplet feel), as Guitar Pro prints it.

use gscore_model::Feel;

use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
use crate::glyph::Glyph;

/// Scale of the beamed text notes and of the flagged metronome notes, chosen so both
/// stand about as tall as the tempo mark's quarter note.
const TEXT_NOTE_SCALE: f32 = 0.58;
const NOTE_SCALE: f32 = 0.85;
/// Font size of the "=".
const EQUALS_SIZE: f32 = 1.7;
/// Gap around the "=" and between the played notes.
const GAP: f32 = 0.5;
/// The tuplet bracket: how far above the baseline, its hooks, and the number's scale.
const BRACKET_RISE: f32 = 2.9;
const BRACKET_HOOK: f32 = 0.35;
const BRACKET_THICKNESS: f32 = 0.1;
const TUPLET_SCALE: f32 = 0.55;

/// How one played note is written: its glyph and whether it is dotted.
type Played = (Glyph, bool);

/// The notes the feel plays a written pair as, and whether they form a triplet.
fn played(feel: Feel) -> (Vec<Played>, bool) {
    use Glyph::{Metronome8th, Metronome16th, Metronome32nd, MetronomeQuarter};
    match feel {
        Feel::None => (Vec::new(), false),
        Feel::Eighth => (vec![(MetronomeQuarter, false), (Metronome8th, false)], true),
        Feel::Sixteenth => (vec![(Metronome8th, false), (Metronome16th, false)], true),
        Feel::DottedEighth => (vec![(Metronome8th, true), (Metronome16th, false)], false),
        Feel::DottedSixteenth => (vec![(Metronome16th, true), (Metronome32nd, false)], false),
        Feel::ScottishEighth => (vec![(Metronome16th, false), (Metronome8th, true)], false),
        Feel::ScottishSixteenth => (vec![(Metronome32nd, false), (Metronome16th, true)], false),
    }
}

/// Draws the mark for `feel` with its left edge at `x`; returns its width. A straight
/// feel (after a swung one) is written as the pair twice ("♫ = ♫").
pub(super) fn draw_feel(
    x: f32,
    baseline: f32,
    feel: Feel,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) -> f32 {
    let music = &ctx.fonts.music;
    let glyph = |glyph: Glyph, at: f32, scale: f32, ops: &mut Vec<DrawOp>| -> f32 {
        ops.push(DrawOp::Glyph {
            glyph,
            at: Point::new(at, baseline),
            scale,
            color: ColorToken::Annotation,
        });
        music.advance(glyph) * scale
    };
    let pair = |sixteenth: bool| {
        if sixteenth {
            [Glyph::TextNote, Glyph::TextBeam16th, Glyph::TextEnd16th]
        } else {
            [Glyph::TextNote, Glyph::TextBeam8th, Glyph::TextEnd8th]
        }
    };
    let mut cursor = x;
    for part in pair(feel.is_sixteenth()) {
        cursor += glyph(part, cursor, TEXT_NOTE_SCALE, ops);
    }
    cursor += GAP;
    ops.push(DrawOp::Text {
        text: "=".to_owned(),
        at: Point::new(cursor, baseline),
        size: EQUALS_SIZE,
        font: TextFont::Bold,
        anchor: TextAnchor::Start,
        color: ColorToken::Annotation,
    });
    cursor += ctx.fonts.text.width("=", EQUALS_SIZE) + GAP;
    let (notes, triplet) = played(feel);
    if notes.is_empty() {
        for part in pair(false) {
            cursor += glyph(part, cursor, TEXT_NOTE_SCALE, ops);
        }
        return cursor - x;
    }
    let start = cursor;
    for (index, (note, dotted)) in notes.into_iter().enumerate() {
        if index > 0 {
            cursor += GAP;
        }
        cursor += glyph(note, cursor, NOTE_SCALE, ops);
        if dotted {
            cursor += GAP * 0.4;
            cursor += glyph(Glyph::MetronomeDot, cursor, NOTE_SCALE, ops);
        }
    }
    if triplet {
        draw_triplet_bracket(start, cursor, baseline - BRACKET_RISE, ctx, ops);
    }
    cursor - x
}

/// A tuplet bracket from `left` to `right` at height `y`, with a 3 in a gap in the middle.
fn draw_triplet_bracket(left: f32, right: f32, y: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
    let number = Glyph::Tuplet3;
    let width = ctx.fonts.music.advance(number) * TUPLET_SCALE;
    let middle = (left + right) / 2.0;
    let gap = width / 2.0 + 0.15;
    let line = |from: (f32, f32), to: (f32, f32), ops: &mut Vec<DrawOp>| {
        ops.push(DrawOp::Line {
            from: Point::new(from.0, from.1),
            to: Point::new(to.0, to.1),
            thickness: BRACKET_THICKNESS,
            color: ColorToken::Annotation,
        });
    };
    line((left, y + BRACKET_HOOK), (left, y), ops);
    line((left, y), (middle - gap, y), ops);
    line((middle + gap, y), (right, y), ops);
    line((right, y), (right, y + BRACKET_HOOK), ops);
    ops.push(DrawOp::Glyph {
        glyph: number,
        at: Point::new(middle - width / 2.0, y + 0.3),
        scale: TUPLET_SCALE,
        color: ColorToken::Annotation,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_swung_feel_plays_two_notes() {
        for feel in Feel::ALL {
            let (notes, triplet) = played(feel);
            assert_eq!(notes.len(), if feel == Feel::None { 0 } else { 2 });
            assert_eq!(triplet, matches!(feel, Feel::Eighth | Feel::Sixteenth));
        }
    }
}
