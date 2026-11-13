//! Where a pitch goes on a five-line staff: key signatures, spelling, clef, staff position.

use crate::glyph::Glyph;

/// How a written note (or a key signature) alters its letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Alter {
    Natural,
    Sharp,
    Flat,
}

impl Alter {
    /// How many semitones the alteration adds to the letter's natural pitch class, modulo
    /// 12.
    fn semitones(self) -> i32 {
        match self {
            Alter::Natural => 0,
            Alter::Sharp => 1,
            Alter::Flat => 11,
        }
    }

    /// The accidental that writes this alteration.
    pub(crate) fn glyph(self) -> Glyph {
        match self {
            Alter::Natural => Glyph::Natural,
            Alter::Sharp => Glyph::Sharp,
            Alter::Flat => Glyph::Flat,
        }
    }
}

/// The letter and alteration of each pitch class, spelled with sharps: (letter 0 = C ... 6 =
/// B, alteration).
const SHARP_SPELLING: [(usize, bool); 12] = [
    (0, false),
    (0, true),
    (1, false),
    (1, true),
    (2, false),
    (3, false),
    (3, true),
    (4, false),
    (4, true),
    (5, false),
    (5, true),
    (6, false),
];

/// Letter and alteration of each pitch class, spelled with flats: (letter, flat).
const FLAT_SPELLING: [(usize, bool); 12] = [
    (0, false),
    (1, true),
    (1, false),
    (2, true),
    (2, false),
    (3, false),
    (4, true),
    (4, false),
    (5, true),
    (5, false),
    (6, true),
    (6, false),
];

/// The letters a key signature alters, in the order they are written: sharps F C G D A E B,
/// flats B E A D G C F.
const SHARP_LETTERS: [usize; 7] = [3, 0, 4, 1, 5, 2, 6];
const FLAT_LETTERS: [usize; 7] = [6, 2, 5, 1, 4, 0, 3];
/// Pitch class of each natural letter (C, D, E, F, G, A, B).
const NATURAL_PITCH_CLASSES: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];

/// How the key signature alters each letter: `key` sharps (positive) or flats (negative).
/// Values outside -7..=7 are clamped.
pub(crate) fn key_alters(key: i8) -> [Alter; 7] {
    let mut alters = [Alter::Natural; 7];
    let key = key.clamp(-7, 7);
    let (letters, alter): (&[usize; 7], Alter) = if key > 0 {
        (&SHARP_LETTERS, Alter::Sharp)
    } else {
        (&FLAT_LETTERS, Alter::Flat)
    };
    for &letter in letters.iter().take(usize::from(key.unsigned_abs())) {
        if let Some(slot) = alters.get_mut(letter) {
            *slot = alter;
        }
    }
    alters
}

/// The pitch class of a letter with the key signature's alteration.
fn altered_pitch_class(letter: usize, alter: Alter) -> Option<i32> {
    NATURAL_PITCH_CLASSES
        .get(letter)
        .map(|natural| natural.saturating_add(alter.semitones()).rem_euclid(12))
}

/// A written note: its diatonic step (`octave * 7 + letter`, so C4 is 28) and its
/// alteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Spelled {
    pub(crate) step: i32,
    pub(crate) alter: Alter,
}

/// Spells MIDI pitch `midi` in `key`: a pitch class the key signature alters uses the
/// signature's spelling (B flat in F major); other pitch classes are spelled with flats in
/// a flat key and with sharps otherwise (A flat, not G sharp, in B flat major), as Guitar
/// Pro writes them.
pub(crate) fn spell_in_key(midi: u8, key: i8) -> Spelled {
    let class = i32::from(midi % 12);
    let alters = key_alters(key);
    let (letter, alter) = alters
        .iter()
        .enumerate()
        .find_map(|(letter, &alter)| {
            (altered_pitch_class(letter, alter) == Some(class)).then_some((letter, alter))
        })
        .unwrap_or_else(|| {
            let (table, altered) = if key < 0 {
                (&FLAT_SPELLING, Alter::Flat)
            } else {
                (&SHARP_SPELLING, Alter::Sharp)
            };
            let (letter, changed) = table
                .get(usize::from(midi % 12))
                .copied()
                .unwrap_or((0, false));
            (letter, if changed { altered } else { Alter::Natural })
        });
    // MIDI octave -1 starts at 0: octave = midi / 12 - 1.
    let octave = i32::from(midi / 12).saturating_sub(1);
    let letter = i32::try_from(letter).unwrap_or(0);
    Spelled {
        step: octave.saturating_mul(7).saturating_add(letter),
        alter,
    }
}

/// The clef of a notation staff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Clef {
    /// Treble clef: guitars.
    Treble,
    /// Bass clef: basses.
    Bass,
    /// Alto clef: C on the middle line.
    Alto,
    /// Tenor clef: C on the fourth line from the bottom.
    Tenor,
    /// Percussion clef: drum kits, whose notes are placed by [`super::drums`].
    Percussion,
}

