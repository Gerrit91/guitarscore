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
