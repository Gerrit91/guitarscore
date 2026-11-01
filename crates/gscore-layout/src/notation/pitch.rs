//! Where a pitch goes on a five-line staff: spelling (with sharps), clef, staff position.

use crate::glyph::Glyph;

/// Letter and alteration of each pitch class, spelled with sharps: (letter 0 = C ... 6 = B,
/// sharp).
const SPELLING: [(i32, bool); 12] = [
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

/// A written note: its diatonic step (`octave * 7 + letter`, so C4 is 28) and whether it
/// carries a sharp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Spelled {
    pub(crate) step: i32,
    pub(crate) sharp: bool,
}

/// Spells MIDI pitch `midi` with sharps (no key signature yet).
pub(crate) fn spell(midi: u8) -> Spelled {
    let class = usize::from(midi % 12);
    let (letter, sharp) = SPELLING.get(class).copied().unwrap_or((0, false));
    // MIDI octave -1 starts at 0: octave = midi / 12 - 1.
    let octave = i32::from(midi / 12).saturating_sub(1);
    Spelled {
        step: octave.saturating_mul(7).saturating_add(letter),
        sharp,
    }
}

/// The clef of a notation staff. Guitar and bass music is written an octave above where
/// it sounds, as Guitar Pro and most guitar notation do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Clef {
    /// Treble clef: guitars.
    Treble,
    /// Bass clef: basses.
    Bass,
}

/// Semitones between the sounding and the written pitch.
const WRITTEN_OCTAVE_UP: u8 = 12;
/// Below this lowest open string, a track gets the bass clef (F1: 4- and 5-string basses
/// start at E1 or lower; 7- and 8-string guitars at B1 and F#1).
const BASS_BELOW: u8 = 29;

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
            Clef::Treble => 4 * 7 + 2,
            Clef::Bass => 2 * 7 + 4,
        }
    }

    /// The clef glyph and the staff position its origin sits on (the G line for the
    /// treble clef, the F line for the bass clef).
    pub(crate) fn glyph(self) -> (Glyph, i32) {
        match self {
            Clef::Treble => (Glyph::GClef, 2),
            Clef::Bass => (Glyph::FClef, 6),
        }
    }

    /// Where a note sounding at `midi` is written: its spelling and its staff position
    /// (0 = bottom line, 1 = first space, 8 = top line; negative below the staff).
    pub(crate) fn place(self, midi: u8) -> (Spelled, i32) {
        let spelled = spell(midi.saturating_add(WRITTEN_OCTAVE_UP));
        (
            spelled,
            spelled.step.saturating_sub(self.bottom_line_step()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitches_are_spelled_with_sharps() {
        assert_eq!(
            spell(60),
            Spelled {
                step: 28,
                sharp: false
            }
        ); // C4
        assert_eq!(
            spell(61),
            Spelled {
                step: 28,
                sharp: true
            }
        ); // C#4
        assert_eq!(spell(64).step, 30); // E4
        assert_eq!(spell(0).step, -7); // C-1
    }

    #[test]
    fn guitar_notes_are_written_an_octave_up_on_the_treble_staff() {
        // Low E (E2, 40) is written E3: three ledger lines below the staff.
        assert_eq!(Clef::Treble.place(40).1, -7);
        // Open B string (B3, 59) is written B4: the middle line.
        assert_eq!(Clef::Treble.place(59).1, 4);
        // Bass low E (E1, 28) is written E2 in the bass clef: below the bottom line G2.
        assert_eq!(Clef::Bass.place(28).1, -2);
        assert_eq!(Clef::for_lowest_string(28), Clef::Bass);
        assert_eq!(Clef::for_lowest_string(23), Clef::Bass); // 5-string bass, B0
        assert_eq!(Clef::for_lowest_string(30), Clef::Treble); // 8-string guitar, F#1
        assert_eq!(Clef::for_lowest_string(40), Clef::Treble);
    }
}
