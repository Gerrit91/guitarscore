//! A bar's jumps and codas over the system's top staff, as Guitar Pro writes them: the
//! signs the music jumps to (segno, coda) at the bar's start, what it does at the bar's end
//! ("Fine", "D.S. al Coda", "To Coda") right-aligned at its end.

use gscore_model::Direction;

use super::Placed;
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
use crate::glyph::Glyph;
use crate::score_mark::{ScoreMark, ScoreMarkKind};

/// How much taller the row is than a row of text: the coda sign reaches below its baseline.
pub(super) const EXTRA_HEIGHT: f32 = 0.8;
/// The signs' size against the font's: about the height of the row's text.
const SIGN_SCALE: f32 = 0.62;
/// Space between two signs, and between the bar's edges and what is written in it.
const GAP: f32 = 0.5;
/// The text's size: as prominent as a tempo mark (Guitar Pro writes jumps boldly).
const TEXT_SIZE: f32 = 1.4;
/// Space between two texts at a bar's end.
const TEXT_GAP: f32 = 0.6;
/// A coda sign after "To Coda": its scale against the signs at a bar's start.
const TO_CODA_SCALE: f32 = 0.8;

/// The signs a target is drawn with (a segno segno is two segnos).
fn target_glyphs(direction: Direction) -> &'static [Glyph] {
    match direction {
        Direction::Segno => &[Glyph::Segno],
        Direction::SegnoSegno => &[Glyph::Segno, Glyph::Segno],
        Direction::Coda => &[Glyph::Coda],
        Direction::DoubleCoda => &[Glyph::CodaSquare],
        _ => &[],
    }
}

/// What the score writes for a sign at the bar's end, and the sign that follows the text.
pub(crate) fn end_text(direction: Direction) -> Option<(&'static str, Option<Glyph>)> {
    let text = match direction {
        Direction::Fine => "Fine",
        Direction::DaCapo => "D.C.",
        Direction::DaCapoAlCoda => "D.C. al Coda",
        Direction::DaCapoAlDoubleCoda => "D.C. al Double Coda",
        Direction::DaCapoAlFine => "D.C. al Fine",
        Direction::DaSegno => "D.S.",
        Direction::DaSegnoAlCoda => "D.S. al Coda",
        Direction::DaSegnoAlDoubleCoda => "D.S. al Double Coda",
        Direction::DaSegnoAlFine => "D.S. al Fine",
        Direction::DaSegnoSegno => "D.S.S.",
        Direction::DaSegnoSegnoAlCoda => "D.S.S. al Coda",
        Direction::DaSegnoSegnoAlDoubleCoda => "D.S.S. al Double Coda",
        Direction::DaSegnoSegnoAlFine => "D.S.S. al Fine",
        Direction::DaCoda => return Some(("To Coda", Some(Glyph::Coda))),
        Direction::DaDoubleCoda => return Some(("To Double Coda", Some(Glyph::CodaSquare))),
        Direction::Coda | Direction::DoubleCoda | Direction::Segno | Direction::SegnoSegno => {
            return None;
        }
    };
    Some((text, None))
}

/// Draws the bar's signs on `baseline` and makes them clickable as one mark.
pub(super) fn draw(
    placed: Placed<'_>,
    baseline: f32,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
    clickable: &mut Vec<ScoreMark>,
) {
    let measure = placed.layout.index();
    let directions = &placed.marks.directions;
    let font = &ctx.fonts.music;
    let mut top = baseline - ctx.fonts.text.cap_height * TEXT_SIZE;
    let mut bottom = baseline + 0.3;
    // The targets, left to right from the bar's start.
    let start = placed.left + GAP;
    let mut x = start;
    for glyph in directions
        .iter()
        .flat_map(|&direction| target_glyphs(direction))
    {
        let bbox = font.bbox(*glyph);
        ops.push(DrawOp::Glyph {
            glyph: *glyph,
            at: Point::new(x - bbox.x_min * SIGN_SCALE, baseline),
            scale: SIGN_SCALE,
            color: ColorToken::Notation,
        });
        top = top.min(baseline - bbox.y_max * SIGN_SCALE);
        bottom = bottom.max(baseline - bbox.y_min * SIGN_SCALE);
        x += (bbox.x_max - bbox.x_min) * SIGN_SCALE + GAP;
    }
    if x > start {
        clickable.push(ScoreMark::new(
            ScoreMarkKind::Directions { measure },
            start,
            top,
            x - GAP,
            bottom,
        ));
    }
    // The texts, right to left from the bar's end (a fine before a D.C. al fine reads
    // "Fine  D.C. al Fine").
    let end = placed.right - GAP;
    let mut right = end;
    for (text, sign) in directions.iter().rev().filter_map(|&d| end_text(d)) {
        if let Some(glyph) = sign {
            let bbox = font.bbox(glyph);
            let scale = SIGN_SCALE * TO_CODA_SCALE;
            let width = (bbox.x_max - bbox.x_min) * scale;
            ops.push(DrawOp::Glyph {
                glyph,
                at: Point::new(right - width - bbox.x_min * scale, baseline),
                scale,
                color: ColorToken::Notation,
            });
            top = top.min(baseline - bbox.y_max * scale);
            bottom = bottom.max(baseline - bbox.y_min * scale);
            right -= width + GAP * 0.6;
        }
        ops.push(DrawOp::Text {
            text: text.to_owned(),
            at: Point::new(right, baseline),
            size: TEXT_SIZE,
            font: TextFont::Bold,
            anchor: TextAnchor::End,
            color: ColorToken::Notation,
        });
        // The metrics' average advance runs wide for lower case, about as much as bold
        // runs wider than regular: the estimate is used as it is.
        right -= ctx.fonts.text.width(text, TEXT_SIZE) + TEXT_GAP;
    }
    if right < end {
        clickable.push(ScoreMark::new(
            ScoreMarkKind::Directions { measure },
            right + GAP,
            top,
            end,
            bottom,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sign_is_either_drawn_at_the_start_or_written_at_the_end() {
        for direction in Direction::ALL {
            let at_start = !target_glyphs(direction).is_empty();
            let at_end = end_text(direction).is_some();
            assert!(at_start != at_end, "{direction:?}");
            // Fine is a target the music stops at, but it is written as text.
            assert_eq!(
                direction.is_target(),
                at_start || direction == Direction::Fine,
                "{direction:?}"
            );
        }
    }
}
