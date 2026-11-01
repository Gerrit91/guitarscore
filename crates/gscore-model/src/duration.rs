//! Exact musical lengths and time signatures.

use std::cmp::Ordering;
use std::fmt;
use std::num::{NonZeroU16, NonZeroU64};

use gscore_gp::Duration as GpDuration;

use crate::error::ModelError;

/// Shortest note value accepted (a 128th note).
const MAX_NOTE_VALUE: u16 = 128;
/// Largest tuplet numbers accepted (Guitar Pro uses up to 13).
const MAX_TUPLET: u8 = 64;

/// An exact musical length, as a fraction of a whole note: `1/4` is a quarter note,
/// `3/16` a dotted eighth, `1/12` an eighth in a triplet.
///
/// Always stored reduced, with a non-zero denominator, so `==` compares lengths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Duration {
    num: u64,
    den: u64,
}

impl Duration {
    /// No length at all.
    pub const ZERO: Duration = Duration { num: 0, den: 1 };

    /// `num/den` of a whole note. Fails if `den` is zero.
    pub fn new(num: u64, den: u64) -> Result<Self, ModelError> {
        let den = NonZeroU64::new(den)
            .ok_or_else(|| ModelError::Invalid("duration with denominator 0".to_owned()))?;
        Ok(Duration::reduced(num, den))
    }

    /// `num/den` in lowest terms. Infallible: the denominator cannot be zero.
    fn reduced(num: u64, den: NonZeroU64) -> Self {
        // gcd(num, den) >= 1 because den >= 1, so the fallback is never used.
        let g = NonZeroU64::new(gcd(num, den.get())).unwrap_or(NonZeroU64::MIN);
        Duration {
            num: num / g,
            den: den.get() / g,
        }
    }

    /// Numerator of the reduced fraction.
    pub fn numerator(self) -> u64 {
        self.num
    }

    /// Denominator of the reduced fraction (never zero).
    pub fn denominator(self) -> u64 {
        self.den
    }

    /// The sum of two lengths, or an error on overflow.
    pub fn checked_add(self, other: Duration) -> Result<Duration, ModelError> {
        let overflow = || ModelError::Overflow("a sum of durations");
        let g = gcd(self.den, other.den);
        let other_scale = self.den.checked_div(g).ok_or_else(overflow)?;
        let self_scale = other.den.checked_div(g).ok_or_else(overflow)?;
        let num = self
            .num
            .checked_mul(self_scale)
            .and_then(|a| {
                other
                    .num
                    .checked_mul(other_scale)
                    .and_then(|b| a.checked_add(b))
            })
            .ok_or_else(overflow)?;
        let den = self.den.checked_mul(self_scale).ok_or_else(overflow)?;
        Duration::new(num, den)
    }

    /// `self - other`, or an error if `other` is longer (lengths are never negative) or on
    /// overflow.
    pub fn checked_sub(self, other: Duration) -> Result<Duration, ModelError> {
        let overflow = || ModelError::Overflow("a difference of durations");
        let g = gcd(self.den, other.den);
        let other_scale = self.den.checked_div(g).ok_or_else(overflow)?;
        let self_scale = other.den.checked_div(g).ok_or_else(overflow)?;
        let lhs = self.num.checked_mul(self_scale).ok_or_else(overflow)?;
        let rhs = other.num.checked_mul(other_scale).ok_or_else(overflow)?;
        let num = lhs.checked_sub(rhs).ok_or_else(|| {
            ModelError::Invalid(format!("duration {self} minus the longer {other}"))
        })?;
        let den = self.den.checked_mul(self_scale).ok_or_else(overflow)?;
        Duration::new(num, den)
    }

    /// Converts a Guitar Pro beat duration: note value, dots and tuplet.
    pub(crate) fn from_gp(gp: &GpDuration) -> Result<Self, ModelError> {
        let value = gp.value;
        if !(1..=MAX_NOTE_VALUE).contains(&value) || !value.is_power_of_two() {
            return Err(ModelError::Invalid(format!(
                "note value 1/{value} is not a power of two up to 1/{MAX_NOTE_VALUE}"
            )));
        }
        let (dot_num, dot_den) = match (gp.dotted, gp.double_dotted) {
            (false, false) => (1, 1),
            (true, false) => (3, 2),
            (false, true) => (7, 4),
            (true, true) => {
                return Err(ModelError::Invalid(
                    "duration is both dotted and double-dotted".to_owned(),
                ));
            }
        };
        // `enters` notes in the time of `times`: each note lasts `times/enters` as long.
        let (enters, times) = (gp.tuplet_enters, gp.tuplet_times);
        if !(1..=MAX_TUPLET).contains(&enters) || !(1..=MAX_TUPLET).contains(&times) {
            return Err(ModelError::Invalid(format!(
                "tuplet {enters}:{times} is out of range 1..={MAX_TUPLET}"
            )));
        }
        // All factors are small (at most 128 * 4 * 64), so this cannot overflow in practice.
        let overflow = || ModelError::Overflow("a beat duration");
        let num = u64::from(times).checked_mul(dot_num).ok_or_else(overflow)?;
        let den = u64::from(value)
            .checked_mul(dot_den)
            .and_then(|d| d.checked_mul(u64::from(enters)))
            .ok_or_else(overflow)?;
        Duration::new(num, den)
    }
}

