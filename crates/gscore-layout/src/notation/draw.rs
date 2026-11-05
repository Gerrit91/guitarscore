//! Painting a measure of standard notation: noteheads, ledger lines, accidentals, stems,
//! flags, beams, dots, rests, ties and tuplet numbers, and the marks by the heads
//! ([`super::marks`]).

use super::beams::{beam_groups, draw_beams, draw_ties};
use super::marks;
use super::pitch::key_alters;
use super::{Alter, Chord, Clef, DOT_GAP, MIDDLE, NotationItem};
use crate::Context;
use crate::draw::{ColorToken, DrawOp, Point, TextAnchor, TextFont};
use crate::glyph::{Flag, Glyph};
use crate::ornaments::{CIRCLE_RADIUS, draw_circled_symbol, draw_slanted_wiggle};
use crate::staff::Staff;

/// Normal stem length (one octave).
const STEM_LENGTH: f32 = 3.5;
/// Gap between an accidental and its notehead.
const ACCIDENTAL_GAP: f32 = 0.25;
/// Ledger lines start two positions outside the staff.
const LEDGER_BELOW: i32 = -2;
const LEDGER_ABOVE: i32 = 10;
/// Space between the system start and the clef, and after the clef.
const CLEF_GAPS: (f32, f32) = (0.6, 1.0);
/// Half the length of a rhythm slash along its stroke, and the stroke's thickness.
const SLASH_HALF: f32 = 0.55;
const SLASH_THICKNESS: f32 = 0.24;
/// Space between a clef and a key signature, and between a key signature and the first
/// measure.
pub(crate) const KEY_GAPS: (f32, f32) = (0.3, 0.6);

/// A key signature as accidentals on staff positions, with the room it takes. Signatures
/// follow the clef when the notation staff is shown; a tab-only score draws them centered
/// on the tab staff.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KeySigLayout {
    glyphs: Vec<(Glyph, i32)>,
    /// Whether the positions are drawn on a tab staff (no pitch lines) instead of notation.
    on_tab: bool,
    /// Room from its left edge to its right edge.
    pub(crate) width: f32,
}

impl KeySigLayout {
    /// The key signature that changes the key from `previous` to `key`: naturals cancel
    /// what the old signature altered and the new one does not, then the new accidentals.
    /// `on_tab` centers the accidentals on a tab staff; otherwise `clef` places them.
    pub(crate) fn change(
        clef: Clef,
        previous: i8,
        key: i8,
        on_tab: bool,
        ctx: &Context<'_>,
    ) -> Option<Self> {
        let new_alters = key_alters(key);
        let mut glyphs = Vec::new();
        for (letter, position) in clef.key_signature_marks(previous) {
            let natural_now = new_alters
                .get(letter)
                .is_none_or(|alter| *alter == Alter::Natural);
            if natural_now {
                glyphs.push((Glyph::Natural, position));
            }
        }
        for (letter, position) in clef.key_signature_marks(key) {
            let alter = new_alters.get(letter).copied().unwrap_or(Alter::Natural);
            glyphs.push((alter.glyph(), position));
        }
        Self::from_glyphs(glyphs, on_tab, ctx)
    }

    /// The plain key signature of `key` (no cancellations).
    pub(crate) fn plain(clef: Clef, key: i8, on_tab: bool, ctx: &Context<'_>) -> Option<Self> {
        if key == 0 {
            return None;
        }
        let alters = key_alters(key);
        let glyphs = clef
            .key_signature_marks(key)
            .into_iter()
            .map(|(letter, position)| {
                let alter = alters.get(letter).copied().unwrap_or(Alter::Natural);
                (alter.glyph(), position)
            })
            .collect();
        Self::from_glyphs(glyphs, on_tab, ctx)
    }

    fn from_glyphs(glyphs: Vec<(Glyph, i32)>, on_tab: bool, ctx: &Context<'_>) -> Option<Self> {
        if glyphs.is_empty() {
            return None;
        }
        let width = glyphs
            .iter()
            .map(|(glyph, _)| ctx.fonts.music.advance(*glyph))
            .sum();
        Some(KeySigLayout {
            glyphs,
            on_tab,
            width,
        })
    }

