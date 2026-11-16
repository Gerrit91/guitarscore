//! Multi-bar rests: a run of bars where every shown staff rests is written as one bar with
//! a thick horizontal bar across its middle line and the number of bars above, as parts
//! are printed.
//!
//! The run's first bar leads it: it keeps its signatures and its number and takes the
//! width of one wide bar. The bars after it are folded into it: they take no room and draw
//! nothing but their barline, which falls on the lead's end (so a closing double bar or
//! repeat still shows). The score decides which bars form a run (`score::rests`).

use super::{Column, MeasureLayout, Tops};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, Rect};
use crate::glyph::{Digit, Glyph};
use crate::notation;
use crate::ornaments::ColumnMarks;

/// The width a multi-bar rest's content takes at a natural stretch.
const RUN_SPACE: f32 = 12.0;
/// The least it is squeezed to.
const RUN_FLOOR: f32 = 8.0;
/// Room between the bar's ends and the thick bar.
const BAR_INSET: f32 = 1.2;
/// The thick bar's height.
const BAR_THICKNESS: f32 = 0.9;
/// How far the strokes at its ends reach above and below the middle line.
const SERIF_REACH: f32 = 1.0;
/// The strokes' thickness.
const SERIF_THICKNESS: f32 = 0.18;
/// Room between the count's digits and the topmost staff line.
const COUNT_GAP: f32 = 0.6;
/// Half a time-signature digit's height: the digits are centered on their origin.
const DIGIT_HALF: f32 = 1.0;

/// A measure's part in a multi-bar rest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum RestRun {
    /// Not in one: the measure shows its own rests.
    #[default]
    None,
    /// The first bar of a run of this many bars: it is drawn as the multi-bar rest.
    Leads(u32),
    /// A later bar of a run: no room, nothing drawn but its barline.
    Folded,
}

impl MeasureLayout {
    /// Its part in a multi-bar rest.
    pub(crate) fn rest_run(&self) -> RestRun {
        self.rest_run
    }

    /// Whether the bar starts with a clef, key or time signature of its own.
    pub(crate) fn shows_signature_change(&self) -> bool {
        self.clef_change || self.key_signature.is_some() || self.time_signature.is_some()
    }

    /// This measure as the first bar of a multi-bar rest of `count` bars: its signatures,
    /// then one wide column where every beat of the run's first bar sits (for the cursor).
    pub(crate) fn leading_rest_run(&self, count: u32) -> Self {
        MeasureLayout {
            columns: vec![self.blank_column(RUN_SPACE, RUN_FLOOR)],
            tail_spacing: 0.0,
            rest_run: RestRun::Leads(count),
            ..self.clone()
        }
    }

    /// This measure folded into the multi-bar rest before it: no signatures, no room.
    pub(crate) fn folded_into_rest_run(&self) -> Self {
        MeasureLayout {
            columns: vec![self.blank_column(0.0, 0.0)],
            clef_change: false,
            clef_width: 0.0,
            key_signature: None,
            time_signature: None,
            signature_pad: 0.0,
            lead: 0.0,
            tail_spacing: 0.0,
            rest_run: RestRun::Folded,
            ..self.clone()
        }
    }

    /// One column without anything drawn in it, `space` wide (squeezed to `floor`), that
    /// holds every beat of the measure.
    fn blank_column(&self, space: f32, floor: f32) -> Column {
        let first = self.columns.first();
        Column {
            onset: first.map_or(self.end, |column| column.onset),
            space,
            floor,
            items: Vec::new(),
            marks: ColumnMarks::default(),
            notation: Vec::new(),
            slash: Vec::new(),
            notation_offset: 0.0,
            graces: Vec::new(),
            grace_width: 0.0,
            main_left: 0.0,
            beat: 0,
            tuplet: None,
            octave: None,
            chord: None,
            beats: self
                .columns
                .iter()
                .flat_map(|column| column.beats.iter().copied())
                .collect(),
            text: None,
            spacing: 0.0,
            dynamic: None,
            tempo: None,
        }
    }

    /// Draws a leading bar's thick bar on every shown staff and the number of bars above
    /// the topmost, between its signatures (the measure starting at `x`) and its end.
    pub(super) fn draw_rest_run(
        &self,
        count: u32,
        x: f32,
        stretch: f32,
        tops: Tops,
        ctx: &Context<'_>,
        ops: &mut Vec<DrawOp>,
    ) {
        let start = x + self.fixed_width();
        let end = self.width(stretch) + x;
        let (left, right) = (start + BAR_INSET, end - BAR_INSET);
        if right <= left {
            return;
        }
        let notation_middle = notation::staff().middle();
        let mut middles = Vec::with_capacity(3);
        if let Some(top) = tops.slash {
            middles.push(top + notation_middle);
        }
        if let Some(top) = tops.notation {
            middles.push(top + notation_middle);
        }
        if tops.show_tab || middles.is_empty() {
            middles.push(tops.tab + ctx.staff.height() / 2.0);
        }
        for middle in middles {
            ops.push(DrawOp::Rect {
                rect: Rect {
                    origin: Point::new(left, middle - BAR_THICKNESS / 2.0),
                    width: right - left,
                    height: BAR_THICKNESS,
                },
                color: ColorToken::Notation,
            });
            for at in [left, right] {
                ops.push(DrawOp::Line {
                    from: Point::new(at, middle - SERIF_REACH),
                    to: Point::new(at, middle + SERIF_REACH),
                    thickness: SERIF_THICKNESS,
                    color: ColorToken::Notation,
                });
            }
        }
        // The count in time-signature digits, centered over the thick bar.
        let digits: Vec<Glyph> = count
            .to_string()
            .chars()
            .filter_map(|digit| digit.to_digit(10).and_then(Digit::new))
            .map(Glyph::TimeSigDigit)
            .collect();
        let font = &ctx.fonts.music;
        let width: f32 = digits.iter().map(|&glyph| font.advance(glyph)).sum();
        let mut at = (left + right) / 2.0 - width / 2.0;
        let y = tops.top_line() - COUNT_GAP - DIGIT_HALF;
        for glyph in digits {
            ops.push(DrawOp::Glyph {
                glyph,
                at: Point::new(at, y),
                scale: 1.0,
                color: ColorToken::Notation,
            });
            at += font.advance(glyph);
        }
    }
}
