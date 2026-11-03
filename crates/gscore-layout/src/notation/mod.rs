//! Standard notation on a five-line staff above the tab, as Guitar Pro shows it: the same
//! columns as the tab, so a note and its fret number line up.
//!
//! This module turns a column's beats into a [`NotationItem`] (a chord or a rest) and
//! decides accidentals per measure; [`draw`] paints a measure's items (noteheads, ledger
//! lines, stems, flags, beams, dots, rests, ties, tuplet numbers). Only the first voice is
//! written; key signatures, grace notes and articulations are not drawn yet. Drum tracks
//! get a percussion staff instead: each drum sound has its own place and notehead
//! ([`drums`]) and stems point up.

mod beams;
mod draw;
mod drums;
mod pitch;

use std::collections::HashMap;

use gscore_model::{BeatKind, BeatView, NoteKind, TrackView};

pub(crate) use draw::{BeamPlan, draw_measure, draw_staff_start, prefix_width};
pub(crate) use pitch::Clef;

use crate::error::LayoutError;
use crate::glyph::Glyph;
use crate::metrics::MusicFont;
use crate::staff::Staff;

/// Lines of a notation staff.
pub(crate) const LINES: usize = 5;
/// Distance between the notation staff's lines: one staff space, by definition.
pub(crate) const LINE_SPACING: f32 = 1.0;
/// Staff position of the middle line (B4 in treble).
pub(crate) const MIDDLE: i32 = 4;
/// Space between an accidental and its notehead.
const ACCIDENTAL_GAP: f32 = 0.25;
/// Space between a notehead and its first dot, and between dots.
pub(crate) const DOT_GAP: f32 = 0.3;

/// The notation staff's geometry.
pub(crate) fn staff() -> Staff {
    Staff::new(LINES, LINE_SPACING)
}

/// Notation settings of a track: its clef and where its notes' pitches come from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Notation<'a> {
    pub(crate) clef: Clef,
    pub(crate) track: TrackView<'a>,
}

impl<'a> Notation<'a> {
    /// Notation for `track`: a percussion staff for drum kits, else a pitched staff, or
    /// `None` if the track has no tuning to work out pitches from (voice and keyboard
    /// tracks without strings).
    pub(crate) fn for_track(track: TrackView<'a>) -> Result<Option<Self>, LayoutError> {
        if track.is_percussion() {
            return Ok(Some(Notation {
                clef: Clef::Percussion,
                track,
            }));
        }
        let Some(tuning) = track.tuning()? else {
            return Ok(None);
        };
        let lowest = tuning.iter().map(|s| s.pitch.get()).min().unwrap_or(40);
        Ok(Some(Notation {
            clef: Clef::for_lowest_string(lowest),
            track,
        }))
    }
}

/// What the notation staff shows in one column.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NotationItem {
    /// Notes played together.
    Chord(Chord),
    /// A rest.
    Rest {
        /// The rest glyph.
        glyph: Glyph,
        /// Augmentation dots.
        dots: u8,
    },
}

/// Notes of one beat, written on one stem.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Chord {
    /// Noteheads from the lowest to the highest.
    pub(crate) heads: Vec<Head>,
    /// Note value: 1 whole, 2 half, 4 quarter, ...
    pub(crate) value: u16,
    /// Augmentation dots.
    pub(crate) dots: u8,
    /// The tuplet's number of notes (3 for a triplet), if any.
    pub(crate) tuplet: Option<u8>,
    /// A fixed stem direction (up for drums); `None` decides by the noteheads.
    pub(crate) stem_up: Option<bool>,
}

/// One notehead.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Head {
    /// Staff position: 0 bottom line, 1 first space, ... (negative below the staff).
    pub(crate) position: i32,
    /// Diatonic step and sharp, for accidentals.
    step: i32,
    sharp: bool,
    /// The accidental shown before the head, if any (decided per measure).
    pub(crate) accidental: Option<Glyph>,
    /// Continues a tied note: no accidental, a tie curve from the previous chord.
    pub(crate) tied: bool,
    /// A dead note: cross notehead.
    pub(crate) dead: bool,
    /// A notehead other than the note value's (drum sounds).
    pub(crate) glyph: Option<Glyph>,
}

