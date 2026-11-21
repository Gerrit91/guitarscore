//! The fingering (M23 step 2): where on the neck a pitch is played. Every input that
//! gives a pitch rather than a string and a fret (note names, the keyboard tool, a MIDI
//! keyboard) goes through it.
//!
//! A [`Neck`] is a staff's strings (their open pitches, capo and transposition included)
//! and frets. It places a note close to the hand's position (the frets of the notes
//! before it), a chord's notes on different strings within a span a hand can hold, and
//! prefers open strings only in low positions, where they fit. A pitch it cannot place is
//! a [`FingeringError`] the user can act on, never a panic.

use std::fmt;

use gscore_model::{BeatIndex, BeatKind, Fret, MeasureIndex, NoteKind, Song, StringNumber};

use crate::cursor::Cursor;

/// Widest fretted span of a chord (from the lowest to the highest fretted note).
const MAX_SPAN: u8 = 4;
/// Most notes a chord can have (one per string of an eight-string instrument).
const MAX_CHORD_NOTES: usize = 8;
/// How far an unpenalized hand reaches from its position, in frets.
const REACH: f32 = 3.0;
/// Up to which hand position open strings count as part of the hand's shape.
const OPEN_POSITION: f32 = 5.0;
/// How many beats back the hand's position is looked for.
const HAND_LOOKBACK: usize = 16;

/// A note's place: a string and a fret on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Place {
    /// The string.
    pub string: StringNumber,
    /// The fret.
    pub fret: Fret,
}

/// Why a pitch or chord has no place on the neck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FingeringError {
    /// The staff has no tuned strings (a drum kit, or a file that stores no tuning).
    NoStrings,
    /// The pitch is below the lowest open string or above the highest fret.
    OutOfRange {
        /// The pitch (MIDI note number).
        pitch: u8,
        /// The lowest pitch the neck has.
        lowest: u8,
        /// The highest.
        highest: u8,
    },
    /// Every string that could play the pitch is used by the beat already.
    NoFreeString {
        /// The pitch.
        pitch: u8,
    },
    /// More notes than strings.
    TooManyNotes {
        /// How many notes the chord has.
        notes: usize,
        /// How many strings there are.
        strings: usize,
    },
    /// The chord's notes cannot be held on different strings within a hand's span.
    ChordTooWide,
}

impl fmt::Display for FingeringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FingeringError::NoStrings => {
                f.write_str("this track has no tuned strings to put the note on")
            }
            FingeringError::OutOfRange {
                pitch,
                lowest,
                highest,
            } => write!(
                f,
                "{} is out of this track's range ({} to {})",
                pitch_name(*pitch),
                pitch_name(*lowest),
                pitch_name(*highest)
            ),
            FingeringError::NoFreeString { pitch } => write!(
                f,
                "no free string of this track can play {}",
                pitch_name(*pitch)
            ),
            FingeringError::TooManyNotes { notes, strings } => write!(
                f,
                "a chord of {notes} notes does not fit on {strings} strings"
            ),
            FingeringError::ChordTooWide => {
                f.write_str("the chord's notes cannot be held on this track's strings")
            }
        }
    }
}

impl std::error::Error for FingeringError {}

/// A pitch's name with its octave (`C4` is MIDI 60), spelled with sharps.
pub fn pitch_name(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let name = NAMES.get(usize::from(pitch % 12)).copied().unwrap_or("?");
    format!("{name}{}", i16::from(pitch / 12).saturating_sub(1))
}

/// The strings and frets of one staff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Neck {
    /// Each string with the pitch it sounds open, string 1 (highest) first.
    strings: Vec<(StringNumber, u8)>,
    /// How many frets there are.
    frets: u8,
}

impl Neck {
    /// A neck from its strings (each with its open pitch, in any order) and fret count.
    pub fn new(mut strings: Vec<(StringNumber, u8)>, frets: u8) -> Self {
        strings.sort_by_key(|(string, _)| *string);
        strings.dedup_by_key(|(string, _)| *string);
        Neck { strings, frets }
    }

