//! One measure laid out on its own: its time signature (if shown) and its beat columns.
//!
//! A [`MeasureLayout`] depends only on the measure, whether it shows its time signature,
//! and the settings, never on where it ends up. That makes it the unit to cache per measure
//! and document revision once editing needs fast relayout (M7); placing measures into
//! systems only chooses an x position and a stretch factor.

use std::collections::BTreeMap;

use gscore_model::{
    BeatKind, BeatView, Duration, MeasureIndex, MeasureView, NoteKind, TimeSignature,
};

use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, Rect, TextAnchor, TextFont};
use crate::error::LayoutError;
use crate::glyph::Glyph;

/// Space before and after a time signature.
const TIME_SIG_PAD: (f32, f32) = (0.8, 0.6);
/// How much of the staff height a time signature covers.
const TIME_SIG_HEIGHT_RATIO: f32 = 0.8;
/// Smallest space between the start of a measure and its first beat.
const MIN_LEAD: f32 = 1.5;
/// Gap between a rest and its first dot.
const DOT_GAP: f32 = 0.3;
/// Extra room around a fret number's mask, horizontally and vertically.
const MASK_PAD: (f32, f32) = (0.15, 0.2);
/// Shown instead of a fret number the model cannot read.
const UNREADABLE: &str = "?";
/// How far a measure number sits above the staff (to its baseline).
const MEASURE_NUMBER_RISE: f32 = 0.8;

/// A measure, laid out independently of its position.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeasureLayout {
    index: MeasureIndex,
    time_signature: Option<TimeSigLayout>,
    /// Fixed space from the measure start (after the time signature) to the first column.
    lead: f32,
    columns: Vec<Column>,
}

/// A time signature as glyph rows, scaled to the staff.
#[derive(Debug, Clone, PartialEq)]
struct TimeSigLayout {
    numerator: Vec<Glyph>,
    denominator: Vec<Glyph>,
    scale: f32,
    /// Width including the padding around it.
    width: f32,
}

/// Everything that starts at the same time in a measure, across voices.
#[derive(Debug, Clone, PartialEq)]
struct Column {
    /// Natural space from this column to the next one (or to the barline).
    space: f32,
    items: Vec<Item>,
}

/// One thing drawn in a column.
#[derive(Debug, Clone, PartialEq)]
enum Item {
    /// A fret number (or `x`, `(5)`, ...) on a staff line (0 = top).
    Fret {
        line: usize,
        text: String,
        width: f32,
    },
    /// A rest, centered on the staff.
    Rest { glyph: Glyph, dots: u8 },
}

impl MeasureLayout {
    /// Lays out `measure`; `show_time_signature` when it starts the piece or changes the
    /// time signature.
    pub(crate) fn new(
        measure: MeasureView<'_>,
        show_time_signature: bool,
        ctx: &Context<'_>,
    ) -> Result<Self, LayoutError> {
        let signature = measure.time_signature()?;
        let time_signature = show_time_signature.then(|| TimeSigLayout::new(signature, ctx));
        let starts = beat_starts(measure, signature)?;
        let mut columns: Vec<(Vec<Item>, f32)> = Vec::with_capacity(starts.onsets.len());
        for (onset, beats) in &starts.onsets {
            let next = starts
                .onsets
                .range((std::ops::Bound::Excluded(onset), std::ops::Bound::Unbounded))
                .next()
                .map_or(starts.end, |(next, _)| *next);
            let length = next.checked_sub(*onset).map_or(0.0, as_f64);
            columns.push((column_items(beats, ctx)?, ctx.options.spacing.after(length)));
        }
        if columns.is_empty() {
            // An empty measure still gets the room its length asks for.
            columns.push((Vec::new(), ctx.options.spacing.after(as_f64(starts.end))));
        }

        // Keep neighbouring fret numbers apart, whatever the durations say.
        let min_gap = ctx.options.spacing.min_gap;
        let half_widths: Vec<f32> = columns
            .iter()
            .map(|(items, _)| half_width(items, ctx))
            .collect();
        let lead = half_widths
            .first()
            .map_or(MIN_LEAD, |first| (first + min_gap).max(MIN_LEAD));
        let columns = columns
            .into_iter()
            .enumerate()
            .map(|(i, (items, space))| {
                let own = half_widths.get(i).copied().unwrap_or_default();
                let next = half_widths
                    .get(i.saturating_add(1))
                    .copied()
                    .unwrap_or_default();
                Column {
                    space: space.max(own + next + min_gap),
                    items,
                }
            })
            .collect();

        Ok(MeasureLayout {
            index: measure.index(),
            time_signature,
            lead,
            columns,
        })
    }

    /// The width that does not stretch: time signature and lead.
    pub(crate) fn fixed_width(&self) -> f32 {
        self.time_signature.as_ref().map_or(0.0, |t| t.width) + self.lead
    }