impl Ord for Duration {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare a/b with c/d as a*d with c*b. A u64 * u64 product always fits in u128,
        // so `saturating_mul` never actually saturates; it just states that.
        let lhs = u128::from(self.num).saturating_mul(u128::from(other.den));
        let rhs = u128::from(other.num).saturating_mul(u128::from(self.den));
        lhs.cmp(&rhs)
    }
}

impl PartialOrd for Duration {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.num, self.den)
    }
}

/// A note value as Guitar Pro stores it: a `1/value` note, optionally dotted, optionally
/// part of a tuplet.
///
/// Unlike [`Duration`], which is an exact length, a note value keeps the notation: a quarter
/// and two eighths are both a quarter note long, but only one is written as `1/4`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoteValue {
    value: u16,
    dots: u8,
    tuplet: Option<(u8, u8)>,
}

impl NoteValue {
    /// A `1/value` note with `dots` augmentation dots (0 to 2) and, if given,
    /// `enters` notes in the time of `times`.
    pub fn new(value: u16, dots: u8, tuplet: Option<(u8, u8)>) -> Result<Self, ModelError> {
        if !(1..=MAX_NOTE_VALUE).contains(&value) || !value.is_power_of_two() {
            return Err(ModelError::Invalid(format!(
                "note value 1/{value} is not a power of two up to 1/{MAX_NOTE_VALUE}"
            )));
        }
        if dots > 2 {
            return Err(ModelError::Invalid(format!(
                "{dots} augmentation dots (supported: 0, 1 or 2)"
            )));
        }
        if let Some((enters, times)) = tuplet
            && (!(1..=MAX_TUPLET).contains(&enters) || !(1..=MAX_TUPLET).contains(&times))
        {
            return Err(ModelError::Invalid(format!(
                "tuplet {enters}:{times} is out of range 1..={MAX_TUPLET}"
            )));
        }
        Ok(NoteValue {
            value,
            dots,
            tuplet: tuplet.filter(|&(enters, times)| enters != 1 || times != 1),
        })
    }

    /// The note value's denominator: `4` for a quarter note.
    pub fn value(self) -> u16 {
        self.value
    }

    /// The number of augmentation dots (0, 1 or 2).
    pub fn dots(self) -> u8 {
        self.dots
    }

    /// The tuplet as `enters:times` (3:2 is a triplet), if it has one.
    pub fn tuplet(self) -> Option<(u8, u8)> {
        self.tuplet
    }

    /// This note value's exact length.
    pub fn duration(self) -> Result<Duration, ModelError> {
        Duration::from_gp(&self.to_gp())
    }

    /// The Guitar Pro representation used when writing an edit into the song.
    pub(crate) fn to_gp(self) -> GpDuration {
        let (dotted, double_dotted) = match self.dots {
            0 => (false, false),
            1 => (true, false),
            _ => (false, true),
        };
        let (tuplet_enters, tuplet_times) = self.tuplet.unwrap_or((1, 1));
        GpDuration {
            value: self.value,
            dotted,
            double_dotted,
            tuplet_enters,
            tuplet_times,
            ..GpDuration::default()
        }
    }

    /// Parses a note value as written on the command line: `1/4`, `4`, `quarter`,
    /// `1/8.` (dotted), `1/16..` (double-dotted), optionally followed by `*3:2` for a
    /// tuplet.
    pub fn parse(text: &str) -> Result<Self, ModelError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ModelError::Invalid("empty note value".to_owned()));
        }
        let (main, tuplet) = match text.split_once('*') {
            Some((main, rest)) => {
                if rest.contains('*') {
                    return Err(ModelError::Invalid(format!(
                        "more than one tuplet in note value \"{text}\""
                    )));
                }
                (main, Some(parse_tuplet(rest)?))
            }
            None => (text, None),
        };
        let main = main.trim();
        let trimmed = main.trim_end_matches('.');
        let dots = u8::try_from(main.len().saturating_sub(trimmed.len()))
            .map_err(|_| ModelError::Invalid("too many augmentation dots".to_owned()))?;
        let value = parse_note_value(trimmed)?;
        NoteValue::new(value, dots, tuplet)
    }
}

