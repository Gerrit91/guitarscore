//! One measure laid out on its own: its time signature (if shown) and its beat columns.
//!
//! A [`MeasureLayout`] depends only on the measure, whether it shows its time signature,
//! and the settings, never on where it ends up. That makes it the unit to cache per measure
//! and document revision once editing needs fast relayout (M8); placing measures into
//! systems only chooses an x position and a stretch factor.

mod columns;
mod draw;

use gscore_model::{BeatView, Duration, MeasureIndex, MeasureView, TimeSignature};

use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point};
use crate::error::LayoutError;
use crate::glyph::Glyph;
use crate::notation::{self, NotationItem};
use crate::ornaments::{ColumnMarks, Fret};

use columns::{beat_starts, column_items};

/// Space before and after a time signature.
const TIME_SIG_PAD: (f32, f32) = (0.8, 0.6);
/// How much of the staff height a time signature covers.
const TIME_SIG_HEIGHT_RATIO: f32 = 0.7;
/// Smallest space between the start of a measure and its first beat.
const MIN_LEAD: f32 = 1.5;
/// Gap between a rest and its first dot.
const DOT_GAP: f32 = 0.3;
/// Shown instead of a fret number the model cannot read.
const UNREADABLE: &str = "?";
/// How far a measure number sits above the staff (to its baseline).
const MEASURE_NUMBER_RISE: f32 = 0.8;

/// Where a system's staves are: the tab's top line, and the notation staff's top line if
/// notation is shown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tops {
    pub(crate) tab: f32,
    pub(crate) notation: Option<f32>,
}

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
    /// Effects that belong to the beat rather than to one note.
    marks: ColumnMarks,
    /// What the notation staff shows here (first voice), if notation is on.
    notation: Option<NotationItem>,
    /// The beat this column starts in (beams join notes within one beat).
    beat: u64,
}

/// The notes of one column and the effects that belong to its beat.
#[derive(Debug, Clone, PartialEq)]
struct ColumnData {
    items: Vec<Item>,
    marks: ColumnMarks,
    notation: Option<NotationItem>,
    beat: u64,
}

/// One thing drawn in a column.
#[derive(Debug, Clone, PartialEq)]
enum Item {
    /// A fret number (or `x`, `x`, ...) on a staff line (0 = top).
    Fret(Fret),
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
        let beat_length = as_f64(beat_length(signature)?);
        let mut columns: Vec<(ColumnData, f32)> = Vec::with_capacity(starts.onsets.len());
        for (onset, beats) in &starts.onsets {
            let next = starts
                .onsets
                .range((std::ops::Bound::Excluded(onset), std::ops::Bound::Unbounded))
                .next()
                .map_or(starts.end, |(next, _)| *next);
            let length = next.checked_sub(*onset).map_or(0.0, as_f64);
            let plain: Vec<BeatView<'_>> = beats.iter().map(|&(_, beat)| beat).collect();
            let mut data = column_items(&plain, ctx)?;
            if let Some(notation) = &ctx.notation {
                data.notation = notation::column_item(beats, notation)?;
            }
            // A tiny epsilon keeps exact beat starts (computed in floats) in their beat.
            data.beat = if beat_length > 0.0 {
                (as_f64(*onset) / beat_length + 1e-9).floor() as u64
            } else {
                0
            };
            columns.push((data, ctx.options.spacing.after(length)));
        }
        if columns.is_empty() {
            // An empty measure still gets the room its length asks for.
            columns.push((
                ColumnData {
                    items: Vec::new(),
                    marks: ColumnMarks::default(),
                    notation: None,
                    beat: 0,
                },
                ctx.options.spacing.after(as_f64(starts.end)),
            ));
        }