    /// Draws the signature with its left edge at `x`, on the staff whose top line is `top`.
    pub(crate) fn draw_at(&self, x: f32, top: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
        let mut at = x;
        for (glyph, position) in &self.glyphs {
            let y = if self.on_tab {
                tab_key_y(top, *position, &ctx.staff)
            } else {
                y_of(top, *position)
            };
            ops.push(DrawOp::Glyph {
                glyph: *glyph,
                at: Point::new(at, y),
                scale: 1.0,
                color: ColorToken::Notation,
            });
            at += ctx.fonts.music.advance(*glyph);
        }
    }
}

/// The y of staff position `position` on a tab staff: a five-line frame centered on the
/// staff, since tablature has no pitch lines of its own.
fn tab_key_y(top: f32, position: i32, tab: &Staff) -> f32 {
    let frame_top = top + (tab.height() - 4.0) / 2.0;
    y_of(frame_top, position)
}

/// A notation item at its column's center `x`; `beat` is the beat it starts in (beams
/// join notes within one beat).
#[derive(Debug, Clone, Copy)]
pub(crate) struct BeamPlan<'m> {
    pub(crate) x: f32,
    pub(crate) item: &'m NotationItem,
    pub(crate) beat: u64,
}

/// The staff positions of the ledger lines a head at `position` needs (lines between the
/// staff and the head).
pub(crate) fn ledger_positions(position: i32) -> Vec<i32> {
    if position <= LEDGER_BELOW {
        (position..=LEDGER_BELOW).filter(|p| p % 2 == 0).collect()
    } else if position >= LEDGER_ABOVE {
        (LEDGER_ABOVE..=position).filter(|p| p % 2 == 0).collect()
    } else {
        Vec::new()
    }
}

/// The y of staff position `position` on a notation staff whose top line is at `top`.
pub(crate) fn y_of(top: f32, position: i32) -> f32 {
    top + 4.0 - position as f32 * 0.5
}

/// Tie curves: distance from the notehead and height of the arch.
const TIE_OFFSET: f32 = 0.6;
const TIE_ARCH: f32 = 0.5;

/// Draws a tie between two noteheads, curving away from the stems (`up` for an up-stem).
pub(crate) fn draw_tie_curve(
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    up: bool,
    thickness: f32,
    ops: &mut Vec<DrawOp>,
) {
    if to_x <= from_x {
        return;
    }
    let direction = if up { 1.0 } else { -1.0 };
    let handle = (to_x - from_x) / 3.0;
    let y = |at: f32| at + direction * TIE_OFFSET;
    ops.push(DrawOp::Curve {
        from: Point::new(from_x, y(from_y)),
        to: Point::new(to_x, y(to_y)),
        control1: Point::new(from_x + handle, y(from_y) + direction * TIE_ARCH),
        control2: Point::new(to_x - handle, y(to_y) + direction * TIE_ARCH),
        thickness,
        color: ColorToken::Notation,
    });
}

/// Room a clef change takes at a measure start: the clef and the gap after it.
pub(crate) fn clef_change_width(clef: Clef, ctx: &Context<'_>) -> f32 {
    let (glyph, _) = clef.glyph();
    CLEF_GAPS.0 + ctx.fonts.music.advance(glyph) + CLEF_GAPS.1
}

/// Draws a clef change at a measure start whose left edge is `x`.
pub(crate) fn draw_clef_change(x: f32, top: f32, clef: Clef, ops: &mut Vec<DrawOp>) {
    let (glyph, position) = clef.glyph();
    ops.push(DrawOp::Glyph {
        glyph,
        at: Point::new(x + CLEF_GAPS.0, y_of(top, position)),
        scale: 1.0,
        color: ColorToken::Notation,
    });
}