    /// The neck of the cursor's staff: its tuning (or the stand-in tuning a keyboard or
    /// voice track stores), capo and transposition included, so a pitch is what the note
    /// sounds at.
    pub fn of_staff(song: &Song, cursor: &Cursor) -> Result<Self, FingeringError> {
        let track = song
            .staff(cursor.track, cursor.staff)
            .ok_or(FingeringError::NoStrings)?;
        if track.is_percussion() {
            return Err(FingeringError::NoStrings);
        }
        let tuning = track
            .tuning()
            .ok()
            .flatten()
            .or_else(|| track.note_tuning().ok().flatten())
            .ok_or(FingeringError::NoStrings)?;
        let strings: Vec<(StringNumber, u8)> = tuning
            .iter()
            .filter_map(|string| {
                let open = track.sounding_pitch_at(string.string, Fret::OPEN).ok()?;
                Some((string.string, open.get()))
            })
            .collect();
        if strings.is_empty() {
            return Err(FingeringError::NoStrings);
        }
        Ok(Neck::new(strings, track.fret_count()))
    }

    /// How many strings it has.
    pub fn string_count(&self) -> usize {
        self.strings.len()
    }

    /// How many frets it has.
    pub fn frets(&self) -> u8 {
        self.frets
    }

    /// The pitch a place sounds at, if the neck has its string.
    pub fn pitch(&self, place: Place) -> Option<u8> {
        let (_, open) = self.strings.iter().find(|(s, _)| *s == place.string)?;
        open.checked_add(place.fret.get())
    }

    /// The lowest and the highest pitch the neck can play.
    pub fn range(&self) -> Option<(u8, u8)> {
        let lowest = self.strings.iter().map(|(_, open)| *open).min()?;
        let highest = self
            .strings
            .iter()
            .map(|(_, open)| open.saturating_add(self.frets))
            .max()?;
        Some((lowest, highest))
    }

