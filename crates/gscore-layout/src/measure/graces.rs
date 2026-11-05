//! Grace notes: read from the model, measured, and drawn small before their main note.
//!
//! On the notation staff a grace note is a cue-sized head with its own accidental, an up
//! stem and an eighth flag; an acciaccatura (played before the beat) has a slash through
//! stem and flag. On the tab it is a smaller fret number. The group sits left of the main
//! note's own left edge (its accidental included), so the two never overlap.

use gscore_model::{Grace as ModelGrace, NoteKind, NoteView};

use super::{Column, Tops, UNREADABLE};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, Rect, TextAnchor, TextFont};
use crate::glyph::{Flag, Glyph};
use crate::notation;

/// Scale of grace notes on the notation staff against normal notes (cue size).
pub(super) const GRACE_SCALE: f32 = 0.65;
/// Scale of grace fret numbers on the tab against normal ones: small enough to tell apart,
/// large enough to read.
const TAB_SCALE: f32 = 0.78;
/// Gap after a grace group, past its flag and slash, before the next group or the main
/// note.
const GAP: f32 = 0.3;
/// How far the acciaccatura slash reaches right of the stem.
const SLASH_RIGHT: f32 = 0.6;
/// Space between a grace accidental and its head, before scaling.
const ACCIDENTAL_GAP: f32 = 0.2;
/// Stem length of a grace note, before scaling (a normal stem is 3.5 spaces).
const STEM_LENGTH: f32 = 3.5;
/// Paper kept clear around a grace fret number, so the string line does not cross it.
const TAB_PAD: f32 = 0.1;

/// A grace note drawn just before a column's main note.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Grace {
    /// The grace notes that share a stem (one grace beat, or one attached grace).
    pub(super) group: usize,
    /// Staff position on the notation staff, when it can be worked out.
    pub(super) position: Option<i32>,
    /// The accidental before the head (against the key signature).
    pub(super) accidental: Option<Glyph>,
    /// The tab staff's line, when the string is on the staff.
    pub(super) line: Option<usize>,
    /// The fret number (or `x`) for the tab.
    pub(super) text: String,
    /// The fret, when the model can read one (for the H/P of a hammer-on).
    pub(super) fret: Option<u8>,
    pub(super) dead: bool,
    /// Played on the beat (an appoggiatura, without a slash).
    pub(super) on_beat: bool,
    /// Played with a hammer-on or pull-off into the main note.
    pub(super) hammer: bool,
}

/// A grace note from a GP6/GP7 grace beat: the note itself carries string and fret.
pub(super) fn from_note(note: NoteView<'_>, ctx: &Context<'_>, key: i8) -> Option<Grace> {
    let string = note.string().ok()?;
    let effect = note.grace();
    let dead = note.kind() == NoteKind::Dead || effect.is_some_and(|grace| grace.dead);
    let placed = ctx.notation.and_then(|notation| {
        if notation.clef == notation::Clef::Percussion {
            return None;
        }
        let pitch = notation.track.sounding_pitch(&note).ok()?;
        Some(notation.clef.place_grace(pitch.get(), key))
    });
    let fret = note.fret().ok().map(|fret| fret.get());
    let text = if dead {
        "x".to_owned()
    } else {
        fret.map_or_else(|| UNREADABLE.to_owned(), |fret| fret.to_string())
    };
    Some(grace(
        ctx,
        string.get(),
        placed,
        text,
        fret,
        dead,
        effect.is_some_and(|grace| grace.on_beat),
        effect.is_some_and(|grace| grace.hammer),
    ))
}

/// A grace note attached to a normal note.
pub(super) fn from_effect(
    note: NoteView<'_>,
    effect: ModelGrace,
    ctx: &Context<'_>,
    key: i8,
) -> Option<Grace> {
    let string = note.string().ok()?;
    let placed = ctx.notation.and_then(|notation| {
        if notation.clef == notation::Clef::Percussion {
            return None;
        }
        let pitch = notation
            .track
            .pitch_at(string, effect.fret)
            .or_else(|_| notation.track.sounding_pitch(&note))
            .ok()?;
        Some(notation.clef.place_grace(pitch.get(), key))
    });
    let text = if effect.dead {
        "x".to_owned()
    } else {
        effect.fret.get().to_string()
    };
    Some(grace(
        ctx,
        string.get(),
        placed,
        text,
        Some(effect.fret.get()),
        effect.dead,
        effect.on_beat,
        effect.hammer,
    ))
}

