//! Guitar Pro song data: chord.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::model::*;

/// A chord annotation for beats
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chord {
    pub length: u8,
    pub sharp: Option<bool>,
    pub root: Option<PitchClass>,
    pub kind: Option<ChordType>,
    pub extension: Option<ChordExtension>,
    pub bass: Option<PitchClass>,
    pub tonality: Option<ChordAlteration>,
    pub add: Option<bool>,
    pub name: String,
    pub fifth: Option<ChordAlteration>,
    pub ninth: Option<ChordAlteration>,
    pub eleventh: Option<ChordAlteration>,
    pub first_fret: Option<u8>,
    pub strings: Vec<i8>,
    pub barres: Vec<Barre>,
    pub omissions: Vec<bool>,
    pub fingerings: Vec<Fingering>,
    pub show: Option<bool>,
    pub new_format: Option<bool>,
}

/// A single barre
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Barre {
    pub fret: i8,
    /// First string from the bottom of the barre
    pub start: i8,
    /// Last string on the top of the barre
    pub end: i8,
}

/// A chord diagram (a chord shape with its frets) as Guitar Pro stores it beside a beat.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChordDiagram {
    /// The chord's name ("Gb7/#11").
    pub name: String,
    /// Number of strings the diagram shows.
    pub string_count: u8,
    /// Number of frets the diagram shows.
    pub fret_count: u8,
    /// The fret at the top of the diagram (0 means the nut).
    pub base_fret: u8,
    /// A fret per string: `(string, fret)`; string 0 is the lowest string in the file.
    pub frets: Vec<(u8, i8)>,
}

pub const SHARP_NOTES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

pub const FLAT_NOTES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PitchClass {
    pub note: String,
    pub just: i8,
    /// flat (-1), none (0) or sharp (1).
    pub accidental: i8,
    pub value: i8,
    pub sharp: bool,
}

/// The name of pitch class `value` in `table`, counting around the octave (so 12 is C and
/// -1 is B).
fn note_name(table: &[&'static str; 12], value: i8) -> &'static str {
    let index = usize::try_from(value.rem_euclid(12)).unwrap_or_default();
    table.get(index).copied().unwrap_or_default()
}

/// Whether a note name is flat (`-1`), sharp (`1`) or natural (`0`).
fn accidental_of(note: &str) -> i8 {
    if note.ends_with('b') {
        -1
    } else if note.ends_with('#') {
        1
    } else {
        0
    }
}

impl PitchClass {
    pub fn from(just: i8, accidental: Option<i8>, sharp: Option<bool>) -> PitchClass {
        let mut p = PitchClass {
            just,
            accidental: 0,
            value: -1,
            sharp: true,
            note: String::with_capacity(2),
        };
        let pitch: i8;
        let accidental2: i8;
        if let Some(a) = accidental {
            pitch = p.just;
            accidental2 = a;
        } else {
            let value = p.just.wrapping_rem(12);
            p.note = note_name(&SHARP_NOTES, value).to_owned();
            accidental2 = accidental_of(&p.note);
            if accidental2 < 0 {
                p.sharp = false;
            }
            pitch = value.saturating_sub(accidental2);
        }
        p.just = pitch.wrapping_rem(12);
        p.accidental = accidental2;
        p.value = p.just.saturating_add(accidental2);
        if sharp.is_none() {
            p.sharp = p.accidental >= 0;
        }
        p
    }

    pub fn from_note(note: String) -> PitchClass {
        let accidental = accidental_of(&note);
        let value = (0i8..12)
            .find(|&i| note_name(&SHARP_NOTES, i) == note || note_name(&FLAT_NOTES, i) == note)
            .unwrap_or(-1);
        let just = value.saturating_sub(accidental).wrapping_rem(12);
        PitchClass {
            note,
            just,
            accidental,
            value: just.saturating_add(accidental),
            sharp: accidental >= 0,
        }
    }
}

impl std::fmt::Display for PitchClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let table = if self.sharp {
            &SHARP_NOTES
        } else {
            &FLAT_NOTES
        };
        f.write_str(note_name(table, self.value))
    }
}

#[cfg(test)]
mod test {
    use crate::model::PitchClass;

    #[test]
    fn test_pitch_1() {
        let p = PitchClass::from_note("D#".to_string());
        assert!(p.sharp, "D# is sharp? {}", true);
        assert_eq!(1, p.accidental);
    }
    #[test]
    fn test_pitch_2() {
        let p = PitchClass::from(4, Some(-1), None);
        assert_eq!(3, p.value);
        assert!(!p.sharp);
        assert_eq!("Eb", p.to_string(), "Note should be Eb");
    }
    #[test]
    fn test_pitch_3() {
        let p = PitchClass::from(4, Some(-1), Some(true));
        assert_eq!(3, p.value);
        assert_eq!("D#", p.to_string(), "Note should be D#");
    }
    #[test]
    fn test_pitch_4() {
        let p = PitchClass::from(3, None, None);
        assert_eq!("D#", p.to_string());
    }
    #[test]
    fn test_pitch_5() {
        let p = PitchClass::from(3, None, Some(true));
        assert_eq!("D#", p.to_string(), "Note should be D#");
    }
    #[test]
    fn pitch_classes_past_the_octave_wrap_around_instead_of_panicking() {
        // B# (11 + 1) and Cb (0 - 1) used to index past the note tables.
        assert_eq!(PitchClass::from(11, Some(1), Some(true)).to_string(), "C");
        assert_eq!(PitchClass::from(0, Some(-1), Some(false)).to_string(), "B");
        assert_eq!(PitchClass::from(-128, None, None).to_string(), "E");
        assert_eq!(PitchClass::from_note("H".to_owned()).to_string(), "B");
    }
}
