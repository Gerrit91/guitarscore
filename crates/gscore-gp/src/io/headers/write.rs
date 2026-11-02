//! Writing measure headers, the clipboard header and directions (GP3-5).

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use std::collections::HashMap;

use crate::io::*;

/// Writes `header`; `previous` decides which signatures need writing again.
pub(super) fn measure_header(
    data: &mut Vec<u8>,
    header: &MeasureHeader,
    previous: Option<&MeasureHeader>,
    version: &(u8, u8, u8),
) -> GpResult<()> {
    let signature = &header.time_signature;
    let mut flags: u8 = 0x00;
    match previous {
        Some(p) => {
            if signature.numerator != p.time_signature.numerator {
                flags |= 0x01;
            }
            if signature.denominator.value != p.time_signature.denominator.value {
                flags |= 0x02;
            }
        }
        None => flags |= 0x03,
    }
    if header.repeat_open {
        flags |= 0x04;
    }
    if header.repeat_close > -1 {
        flags |= 0x08;
    }
    if header.repeat_alternative > 0 {
        flags |= 0x10;
    }
    if header.marker.is_some() {
        flags |= 0x20;
    }
    if header.double_bar {
        flags |= 0x80;
    }
    if version.0 >= 4 && previous.is_none_or(|p| header.key_signature != p.key_signature) {
        flags |= 0x40;
    }
    if version.0 >= 5
        && let Some(p) = previous
    {
        if *signature != p.time_signature {
            flags |= 0x03;
        }
        write_placeholder_default(data, 1);
    }
    write_byte(data, flags);
    if (flags & 0x01) == 0x01 {
        write_signed_byte(data, signature.numerator);
    }
    if (flags & 0x02) == 0x02 {
        write_signed_byte(
            data,
            signature.denominator.value.to_i8_gp("denominator value")?,
        );
    }
    if (flags & 0x08) == 0x08 {
        // GP5 counts the repeats including the first pass.
        let close = if version.0 < 5 {
            header.repeat_close
        } else {
            header.repeat_close.saturating_add(1)
        };
        write_signed_byte(data, close);
    }
    if (flags & 0x10) == 0x10 {
        if version.0 == 5 {
            write_byte(data, header.repeat_alternative);
        } else {
            write_byte(data, alternative_number(header.repeat_alternative));
        }
    }
    if (flags & 0x20) == 0x20
        && let Some(marker) = &header.marker
    {
        write_int_size_string(data, &marker.title);
        write_color(data, marker.color);
    }
    if version.0 >= 4 && (flags & 0x40) == 0x40 {
        write_signed_byte(data, header.key_signature.key);
        write_signed_byte(data, i8::from(header.key_signature.is_minor));
    }
    if version.0 >= 5 {
        if (flags & 0x03) != 0 {
            for &beam in &signature.beams {
                write_byte(data, beam);
            }
        }
        if (flags & 0x10) == 0 {
            write_placeholder_default(data, 1);
        }
        write_byte(data, from_triplet_feel(&header.triplet_feel));
    }
    Ok(())
}

/// GP3/4 store an ending as the number of the last of the first run of set bits (the
/// bitmask GP5 stores, turned back into "1st", "2nd", ...).
fn alternative_number(endings: u8) -> u8 {
    let bits = 8_u8.saturating_sub(u8::try_from(endings.leading_zeros()).unwrap_or(8));
    let mut first_one = false;
    let mut out: u8 = 0;
    for i in 0..=bits {
        out = i;
        if endings & 1_u8.checked_shl(u32::from(i)).unwrap_or(0) > 0 {
            first_one = true;
        } else if first_one {
            break;
        }
    }
    out
}

/// Writes the clipboard header, if the song is a clipboard copy.
pub(super) fn clipboard(
    data: &mut Vec<u8>,
    clipboard: Option<&Clipboard>,
    version: &(u8, u8, u8),
) -> GpResult<()> {
    if let Some(c) = clipboard {
        write_i32(data, c.start_measure);
        write_i32(data, c.stop_measure);
        write_i32(data, c.start_track);
        write_i32(data, c.stop_track);
        if version.0 == 5 {
            write_i32(data, c.start_beat);
            write_i32(data, c.stop_beat);
            write_i32(data, i32::from(c.sub_bar_copy));
        }
    }
    Ok(())
}

/// The order GP5 stores direction signs in.
const DIRECTIONS: [DirectionSign; 19] = [
    DirectionSign::Coda,
    DirectionSign::DoubleCoda,
    DirectionSign::Segno,
    DirectionSign::SegnoSegno,
    DirectionSign::Fine,
    DirectionSign::DaCapo,
    DirectionSign::DaCapoAlCoda,
    DirectionSign::DaCapoAlDoubleCoda,
    DirectionSign::DaCapoAlFine,
    DirectionSign::DaSegno,
    DirectionSign::DaSegnoAlCoda,
    DirectionSign::DaSegnoAlDoubleCoda,
    DirectionSign::DaSegnoAlFine,
    DirectionSign::DaSegnoSegno,
    DirectionSign::DaSegnoSegnoAlCoda,
    DirectionSign::DaSegnoSegnoAlDoubleCoda,
    DirectionSign::DaSegnoSegnoAlFine,
    DirectionSign::DaCoda,
    DirectionSign::DaDoubleCoda,
];

/// Writes where each direction sign is (its measure number, -1 if it is not used).
pub(super) fn directions(data: &mut Vec<u8>, headers: &[MeasureHeader]) -> GpResult<()> {
    let mut map: HashMap<DirectionSign, i16> = HashMap::with_capacity(DIRECTIONS.len());
    for (i, header) in headers.iter().enumerate() {
        if let Some(d) = &header.direction {
            map.insert(
                d.clone(),
                i.saturating_add(1).to_i16_gp("direction measure index")?,
            );
        }
    }
    for d in &DIRECTIONS {
        write_i16(data, map.get(d).copied().unwrap_or(-1));
    }
    Ok(())
}