/// Room from a system start to where a key signature starts: the clef and the gap after it.
pub(crate) fn key_start_width(clef: Clef, ctx: &Context<'_>) -> f32 {
    let (glyph, _) = clef.glyph();
    CLEF_GAPS.0 + ctx.fonts.music.advance(glyph) + KEY_GAPS.0
}

/// The room the clef takes at the start of a system; with a key signature that starts the
/// system, the signature follows after [`KEY_GAPS`].
pub(crate) fn prefix_width(clef: Clef, key: i8, on_tab: bool, ctx: &Context<'_>) -> f32 {
    let (glyph, _) = clef.glyph();
    let clef_width = CLEF_GAPS.0 + ctx.fonts.music.advance(glyph);
    match KeySigLayout::plain(clef, key, on_tab, ctx) {
        Some(key_signature) => clef_width + KEY_GAPS.0 + key_signature.width + KEY_GAPS.1,
        None => clef_width + CLEF_GAPS.1,
    }
}

/// Draws the clef at the start of a system whose left edge is `x`, then the key signature
/// unless the first measure draws its own (a key change) at the measure start.
pub(crate) fn draw_staff_start(
    x: f32,
    top: f32,
    clef: Clef,
    key: i8,
    key_drawn_by_measure: bool,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let (glyph, position) = clef.glyph();
    let clef_width = CLEF_GAPS.0 + ctx.fonts.music.advance(glyph);
    ops.push(DrawOp::Glyph {
        glyph,
        at: Point::new(x + CLEF_GAPS.0, y_of(top, position)),
        scale: 1.0,
        color: ColorToken::Notation,
    });
    if key_drawn_by_measure {
        return;
    }
    if let Some(key_signature) = KeySigLayout::plain(clef, key, false, ctx) {
        key_signature.draw_at(x + clef_width + KEY_GAPS.0, top, ctx, ops);
    }
}

/// Room a tab staff's system start takes: the TAB clef, then the key signature unless the
/// first measure draws it (a key change).
pub(crate) fn tab_prefix_width(key: i8, key_drawn_by_measure: bool, ctx: &Context<'_>) -> f32 {
    let clef = ctx.staff.clef_width(&ctx.fonts.music);
    if key_drawn_by_measure {
        return clef + KEY_GAPS.0;
    }
    match KeySigLayout::plain(Clef::Treble, key, true, ctx) {
        Some(key_signature) => clef + KEY_GAPS.0 + key_signature.width + KEY_GAPS.1,
        None => ctx.staff.prefix_width(&ctx.fonts.music),
    }
}

/// Draws the TAB clef and, unless the first tab-only measure draws its own, the key
/// signature, at the start of a system whose left edge is `x`.
pub(crate) fn draw_tab_staff_start(
    x: f32,
    top: f32,
    key: i8,
    key_drawn_by_measure: bool,
    show_key: bool,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    ctx.staff.draw_clef(x, top, &ctx.fonts.music, ops);
    if !show_key || key_drawn_by_measure {
        return;
    }
    if let Some(key_signature) = KeySigLayout::plain(Clef::Treble, key, true, ctx) {
        let at = x + ctx.staff.clef_width(&ctx.fonts.music) + KEY_GAPS.0;
        key_signature.draw_at(at, top, ctx, ops);
    }
}

/// Where a chord's stem goes.
#[derive(Debug, Clone, Copy)]
pub(super) struct Stem {
    pub(super) up: bool,
    pub(super) x: f32,
    /// The end at the noteheads.
    pub(super) from: f32,
    /// The free end (at the beam or flag).
    pub(super) to: f32,
}