/// Semitones between the sounding and the written pitch.
const WRITTEN_OCTAVE_UP: u8 = 12;
/// Below this lowest open string, a track gets the bass clef (F1: 4- and 5-string basses
/// start at E1 or lower; 7- and 8-string guitars at B1 and F#1).
const BASS_BELOW: u8 = 29;

/// Where the accidentals of a key signature sit, per clef: staff position (0 = bottom line)
/// in the order they are written. Treble starts on F5, bass on F3.
const TREBLE_SHARPS: [i32; 7] = [8, 5, 9, 6, 3, 7, 4];
const TREBLE_FLATS: [i32; 7] = [4, 7, 3, 6, 2, 5, 1];
const BASS_SHARPS: [i32; 7] = [6, 3, 7, 4, 1, 5, 2];
const BASS_FLATS: [i32; 7] = [2, 5, 1, 4, 0, 3, -1];
const ALTO_SHARPS: [i32; 7] = [7, 4, 8, 5, 2, 6, 3];
const ALTO_FLATS: [i32; 7] = [3, 6, 2, 5, 1, 4, 0];
const TENOR_SHARPS: [i32; 7] = [9, 6, 10, 7, 4, 8, 5];
const TENOR_FLATS: [i32; 7] = [5, 8, 4, 7, 3, 6, 2];

impl Clef {
    /// The clef for a track whose lowest open string sounds at `lowest`.
    pub(crate) fn for_lowest_string(lowest: u8) -> Self {
        if lowest < BASS_BELOW {
            Clef::Bass
        } else {
            Clef::Treble
        }
    }

    /// The diatonic step on the bottom line: E4 in treble, G2 in bass.
    fn bottom_line_step(self) -> i32 {
        match self {
            // Drums are not placed by pitch; treble positions are a harmless fallback.
            Clef::Treble | Clef::Percussion => 4 * 7 + 2,
            Clef::Bass => 2 * 7 + 4,
            // C4 sits on the middle line (alto) or the fourth line (tenor).
            Clef::Alto => 4 * 7 - 4,
            Clef::Tenor => 4 * 7 - 6,
        }
    }

    /// The clef glyph and the staff position its origin sits on (the G line for the
    /// treble clef, the F line for the bass clef).
    pub(crate) fn glyph(self) -> (Glyph, i32) {
        match self {
            Clef::Treble => (Glyph::GClef, 2),
            Clef::Bass => (Glyph::FClef, 6),
            Clef::Alto => (Glyph::CClef, 4),
            Clef::Tenor => (Glyph::CClef, 6),
            Clef::Percussion => (Glyph::PercussionClef, 4),
        }
    }

    /// Where a note sounding at `midi` is written in `key`: its spelling and its staff
    /// position (0 = bottom line, 1 = first space, 8 = top line; negative below the staff).
    /// A string instrument (`octave_up`: guitar and bass music) is written an octave above
    /// where it sounds, as Guitar Pro and most guitar notation do; a keyboard, a voice or
    /// another instrument is written at pitch.
    pub(crate) fn place(self, midi: u8, key: i8, octave_up: bool) -> (Spelled, i32) {
        let written = if octave_up {
            midi.saturating_add(WRITTEN_OCTAVE_UP)
        } else {
            midi
        };
        let spelled = spell_in_key(written, key);
        (
            spelled,
            spelled.step.saturating_sub(self.bottom_line_step()),
        )
    }

    /// Where a grace note sounding at `midi` is written in `key`: its staff position and
    /// the accidental it shows (when its letter differs from the key signature). Grace
    /// notes do not take part in the measure's accidental bookkeeping.
    pub(crate) fn place_grace(self, midi: u8, key: i8, octave_up: bool) -> (i32, Option<Glyph>) {
        let (spelled, position) = self.place(midi, key, octave_up);
        let letter = usize::try_from(spelled.step.rem_euclid(7)).unwrap_or(0);
        let in_key = key_alters(key)
            .get(letter)
            .copied()
            .unwrap_or(Alter::Natural);
        let accidental = (spelled.alter != in_key).then(|| spelled.alter.glyph());
        (position, accidental)
    }