    /// The width that stretches when a system is justified.
    pub(crate) fn stretchable_width(&self) -> f32 {
        self.columns.iter().map(|c| c.space).sum()
    }

    /// The width with the columns stretched by `stretch`.
    pub(crate) fn width(&self, stretch: f32) -> f32 {
        self.fixed_width() + self.stretchable_width() * stretch
    }

    /// Draws the measure's content (not the staff lines or barline) starting at `x`, with
    /// the staff's top line at `top`.
    pub(crate) fn draw(
        &self,
        x: f32,
        top: f32,
        stretch: f32,
        ctx: &Context<'_>,
        ops: &mut Vec<DrawOp>,
    ) {
        ops.push(DrawOp::Text {
            text: self.index.get().saturating_add(1).to_string(),
            at: Point::new(x, top - MEASURE_NUMBER_RISE),
            size: ctx.options.measure_number_size,
            font: TextFont::Regular,
            anchor: TextAnchor::Start,
            color: ColorToken::Annotation,
        });
        let mut cursor = x;
        if let Some(time_signature) = &self.time_signature {
            time_signature.draw(cursor, top, ctx, ops);
            cursor += time_signature.width;
        }
        cursor += self.lead;
        for column in &self.columns {
            for item in &column.items {
                item.draw(cursor, top, ctx, ops);
            }
            cursor += column.space * stretch;
        }
    }
}

impl TimeSigLayout {
    fn new(signature: TimeSignature, ctx: &Context<'_>) -> Self {
        // Each digit row is two staff spaces high at scale 1; together they cover most of the
        // staff.
        let scale = (ctx.staff.height() * TIME_SIG_HEIGHT_RATIO / 4.0).max(1.0);
        let numerator = Glyph::time_sig_digits(u32::from(signature.numerator()));
        let denominator = Glyph::time_sig_digits(u32::from(signature.denominator()));
        let row = |glyphs: &[Glyph]| -> f32 {
            glyphs
                .iter()
                .map(|g| ctx.fonts.music.advance(*g))
                .sum::<f32>()
                * scale
        };
        let width = TIME_SIG_PAD.0 + row(&numerator).max(row(&denominator)) + TIME_SIG_PAD.1;
        TimeSigLayout {
            numerator,
            denominator,
            scale,
            width,
        }
    }

    fn draw(&self, x: f32, top: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
        let font = &ctx.fonts.music;
        let row_width = |glyphs: &[Glyph]| -> f32 {
            glyphs.iter().map(|g| font.advance(*g)).sum::<f32>() * self.scale
        };
        let inner = self.width - TIME_SIG_PAD.0 - TIME_SIG_PAD.1;
        let middle = top + ctx.staff.middle();
        // Time signature digits have their origin at their vertical middle.
        for (glyphs, y) in [
            (&self.numerator, middle - self.scale),
            (&self.denominator, middle + self.scale),
        ] {
            let mut at = x + TIME_SIG_PAD.0 + (inner - row_width(glyphs)) / 2.0;
            for glyph in glyphs {
                ops.push(DrawOp::Glyph {
                    glyph: *glyph,
                    at: Point::new(at, y),
                    scale: self.scale,
                    color: ColorToken::Notation,
                });
                at += font.advance(*glyph) * self.scale;
            }
        }
    }
}

impl Item {
    fn draw(&self, x: f32, top: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
        match self {
            Item::Fret { line, text, width } => {
                let size = ctx.options.fret_size;
                let line_y = top + ctx.staff.line_y(*line);
                let cap = ctx.fonts.text.cap_height * size;
                // Hide the staff line behind the number.
                ops.push(DrawOp::Rect {
                    rect: Rect {
                        origin: Point::new(
                            x - width / 2.0 - MASK_PAD.0,
                            line_y - cap / 2.0 - MASK_PAD.1,
                        ),
                        width: width + 2.0 * MASK_PAD.0,
                        height: cap + 2.0 * MASK_PAD.1,
                    },
                    color: ColorToken::Paper,
                });
                ops.push(DrawOp::Text {
                    text: text.clone(),
                    at: Point::new(x, line_y + cap / 2.0),
                    size,
                    font: TextFont::Regular,
                    anchor: TextAnchor::Middle,
                    color: ColorToken::FretNumber,
                });
            }
            Item::Rest { glyph, dots } => {
                let font = &ctx.fonts.music;
                let bbox = font.bbox(*glyph);
                let middle = top + ctx.staff.middle();
                let left = x - (bbox.x_min + bbox.x_max) / 2.0;
                ops.push(DrawOp::Glyph {
                    glyph: *glyph,
                    at: Point::new(left, rest_y(*glyph, top, ctx)),
                    scale: 1.0,
                    color: ColorToken::Notation,
                });
                let dot = Glyph::AugmentationDot;
                let mut dot_x = left + font.advance(*glyph) + DOT_GAP;
                for _ in 0..*dots {
                    // Dots sit in a space, half a staff space above the middle.
                    ops.push(DrawOp::Glyph {
                        glyph: dot,
                        at: Point::new(dot_x, middle - 0.5 + font.bbox(dot).middle_above_origin()),
                        scale: 1.0,
                        color: ColorToken::Notation,
                    });
                    dot_x += font.advance(dot) + DOT_GAP;
                }
            }
        }
    }

