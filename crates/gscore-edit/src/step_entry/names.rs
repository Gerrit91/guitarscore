//! Notes by name: the letter's pitch class with the key signature's accidental, at the
//! octave nearest the note before.

use crate::action::Letter;
use crate::editor::EditError;
use crate::fingering::{FingeringError, Neck, notes_before};
use crate::tab_editor::{Outcome, TabEditor};

/// The order sharps are added to a key signature, as letter indices from C.
const SHARP_ORDER: [usize; 7] = [3, 0, 4, 1, 5, 2, 6];
/// The order flats are added.
const FLAT_ORDER: [usize; 7] = [6, 2, 5, 1, 4, 0, 3];

impl TabEditor {
    /// A note by its name: the pitch with that letter (and the key signature's
    /// accidental) nearest the note before it.
    pub(super) fn step_letter(
        &mut self,
        letter: Letter,
        chord: bool,
    ) -> Result<Outcome, EditError> {
        let song = self.editor.song();
        let neck = Neck::of_staff(song, &self.cursor)?;
        let key = song
            .key_signature(self.cursor.measure)
            .map_or(0, |signature| signature.key);
        let class = i16::from(letter.pitch_class())
            .saturating_add(i16::from(key_alter(letter, key)))
            .rem_euclid(12);
        let reference = self.reference_pitch(&neck);
        let pitch = nearest_pitch(class, reference, &neck).ok_or(FingeringError::NoStrings)?;
        let places = self.place_pitches(&[pitch], chord)?;
        self.enter_places(places, chord)
    }

    /// The pitch a note by name is entered near: the note entered last, else the nearest
    /// note before the cursor, else a third of the way up the neck.
    pub(super) fn reference_pitch(&self, neck: &Neck) -> i16 {
        let last = self
            .live_step()
            .and_then(|step| step.notes.last())
            .and_then(|place| neck.pitch(*place));
        let before = || {
            notes_before(self.editor.song(), &self.cursor, false)
                .and_then(|places| places.iter().filter_map(|p| neck.pitch(*p)).max())
        };
        let default = || {
            neck.range().map_or(60, |(low, high)| {
                low.saturating_add(high.saturating_sub(low) / 3)
            })
        };
        i16::from(last.or_else(before).unwrap_or_else(default))
    }
}

/// How the key signature `key` (sharps positive, flats negative) alters `letter`.
fn key_alter(letter: Letter, key: i8) -> i8 {
    let count = usize::from(key.unsigned_abs()).min(7);
    let (order, alter) = if key > 0 {
        (&SHARP_ORDER, 1)
    } else {
        (&FLAT_ORDER, -1)
    };
    if order
        .iter()
        .take(count)
        .any(|&index| index == letter.index())
    {
        alter
    } else {
        0
    }
}

/// The pitch of pitch class `class` nearest `reference` (upwards on a tie) that the neck
/// can play, moved by octaves into its range when the nearest is outside it.
fn nearest_pitch(class: i16, reference: i16, neck: &Neck) -> Option<u8> {
    let (low, high) = neck.range()?;
    let below = reference.saturating_sub(reference.saturating_sub(class).rem_euclid(12));
    let above = below.saturating_add(12);
    let mut pitch = if reference.saturating_sub(below) < above.saturating_sub(reference) {
        below
    } else {
        above
    };
    while pitch < i16::from(low) {
        pitch = pitch.saturating_add(12);
    }
    while pitch > i16::from(high) {
        pitch = pitch.saturating_sub(12);
    }
    u8::try_from(pitch)
        .ok()
        .filter(|p| (low..=high).contains(p))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use gscore_model::StringNumber;

    #[test]
    fn key_signatures_alter_their_letters() {
        assert_eq!(key_alter(Letter::F, 1), 1);
        assert_eq!(key_alter(Letter::C, 1), 0);
        assert_eq!(key_alter(Letter::C, 2), 1);
        assert_eq!(key_alter(Letter::B, -1), -1);
        assert_eq!(key_alter(Letter::E, -1), 0);
        assert_eq!(key_alter(Letter::F, -7), -1);
        assert_eq!(key_alter(Letter::B, 7), 1);
        assert_eq!(key_alter(Letter::A, 0), 0);
    }

    #[test]
    fn a_letter_goes_to_the_nearest_octave_in_range() {
        let neck = Neck::new(vec![(StringNumber::new(1).unwrap(), 40)], 24);
        // C nearest E3 (52) is C3 (48); A nearest E3 is A3 (57).
        assert_eq!(nearest_pitch(0, 52, &neck), Some(48));
        assert_eq!(nearest_pitch(9, 52, &neck), Some(57));
        // A tritone away: upwards.
        assert_eq!(nearest_pitch(10, 52, &neck), Some(58));
        // C nearest E2 is C2 (36), below the neck: up an octave.
        assert_eq!(nearest_pitch(0, 40, &neck), Some(48));
        // Nothing playable at all.
        assert_eq!(nearest_pitch(0, 60, &Neck::new(Vec::new(), 24)), None);
    }
}