impl Chord {
    /// The notehead glyph for this chord's value.
    pub(crate) fn head_glyph(&self, head: &Head) -> Glyph {
        if let Some(glyph) = head.glyph {
            return glyph;
        }
        if head.dead {
            return Glyph::NoteheadX;
        }
        match self.value {
            1 => Glyph::NoteheadWhole,
            2 => Glyph::NoteheadHalf,
            _ => Glyph::NoteheadBlack,
        }
    }

    /// How many beams or flag strokes the value needs (0 for quarters and longer).
    pub(crate) fn beams(&self) -> u8 {
        match self.value {
            8 => 1,
            16 => 2,
            32 => 3,
            64 => 4,
            128 => 5,
            _ => 0,
        }
    }

    /// Whether neighbouring heads one step apart need to sit side by side.
    pub(crate) fn has_second(&self) -> bool {
        self.heads
            .windows(2)
            .any(|pair| matches!(pair, [a, b] if b.position.abs_diff(a.position) == 1))
    }
}

/// The notation item of a column: the chord or rest of its first voice, if that voice has
/// a beat starting here. `beats` are `(voice, beat)` pairs.
pub(crate) fn column_item(
    beats: &[(usize, BeatView<'_>)],
    notation: &Notation<'_>,
) -> Result<Option<NotationItem>, LayoutError> {
    let Some(&(_, beat)) = beats.iter().find(|(voice, _)| *voice == 0) else {
        return Ok(None);
    };
    let value = beat.note_value()?;
    match beat.kind() {
        BeatKind::Rest => Ok(Glyph::rest(value.value()).map(|glyph| NotationItem::Rest {
            glyph,
            dots: value.dots(),
        })),
        BeatKind::Empty => Ok(None),
        BeatKind::Normal => {
            let mut heads: Vec<Head> = Vec::new();
            for note in beat.notes() {
                let kind = note.kind();
                if !matches!(kind, NoteKind::Normal | NoteKind::Tie | NoteKind::Dead) {
                    continue;
                }
                // Unreadable notes are listed as issues by the model; leave them out here.
                let Ok(pitch) = notation.track.sounding_pitch(&note) else {
                    continue;
                };
                let head = if notation.clef == Clef::Percussion {
                    // A drum's "pitch" is the sound; it has a place, never an accidental.
                    let drum = drums::drum_note(pitch.get());
                    Head {
                        position: drum.position,
                        step: drum.position,
                        sharp: false,
                        accidental: None,
                        tied: kind == NoteKind::Tie,
                        dead: false,
                        glyph: drum.head.glyph(),
                    }
                } else {
                    let (spelled, position) = notation.clef.place(pitch.get());
                    Head {
                        position,
                        step: spelled.step,
                        sharp: spelled.sharp,
                        accidental: None,
                        tied: kind == NoteKind::Tie,
                        dead: kind == NoteKind::Dead,
                        glyph: None,
                    }
                };
                // Two sounds on one place share a head only when they look the same.
                if heads
                    .iter()
                    .any(|h| h.position == head.position && h.glyph == head.glyph)
                {
                    continue;
                }
                heads.push(head);
            }
            if heads.is_empty() {
                return Ok(None);
            }
            heads.sort_by_key(|h| h.position);
            Ok(Some(NotationItem::Chord(Chord {
                heads,
                value: value.value(),
                dots: value.dots(),
                tuplet: value.tuplet().map(|(enters, _)| enters),
                stem_up: (notation.clef == Clef::Percussion).then_some(true),
            })))
        }
    }
}

/// Decides the accidentals of a measure's items, in time order: a sharp or natural is shown
/// when a note differs from what the same step last was in this measure (no key signature
/// yet, so every step starts natural). Tied notes keep the earlier accidental silently.
pub(crate) fn assign_accidentals<'i>(items: impl Iterator<Item = &'i mut NotationItem>) {
    let mut sharp_now: HashMap<i32, bool> = HashMap::new();
    for item in items {
        let NotationItem::Chord(chord) = item else {
            continue;
        };
        for head in &mut chord.heads {
            let current = sharp_now.get(&head.step).copied().unwrap_or(false);
            if !head.tied && head.sharp != current {
                head.accidental = Some(if head.sharp {
                    Glyph::Sharp
                } else {
                    Glyph::Natural
                });
            }
            sharp_now.insert(head.step, head.sharp);
        }
    }
}

/// Which columns' chords are beamed (a beamable chord with a beamable neighbour in the
/// same beat), from each column's item and beat. Beamed chords need no room for a flag.
pub(crate) fn beamed_columns<'i>(
    columns: impl Iterator<Item = (Option<&'i NotationItem>, u64)>,
) -> Vec<bool> {
    let beamable: Vec<Option<u64>> = columns
        .map(|(item, beat)| match item {
            Some(NotationItem::Chord(chord)) if chord.beams() > 0 => Some(beat),
            _ => None,
        })
        .collect();
    (0..beamable.len())
        .map(|i| {
            let Some(beat) = beamable.get(i).copied().flatten() else {
                return false;
            };
            let same =
                |j: Option<usize>| j.and_then(|j| beamable.get(j).copied().flatten()) == Some(beat);
            same(i.checked_sub(1)) || same(i.checked_add(1))
        })
        .collect()
}

/// Room an item needs left and right of its column's center line; `beamed` chords have no
/// flag.
pub(crate) fn extent(item: &NotationItem, beamed: bool, font: &MusicFont) -> (f32, f32) {
    match item {
        NotationItem::Rest { glyph, dots } => {
            let bbox = font.bbox(*glyph);
            let half = (bbox.x_max - bbox.x_min) / 2.0;
            let dots = f32::from(*dots) * (font.advance(Glyph::AugmentationDot) + DOT_GAP);
            (half, half + dots)
        }
        NotationItem::Chord(chord) => {
            let head_width = chord
                .heads
                .iter()
                .map(|h| font.advance(chord.head_glyph(h)))
                .fold(0.0, f32::max);
            let half = head_width / 2.0;
            let accidental = chord
                .heads
                .iter()
                .filter_map(|h| h.accidental)
                .map(|g| font.advance(g) + ACCIDENTAL_GAP)
                .fold(0.0, f32::max);
            let second = if chord.has_second() { head_width } else { 0.0 };
            let dots = f32::from(chord.dots) * (font.advance(Glyph::AugmentationDot) + DOT_GAP);
            // A flag sticks out to the right of an up-stem.
            let flag = if chord.beams() > 0 && !beamed {
                1.0
            } else {
                0.0
            };
            (half + accidental, half + second + dots.max(flag))
        }
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    fn head(step: i32, sharp: bool, tied: bool) -> Head {
        Head {
            position: step - 30,
            step,
            sharp,
            accidental: None,
            tied,
            dead: false,
            glyph: None,
        }
    }

    fn chord(heads: Vec<Head>) -> NotationItem {
        NotationItem::Chord(Chord {
            heads,
            value: 4,
            dots: 0,
            tuplet: None,
            stem_up: None,
        })
    }

    fn accidentals(items: &[NotationItem]) -> Vec<Option<Glyph>> {
        items
            .iter()
            .flat_map(|item| match item {
                NotationItem::Chord(c) => c.heads.iter().map(|h| h.accidental).collect(),
                NotationItem::Rest { .. } => Vec::new(),
            })
            .collect()
    }

    #[test]
    fn accidentals_last_until_the_barline() {
        // F#, F#, F, F#: sharp, nothing, natural, sharp again.
        let mut items = vec![
            chord(vec![head(31, true, false)]),
            chord(vec![head(31, true, false)]),
            chord(vec![head(31, false, false)]),
            chord(vec![head(31, true, false)]),
        ];
        assign_accidentals(items.iter_mut());
        assert_eq!(
            accidentals(&items),
            [
                Some(Glyph::Sharp),
                None,
                Some(Glyph::Natural),
                Some(Glyph::Sharp)
            ]
        );
    }

    #[test]
    fn tied_notes_do_not_repeat_their_accidental() {
        let mut items = vec![
            chord(vec![head(31, true, false)]),
            chord(vec![head(31, true, true)]),
        ];
        assign_accidentals(items.iter_mut());
        assert_eq!(accidentals(&items), [Some(Glyph::Sharp), None]);
    }

    #[test]
    fn note_values_choose_heads_and_beams() {
        let mut c = Chord {
            heads: vec![head(30, false, false)],
            value: 1,
            dots: 0,
            tuplet: None,
            stem_up: None,
        };
        let h = c.heads[0].clone();
        assert_eq!(c.head_glyph(&h), Glyph::NoteheadWhole);
        c.value = 16;
        assert_eq!((c.head_glyph(&h), c.beams()), (Glyph::NoteheadBlack, 2));
        c.heads.push(head(31, false, false));
        assert!(c.has_second());
    }
}