/// Draws one measure of notation on the staff whose top line is at `top`. Returns the
/// highest point of each item (heads, stems and marks), in the order of `placed`, so the
/// marks above the staff can clear high notes. `show_brush` draws the brush arrow here;
/// with a tab below it is drawn there instead (Guitar Pro draws it on the tab only).
pub(crate) fn draw_measure(
    placed: &[BeamPlan<'_>],
    top: f32,
    show_brush: bool,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) -> Vec<f32> {
    let mut ceilings = vec![f32::MAX; placed.len()];
    let groups = beam_groups(placed);
    let mut stems: Vec<Option<Stem>> = vec![None; placed.len()];
    let mut beamed = vec![false; placed.len()];

    // Beamed groups share a direction and a (horizontal) beam line.
    for group in &groups {
        let chords: Vec<(usize, &Chord)> = group
            .iter()
            .filter_map(|&i| chord_at(placed, i).map(|c| (i, c)))
            .collect();
        let up = chords
            .iter()
            .find_map(|(_, c)| c.stem_up)
            .unwrap_or_else(|| mean_position(chords.iter().map(|(_, c)| *c)) < MIDDLE as f32);
        let beam_y = chords.iter().map(|(_, c)| free_end(c, up, top)).fold(
            if up { f32::MAX } else { f32::MIN },
            |a, b| {
                if up { a.min(b) } else { a.max(b) }
            },
        );
        for &(i, chord) in &chords {
            if let Some(p) = placed.get(i) {
                let mut stem = stem_for(chord, p.x, up, top, ctx);
                stem.to = beam_y;
                if let Some(slot) = stems.get_mut(i) {
                    *slot = Some(stem);
                }
                if let Some(flag) = beamed.get_mut(i) {
                    *flag = true;
                }
            }
        }
    }
    // The other chords get their own stem (halves and shorter).
    for (i, p) in placed.iter().enumerate() {
        if let NotationItem::Chord(chord) = p.item
            && chord.value >= 2
            && stems.get(i).is_some_and(Option::is_none)
        {
            let up = chord
                .stem_up
                .unwrap_or_else(|| mean_position(std::iter::once(chord)) < MIDDLE as f32);
            let mut stem = stem_for(chord, p.x, up, top, ctx);
            stem.to = free_end(chord, up, top);
            if let Some(slot) = stems.get_mut(i) {
                *slot = Some(stem);
            }
        }
    }

    for (i, p) in placed.iter().enumerate() {
        let stem = stems.get(i).copied().flatten();
        match p.item {
            NotationItem::Rest { glyph, dots, .. } => draw_rest(p.x, *glyph, *dots, top, ctx, ops),
            NotationItem::Chord(chord) => {
                let up = stem.is_none_or(|s| s.up);
                draw_heads(p.x, chord, up, top, ctx, ops);
                let mut highest = marks::draw_articulations(chord, p.x, stem, top, ctx, ops);
                if let Some(head) = chord.heads.last() {
                    highest = highest.min(y_of(top, head.position) - 0.5);
                }
                if let Some(stem) = stem {
                    highest = highest.min(stem.to.min(stem.from));
                }
                if let Some(slot) = ceilings.get_mut(i) {
                    *slot = highest;
                }
                if show_brush {
                    marks::draw_brush(chord, p.x, top, ctx, ops);
                }
                marks::draw_arpeggio(chord, p.x, top, ctx, ops);
                if let Some(stem) = stem {
                    ops.push(DrawOp::Line {
                        from: Point::new(stem.x, stem.from),
                        to: Point::new(stem.x, stem.to),
                        thickness: ctx.fonts.music.engraving().stem_thickness,
                        color: ColorToken::Notation,
                    });
                    let unbeamed = !beamed.get(i).copied().unwrap_or(false);
                    if unbeamed && let Some(flag) = Flag::new(chord.beams(), stem.up) {
                        let half = ctx.fonts.music.engraving().stem_thickness / 2.0;
                        ops.push(DrawOp::Glyph {
                            glyph: Glyph::Flag(flag),
                            at: Point::new(stem.x - half, stem.to),
                            scale: 1.0,
                            color: ColorToken::Notation,
                        });
                    }
                }
            }
        }
    }
    for group in &groups {
        draw_beams(group, placed, &stems, ctx, ops);
    }
    super::tuplets::draw_tuplets(placed, &stems, &groups, top, ops);
    draw_ties(placed, &stems, top, ctx, ops);
    marks::draw_links(placed, &stems, top, ctx, ops);
    draw_string_numbers(placed, top, ctx, ops);
    draw_pick_slides(placed, top, ctx, ops);
    ceilings
}

/// Circled string numbers above or below the staff: low notes take them above the staff,
/// high notes below, as Guitar Pro writes them.
fn draw_string_numbers(
    placed: &[BeamPlan<'_>],
    top: f32,
    ctx: &Context<'_>,
    ops: &mut Vec<DrawOp>,
) {
    let height = super::staff().height();
    for p in placed {
        let NotationItem::Chord(chord) = p.item else {
            continue;
        };
        for head in &chord.heads {
            let Some(number) = head.string_number else {
                continue;
            };
            let above = head.position <= MIDDLE;
            let y = if above { top - 2.4 } else { top + height + 1.4 };
            draw_circled_symbol(&number.to_string(), p.x, y, CIRCLE_RADIUS, ctx, ops);
        }
    }
}

/// A wavy line from a pick-slide note's head to the next chord's head on the same staff.
fn draw_pick_slides(placed: &[BeamPlan<'_>], top: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
    let font = &ctx.fonts.music;
    for (index, p) in placed.iter().enumerate() {
        let NotationItem::Chord(chord) = p.item else {
            continue;
        };
        let Some(head) = chord.heads.iter().find(|head| head.pick_slide) else {
            continue;
        };
        let width = font.advance(chord.head_glyph(head));
        let start = p.x + width / 2.0 + 0.15;
        let (end, target) = match placed.get(index.saturating_add(1)) {
            Some(next) => match next.item {
                NotationItem::Chord(next_chord) => {
                    let next_head = next_chord
                        .heads
                        .iter()
                        .find(|next_head| next_head.string == head.string)
                        .or_else(|| next_chord.heads.first());
                    let Some(next_head) = next_head else {
                        continue;
                    };
                    (
                        next.x - font.advance(next_chord.head_glyph(next_head)) / 2.0 - 0.15,
                        y_of(top, next_head.position),
                    )
                }
                NotationItem::Rest { .. } => (next.x - 1.0, y_of(top, head.position)),
            },
            None => (start + 2.0, y_of(top, head.position)),
        };
        if end > start {
            // The scrape follows the pitch change, so it slants like the reference.
            let from = Point::new(start, y_of(top, head.position));
            draw_slanted_wiggle(from, Point::new(end, target), 0.16, 0.45, ops);
        }
    }
}

pub(super) fn chord_at<'m>(placed: &[BeamPlan<'m>], index: usize) -> Option<&'m Chord> {
    match placed.get(index)?.item {
        NotationItem::Chord(chord) => Some(chord),
        NotationItem::Rest { .. } => None,
    }
}

