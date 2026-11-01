//! Guitar Pro song data: key signature.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see LICENSE-guitarpro next to this file). Changes: docs/guitarpro.md.

use crate::gp::*;

pub const DURATION_QUARTER_TIME: i64 = 960;

//pub const DURATION_WHOLE: u8 = 1;
//pub const DURATION_HALF: u8 = 2;
pub const DURATION_QUARTER: u8 = 4;

pub const DURATION_EIGHTH: u8 = 8;

pub const DURATION_SIXTEENTH: u8 = 16;

pub const DURATION_THIRTY_SECOND: u8 = 32;

pub const DURATION_SIXTY_FOURTH: u8 = 64;

pub const DURATION_HUNDRED_TWENTY_EIGHTH: u8 = 128;

/// A time signature
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeSignature {
    pub numerator: i8,
    pub denominator: Duration,
    pub beams: Vec<u8>,
}

impl Default for TimeSignature {
    fn default() -> Self {
        TimeSignature {
            numerator: 4,
            denominator: Duration::default(),
            beams: vec![2, 2, 2, 2],
        }
    }
}

pub const KEY_SIGNATURES: [&str; 34] = [
    "F♭ major",
    "C♭ major",
    "G♭ major",
    "D♭ major",
    "A♭ major",
    "E♭ major",
    "B♭ major",
    "F major",
    "C major",
    "G major",
    "D major",
    "A major",
    "E major",
    "B major",
    "F# major",
    "C# major",
    "G# major",
    "D♭ minor",
    "A♭ minor",
    "E♭ minor",
    "B♭ minor",
    "F minor",
    "C minor",
    "G minor",
    "D minor",
    "A minor",
    "E minor",
    "B minor",
    "F# minor",
    "C# minor",
    "G# minor",
    "D# minor",
    "A# minor",
    "E# minor",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeySignature {
    pub key: i8,
    pub is_minor: bool,
}

//impl Default for KeySignature { fn default() -> Self { KeySignature { key: 0, is_minor: false, }} }
impl std::fmt::Display for KeySignature {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let base: i8 = if self.is_minor { 23 } else { 8 };
        match base
            .checked_add(self.key)
            .and_then(|v| usize::try_from(v).ok())
            .and_then(|i| KEY_SIGNATURES.get(i))
        {
            Some(name) => write!(f, "{}", name),
            None => write!(f, "Unknown key ({}, minor={})", self.key, self.is_minor),
        }
    }
}

const SUPPORTED_TUPLETS: [(u8, u8); 10] = [
    (1, 1),
    (3, 2),
    (5, 4),
    (6, 4),
    (7, 4),
    (9, 8),
    (10, 8),
    (11, 8),
    (12, 8),
    (13, 8),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duration {
    pub value: u16,
    pub dotted: bool,
    pub double_dotted: bool,
    /// The time resulting with a 64th note and a 3/2 tuplet
    pub min_time: u8,
    //Tuplet division type
    pub tuplet_enters: u8,
    pub tuplet_times: u8,
}

impl Default for Duration {
    fn default() -> Self {
        Duration {
            value: DURATION_QUARTER as u16,
            dotted: false,
            double_dotted: false,
            tuplet_enters: 1,
            tuplet_times: 1,
            min_time: 0,
        }
    }
}

impl Duration {
    //fn convert_time(&self, time: u64) -> u64 { time * self.division_times as u64 / self.division_enters as u64 }

    pub fn is_supported(&self) -> bool {
        SUPPORTED_TUPLETS.contains(&(self.tuplet_enters, self.tuplet_times))
    }

    pub fn _index(&self) -> u8 {
        let mut index = 0u8;
        let mut value = self.value;
        loop {
            value >>= 1;
            if value > 0 {
                index += 1;
            } else {
                break;
            }
        }
        index
    }
    pub fn is_default_tuplet(&self) -> bool {
        self.tuplet_times == 1 && self.tuplet_enters == 1
    }
}

/*/// A *n:m* tuplet.
#[derive(Clone)]
struct Tuplet {
    enters: u8,
    times: u8,
}*/
/*impl Default for Tuplet { fn default() -> Self { Tuplet { enters: 1, times: 1 }}}
impl Tuplet {
    fn is_supported(self) -> bool { SUPPORTED_TUPLETS.contains(&(self.enters, self.times)) }
    fn convert_time(self) -> u8 {
        let result = fraction::Fraction::new(self.enters, self.times);
        if result.denom().expect("Cannot get fraction denominator") == &1 {1}
        else {result.to_u8().expect("Cannot get fraction result")}
    }
}*/

/* Enum ?: const KEY_F_MAJOR_FLAT: (i8, bool) = (-8, false);
const KEY_C_MAJOR_FLAT: (i8, bool) = (-7, false);
const KEY_G_MAJOR_FLAT: (i8, bool) = (-6, false);
const KEY_D_MAJOR_FLAT: (i8, bool) = (-5, false);
const KEY_A_MAJOR_FLAT: (i8, bool) = (-4, false);
const KEY_E_MAJOR_FLAT: (i8, bool) = (-3, false);
const KEY_B_MAJOR_FLAT: (i8, bool) = (-2, false);
const KEY_F_MAJOR: (i8, bool) = (-1, false);
const KEY_C_MAJOR: (i8, bool) = (0, false);
const KEY_G_MAJOR: (i8, bool) = (1, false);
const KEY_D_MAJOR: (i8, bool) = (2, false);
const KEY_A_MAJOR: (i8, bool) = (3, false);
const KEY_E_MAJOR: (i8, bool) = (4, false);
const KEY_B_MAJOR: (i8, bool) = (5, false);
const KEY_F_MAJOR_SHARP: (i8, bool) = (6, false);
const KEY_C_MAJOR_SHARP: (i8, bool) = (7, false);
const KEY_G_MAJOR_SHARP: (i8, bool) = (8, false);
const KEY_D_MINOR_FLAT: (i8, bool) = (-8, true);
const KEY_A_MINOR_FLAT: (i8, bool) = (-7, true);
const KEY_E_MINOR_FLAT: (i8, bool) = (-6, true);
const KEY_B_MINOR_FLAT: (i8, bool) = (-5, true);
const KEY_F_MINOR: (i8, bool) = (-4, true);
const KEY_C_MINOR: (i8, bool) = (-3, true);
const KEY_G_MINOR: (i8, bool) = (-2, true);
const KEY_D_MINOR: (i8, bool) = (-1, true);
const KEY_A_MINOR: (i8, bool) = (0, true);
const KEY_E_MINOR: (i8, bool) = (1, true);
const KEY_B_MINOR: (i8, bool) = (2, true);
const KEY_F_MINOR_SHARP: (i8, bool) = (3, true);
const KEY_C_MINOR_SHARP: (i8, bool) = (4, true);
const KEY_G_MINOR_SHARP: (i8, bool) = (5, true);
const KEY_D_MINOR_SHARP: (i8, bool) = (6, true);
const KEY_A_MINOR_SHARP: (i8, bool) = (7, true);
const KEY_E_MINOR_SHARP: (i8, bool) = (8, true);*/