    /// Every place that sounds `pitch`, string 1 first.
    pub fn places(&self, pitch: u8) -> impl Iterator<Item = Place> + '_ {
        self.strings.iter().filter_map(move |(string, open)| {
            let fret = pitch
                .checked_sub(*open)
                .filter(|fret| *fret <= self.frets)?;
            Some(Place {
                string: *string,
                fret: Fret::new(fret).ok()?,
            })
        })
    }

    /// The error for a pitch no free string can play.
    fn unplayable(&self, pitch: u8) -> FingeringError {
        match self.range() {
            None => FingeringError::NoStrings,
            Some((lowest, highest)) if pitch < lowest || pitch > highest => {
                FingeringError::OutOfRange {
                    pitch,
                    lowest,
                    highest,
                }
            }
            Some(_) => FingeringError::NoFreeString { pitch },
        }
    }

    /// Where `pitch` is easiest to play next to `hand` (the hand's position, see
    /// [`hand_before`]), on a string `taken` does not hold. With other notes `beside` it
    /// (a chord being built), it stays within a hand's span of them where it can.
    pub fn place_note(
        &self,
        pitch: u8,
        hand: Option<f32>,
        beside: &[Place],
    ) -> Result<Place, FingeringError> {
        let free = |place: &Place| !beside.iter().any(|other| other.string == place.string);
        let center = fretted_center(beside).or(hand);
        let candidates: Vec<Place> = self.places(pitch).filter(free).collect();
        let in_span = candidates
            .iter()
            .copied()
            .filter(|place| {
                let mut frets: Vec<u8> = beside.iter().map(|p| p.fret.get()).collect();
                frets.push(place.fret.get());
                span(&frets) <= MAX_SPAN
            })
            .min_by(|a, b| {
                note_cost(a.fret.get(), center).total_cmp(&note_cost(b.fret.get(), center))
            });
        in_span
            .or_else(|| {
                candidates.iter().copied().min_by(|a, b| {
                    note_cost(a.fret.get(), center).total_cmp(&note_cost(b.fret.get(), center))
                })
            })
            .ok_or_else(|| self.unplayable(pitch))
    }

    /// Places every pitch of a chord on its own string, within a hand's span, as close to
    /// `hand` as it can. The places come back in the order of `pitches`.
    pub fn place_chord(
        &self,
        pitches: &[u8],
        hand: Option<f32>,
    ) -> Result<Vec<Place>, FingeringError> {
        if pitches.len() > self.strings.len() || pitches.len() > MAX_CHORD_NOTES {
            return Err(FingeringError::TooManyNotes {
                notes: pitches.len(),
                strings: self.strings.len(),
            });
        }
        let options: Vec<Vec<Place>> = pitches
            .iter()
            .map(|&pitch| {
                let places: Vec<Place> = self.places(pitch).collect();
                if places.is_empty() {
                    Err(self.unplayable(pitch))
                } else {
                    Ok(places)
                }
            })
            .collect::<Result<_, _>>()?;
        let mut search = ChordSearch {
            options: &options,
            pitches,
            hand,
            chosen: Vec::with_capacity(pitches.len()),
            best: None,
        };
        search.run();
        search
            .best
            .map(|(_, places)| places)
            .ok_or(FingeringError::ChordTooWide)
    }

    /// The next place that sounds the same as `place` on another string (towards the
    /// lower strings when `down`, wrapping around), skipping strings `taken` holds.
    pub fn other_string(&self, place: Place, down: bool, taken: &[Place]) -> Option<Place> {
        let pitch = self.pitch(place)?;
        let mut places: Vec<Place> = self
            .places(pitch)
            .filter(|other| {
                other.string == place.string || !taken.iter().any(|t| t.string == other.string)
            })
            .collect();
        if !down {
            places.reverse();
        }
        let at = places.iter().position(|p| p.string == place.string)?;
        places
            .iter()
            .cycle()
            .skip(at.saturating_add(1))
            .take(places.len())
            .copied()
            .find(|p| p.string != place.string)
    }
}

/// A depth-first search over the strings for a chord's notes, keeping the cheapest.
struct ChordSearch<'a> {
    options: &'a [Vec<Place>],
    pitches: &'a [u8],
    hand: Option<f32>,
    chosen: Vec<Place>,
    best: Option<(f32, Vec<Place>)>,
}

impl ChordSearch<'_> {
    fn run(&mut self) {
        let index = self.chosen.len();
        let Some(options) = self.options.get(index) else {
            let cost = chord_cost(&self.chosen, self.pitches, self.hand);
            if self.best.as_ref().is_none_or(|(best, _)| cost < *best) {
                self.best = Some((cost, self.chosen.clone()));
            }
            return;
        };
        for place in options {
            if self.chosen.iter().any(|p| p.string == place.string) {
                continue;
            }
            let mut frets: Vec<u8> = self.chosen.iter().map(|p| p.fret.get()).collect();
            frets.push(place.fret.get());
            if span(&frets) > MAX_SPAN {
                continue;
            }
            self.chosen.push(*place);
            self.run();
            self.chosen.pop();
        }
    }
}

/// From the lowest to the highest fretted fret (open strings take no finger).
fn span(frets: &[u8]) -> u8 {
    let fretted = frets.iter().copied().filter(|fret| *fret > 0);
    let (low, high) = fretted.fold((u8::MAX, 0), |(low, high), fret| {
        (low.min(fret), high.max(fret))
    });
    high.saturating_sub(low)
}

/// The middle of the fretted notes among `places`, if any is fretted.
fn fretted_center(places: &[Place]) -> Option<f32> {
    let frets: Vec<f32> = places
        .iter()
        .map(|p| p.fret.get())
        .filter(|fret| *fret > 0)
        .map(f32::from)
        .collect();
    (!frets.is_empty()).then(|| frets.iter().sum::<f32>() / frets.len() as f32)
}