    /// The key signature's letters with their staff positions, in written order. A
    /// percussion clef has none.
    pub(crate) fn key_signature_marks(self, key: i8) -> Vec<(usize, i32)> {
        let key = key.clamp(-7, 7);
        let letters: &[usize; 7] = if key > 0 {
            &SHARP_LETTERS
        } else {
            &FLAT_LETTERS
        };
        let positions: &[i32; 7] = match self {
            Clef::Treble if key > 0 => &TREBLE_SHARPS,
            Clef::Treble => &TREBLE_FLATS,
            Clef::Bass if key > 0 => &BASS_SHARPS,
            Clef::Bass => &BASS_FLATS,
            Clef::Alto if key > 0 => &ALTO_SHARPS,
            Clef::Alto => &ALTO_FLATS,
            Clef::Tenor if key > 0 => &TENOR_SHARPS,
            Clef::Tenor => &TENOR_FLATS,
            Clef::Percussion => return Vec::new(),
        };
        letters
            .iter()
            .zip(positions.iter())
            .take(usize::from(key.unsigned_abs()))
            .map(|(&letter, &position)| (letter, position))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spelled(step: i32, alter: Alter) -> Spelled {
        Spelled { step, alter }
    }

    #[test]
    fn c_major_spells_with_sharps() {
        assert_eq!(spell_in_key(60, 0), spelled(28, Alter::Natural)); // C4
        assert_eq!(spell_in_key(61, 0), spelled(28, Alter::Sharp)); // C#4
        assert_eq!(spell_in_key(64, 0).step, 30); // E4
        assert_eq!(spell_in_key(0, 0).step, -7); // C-1
    }

    #[test]
    fn a_flat_key_spells_its_accidentals_with_flats() {
        // B flat major: B flat and E flat, and no other pitch class changes spelling.
        let key = -2;
        assert_eq!(spell_in_key(70, key), spelled(34, Alter::Flat)); // Bb4
        assert_eq!(spell_in_key(63, key), spelled(30, Alter::Flat)); // Eb4
        assert_eq!(spell_in_key(67, key), spelled(32, Alter::Natural)); // G4
        // Chromatic notes outside the signature are spelled with flats too.
        assert_eq!(spell_in_key(66, key), spelled(32, Alter::Flat)); // Gb4
        assert_eq!(spell_in_key(68, key), spelled(33, Alter::Flat)); // Ab4
        assert_eq!(spell_in_key(62, key), spelled(29, Alter::Natural)); // D4
        // A note the signature flats, played natural, is its natural letter.
        assert_eq!(spell_in_key(71, key), spelled(34, Alter::Natural)); // B4
    }

    #[test]
    fn a_sharp_key_spells_its_accidentals_with_sharps() {
        assert_eq!(spell_in_key(66, 2), spelled(31, Alter::Sharp)); // F#4 in D major
        assert_eq!(spell_in_key(61, 2), spelled(28, Alter::Sharp)); // C#4
        assert_eq!(spell_in_key(67, 2), spelled(32, Alter::Natural)); // G4
        // Flat spellings of chromatic notes stay sharps in sharp keys.
        assert_eq!(spell_in_key(70, 2), spelled(33, Alter::Sharp)); // A#4
    }

    #[test]
    fn key_signatures_name_the_right_letters() {
        let alters = key_alters(3);
        assert_eq!(alters[3], Alter::Sharp); // F
        assert_eq!(alters[0], Alter::Sharp); // C
        assert_eq!(alters[4], Alter::Sharp); // G
        assert_eq!(alters[1], Alter::Natural); // D
        let flats = key_alters(-4);
        assert_eq!(flats[6], Alter::Flat); // B
        assert_eq!(flats[2], Alter::Flat); // E
        assert_eq!(flats[5], Alter::Flat); // A
        assert_eq!(flats[1], Alter::Flat); // D
        assert_eq!(flats[4], Alter::Natural); // G
        assert!(key_alters(8).iter().filter(|a| **a == Alter::Sharp).count() == 7);
    }

    #[test]
    fn guitar_notes_are_written_an_octave_up_on_the_treble_staff() {
        // Low E (E2, 40) is written E3: three ledger lines below the staff.
        assert_eq!(Clef::Treble.place(40, 0, true).1, -7);
        // Open B string (B3, 59) is written B4: the middle line.
        assert_eq!(Clef::Treble.place(59, 0, true).1, 4);
        // Bass low E (E1, 28) is written E2 in the bass clef: below the bottom line G2.
        assert_eq!(Clef::Bass.place(28, 0, true).1, -2);
        // A piano's bass staff is written at pitch: G2 is the bottom line of the bass clef.
        assert_eq!(Clef::Bass.place(43, 0, false).1, 0);
        assert_eq!(Clef::Treble.place(64, 0, false).1, 0);
        assert_eq!(Clef::for_lowest_string(28), Clef::Bass);
        assert_eq!(Clef::for_lowest_string(23), Clef::Bass); // 5-string bass, B0
        assert_eq!(Clef::for_lowest_string(30), Clef::Treble); // 8-string guitar, F#1
        assert_eq!(Clef::for_lowest_string(40), Clef::Treble);
    }

    #[test]
    fn key_signature_positions_follow_the_clef() {
        // Treble sharps go on F, C, G, D, A, E, B at positions 8, 5, 9, 6, 3, 7, 4.
        assert_eq!(
            Clef::Treble.key_signature_marks(7),
            [(3, 8), (0, 5), (4, 9), (1, 6), (5, 3), (2, 7), (6, 4)]
        );
        assert_eq!(Clef::Bass.key_signature_marks(-2), [(6, 2), (2, 5)]);
        assert!(Clef::Percussion.key_signature_marks(3).is_empty());
        assert!(Clef::Treble.key_signature_marks(0).is_empty());
    }
}