        // Keep neighbouring fret numbers apart, whatever the durations say. A note also
        // never sits right against a barline: measures keep their own padding at both ends.
        let min_gap = ctx.options.spacing.min_gap;
        let barline_padding = ctx.options.spacing.barline_padding.max(0.0);
        let edge_padding = barline_padding.max(min_gap);
        notation::assign_accidentals(columns.iter_mut().filter_map(|(d, _)| d.notation.as_mut()));
        // Room each column needs left and right of its center: the tab's fret numbers are
        // symmetric, notation is not (accidentals left; dots and flags right).
        let beamed =
            notation::beamed_columns(columns.iter().map(|(d, _)| (d.notation.as_ref(), d.beat)));
        let extents: Vec<(f32, f32)> = columns
            .iter()
            .zip(&beamed)
            .map(|((data, _), &beamed)| {
                let tab = half_width(&data.items, ctx);
                let (left, right) = data.notation.as_ref().map_or((0.0, 0.0), |item| {
                    notation::extent(item, beamed, &ctx.fonts.music)
                });
                (tab.max(left), tab.max(right))
            })
            .collect();
        let lead = extents
            .first()
            .map_or(MIN_LEAD, |(left, _)| (left + edge_padding).max(MIN_LEAD));
        let columns = columns
            .into_iter()
            .enumerate()
            .map(|(i, (data, space))| {
                let own = extents.get(i).map_or(0.0, |(_, right)| *right);
                let next = extents
                    .get(i.saturating_add(1))
                    .map_or(0.0, |(left, _)| *left);
                // The last column ends at the barline and needs the wider edge padding.
                let padding = if i.saturating_add(1) < extents.len() {
                    min_gap
                } else {
                    edge_padding
                };
                Column {
                    space: space.max(own + next + padding),
                    items: data.items,
                    marks: data.marks,
                    notation: data.notation,
                    beat: data.beat,
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
        self.draw_at(x, top + ctx.staff.middle(), self.scale, ctx, ops);
    }

    /// Draws the digits centered on `middle` at `scale` (1 on a five-line staff).
    pub(super) fn draw_at(
        &self,
        x: f32,
        middle: f32,
        scale: f32,
        ctx: &Context<'_>,
        ops: &mut Vec<DrawOp>,
    ) {
        let font = &ctx.fonts.music;
        let row_width = |glyphs: &[Glyph]| -> f32 {
            glyphs.iter().map(|g| font.advance(*g)).sum::<f32>() * scale
        };
        let inner = self.width - TIME_SIG_PAD.0 - TIME_SIG_PAD.1;
        // Time signature digits have their origin at their vertical middle.
        for (glyphs, y) in [
            (&self.numerator, middle - scale),
            (&self.denominator, middle + scale),
        ] {
            let mut at = x + TIME_SIG_PAD.0 + (inner - row_width(glyphs)) / 2.0;
            for glyph in glyphs {
                ops.push(DrawOp::Glyph {
                    glyph: *glyph,
                    at: Point::new(at, y),
                    scale,
                    color: ColorToken::Notation,
                });
                at += font.advance(*glyph) * scale;
            }
        }
    }
}

impl Item {
    fn draw(&self, x: f32, top: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
        match self {
            Item::Fret(_) => {}
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
            Item::Fret(fret) => fret.half_width(),
            Item::Rest { glyph, dots } => {
                let font = &ctx.fonts.music;
                let dots = f32::from(*dots) * (font.advance(Glyph::AugmentationDot) + DOT_GAP);
                // The drawn width: some rests (64th, 128th) are wider than their advance.
                let bbox = font.bbox(*glyph);
                let drawn = (bbox.x_max - bbox.x_min).max(font.advance(*glyph));
                drawn / 2.0 + dots
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

/// The beat notes are beamed within: a dotted quarter in compound meters (6/8, 9/8,
/// 12/8), a quarter otherwise.
fn beat_length(signature: TimeSignature) -> Result<Duration, LayoutError> {
    let compound = signature.denominator() == 8
        && signature.numerator() > 3
        && signature.numerator().is_multiple_of(3);
    Ok(if compound {
        Duration::new(3, 8)?
    } else {
        Duration::new(1, 4)?
    })
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
