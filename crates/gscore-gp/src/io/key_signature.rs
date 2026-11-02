//! Guitar Pro reading and writing: key signature.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;
use fraction::ToPrimitive;

impl Duration {
    pub(crate) fn convert_time(&self, time: u32) -> GpResult<u32> {
        let enters =
            time.checked_mul(u32::from(self.tuplet_enters))
                .ok_or(GpError::TypeConversion {
                    context: "tuplet time",
                    value: i64::from(time),
                })?;
        let result = fraction::Fraction::new(enters, u32::from(self.tuplet_times));
        let denom = *result
            .denom()
            .ok_or_else(|| GpError::FormatError("fraction denom is None".to_string()))?;
        if denom == 1 {
            let numer = *result
                .numer()
                .ok_or_else(|| GpError::FormatError("fraction numer is None".to_string()))?;
            numer.to_u32().ok_or(GpError::TypeConversion {
                context: "fraction numer to u32",
                value: numer.to_i64().unwrap_or(i64::MAX),
            })
        } else {
            result.trunc().to_u32().ok_or(GpError::TypeConversion {
                context: "fraction trunc to u32",
                value: 0,
            })
        }
    }
    pub(crate) fn time(&self) -> GpResult<u32> {
        let quarter = f64::from(i32::try_from(DURATION_QUARTER_TIME).unwrap_or(960));
        let mut result = (quarter * 4f64 / f64::from(self.value)).trunc();
        if self.dotted {
            result += (result / 2f64).trunc();
        }
        let time = result.to_u32().ok_or(GpError::TypeConversion {
            context: "duration time to u32",
            value: result.to_i64().unwrap_or(i64::MAX),
        })?;
        self.convert_time(time)
    }
    pub(crate) fn write_duration(&self, data: &mut Vec<u8>, flags: u8) -> GpResult<()> {
        // The note value's bit length minus 3: whole note -2, half -1, quarter 0, ...
        let bits = 16_u32.saturating_sub(self.value.leading_zeros());
        let value = bits.to_i8_gp("duration value bits")?.saturating_sub(3);
        write_signed_byte(data, value);
        if (flags & 0x20) == 0x20 {
            if !self.is_supported() {
                return Ok(());
            }
            write_i32(data, i32::from(self.tuplet_enters)); // the tuplet
        }
        Ok(())
    }
}

/// Read beat duration.
/// Duration is composed of byte signifying duration and an integer that maps to `Tuplet`. The byte maps to following values:
///
/// * *-2*: whole note
/// * *-1*: half note
/// * *0*: quarter note
/// * *1*: eighth note
/// * *2*: sixteenth note
/// * *3*: thirty-second note
///
/// If flag at *0x20* is true, the tuplet is read
pub(crate) fn read_duration(data: &[u8], seek: &mut usize, flags: u8) -> GpResult<Duration> {
    let b = read_signed_byte(data, seek)?;
    // Compute in i16 to avoid overflow when `b` is a garbage byte (e.g. 127).
    // Guitar Pro's valid range is `-2..=3`; anything else is treated as whole
    // note as a safe fallback.
    let shift = i16::from(b).saturating_add(2);
    let val = if (0..16).contains(&shift) {
        1u16 << shift
    } else {
        1u16
    }; // Fallback to 1 (whole note?) or whatever safe
    let mut d = Duration {
        value: val,
        ..Default::default()
    };
    d.dotted = (flags & 0x01) == 0x01;
    if (flags & 0x20) == 0x20 {
        let i_tuplet = read_int(data, seek)?;
        if i_tuplet == 3 {
            d.tuplet_enters = 3;
            d.tuplet_times = 2;
        } else if i_tuplet == 5 {
            d.tuplet_enters = 5;
            d.tuplet_times = 4;
        } else if i_tuplet == 6 {
            d.tuplet_enters = 6;
            d.tuplet_times = 4;
        } else if i_tuplet == 7 {
            d.tuplet_enters = 7;
            d.tuplet_times = 4;
        } else if i_tuplet == 9 {
            d.tuplet_enters = 9;
            d.tuplet_times = 8;
        } else if i_tuplet == 10 {
            d.tuplet_enters = 10;
            d.tuplet_times = 8;
        } else if i_tuplet == 11 {
            d.tuplet_enters = 11;
            d.tuplet_times = 8;
        } else if i_tuplet == 12 {
            d.tuplet_enters = 12;
            d.tuplet_times = 8;
        } else if i_tuplet == 13 {
            d.tuplet_enters = 13;
            d.tuplet_times = 8;
        }
    }
    Ok(d)
}