/// The average staff position of the chords' noteheads.
fn mean_position<'c>(chords: impl Iterator<Item = &'c Chord>) -> f32 {
    let (sum, count) = chords
        .flat_map(|c| c.heads.iter())
        .fold((0.0_f32, 0.0_f32), |(sum, count), h| {
            (sum + h.position as f32, count + 1.0)
        });
    if count > 0.0 {
        sum / count
    } else {
        MIDDLE as f32
    }
}

/// Where a chord's stem would end on its own: an octave beyond the outermost head, but at
/// least at the middle line.
fn free_end(chord: &Chord, up: bool, top: f32) -> f32 {
    let middle = y_of(top, MIDDLE);
    if up {
        let highest = chord.heads.last().map_or(MIDDLE, |h| h.position);
        (y_of(top, highest) - STEM_LENGTH).min(middle)
    } else {
        let lowest = chord.heads.first().map_or(MIDDLE, |h| h.position);
        (y_of(top, lowest) + STEM_LENGTH).max(middle)
    }
}

/// The stem's x and its end at the noteheads, from the notehead's stem anchors.
fn stem_for(chord: &Chord, x: f32, up: bool, top: f32, ctx: &Context<'_>) -> Stem {
    if chord.slash {
        // The stem leaves the slash at its upper or lower end, not through its middle.
        let y = y_of(top, MIDDLE);
        let half = SLASH_HALF;
        return Stem {
            up,
            x: x + half,
            from: if up { y - half } else { y + half },
            to: 0.0,
        };
    }
    let font = &ctx.fonts.music;
    let glyph = chord
        .heads
        .first()
        .map_or(Glyph::NoteheadBlack, |h| chord.head_glyph(h));
    let width = font.advance(glyph);
    let left = x - width / 2.0;
    let half = font.engraving().stem_thickness / 2.0;
    if up {
        let (ax, ay) = font.anchor(glyph, "stemUpSE").unwrap_or((width, 0.168));
        let lowest = chord.heads.first().map_or(MIDDLE, |h| h.position);
        Stem {
            up,
            x: left + ax - half,
            from: y_of(top, lowest) - ay,
            to: 0.0,
        }
    } else {
        let (ax, ay) = font.anchor(glyph, "stemDownNW").unwrap_or((0.0, -0.168));
        let highest = chord.heads.last().map_or(MIDDLE, |h| h.position);
        Stem {
            up,
            x: left + ax + half,
            from: y_of(top, highest) - ay,
            to: 0.0,
        }
    }
}