    /// Half the item's width, for keeping columns apart.
    fn half_width(&self, ctx: &Context<'_>) -> f32 {
        match self {
            Item::Fret { width, .. } => width / 2.0,
            Item::Rest { glyph, dots } => {
                let font = &ctx.fonts.music;
                let dots = f32::from(*dots) * (font.advance(Glyph::AugmentationDot) + DOT_GAP);
                font.advance(*glyph) / 2.0 + dots
            }
        }
    }
}

/// Where a rest's origin goes. A whole rest hangs from the line just above the middle and
/// a half rest sits on the line just below it, as on a five-line staff, so the two can be
/// told apart; other rests are centered on the staff.
fn rest_y(glyph: Glyph, top: f32, ctx: &Context<'_>) -> f32 {
    let staff = &ctx.staff;
    // Even line count: the two lines around the middle. Odd: the middle line and the one
    // above it (whole rest from the 4th line, half rest on the 3rd, as on five lines).
    let lower = staff.lines() / 2;
    let upper = lower.saturating_sub(1);
    match glyph {
        // SMuFL draws the whole rest below its origin and the half rest above it.
        Glyph::RestWhole => top + staff.line_y(upper),
        Glyph::RestHalf => top + staff.line_y(lower),
        _ => top + staff.middle() + ctx.fonts.music.bbox(glyph).middle_above_origin(),
    }
}

fn half_width(items: &[Item], ctx: &Context<'_>) -> f32 {
    items
        .iter()
        .map(|item| item.half_width(ctx))
        .fold(0.0, f32::max)
}

/// Beats grouped by when they start, across voices, and where the measure ends.
struct BeatStarts<'a> {
    onsets: BTreeMap<Duration, Vec<BeatView<'a>>>,
    /// The time signature's length, or the longest voice if one is overfull.
    end: Duration,
}

fn beat_starts<'a>(
    measure: MeasureView<'a>,
    signature: TimeSignature,
) -> Result<BeatStarts<'a>, LayoutError> {
    let mut onsets: BTreeMap<Duration, Vec<BeatView<'a>>> = BTreeMap::new();
    let mut end = signature.measure_length();
    for voice in measure.voices() {
        let mut at = Duration::new(0, 1)?;
        for beat in voice.beats() {
            if beat.kind() == BeatKind::Empty {
                continue;
            }
            onsets.entry(at).or_default().push(beat);
            at = at.checked_add(beat.duration()?)?;
        }
        end = end.max(at);
    }
    Ok(BeatStarts { onsets, end })
}

/// The fret numbers of a column's beats; a rest if no beat has notes.
fn column_items(beats: &[BeatView<'_>], ctx: &Context<'_>) -> Result<Vec<Item>, LayoutError> {
    let mut items = Vec::new();
    for beat in beats.iter().filter(|b| b.kind() == BeatKind::Normal) {
        for note in beat.notes() {
            // A fret the model cannot read (it lists those as issues) is shown as `?`, so
            // one odd note does not hide the whole track.
            let fret = || {
                note.fret()
                    .map_or_else(|_| UNREADABLE.to_owned(), |f| f.get().to_string())
            };
            let text = match note.kind() {
                NoteKind::Normal => fret(),
                NoteKind::Tie => format!("({})", fret()),
                NoteKind::Dead => "x".to_owned(),
                NoteKind::Rest | NoteKind::Unknown => continue,
            };
            // Notes on strings the staff does not have are issues too; there is no line to
            // draw them on.
            let Some(line) = note
                .string()
                .ok()
                .map(|s| usize::from(s.get()).saturating_sub(1))
                .filter(|&line| line < ctx.staff.lines())
            else {
                continue;
            };
            let width = ctx.fonts.text.width(&text, ctx.options.fret_size);
            items.push(Item::Fret { line, text, width });
        }
    }
    if items.is_empty()
        && let Some(rest) = beats.iter().find(|b| b.kind() == BeatKind::Rest)
    {
        let value = rest.note_value()?;
        if let Some(glyph) = Glyph::rest(value.value()) {
            items.push(Item::Rest {
                glyph,
                dots: value.dots(),
            });
        }
    }
    Ok(items)
}

/// A duration in whole notes, as a float for spacing.
fn as_f64(duration: Duration) -> f64 {
    let denominator = duration.denominator() as f64;
    if denominator > 0.0 {
        duration.numerator() as f64 / denominator
    } else {
        0.0
    }
}
