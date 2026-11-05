//! Building a measure's columns: which beats start together, and what each column shows
//! on the tab (fret numbers and their ornaments, or a rest).

use std::collections::BTreeMap;

use gscore_model::{
    BeatKind, BeatView, Duration, Hand, MeasureView, NoteKind, SlideKind, TimeSignature,
};

use super::{ColumnData, Item, UNREADABLE};
use crate::Context;
use crate::error::LayoutError;
use crate::glyph::Glyph;
use crate::ornaments::{self, ColumnMarks, Fret, NoteMarks};

/// Beats grouped by when they start, across voices, and where the measure ends.
pub(super) struct BeatStarts<'a> {
    /// `(voice, beat)` pairs by start time.
    pub(super) onsets: BTreeMap<Duration, Vec<(usize, BeatView<'a>)>>,
    /// Grace-note beats waiting before the regular beat at a voice's onset.
    pub(super) graces: BTreeMap<(usize, Duration), Vec<BeatView<'a>>>,
    /// The time signature's length, or the longest voice if one is overfull.
    pub(super) end: Duration,
}

pub(super) fn beat_starts<'a>(
    measure: MeasureView<'a>,
    signature: TimeSignature,
) -> Result<BeatStarts<'a>, LayoutError> {
    let mut onsets: BTreeMap<Duration, Vec<(usize, BeatView<'a>)>> = BTreeMap::new();
    let mut graces: BTreeMap<(usize, Duration), Vec<BeatView<'a>>> = BTreeMap::new();
    let mut end = signature.measure_length();
    for voice in measure.voices() {
        let voice_index = voice.index().get();
        let mut at = Duration::new(0, 1)?;
        let mut pending: Vec<BeatView<'a>> = Vec::new();
        for beat in voice.beats() {
            if beat.kind() == BeatKind::Empty {
                continue;
            }
            // A grace beat is an ornament for the next beat and takes no time.
            if beat.is_grace() {
                pending.push(beat);
                continue;
            }
            onsets.entry(at).or_default().push((voice_index, beat));
            if !pending.is_empty() {
                graces
                    .entry((voice_index, at))
                    .or_default()
                    .append(&mut pending);
            }
            at = at.checked_add(beat.duration()?)?;
        }
        if !pending.is_empty() {
            // Grace notes at the end of the measure keep a column of their own, before the
            // barline.
            onsets.entry(at).or_default();
            graces
                .entry((voice_index, at))
                .or_default()
                .append(&mut pending);
        }
        end = end.max(at);
    }
    Ok(BeatStarts {
        onsets,
        graces,
        end,
    })
}