#[allow(clippy::too_many_arguments)]
fn grace(
    ctx: &Context<'_>,
    string: u8,
    placed: Option<(i32, Option<Glyph>)>,
    text: String,
    fret: Option<u8>,
    dead: bool,
    on_beat: bool,
    hammer: bool,
) -> Grace {
    let line = usize::from(string).saturating_sub(1);
    Grace {
        group: 0,
        position: placed.map(|(position, _)| position),
        accidental: placed.and_then(|(_, accidental)| accidental),
        line: (line < ctx.staff.lines()).then_some(line),
        text,
        fret,
        dead,
        on_beat,
        hammer,
    }
}

/// The groups of `graces` in order, each with its notes.
fn groups(graces: &[Grace]) -> Vec<Vec<&Grace>> {
    let mut groups: Vec<Vec<&Grace>> = Vec::new();
    for grace in graces {
        match groups.iter_mut().find(|group| {
            group
                .first()
                .is_some_and(|first| first.group == grace.group)
        }) {
            Some(group) => group.push(grace),
            None => groups.push(vec![grace]),
        }
    }
    groups
}

/// How one grace group is measured: room for accidentals, then the body (head or fret
/// number, whichever is wider) centered after it.
struct GroupSize {
    accidental: f32,
    head: f32,
    body: f32,
    /// What sticks out right of the body: the flag and the slash.
    trail: f32,
}

impl GroupSize {
    fn of(notes: &[&Grace], ctx: &Context<'_>) -> Self {
        let font = &ctx.fonts.music;
        let with_head = notes.iter().filter(|grace| grace.position.is_some());
        let head = with_head
            .clone()
            .map(|grace| font.advance(head_glyph(grace)) * GRACE_SCALE)
            .fold(0.0, f32::max);
        let accidental = with_head
            .filter_map(|grace| grace.accidental)
            .map(|glyph| (font.advance(glyph) + ACCIDENTAL_GAP) * GRACE_SCALE)
            .fold(0.0, f32::max);
        let tab = notes
            .iter()
            .filter(|grace| grace.line.is_some())
            .map(|grace| tab_width(grace, ctx) + 2.0 * TAB_PAD)
            .fold(0.0, f32::max);
        let body = head.max(tab);
        let trail = if head > 0.0 {
            let flag = Flag::new(1, true).map_or(0.0, |flag| font.advance(Glyph::Flag(flag)));
            // The stem stands at the head's right edge; the flag and slash start there.
            let reach = (flag * GRACE_SCALE).max(SLASH_RIGHT);
            (reach - (body - head) / 2.0).max(0.0)
        } else {
            0.0
        };
        GroupSize {
            accidental,
            head,
            body,
            trail,
        }
    }

    /// The group's width, with what sticks out and the gap after it.
    fn width(&self) -> f32 {
        self.accidental + self.body + self.trail + GAP
    }
}

fn head_glyph(grace: &Grace) -> Glyph {
    if grace.dead {
        Glyph::NoteheadX
    } else {
        Glyph::NoteheadBlack
    }
}

fn tab_size(ctx: &Context<'_>) -> f32 {
    ctx.options.fret_size * TAB_SCALE
}

pub(super) fn tab_width(grace: &Grace, ctx: &Context<'_>) -> f32 {
    ctx.fonts.text.width(&grace.text, tab_size(ctx))
}

/// Room the grace-note groups take before a column's main note, including the gap to it.
pub(super) fn width(graces: &[Grace], ctx: &Context<'_>) -> f32 {
    groups(graces)
        .iter()
        .map(|notes| GroupSize::of(notes, ctx).width())
        .sum()
}

/// Grace notes of one column at `x`, left of its main note, on the notation staff and the
/// tab. Notes of one grace beat share a stem.
pub(super) fn draw(x: f32, column: &Column, tops: Tops, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
    if column.graces.is_empty() {
        return;
    }
    let mut at = x - column.main_left - column.grace_width;
    for notes in groups(&column.graces) {
        let size = GroupSize::of(&notes, ctx);
        let center = at + size.accidental + size.body / 2.0;
        if let Some(top) = tops.notation {
            draw_heads(center, top, &notes, &size, ctx, ops);
        }
        if tops.show_tab {
            for grace in &notes {
                if let Some(line) = grace.line {
                    draw_fret(center, tops.tab, line, grace, ctx, ops);
                }
            }
        }
        super::grace_marks::draw_hammer(center, x, tops, column, &notes, ctx, ops);
        at += size.width();
    }
}