/// How awkward fret `fret` is for a hand at `hand`: lower is better.
fn note_cost(fret: u8, hand: Option<f32>) -> f32 {
    let position = f32::from(fret);
    if fret == 0 {
        // Open strings belong to the low positions; higher up they mean leaving the shape.
        return match hand {
            Some(hand) if hand > OPEN_POSITION => 4.0 + (hand - OPEN_POSITION) * 0.5,
            _ => 0.0,
        };
    }
    let high = (position - 12.0).max(0.0) * 0.05;
    match hand {
        // Without a hand position, low frets are easiest.
        None => position * 0.6 + high,
        Some(hand) => {
            let distance = (position - hand).abs();
            distance + (distance - REACH).max(0.0) * 1.5 + high
        }
    }
}

/// How awkward a chord's places are: each note's cost from the hand (or the chord's own
/// position), its span, and strings crossed against the pitches' order.
fn chord_cost(places: &[Place], pitches: &[u8], hand: Option<f32>) -> f32 {
    let center = hand.or_else(|| fretted_center(places));
    let notes: f32 = places
        .iter()
        .map(|place| note_cost(place.fret.get(), center))
        .sum();
    let frets: Vec<u8> = places.iter().map(|p| p.fret.get()).collect();
    let mut crossed = 0_u16;
    for (i, (a, pa)) in places.iter().zip(pitches).enumerate() {
        for (b, pb) in places.iter().zip(pitches).skip(i.saturating_add(1)) {
            // A higher pitch belongs on a higher string (a lower string number).
            if (pa > pb) != (a.string < b.string) && pa != pb {
                crossed = crossed.saturating_add(1);
            }
        }
    }
    // Without a hand, the chord's own height counts, so low chords win.
    let height = if hand.is_none() {
        center.unwrap_or(0.0) * 0.3
    } else {
        0.0
    };
    notes + f32::from(span(&frets)) * 0.5 + f32::from(crossed) * 2.0 + height
}

/// Where the hand is before the cursor: the middle of the fretted notes of the nearest
/// earlier beat that has any (beats with only open strings or rests are passed over), in
/// the cursor's voice, looking back at most a few beats. `None` at the start.
pub fn hand_before(song: &Song, cursor: &Cursor) -> Option<f32> {
    notes_before(song, cursor, true).and_then(|places| fretted_center(&places))
}

/// The notes of the nearest earlier beat in the cursor's voice that has notes (with
/// `fretted`, one with a fretted note), looking back at most a few beats.
pub(crate) fn notes_before(song: &Song, cursor: &Cursor, fretted: bool) -> Option<Vec<Place>> {
    let staff = song.staff(cursor.track, cursor.staff)?;
    let mut measure = cursor.measure.get();
    let mut beat = cursor.beat.get();
    for _ in 0..HAND_LOOKBACK {
        if beat == 0 {
            measure = measure.checked_sub(1)?;
            beat = staff
                .measure(MeasureIndex::new(measure))
                .and_then(|m| m.voice(cursor.voice))
                .map_or(0, |v| v.beats().len());
            continue;
        }
        beat = beat.saturating_sub(1);
        let Some(view) = staff
            .measure(MeasureIndex::new(measure))
            .and_then(|m| m.voice(cursor.voice))
            .and_then(|v| v.beat(BeatIndex::new(beat)))
        else {
            continue;
        };
        if view.kind() == BeatKind::Empty || view.is_grace() {
            continue;
        }
        let places: Vec<Place> = view
            .notes()
            .filter(|note| note.kind() != NoteKind::Rest)
            .filter_map(|note| {
                Some(Place {
                    string: note.string().ok()?,
                    fret: note.fret().ok()?,
                })
            })
            .collect();
        let found = if fretted {
            places.iter().any(|place| place.fret.get() > 0)
        } else {
            !places.is_empty()
        };
        if found {
            return Some(places);
        }
    }
    None
}

#[cfg(test)]
mod tests;
