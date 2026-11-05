//! One measure laid out on its own: its time signature (if shown) and its beat columns.
//!
//! A [`MeasureLayout`] depends only on the measure, whether it shows its time signature,
//! and the settings, never on where it ends up. That makes it the unit to cache per measure
//! and document revision once editing needs fast relayout (M8); placing measures into
//! systems only chooses an x position and a stretch factor.

mod align;
mod columns;
mod draw;
mod effects;
mod grace_marks;
mod graces;

pub(crate) use align::align;
pub(crate) use draw::draw_barline_ties;

use gscore_model::{
    BeatIndex, BeatView, Duration, Dynamic, MeasureIndex, MeasureView, TimeSignature, VoiceIndex,
};

use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point};
use crate::error::LayoutError;
use crate::glyph::Glyph;
use crate::notation::{self, KeySigLayout, Notation, NotationItem};
use crate::ornaments::{self, ColumnMarks, Fret};

use columns::{as_f64, beat_length, beat_starts, column_items};
use graces::Grace;

/// Space before and after a time signature.
const TIME_SIG_PAD: (f32, f32) = (0.8, 0.6);
/// Extra room on each side for the parentheses of a free-time signature.
const FREE_TIME_PAD: f32 = 0.55;
/// How much of the staff height a time signature covers.
const TIME_SIG_HEIGHT_RATIO: f32 = 0.7;
/// Smallest space between the start of a measure and its first beat.
const MIN_LEAD: f32 = 1.5;
/// Gap between a rest and its first dot.
const DOT_GAP: f32 = 0.3;
/// Shown instead of a fret number the model cannot read.
const UNREADABLE: &str = "?";
/// How far measure numbers sit above the staff (to its baseline).
const MEASURE_NUMBER_RISE: f32 = 0.8;
/// How far a second voice's noteheads move right when they share a staff position with the
/// first voice's.
const VOICE_OFFSET: f32 = 0.6;
/// Half the width of a dead slapped beat's cross.
pub(crate) const DEAD_SLAP_HALF: f32 = 1.4;
/// Where a system's staves are: the tab's top line, and the notation staff's top line if
/// notation is shown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tops {
    pub(crate) tab: f32,
    pub(crate) notation: Option<f32>,
    /// The slash-notation staff's top line, if the track shows one.
    pub(crate) slash: Option<f32>,
    /// Whether the tab staff is drawn (a track may show notation only).
    pub(crate) show_tab: bool,
    /// Whether the measure number is written above the staves (only over a system's top
    /// track when several tracks are shown).
    pub(crate) numbers: bool,
    /// Where the bottom of the beat effects (pick strokes, tapping, fade-ins) goes.
    pub(crate) beat_bottom: f32,
    /// The line the ornament lanes (trill, vibrato, whammy) are measured up from: the tab's
    /// top line, or just below a notation-only staff's.
    pub(crate) lanes: f32,
}

/// A measure, laid out independently of its position.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeasureLayout {
    index: MeasureIndex,
    /// The clef this measure's notation staff uses.
    clef: notation::Clef,
    /// Whether the measure starts with a clef change (drawn before its key signature).
    clef_change: bool,
    /// Room the clef change takes, including the gaps around it.
    clef_width: f32,
    /// The key signature when it changes here (or starts the score), on the staff that
    /// carries it.
    key_signature: Option<KeySigLayout>,
    time_signature: Option<TimeSigLayout>,
    /// Free time: the measure's time signature is written in parentheses.
    free_time: bool,
    /// Guitar Pro's bar-repetition (simile) mark.
    simile: Option<gscore_model::SimileMark>,
    /// Room after the key signature, so the time signature lines up with other tracks'
    /// (0 on its own; set by [`align`]).
    signature_pad: f32,
    /// Fixed space from the measure start (after the signature changes) to the first column.
    lead: f32,
    columns: Vec<Column>,
    /// When the measure ends: the time signature's length, or the longest voice.
    end: Duration,
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
    /// When the column starts in the measure.
    onset: Duration,
    /// Natural space from this column to the next one (or to the barline).
    space: f32,
    items: Vec<Item>,
    /// Effects that belong to the beat rather than to one note.
    marks: ColumnMarks,
    /// What the notation staff shows here, per voice (first and second, ...).
    notation: Vec<Option<NotationItem>>,
    /// What the slash-notation staff shows here, per voice, if the track has one.
    slash: Vec<Option<NotationItem>>,
    /// How far the second and further voices move right to clear the first voice's
    /// noteheads when they share a staff position.
    notation_offset: f32,
    /// Quick grace notes drawn before the column's main note.
    graces: Vec<Grace>,
    /// Room the grace notes take, including the gap to the main note.
    grace_width: f32,
    /// How far the main note reaches left of the column's center (its accidentals
    /// included): the grace notes end there.
    main_left: f32,
    /// The beat this column starts in (beams join notes within one beat).
    beat: u64,
    /// The tuplet this column belongs to (`enters:times`), if any. The tab staff shows it
    /// when no notation staff is there to carry it.
    tuplet: Option<(u8, u8)>,
    /// The octave sign written at this column, if any.
    octave: Option<gscore_model::OctaveSign>,
    /// The model beats that start here, as `(voice, beat)`.
    beats: Vec<(VoiceIndex, BeatIndex)>,
    /// Guitar Pro's beat text, written above the staff at this column.
    text: Option<String>,
    /// The dynamic of the first voice's beat (else another voice's), if it has notes.
    dynamic: Option<Dynamic>,
    /// A tempo change on one of the beats (GP3-5 store tempo changes on beats).
    tempo: Option<u32>,
}