/// The heads of one grace group, centered at `x`, with ledger lines, accidentals, one up
/// stem, an eighth flag and (unless it is an appoggiatura) the slash of an acciaccatura.
fn draw_heads(
    x: f32,
    top: f32,
    notes: &[&Grace],
    size: &GroupSize,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let positions: Vec<i32> = notes.iter().filter_map(|grace| grace.position).collect();
    let (Some(&low), Some(&high)) = (positions.iter().min(), positions.iter().max()) else {
        return;
    };
    let font = &ctx.fonts.music;
    let engraving = font.engraving();
    let left = x - size.head / 2.0;
    let mut ledgers: Vec<i32> = positions
        .iter()
        .flat_map(|&position| notation::ledger_positions(position))
        .collect();
    ledgers.sort_unstable();
    ledgers.dedup();
    let extension = engraving.leger_line_extension * GRACE_SCALE;
    for position in ledgers {
        let y = notation::y_of(top, position);
        ops.push(DrawOp::Line {
            from: Point::new(left - extension, y),
            to: Point::new(left + size.head + extension, y),
            thickness: engraving.leger_line_thickness,
            color: ColorToken::Staff,
        });
    }
    for grace in notes {
        let Some(position) = grace.position else {
            continue;
        };
        let y = notation::y_of(top, position);
        let glyph = head_glyph(grace);
        let width = font.advance(glyph) * GRACE_SCALE;
        ops.push(DrawOp::Glyph {
            glyph,
            at: Point::new(x - width / 2.0, y),
            scale: GRACE_SCALE,
            color: ColorToken::Notation,
        });
        if let Some(accidental) = grace.accidental {
            let offset = (font.advance(accidental) + ACCIDENTAL_GAP) * GRACE_SCALE;
            ops.push(DrawOp::Glyph {
                glyph: accidental,
                at: Point::new(left - offset, y),
                scale: GRACE_SCALE,
                color: ColorToken::Notation,
            });
        }
    }
    // Grace stems point up whatever the staff position, as engravers write them.
    let thickness = engraving.stem_thickness * GRACE_SCALE;
    let stem_x = left + size.head - thickness / 2.0;
    let from = notation::y_of(top, low);
    let length = STEM_LENGTH * GRACE_SCALE;
    let end = notation::y_of(top, high) - length;
    ops.push(DrawOp::Line {
        from: Point::new(stem_x, from),
        to: Point::new(stem_x, end),
        thickness,
        color: ColorToken::Notation,
    });
    if let Some(flag) = Flag::new(1, true) {
        ops.push(DrawOp::Glyph {
            glyph: Glyph::Flag(flag),
            at: Point::new(stem_x - thickness / 2.0, end),
            scale: GRACE_SCALE,
            color: ColorToken::Notation,
        });
    }
    if !notes.iter().all(|grace| grace.on_beat) {
        // The slash crosses stem and flag, rising to the right; the head stays clear.
        let middle = end + length * 0.4;
        ops.push(DrawOp::Line {
            from: Point::new(stem_x - 0.5, middle + 0.4),
            to: Point::new(stem_x + SLASH_RIGHT, middle - 0.4),
            thickness: thickness.max(0.1),
            color: ColorToken::Notation,
        });
    }
}

/// A smaller grace fret number on the tab, on paper so the string line does not cross it.
/// The tab carries no slash: at this size it would hide the number.
fn draw_fret(
    x: f32,
    top: f32,
    line: usize,
    grace: &Grace,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let size = tab_size(ctx);
    let line_y = top + ctx.staff.line_y(line);
    let width = tab_width(grace, ctx);
    let cap = ctx.fonts.text.cap_height * size;
    ops.push(DrawOp::Rect {
        rect: Rect {
            origin: Point::new(x - width / 2.0 - TAB_PAD, line_y - cap / 2.0 - TAB_PAD),
            width: width + 2.0 * TAB_PAD,
            height: cap + 2.0 * TAB_PAD,
        },
        color: ColorToken::Paper,
    });
    ops.push(DrawOp::Text {
        text: grace.text.clone(),
        at: Point::new(x, line_y + cap / 2.0),
        size,
        font: TextFont::Regular,
        anchor: TextAnchor::Middle,
        color: ColorToken::FretNumber,
    });
}