/// The fret numbers of a column's beats; a rest if no beat has notes. `first_column` marks
/// the measure's first column, where a tie carried across a barline writes `(7)`.
pub(super) fn column_items(
    beats: &[BeatView<'_>],
    ctx: &Context<'_>,
    first_column: bool,
) -> Result<ColumnData, LayoutError> {
    let mut items = Vec::new();
    let mut marks = ColumnMarks::default();
    for beat in beats {
        if let Some(legato) = beat.legato() {
            marks.legato_origin = marks.legato_origin || legato.origin;
            marks.legato_destination = marks.legato_destination || legato.destination;
        }
        if marks.whammy.is_none() {
            marks.whammy = beat.whammy_shape();
        }
        marks.add_vibrato(beat.vibrato());
        if beat.kind() == BeatKind::Normal {
            marks.pick = marks.pick.or(beat.pick_stroke());
            marks.brush = marks.brush.or(beat.brush_stroke());
            marks.slap = marks.slap.or(beat.slap());
            marks.fade = marks.fade.or(beat.fade());
            marks.arpeggio = marks.arpeggio.or(beat.arpeggio());
            marks.rasgueado |= beat.has_rasgueado();
        }
        marks.dead_slapped |= beat.is_dead_slapped();
    }
    for beat in beats
        .iter()
        .filter(|b| b.kind() == BeatKind::Normal && !b.is_dead_slapped())
    {
        for note in beat.notes() {
            // A fret the model cannot read (it lists those as issues) is shown as `?`, so
            // one odd note does not hide the whole track.
            let fret = || {
                note.fret()
                    .map_or_else(|_| UNREADABLE.to_owned(), |f| f.get().to_string())
            };
            let fret_value = note.fret().ok().map(|f| f.get());
            // A tie continuation is not written in tablature: the origin shows the number
            // and a curve marks the tie. Its column stays so the curve has an endpoint.
            let (base, tie_destination) = match note.kind() {
                NoteKind::Normal => (fret(), false),
                // A tie carried into a measure writes its fret in parentheses in the
                // measure's first column; within the measure it stays hidden.
                NoteKind::Tie => {
                    if first_column {
                        (format!("({})", fret()), true)
                    } else {
                        (String::new(), true)
                    }
                }
                NoteKind::Dead => ("x".to_owned(), false),
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
            if tie_destination {
                let width = ctx.fonts.text.width(&base, ctx.options.fret_size);
                items.push(Item::Fret(Fret {
                    line,
                    text: base,
                    width,
                    marks: NoteMarks::new(),
                    fret: fret_value,
                    tie_destination: true,
                }));
                continue;
            }
            let mut note_marks = NoteMarks::new();
            note_marks.hammer = note.is_hammer();
            note_marks.bend = note.bend();
            note_marks.slides = note.slides().collect();
            note_marks.harmonic = note.harmonic();
            marks.add_vibrato(note.vibrato());
            note_marks.accent = note.accent();
            note_marks.staccato = note.is_staccato();
            note_marks.trill = note.trill();
            note_marks.tremolo = note.tremolo_speed();
            note_marks.pick_slide = note
                .slides()
                .find(|slide| matches!(slide, SlideKind::PickSlideDown | SlideKind::PickSlideUp));
            note_marks.left_hand_tap = note.is_left_hand_tapped();
            note_marks.string_number = note
                .shows_string_number()
                .then(|| note.string().ok().map(|s| s.get()))
                .flatten();
            note_marks.finger_left = note.finger(Hand::Left).map(|f| f.sign(Hand::Left));
            note_marks.finger_right = note.finger(Hand::Right).map(|f| f.sign(Hand::Right));
            note_marks.palm_mute = note.is_palm_muted();
            note_marks.let_ring = note.is_let_ring();
            let mut text = ornaments::fret_text(&base, &note_marks);
            if note.is_ghost() {
                text = format!("({text})");
            }
            let width = ctx.fonts.text.width(&text, ctx.options.fret_size);
            items.push(Item::Fret(Fret {
                line,
                text,
                width,
                marks: note_marks,
                fret: fret_value,
                tie_destination: false,
            }));
        }
    }
    // With notation shown, rests are written there only, as in Guitar Pro. A dead slapped
    // beat writes a large cross instead of any rest.
    if items.is_empty()
        && !marks.dead_slapped
        && ctx.notation.is_none()
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
    let text = beats
        .iter()
        .map(|beat| beat.text().trim())
        .find(|text| !text.is_empty())
        .map(str::to_owned);
    Ok(ColumnData {
        items,
        marks,
        notation: Vec::new(),
        graces: Vec::new(),
        grace_width: 0.0,
        beat: 0,
        tuplet: None,
        beats: Vec::new(),
        text,
    })
}

/// The beat notes are beamed within: a dotted quarter in compound meters (6/8, 9/8,
/// 12/8), a quarter otherwise.
pub(super) fn beat_length(signature: TimeSignature) -> Result<Duration, LayoutError> {
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
pub(super) fn as_f64(duration: Duration) -> f64 {
    let denominator = duration.denominator() as f64;
    if denominator > 0.0 {
        duration.numerator() as f64 / denominator
    } else {
        0.0
    }
}