/// The notes of one column and the effects that belong to its beat.
#[derive(Debug, Clone, PartialEq)]
struct ColumnData {
    items: Vec<Item>,
    marks: ColumnMarks,
    notation: Vec<Option<NotationItem>>,
    slash: Vec<Option<NotationItem>>,
    graces: Vec<Grace>,
    grace_width: f32,
    beat: u64,
    tuplet: Option<(u8, u8)>,
    octave: Option<gscore_model::OctaveSign>,
    beats: Vec<(VoiceIndex, BeatIndex)>,
    text: Option<String>,
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
    /// The measure it lays out.
    pub(crate) fn index(&self) -> MeasureIndex {
        self.index
    }

    /// The same layout for the measure now at `index` (it moved when a measure before it
    /// was inserted or removed).
    pub(crate) fn moved_to(&self, index: MeasureIndex) -> Self {
        MeasureLayout {
            index,
            ..self.clone()
        }
    }

    /// Lays out `measure` in `key`, which `previous` differs from when the key signature
    /// changes here; `show_time_signature` when it starts the piece or changes the time
    /// signature. `clef` is the clef in effect here and `previous_clef` the one before, so
    /// a change is written at the measure's start.
    pub(crate) fn new(
        measure: MeasureView<'_>,
        show_time_signature: bool,
        free_time: bool,
        key: i8,
        previous_key: Option<i8>,
        clef: notation::Clef,
        previous_clef: Option<notation::Clef>,
        ctx: &Context<'_>,
    ) -> Result<Self, LayoutError> {
        // The staff's items and key signature are laid out in this measure's clef.
        let local;
        let ctx = match ctx.notation {
            Some(notation) => {
                local = Context {
                    notation: Some(Notation { clef, ..notation }),
                    ..*ctx
                };
                &local
            }
            None => ctx,
        };
        let signature = measure.time_signature()?;
        let time_signature =
            show_time_signature.then(|| TimeSigLayout::new(signature, free_time, ctx));
        // The key signature goes on the notation staff; a tab-only score shows it on the
        // tab staff. Drums have none.
        let key_signature = if previous_key == Some(key) {
            None
        } else {
            match ctx.notation {
                Some(notation) if notation.clef != notation::Clef::Percussion => {
                    KeySigLayout::change(notation.clef, previous_key.unwrap_or(0), key, false, ctx)
                }
                Some(_) => None,
                None => KeySigLayout::change(
                    notation::Clef::Treble,
                    previous_key.unwrap_or(0),
                    key,
                    true,
                    ctx,
                ),
            }
        };
        let starts = beat_starts(measure, signature)?;
        let beat_length = as_f64(beat_length(signature)?);
        type Pending = (ColumnData, f32, Option<Dynamic>, Option<u32>);
        let mut columns: Vec<Pending> = Vec::with_capacity(starts.onsets.len());
        // Each column's start, beside `columns`.
        let mut onsets: Vec<Duration> = Vec::with_capacity(starts.onsets.len());
        for (column_index, (onset, beats)) in starts.onsets.iter().enumerate() {
            let next = starts
                .onsets
                .range((std::ops::Bound::Excluded(onset), std::ops::Bound::Unbounded))
                .next()
                .map_or(starts.end, |(next, _)| *next);
            let length = next.checked_sub(*onset).map_or(0.0, as_f64);
            let plain: Vec<BeatView<'_>> = beats.iter().map(|&(_, beat)| beat).collect();
            let mut data = column_items(&plain, ctx, column_index == 0)?;
            // The shortest tuplet of the beats starting here; all normal beats of one
            // tuplet share it.
            data.tuplet = plain
                .iter()
                .filter(|beat| beat.kind() != gscore_model::BeatKind::Empty)
                .find_map(|beat| beat.note_value().ok()?.tuplet());
            data.octave = plain.iter().find_map(BeatView::octave);
            if let Some(notation) = &ctx.notation {
                data.notation = notation::column_items(beats, notation, key)?;
            }
            if ctx.slash {
                data.slash = notation::slash_column_items(beats)?;
            }
            let voice_count = measure.voices().len();
            let mut group = 0_usize;
            for voice in 0..voice_count {
                if let Some(grace_beats) = starts.graces.get(&(voice, *onset)) {
                    for grace_beat in grace_beats {
                        let mut drew = false;
                        for note in grace_beat.notes() {
                            if let Some(grace) = graces::from_note(note, ctx, key) {
                                data.graces.push(Grace { group, ..grace });
                                drew = true;
                            }
                        }
                        if drew {
                            group = group.saturating_add(1);
                        }
                    }
                }
            }
            // GP3-5 attach grace notes to a normal note instead of a beat of their own.
            for &(_, beat) in beats {
                for note in beat.notes() {
                    if let Some(grace) = note.grace()
                        && let Some(grace) = graces::from_effect(note, grace, ctx, key)
                    {
                        data.graces.push(Grace { group, ..grace });
                        group = group.saturating_add(1);
                    }
                }
            }
            data.grace_width = graces::width(&data.graces, ctx);
            // A tiny epsilon keeps exact beat starts (computed in floats) in their beat.
            data.beat = if beat_length > 0.0 {
                (as_f64(*onset) / beat_length + 1e-9).floor() as u64
            } else {
                0
            };
            data.beats = beats
                .iter()
                .map(|&(voice, beat)| (VoiceIndex::new(voice), beat.index()))
                .collect();
            let mut by_voice: Vec<&(usize, BeatView<'_>)> = beats.iter().collect();
            by_voice.sort_by_key(|(voice, _)| *voice);
            let dynamic = by_voice.iter().find_map(|(_, beat)| beat.dynamic());
            let tempo = beats.iter().find_map(|(_, beat)| beat.tempo_change());
            columns.push((data, ctx.options.spacing.after(length), dynamic, tempo));
            onsets.push(*onset);
        }
        if columns.is_empty() {
            // An empty measure still gets the room its length asks for.
            columns.push((
                ColumnData {
                    items: Vec::new(),
                    marks: ColumnMarks::default(),
                    notation: Vec::new(),
                    slash: Vec::new(),
                    graces: Vec::new(),
                    grace_width: 0.0,
                    beat: 0,
                    tuplet: None,
                    octave: None,
                    beats: Vec::new(),
                    text: None,
                },
                ctx.options.spacing.after(as_f64(starts.end)),
                None,
                None,
            ));
            onsets.push(Duration::new(0, 1)?);
        }
        // When a measure has a second voice, the first voice points its stems up and the
        // second down throughout, so their stems and beams do not run into each other.
        let two_voices = columns
            .iter()
            .any(|(data, _, _, _)| data.notation.get(1).is_some_and(Option::is_some));
        if two_voices
            && ctx
                .notation
                .is_some_and(|n| n.clef != notation::Clef::Percussion)
        {
            for (data, _, _, _) in &mut columns {
                for (voice, item) in data.notation.iter_mut().enumerate() {
                    if let Some(NotationItem::Chord(chord)) = item {
                        chord.stem_up = Some(voice == 0);
                    }
                }
            }
        }

        // Keep neighbouring fret numbers apart, whatever the durations say. A note also
        // never sits right against a barline: measures keep their own padding at both ends.
        let min_gap = ctx.options.spacing.min_gap;
        let barline_padding = ctx.options.spacing.barline_padding.max(0.0);
        let edge_padding = barline_padding.max(min_gap);
        notation::assign_accidentals(
            columns
                .iter_mut()
                .flat_map(|(d, _, _, _)| d.notation.iter_mut().flatten()),
            key,
        );
        // Room each column needs left and right of its center: the tab's fret numbers are
        // symmetric, notation is not (accidentals left; dots and flags right). A second
        // voice whose notehead shares a staff position with the first moves right.
        let offsets: Vec<f32> = columns
            .iter()
            .map(|(data, _, _, _)| voice_offset(&data.notation))
            .collect();
        let voices = columns
            .iter()
            .map(|(data, _, _, _)| data.notation.len())
            .max()
            .unwrap_or(0);
        let beamed: Vec<Vec<bool>> = (0..voices)
            .map(|voice| {
                notation::beamed_columns(columns.iter().map(|(data, _, _, _)| {
                    (data.notation.get(voice).and_then(Option::as_ref), data.beat)
                }))
            })
            .collect();
        // Each column's room left of its center for the main note alone, then with its
        // grace notes before it, and right of its center.
        let extents: Vec<(f32, f32, f32)> = columns
            .iter()
            .enumerate()
            .map(|(index, (data, _, _, _))| {
                let tab = half_width(&data.items, ctx);
                // A brush or arpeggio arrow stands left of the fret numbers.
                let mut tab_left = if data.marks.brush.is_some() {
                    tab + ornaments::BRUSH_ROOM
                } else {
                    tab
                };
                if data.marks.arpeggio.is_some() {
                    tab_left = tab_left.max(tab + ornaments::ARPEGGIO_ROOM);
                }
                let mut left = 0.0_f32;
                let mut right = 0.0_f32;
                for (voice, item) in data.notation.iter().enumerate() {
                    let Some(item) = item else { continue };
                    let beamed = beamed
                        .get(voice)
                        .and_then(|b| b.get(index))
                        .copied()
                        .unwrap_or(false);
                    let (item_left, item_right) = notation::extent(item, beamed, ctx);
                    let shift = if voice > 0 {
                        offsets.get(index).copied().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    left = left.max(item_left + shift);
                    right = right.max(item_right + shift);
                }
                // A dead slapped beat writes a large cross on both staves, so its column
                // needs room for it even though it has no items.
                if data.marks.dead_slapped {
                    left = left.max(DEAD_SLAP_HALF);
                    right = right.max(DEAD_SLAP_HALF);
                    tab_left = tab_left.max(DEAD_SLAP_HALF);
                }
                let main = tab_left.max(left);
                (main, main + data.grace_width, tab.max(right))
            })
            .collect();
        let lead = extents
            .first()
            .map_or(MIN_LEAD, |(_, left, _)| (left + edge_padding).max(MIN_LEAD));
        let columns = columns
            .into_iter()
            .enumerate()
            .map(|(i, (data, space, dynamic, tempo))| {
                let own = extents.get(i).map_or(0.0, |(_, _, right)| *right);
                let next = extents
                    .get(i.saturating_add(1))
                    .map_or(0.0, |(_, left, _)| *left);
                // The last column ends at the barline and needs the wider edge padding.
                let padding = if i.saturating_add(1) < extents.len() {
                    min_gap
                } else {
                    edge_padding
                };
                Column {
                    onset: onsets.get(i).copied().unwrap_or(starts.end),
                    space: space.max(own + next + padding),
                    items: data.items,
                    marks: data.marks,
                    notation: data.notation,
                    slash: data.slash,
                    notation_offset: offsets.get(i).copied().unwrap_or(0.0),
                    graces: data.graces,
                    grace_width: data.grace_width,
                    main_left: extents.get(i).map_or(0.0, |(main, _, _)| *main),
                    beat: data.beat,
                    tuplet: data.tuplet,
                    octave: data.octave,
                    beats: data.beats,
                    text: data.text,
                    dynamic,
                    tempo,
                }
            })
            .collect();

        let pitched = ctx
            .notation
            .is_some_and(|notation| notation.clef != notation::Clef::Percussion);
        let clef_change = pitched && previous_clef.is_some_and(|previous| previous != clef);
        let clef_width = if clef_change {
            notation::clef_change_width(clef, ctx)
        } else {
            0.0
        };
        Ok(MeasureLayout {
            index: measure.index(),
            clef,
            clef_change,
            clef_width,
            key_signature,
            time_signature,
            free_time,
            simile: measure.simile_mark(),
            signature_pad: 0.0,
            lead,
            columns,
            end: starts.end,
        })
    }

    /// Whether any column holds a note (a fret number, a notehead or a grace note), not
    /// only rests.
    pub(crate) fn has_notes(&self) -> bool {
        self.columns.iter().any(|column| {
            !column.graces.is_empty()
                || column
                    .items
                    .iter()
                    .any(|item| matches!(item, Item::Fret(_)))
                || column
                    .notation
                    .iter()
                    .chain(&column.slash)
                    .flatten()
                    .any(|item| matches!(item, NotationItem::Chord(_)))
                || column.marks.dead_slapped
        })
    }

    /// The clef of this measure's notation staff.
    pub(crate) fn clef(&self) -> notation::Clef {
        self.clef
    }

    /// Whether the measure carries a key signature (a change, or the score's first).
    pub(crate) fn has_key_signature(&self) -> bool {
        self.key_signature.is_some()
    }

    /// The beat text of each column that has one, with its column index.
    pub(crate) fn beat_texts(&self) -> impl Iterator<Item = (usize, &str)> {
        self.columns
            .iter()
            .enumerate()
            .filter_map(|(index, column)| column.text.as_deref().map(|text| (index, text)))
    }

    /// Whether any column carries beat text.
    pub(crate) fn has_beat_text(&self) -> bool {
        self.columns.iter().any(|column| column.text.is_some())
    }

    /// The width that does not stretch: key and time signature, and the lead.
    pub(crate) fn fixed_width(&self) -> f32 {
        self.clef_width
            + self.key_signature.as_ref().map_or(0.0, |k| k.width)
            + self.signature_pad
            + self.time_signature.as_ref().map_or(0.0, |t| t.width)
            + self.lead
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
    fn new(signature: TimeSignature, free_time: bool, ctx: &Context<'_>) -> Self {
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
        let parens = if free_time { 2.0 * FREE_TIME_PAD } else { 0.0 };
        let width =
            TIME_SIG_PAD.0 + row(&numerator).max(row(&denominator)) + TIME_SIG_PAD.1 + parens;
        TimeSigLayout {
            numerator,
            denominator,
            scale,
            width,
        }
    }

    fn draw(&self, x: f32, top: f32, free_time: bool, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
        self.draw_at(x, top + ctx.staff.middle(), self.scale, ctx, ops, free_time);
    }

    /// Parentheses around a free-time signature.
    fn draw_parentheses(
        &self,
        x: f32,
        middle: f32,
        scale: f32,
        free_time: bool,
        ctx: &Context<'_>,
        ops: &mut Vec<DrawOp>,
    ) {
        if !free_time {
            return;
        }
        let font = &ctx.fonts.music;
        let size = (scale * 1.7).max(1.4);
        for (glyph, at_x) in [
            (Glyph::ParenthesisLeft, x + 0.1),
            (
                Glyph::ParenthesisRight,
                x + self.width - font.advance(Glyph::ParenthesisRight) * size - 0.1,
            ),
        ] {
            let y = middle + font.bbox(glyph).middle_above_origin() * size;
            ops.push(DrawOp::Glyph {
                glyph,
                at: Point::new(at_x, y),
                scale: size,
                color: ColorToken::Notation,
            });
        }
    }

    /// Draws the digits centered on `middle` at `scale` (1 on a five-line staff).
    pub(super) fn draw_at(
        &self,
        x: f32,
        middle: f32,
        scale: f32,
        ctx: &Context<'_>,
        ops: &mut Vec<DrawOp>,
        free_time: bool,
    ) {
        let font = &ctx.fonts.music;
        let row_width = |glyphs: &[Glyph]| -> f32 {
            glyphs.iter().map(|g| font.advance(*g)).sum::<f32>() * scale
        };
        let inner = self.width - TIME_SIG_PAD.0 - TIME_SIG_PAD.1;
        self.draw_parentheses(x, middle, scale, free_time, ctx, ops);
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

/// How far the second and further voices move right: enough to clear the first voice's
/// noteheads when a notehead of theirs sits on the same staff position.
fn voice_offset(items: &[Option<NotationItem>]) -> f32 {
    let Some(NotationItem::Chord(first)) = items.first().and_then(Option::as_ref) else {
        return 0.0;
    };
    let collides = items.iter().skip(1).any(|item| match item {
        Some(NotationItem::Chord(chord)) => chord
            .heads
            .iter()
            .any(|head| first.heads.iter().any(|h| h.position == head.position)),
        _ => false,
    });
    if collides { VOICE_OFFSET } else { 0.0 }
}