impl fmt::Display for NoteValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "1/{}", self.value)?;
        for _ in 0..self.dots {
            f.write_str(".")?;
        }
        if let Some((enters, times)) = self.tuplet {
            write!(f, "*{enters}:{times}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for NoteValue {
    type Err = ModelError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        NoteValue::parse(text)
    }
}

/// Parses `1/8`, `8`, `eighth`, ... into the note value denominator.
fn parse_note_value(text: &str) -> Result<u16, ModelError> {
    let value = if let Some((numerator, denominator)) = text.split_once('/') {
        let numerator: u64 = numerator
            .trim()
            .parse()
            .map_err(|_| ModelError::Invalid(format!("note value \"{text}\"")))?;
        let denominator: u64 = denominator
            .trim()
            .parse()
            .map_err(|_| ModelError::Invalid(format!("note value \"{text}\"")))?;
        if numerator != 1 {
            return Err(ModelError::Invalid(format!(
                "note value \"{text}\" is not 1/n"
            )));
        }
        u16::try_from(denominator)
            .map_err(|_| ModelError::Invalid(format!("note value \"{text}\" is too large")))?
    } else if let Ok(value) = text.parse::<u16>() {
        value
    } else {
        note_value_by_name(text)
            .ok_or_else(|| ModelError::Invalid(format!("note value \"{text}\"")))?
    };
    Ok(value)
}

/// Common note names, so scripts can say `quarter` instead of `1/4`.
fn note_value_by_name(name: &str) -> Option<u16> {
    let names: [(&str, u16); 8] = [
        ("whole", 1),
        ("half", 2),
        ("quarter", 4),
        ("eighth", 8),
        ("sixteenth", 16),
        ("thirty-second", 32),
        ("sixty-fourth", 64),
        ("hundred-twenty-eighth", 128),
    ];
    names
        .iter()
        .find(|(candidate, _)| name.eq_ignore_ascii_case(candidate))
        .map(|&(_, value)| value)
}

/// Parses the `enters:times` part of a tuplet note value.
fn parse_tuplet(text: &str) -> Result<(u8, u8), ModelError> {
    let invalid = || ModelError::Invalid(format!("tuplet \"{text}\" (expected enters:times)"));
    let (enters, times) = text.split_once(':').ok_or_else(invalid)?;
    let enters: u8 = enters.trim().parse().map_err(|_| invalid())?;
    let times: u8 = times.trim().parse().map_err(|_| invalid())?;
    if !(1..=MAX_TUPLET).contains(&enters) || !(1..=MAX_TUPLET).contains(&times) {
        return Err(ModelError::Invalid(format!(
            "tuplet {enters}:{times} is out of range 1..={MAX_TUPLET}"
        )));
    }
    Ok((enters, times))
}

/// A time signature such as 4/4, 3/4 or 7/8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimeSignature {
    numerator: u8,
    denominator: NonZeroU16,
}

impl TimeSignature {
    /// `numerator/denominator`; the denominator must be a power of two up to 128.
    pub fn new(numerator: u8, denominator: u16) -> Result<Self, ModelError> {
        if numerator == 0 {
            return Err(ModelError::Invalid(
                "time signature with 0 beats".to_owned(),
            ));
        }
        let invalid_denominator = || {
            ModelError::Invalid(format!(
                "time signature denominator {denominator} is not a power of two up to \
                 {MAX_NOTE_VALUE}"
            ))
        };
        if denominator > MAX_NOTE_VALUE || !denominator.is_power_of_two() {
            return Err(invalid_denominator());
        }
        let denominator = NonZeroU16::new(denominator).ok_or_else(invalid_denominator)?;
        Ok(TimeSignature {
            numerator,
            denominator,
        })
    }

    /// Beats per measure (the upper number).
    pub fn numerator(self) -> u8 {
        self.numerator
    }

    /// Note value of one beat (the lower number).
    pub fn denominator(self) -> u16 {
        self.denominator.get()
    }

    /// Length of a full measure: 3/4 lasts three quarter notes.
    pub fn measure_length(self) -> Duration {
        Duration::reduced(
            u64::from(self.numerator),
            NonZeroU64::from(self.denominator),
        )
    }

    pub(crate) fn from_gp(gp: &gscore_gp::TimeSignature) -> Result<Self, ModelError> {
        let numerator = u8::try_from(gp.numerator).map_err(|_| {
            ModelError::Invalid(format!("time signature with {} beats", gp.numerator))
        })?;
        TimeSignature::new(numerator, gp.denominator.value)
    }
}

impl fmt::Display for TimeSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

/// Greatest common divisor; `gcd(0, 0) == 0`.
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a.checked_rem(b).unwrap_or(0));
    }
    a
}
