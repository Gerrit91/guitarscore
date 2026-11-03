//! Standard notation on a five-line staff above the tab, as Guitar Pro shows it: the same
//! columns as the tab, so a note and its fret number line up.
//!
//! This module turns a column's beats into a [`NotationItem`] per voice (a chord or a rest)
//! and decides accidentals per measure; [`draw`] paints a measure's items (noteheads, ledger
//! lines, stems, flags, beams, dots, rests, ties, tuplet numbers). All voices are written:
//! when two share the staff the first points its stems up and the others down, and a voice
//! whose notehead would sit on the first voice's position moves right. Grace notes are small
//! heads with a slash before their main note. Articulations and other ornaments on the
//! notation staff are not drawn yet (they live on the tab). Drum tracks get a percussion
//! staff instead: each drum sound has its own place and notehead ([`drums`]) and stems point
//! up.

mod beams;
mod draw;
mod drums;
mod pitch;

use std::collections::HashMap;

use gscore_model::{BeatKind, BeatView, NoteKind, TrackView};

pub(crate) use draw::{
    BeamPlan, KeySigLayout, draw_measure, draw_staff_start, draw_tab_staff_start, draw_tie_curve,
    key_start_width, prefix_width, tab_prefix_width, y_of,
};
pub(crate) use pitch::{Alter, Clef};

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
    /// Diatonic step and alteration, for accidentals.
    step: i32,
    alter: Alter,
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

    /// The stem's direction: the fixed one, or the one the noteheads choose (a chord above
    /// the middle line points down).
    pub(crate) fn stem_up(&self) -> bool {
        self.stem_up.unwrap_or_else(|| {
            let (sum, count) = self
                .heads
                .iter()
                .fold((0.0_f32, 0.0_f32), |(sum, count), head| {
                    (sum + head.position as f32, count + 1.0)
                });
            let mean = if count > 0.0 {
                sum / count
            } else {
                MIDDLE as f32
            };
            mean < MIDDLE as f32
        })
    }
}

/// The notation items of a column: one per voice that has a beat starting here, in voice
/// order. `beats` are `(voice, beat)` pairs.
pub(crate) fn column_items(
    beats: &[(usize, BeatView<'_>)],
    notation: &Notation<'_>,
    key: i8,
) -> Result<Vec<Option<NotationItem>>, LayoutError> {
    let voices = beats
        .iter()
        .map(|(voice, _)| voice.saturating_add(1))
        .max()
        .unwrap_or(0);
    let mut items: Vec<Option<NotationItem>> = vec![None; voices];
    for &(voice, beat) in beats {
        let item = beat_item(voice, beat, notation, key)?;
        if let Some(slot) = items.get_mut(voice) {
            *slot = item;
        }
    }
    Ok(items)
}

/// The notation item of one voice's beat, if that voice has a beat starting here.
fn beat_item(
    voice: usize,
    beat: BeatView<'_>,
    notation: &Notation<'_>,
    key: i8,
) -> Result<Option<NotationItem>, LayoutError> {
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
                        alter: Alter::Natural,
                        accidental: None,
                        tied: kind == NoteKind::Tie,
                        dead: false,
                        glyph: drum.head.glyph(),
                    }
                } else {
                    let (spelled, position) = notation.clef.place(pitch.get(), key);
                    Head {
                        position,
                        step: spelled.step,
                        alter: spelled.alter,
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
                // A second (or further) voice points its stems down, as engravers do.
                stem_up: if notation.clef == Clef::Percussion {
                    Some(true)
                } else if voice > 0 {
                    Some(false)
                } else {
                    None
                },
            })))
        }
    }
}

/// Decides the accidentals of a measure's items, in time order: an accidental is shown when
/// a note differs from the key signature's alteration of the same step, or from what that
/// step was earlier in this measure. Tied notes keep the earlier accidental silently.
pub(crate) fn assign_accidentals<'i>(items: impl Iterator<Item = &'i mut NotationItem>, key: i8) {
    let alters = pitch::key_alters(key);
    // The alteration each step (octave and letter) last had in this measure.
    let mut alter_now: HashMap<i32, Alter> = HashMap::new();
    for item in items {
        let NotationItem::Chord(chord) = item else {
            continue;
        };
        for head in &mut chord.heads {
            let letter = usize::try_from(head.step.rem_euclid(7)).unwrap_or(0);
            let key_alter = alters.get(letter).copied().unwrap_or(Alter::Natural);
            let current = alter_now.get(&head.step).copied().unwrap_or(key_alter);
            if !head.tied && head.alter != current {
                head.accidental = Some(head.alter.glyph());
            }
            alter_now.insert(head.step, head.alter);
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

    fn head(step: i32, alter: Alter, tied: bool) -> Head {
        Head {
            position: step - 30,
            step,
            alter,
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
            chord(vec![head(31, Alter::Sharp, false)]),
            chord(vec![head(31, Alter::Sharp, false)]),
            chord(vec![head(31, Alter::Natural, false)]),
            chord(vec![head(31, Alter::Sharp, false)]),
        ];
        assign_accidentals(items.iter_mut(), 0);
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
    fn the_key_signature_sets_the_default_alteration() {
        // G major has F sharp: F# needs no accidental, F natural does.
        let mut items = vec![
            chord(vec![head(31, Alter::Sharp, false)]),
            chord(vec![head(31, Alter::Natural, false)]),
            chord(vec![head(31, Alter::Sharp, false)]),
        ];
        assign_accidentals(items.iter_mut(), 1);
        assert_eq!(
            accidentals(&items),
            [None, Some(Glyph::Natural), Some(Glyph::Sharp)]
        );
        // B flat in F major (one flat): no accidental; B natural needs one.
        let mut flat = vec![
            chord(vec![head(34, Alter::Flat, false)]),
            chord(vec![head(34, Alter::Natural, false)]),
        ];
        assign_accidentals(flat.iter_mut(), -1);
        assert_eq!(accidentals(&flat), [None, Some(Glyph::Natural)]);
    }

    #[test]
    fn tied_notes_do_not_repeat_their_accidental() {
        let mut items = vec![
            chord(vec![head(31, Alter::Sharp, false)]),
            chord(vec![head(31, Alter::Sharp, true)]),
        ];
        assign_accidentals(items.iter_mut(), 0);
        assert_eq!(accidentals(&items), [Some(Glyph::Sharp), None]);
    }

    #[test]
    fn note_values_choose_heads_and_beams() {
        let mut c = Chord {
            heads: vec![head(30, Alter::Natural, false)],
            value: 1,
            dots: 0,
            tuplet: None,
            stem_up: None,
        };
        let h = c.heads[0].clone();
        assert_eq!(c.head_glyph(&h), Glyph::NoteheadWhole);
        c.value = 16;
        assert_eq!((c.head_glyph(&h), c.beams()), (Glyph::NoteheadBlack, 2));
        c.heads.push(head(31, Alter::Natural, false));
        assert!(c.has_second());
    }
}
