//! Guitar Pro reading and writing: key signature.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;
use fraction::ToPrimitive;

impl Duration {
    pub(crate) fn convert_time(&self, time: u32) -> GpResult<u32> {
        let result =
            fraction::Fraction::new(time * self.tuplet_enters as u32, self.tuplet_times as u32);
        let denom = *result
            .denom()
            .ok_or_else(|| GpError::FormatError("fraction denom is None".to_string()))?;
        if denom == 1 {
            let numer = *result
                .numer()
                .ok_or_else(|| GpError::FormatError("fraction numer is None".to_string()))?;
            numer.to_u32().ok_or(GpError::TypeConversion {
                context: "fraction numer to u32",
                value: numer as i64,
            })
        } else {
            result.trunc().to_u32().ok_or(GpError::TypeConversion {
                context: "fraction trunc to u32",
                value: 0,
            })
        }
    }
    pub(crate) fn time(&self) -> GpResult<u32> {
        let mut result =
            (f64::from(DURATION_QUARTER_TIME as i32) * 4f64 / f64::from(self.value)).trunc();
        //println!("\tDuration.time(): result: {}", result);
        if self.dotted {
            result += (result / 2f64).trunc();
        }
        //if self.dotted { result += (result/4f64).trunc() * 3f64; }
        //println!("\tDuration.time(): result: {}", result);
        let time = result.to_u32().ok_or(GpError::TypeConversion {
            context: "duration time to u32",
            value: result as i64,
        })?;
        self.convert_time(time)
    }
    //@classmethod def fromFraction(cls, frac): return cls(frac.denominator, frac.numerator)
    pub(crate) fn write_duration(&self, data: &mut Vec<u8>, flags: u8) -> GpResult<()> {
        let value = (16 - self.value.leading_zeros()).to_i8_gp("duration value bits")? - 3; //value = duration.value.bit_length() - 3
        write_signed_byte(data, value);
        if (flags & 0x20) == 0x20 {
            if !self.is_supported() {
                return Ok(());
            }
            write_i32(data, self.tuplet_enters as i32); //write iTuplet
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
    //println!("read_duration()");
    let b = read_signed_byte(data, seek)?;
    // Compute in i16 to avoid overflow when `b` is a garbage byte (e.g. 127).
    // Guitar Pro's valid range is `-2..=3`; anything else is treated as whole
    // note as a safe fallback.
    let shift = i16::from(b) + 2;
    let val = if (0..16).contains(&shift) {
        1u16 << shift
    } else {
        1u16
    }; // Fallback to 1 (whole note?) or whatever safe
    let mut d = Duration {
        value: val,
        ..Default::default()
    };
    //let b = read_signed_byte(data, seek); println!("B: {}", b); d.value = 1 << (b + 2);
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