/// Noteheads (seconds side by side), ledger lines, accidentals and dots.
fn draw_heads(x: f32, chord: &Chord, up: bool, top: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
    if chord.slash {
        // Rhythm slash: a thick stroke centered on the middle line, regardless of pitch.
        let y = y_of(top, MIDDLE);
        ops.push(DrawOp::Line {
            from: Point::new(x - SLASH_HALF, y + SLASH_HALF),
            to: Point::new(x + SLASH_HALF, y - SLASH_HALF),
            thickness: SLASH_THICKNESS,
            color: ColorToken::Notation,
        });
        return;
    }
    let font = &ctx.fonts.music;
    let engraving = font.engraving();
    // Which heads move beside the stem: in a second, the upper one with an up-stem, the
    // lower one with a down-stem.
    let mut shifted = vec![false; chord.heads.len()];
    let order: Vec<usize> = if up {
        (0..chord.heads.len()).collect()
    } else {
        (0..chord.heads.len()).rev().collect()
    };
    for pair in order.windows(2) {
        if let [a, b] = pair
            && let (Some(ha), Some(hb)) = (chord.heads.get(*a), chord.heads.get(*b))
            && ha.position.abs_diff(hb.position) == 1
            && !shifted.get(*a).copied().unwrap_or(false)
            && let Some(flag) = shifted.get_mut(*b)
        {
            *flag = true;
        }
    }
    for (index, head) in chord.heads.iter().enumerate() {
        let glyph = chord.head_glyph(head);
        let width = font.advance(glyph);
        let shift = if shifted.get(index).copied().unwrap_or(false) {
            if up { width } else { -width }
        } else {
            0.0
        };
        let left = x - width / 2.0 + shift;
        let y = y_of(top, head.position);
        // Ledger lines between the staff and the note.
        for position in ledger_positions(head.position) {
            let ly = y_of(top, position);
            ops.push(DrawOp::Line {
                from: Point::new(left - engraving.leger_line_extension, ly),
                to: Point::new(left + width + engraving.leger_line_extension, ly),
                thickness: engraving.leger_line_thickness,
                color: ColorToken::Staff,
            });
        }
        ops.push(DrawOp::Glyph {
            glyph,
            at: Point::new(left, y),
            scale: 1.0,
            color: ColorToken::Notation,
        });
        // A ghost note's head in parentheses; its accidental and dots move out of the way.
        let paren = if head.ghost {
            let (open, close) = (Glyph::ParenthesisLeft, Glyph::ParenthesisRight);
            ops.push(DrawOp::Glyph {
                glyph: open,
                at: Point::new(left - PAREN_GAP - font.advance(open), y),
                scale: 1.0,
                color: ColorToken::Notation,
            });
            ops.push(DrawOp::Glyph {
                glyph: close,
                at: Point::new(left + width + PAREN_GAP - font.bbox(close).x_min, y),
                scale: 1.0,
                color: ColorToken::Notation,
            });
            paren_room(font)
        } else {
            0.0
        };
        let mut left_edge = (x - width / 2.0).min(left) - paren;
        if let Some(accidental) = head.accidental {
            // Left of the head, or of the column when the head moved right.
            let base = left_edge;
            let accidental_x = base - ACCIDENTAL_GAP - font.advance(accidental);
            ops.push(DrawOp::Glyph {
                glyph: accidental,
                at: Point::new(accidental_x, y),
                scale: 1.0,
                color: ColorToken::Notation,
            });
            left_edge = accidental_x;
        }
        // Fingering signs sit just left of the head (and of its accidental).
        let cap = ctx.fonts.text.cap_height * super::FINGER_SIZE;
        let mut finger_x = left_edge - super::FINGER_GAP;
        for finger in [head.finger_left, head.finger_right].into_iter().flatten() {
            ops.push(DrawOp::Text {
                text: finger.to_owned(),
                at: Point::new(finger_x, y + cap / 2.0),
                size: super::FINGER_SIZE,
                font: TextFont::Regular,
                anchor: TextAnchor::End,
                color: ColorToken::Notation,
            });
            finger_x -= ctx.fonts.text.width(finger, super::FINGER_SIZE) + super::FINGER_GAP;
        }
        // Dots sit in a space: a note on a line gets its dot in the space above.
        let dot_y = if head.position % 2 == 0 {
            y_of(top, head.position.saturating_add(1))
        } else {
            y
        };
        let mut dot_x = x + width / 2.0 + shift.max(0.0) + paren + DOT_GAP;
        for _ in 0..chord.dots {
            draw_dot(dot_x, dot_y, ctx, ops);
            dot_x += font.advance(Glyph::AugmentationDot) + DOT_GAP;
        }
    }
}

