//! Index and value types that say what a number means: track, measure, string, fret, ...
//!
//! Indices are 0-based (for code); their `Display` is 1-based (for people).

use std::fmt;

use crate::error::ModelError;

/// Defines a 0-based index newtype with a 1-based `Display`.
macro_rules! index_type {
    ($(#[$doc:meta])* $name:ident, $label:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(usize);

        impl $name {
            /// Wraps a 0-based index.
            pub const fn new(index: usize) -> Self {
                Self(index)
            }

            /// The 0-based index.
            pub const fn get(self) -> usize {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($label, " {}"), self.0.saturating_add(1))
            }
        }
    };
}

index_type!(
    /// Position of a track in the song.
    TrackIndex,
    "track"
);
index_type!(
    /// Position of a measure (bar) in a track; the same for every track.
    MeasureIndex,
    "measure"
);
index_type!(
    /// Position of a voice in a measure.
    VoiceIndex,
    "voice"
);
index_type!(
    /// Position of a beat in a voice.
    BeatIndex,
    "beat"
);

/// The address of one beat in a song.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BeatPosition {
    /// The track.
    pub track: TrackIndex,
    /// The measure within the track.
    pub measure: MeasureIndex,
    /// The voice within the measure.
    pub voice: VoiceIndex,
    /// The beat within the voice.
    pub beat: BeatIndex,
}

impl fmt::Display for BeatPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}, {}, {}, {}",
            self.track, self.measure, self.voice, self.beat
        )
    }
}

/// A string of a track, 1-based as on a tab staff: string 1 is the highest-pitched one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StringNumber(u8);

impl StringNumber {
    /// String `number` (1-based). Fails for 0.
    pub fn new(number: u8) -> Result<Self, ModelError> {
        if number == 0 {
            return Err(ModelError::Invalid(
                "string number 0 (strings count from 1)".to_owned(),
            ));
        }
        Ok(StringNumber(number))
    }

    /// The 1-based string number.
    pub fn get(self) -> u8 {
        self.0
    }

    pub(crate) fn from_gp(value: i8) -> Result<Self, ModelError> {
        u8::try_from(value)
            .map_err(|_| ModelError::Invalid(format!("string number {value}")))
            .and_then(StringNumber::new)
    }
}

impl fmt::Display for StringNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "string {}", self.0)
    }
}

/// Highest fret value accepted. Percussion tracks store MIDI drum notes in the fret field,
/// so the limit is the MIDI range rather than a guitar's fret count.
const MAX_FRET: u8 = 127;

/// A fret number (0 = open string). On percussion tracks: the MIDI drum note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fret(u8);

impl Fret {
    /// Fret `value`. Fails above 127.
    pub fn new(value: u8) -> Result<Self, ModelError> {
        if value > MAX_FRET {
            return Err(ModelError::Invalid(format!(
                "fret {value} is above {MAX_FRET}"
            )));
        }
        Ok(Fret(value))
    }

    /// The fret number.
    pub fn get(self) -> u8 {
        self.0
    }

    pub(crate) fn from_gp(value: i16) -> Result<Self, ModelError> {
        u8::try_from(value)
            .map_err(|_| ModelError::Invalid(format!("fret {value}")))
            .and_then(Fret::new)
    }
}

/// A MIDI pitch (0..=127; 60 is middle C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pitch(u8);

impl Pitch {
    /// MIDI note number `value`. Fails above 127.
    pub fn new(value: u8) -> Result<Self, ModelError> {
        if value > 127 {
            return Err(ModelError::Invalid(format!(
                "MIDI pitch {value} is above 127"
            )));
        }
        Ok(Pitch(value))
    }

    /// The MIDI note number.
    pub fn get(self) -> u8 {
        self.0
    }

    pub(crate) fn from_gp(value: i8) -> Result<Self, ModelError> {
        u8::try_from(value)
            .map_err(|_| ModelError::Invalid(format!("MIDI pitch {value}")))
            .and_then(Pitch::new)
    }

    /// Parses a note name in scientific pitch notation (`E2`, `C#4`, `Bb3`) or a plain
    /// MIDI number (`40`). Case does not matter.
    pub fn parse(text: &str) -> Result<Self, ModelError> {
        let text = text.trim();
        // A plain number is a MIDI pitch, checked against the same range as a name.
        if let Ok(number) = text.parse::<u8>() {
            return Pitch::new(number);
        }
        let mut chars = text.chars();
        let letter = chars.next().unwrap_or('?').to_ascii_uppercase();
        let semitone = match letter {
            'C' => 0_i16,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => {
                return Err(ModelError::Invalid(format!(
                    "\"{text}\" is not a note name such as E2 or C#4"
                )));
            }
        };
        let rest = chars.as_str();
        let (accidental, rest) = match rest.get(0..1) {
            Some("#") => (1_i16, rest.get(1..).unwrap_or_default()),
            Some("b") => (-1_i16, rest.get(1..).unwrap_or_default()),
            _ => (0_i16, rest),
        };
        let octave: i32 = rest.parse().map_err(|_| {
            ModelError::Invalid(format!("\"{text}\" has no octave such as the 2 in E2"))
        })?;
        // MIDI 0 is C-1, so a note's number is (octave + 1) * 12 + semitone.
        let midi = octave
            .saturating_add(1)
            .saturating_mul(12)
            .saturating_add(i32::from(semitone))
            .saturating_add(i32::from(accidental));
        let midi = u8::try_from(midi)
            .map_err(|_| ModelError::Invalid(format!("\"{text}\" is outside MIDI's range")))?;
        Pitch::new(midi)
            .map_err(|_| ModelError::Invalid(format!("\"{text}\" is outside MIDI's range")))
    }

    /// The note name without octave, spelled with sharps: `C`, `C#`, ..., `B`.
    pub fn note_name(self) -> &'static str {
        // `% 12` is always a valid index; the fallback is never used.
        NOTE_NAMES
            .get(usize::from(self.0 % 12))
            .copied()
            .unwrap_or("?")
    }
}

/// Note names of the 12 pitch classes, spelled with sharps.
const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Scientific pitch notation with sharps: 60 is `C4`, 40 (a guitar's low E) is `E2`,
/// 0 is `C-1`.
impl fmt::Display for Pitch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `% 12` is always a valid index; the fallback is never used.
        let name = NOTE_NAMES
            .get(usize::from(self.0 % 12))
            .copied()
            .unwrap_or("?");
        // MIDI octave -1 starts at 0, so octave = pitch / 12 - 1.
        let octave = i16::from(self.0 / 12).saturating_sub(1);
        write!(f, "{name}{octave}")
    }
}
