//! Guitar Pro song data: key signature.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::model::*;

pub const DURATION_QUARTER_TIME: i64 = 960;

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
    pub fn is_supported(&self) -> bool {
        SUPPORTED_TUPLETS.contains(&(self.tuplet_enters, self.tuplet_times))
    }

    pub fn is_default_tuplet(&self) -> bool {
        self.tuplet_times == 1 && self.tuplet_enters == 1
    }
}