/// Gap between a ghost note's head and its parentheses.
const PAREN_GAP: f32 = 0.08;

/// Room one parenthesis of a ghost note takes beside the head.
pub(super) fn paren_room(font: &crate::metrics::MusicFont) -> f32 {
    font.advance(Glyph::ParenthesisLeft) + PAREN_GAP
}

fn draw_dot(x: f32, y: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
    let dot = Glyph::AugmentationDot;
    ops.push(DrawOp::Glyph {
        glyph: dot,
        at: Point::new(x, y + ctx.fonts.music.bbox(dot).middle_above_origin()),
        scale: 1.0,
        color: ColorToken::Notation,
    });
}

/// A rest: the whole rest hangs from the fourth line, the half rest sits on the middle
/// line, the others are centered on the staff.
fn draw_rest(x: f32, glyph: Glyph, dots: u8, top: f32, ctx: &Context<'_>, ops: &mut Vec<DrawOp>) {
    let font = &ctx.fonts.music;
    let bbox = font.bbox(glyph);
    let y = match glyph {
        Glyph::RestWhole => y_of(top, MIDDLE + 2),
        Glyph::RestHalf => y_of(top, MIDDLE),
        _ => y_of(top, MIDDLE) + bbox.middle_above_origin(),
    };
    let left = x - (bbox.x_min + bbox.x_max) / 2.0;
    ops.push(DrawOp::Glyph {
        glyph,
        at: Point::new(left, y),
        scale: 1.0,
        color: ColorToken::Notation,
    });
    let mut dot_x = left + font.advance(glyph) + DOT_GAP;
    for _ in 0..dots {
        draw_dot(dot_x, y_of(top, MIDDLE + 1), ctx, ops);
        dot_x += font.advance(Glyph::AugmentationDot) + DOT_GAP;
    }
}
