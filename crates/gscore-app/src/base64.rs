//! Standard Base64 with padding, written out here (a few lines) rather than taken from a
//! crate: the song's own state in its notes and the tone files' impulse responses use it.

/// The standard base64 alphabet with padding.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The base64 character for a 6-bit value.
fn digit(value: u32) -> Option<char> {
    ALPHABET
        .get(value as usize & 63)
        .map(|byte| char::from(*byte))
}

/// The 6-bit value of a base64 character.
fn value(byte: u8) -> Option<u32> {
    let value = match byte {
        b'A'..=b'Z' => byte.checked_sub(b'A')?,
        b'a'..=b'z' => byte.checked_sub(b'a')?.checked_add(26)?,
        b'0'..=b'9' => byte.checked_sub(b'0')?.checked_add(52)?,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    };
    Some(u32::from(value))
}

/// `data` in standard padded base64.
pub(crate) fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3).saturating_mul(4));
    for chunk in data.chunks(3) {
        let first = u32::from(chunk.first().copied().unwrap_or(0));
        let second = u32::from(chunk.get(1).copied().unwrap_or(0));
        let third = u32::from(chunk.get(2).copied().unwrap_or(0));
        let bits = first
            .wrapping_shl(16)
            .wrapping_add(second.wrapping_shl(8))
            .wrapping_add(third);
        for index in 0..4_usize {
            let shift = 18_u32.saturating_sub(u32::try_from(index.saturating_mul(6)).unwrap_or(18));
            let six = (bits >> shift) & 63;
            let character = if index > chunk.len() {
                '='
            } else {
                digit(six).unwrap_or('=')
            };
            out.push(character);
        }
    }
    out
}

/// The bytes of standard padded base64 `text`, or `None` if it is not valid base64.
pub(crate) fn decode(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return Some(Vec::new());
    }
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let groups = bytes.len() / 4;
    let mut out = Vec::with_capacity(groups.saturating_mul(3));
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let last = index.checked_add(1) == Some(groups);
        let first = value(chunk.first().copied()?)?;
        let second = value(chunk.get(1).copied()?)?;
        let third_byte = chunk.get(2).copied()?;
        let fourth_byte = chunk.get(3).copied()?;
        let padding = u8::from(third_byte == b'=').checked_add(u8::from(fourth_byte == b'='))?;
        if padding > 2 || (padding > 0 && !last) {
            return None;
        }
        if (third_byte == b'=' && fourth_byte != b'=') || third_byte == b'=' && padding != 2 {
            return None;
        }
        let third = if third_byte == b'=' {
            0
        } else {
            value(third_byte)?
        };
        let fourth = if fourth_byte == b'=' {
            0
        } else {
            value(fourth_byte)?
        };
        let bits = first
            .wrapping_shl(18)
            .wrapping_add(second.wrapping_shl(12))
            .wrapping_add(third.wrapping_shl(6))
            .wrapping_add(fourth);
        let keep = 3_usize.saturating_sub(usize::from(padding));
        for byte_index in 0..keep {
            let shift = 16_u32.saturating_sub(u32::try_from(byte_index).ok()?.saturating_mul(8));
            out.push(u8::try_from((bits >> shift) & 0xFF).ok()?);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    #[test]
    fn base64_round_trips_every_length() {
        for length in 0..100 {
            let data: Vec<u8> = (0..length).map(|index| (index * 37) as u8).collect();
            let text = encode(&data);
            assert!(text.is_ascii(), "{text}");
            assert_eq!(text.len() % 4, 0, "{text}");
            assert_eq!(decode(&text), Some(data), "length {length}");
        }
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
    }

    #[test]
    fn base64_rejects_broken_text() {
        for text in [
            "Zm9",
            "Zm9v=",
            "Zg=a",
            "Z===",
            "****",
            "Zm9v Zm9v",
            "Zg==Zg==",
            "Z!==",
        ] {
            assert!(decode(text).is_none(), "{text}");
        }
        // Padding only at the end, and only as much as the data needs.
        assert_eq!(decode("Zg=="), Some(b"f".to_vec()));
        assert_eq!(decode("Zm8="), Some(b"fo".to_vec()));
    }
}
