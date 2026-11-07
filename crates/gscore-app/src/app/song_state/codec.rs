//! The transport of GuitarScore's state block in a song's notes: compact JSON, deflated,
//! base64-encoded and split into short ASCII lines.
//!
//! One line is
//!
//! ```text
//! ::GS1:<part>/<count>:<crc32>:<base64 chunk>
//! ```
//!
//! `part` and `count` are 1-based, `crc32` (8 lowercase hex digits) covers the whole
//! deflated payload, and every line is at most [`MAX_LINE_CHARS`] characters. Base64 is
//! plain ASCII on purpose: GP3-5 store notes in a single-byte code page, so a tone name
//! with an umlaut would be mangled in another program if the JSON were stored as-is.
//!
//! Reading is all-or-nothing: a missing or doubled part, a broken line, a wrong checksum
//! or a version from the future drops the whole block (never half a state). The caller
//! must also drop these lines from the user's notes; they are ours even when they do not
//! read.

/// The longest line we write. A GP3-5 notice line holds at most 255 bytes; staying well
/// below that leaves room for whatever surrounds a note in other programs.
const MAX_LINE_CHARS: usize = 200;
/// Most lines one block may have. Bounds the work a hostile file can ask for.
const MAX_PARTS: usize = 4096;
/// Most bytes the payload may have compressed and inflated. Our state is far smaller;
/// anything beyond this is a broken or hostile file.
const MAX_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;
/// The prefix of every line; the version (`1`) follows `::GS`.
const MARKER: &str = "::GS";
/// The version of the line form written now.
const VERSION: &str = "1";
/// The deflate level: a small state, compressed quickly and well.
const DEFLATE_LEVEL: u8 = 6;

/// Whether `note` belongs to our block (any version, readable or not).
pub(crate) fn is_ours(note: &str) -> bool {
    note.trim_start().starts_with(MARKER)
}

/// The lines of `json`, deflated, checksummed, base64-encoded and split to at most
/// [`MAX_LINE_CHARS`] characters each. `None` when the state is too large to store.
pub(crate) fn encode(json: &[u8]) -> Option<Vec<String>> {
    let payload = miniz_oxide::deflate::compress_to_vec_zlib(json, DEFLATE_LEVEL);
    if payload.is_empty() || payload.len() > MAX_PAYLOAD_BYTES {
        return None;
    }
    let crc = crc32fast::hash(&payload);
    let base64 = base64_encode(&payload);
    let chunk_len = chunk_length(base64.len())?;
    let count = base64.len().div_ceil(chunk_len);
    if count == 0 || count > MAX_PARTS {
        return None;
    }
    let mut lines = Vec::with_capacity(count);
    for (index, chunk) in base64.as_bytes().chunks(chunk_len).enumerate() {
        let part = index.checked_add(1)?;
        let text = std::str::from_utf8(chunk).ok()?;
        lines.push(format!("{MARKER}{VERSION}:{part}/{count}:{crc:08x}:{text}"));
    }
    Some(lines)
}

/// The JSON payload of a block made of `lines`, or `None` unless every line is present
/// exactly once, the checksum matches and the payload is within limits.
pub(crate) fn decode<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<Vec<u8>> {
    let mut parts: Vec<Option<&str>> = Vec::new();
    let mut count = None;
    let mut crc = None;
    for line in lines {
        let rest = line.trim().strip_prefix(MARKER)?;
        let (version, rest) = rest.split_once(':')?;
        if version != VERSION {
            return None;
        }
        let (part_count, rest) = rest.split_once(':')?;
        let (part, part_count) = part_count.split_once('/')?;
        let (checksum, chunk) = rest.split_once(':')?;
        let part = parse_number(part)?;
        let line_count = parse_number(part_count)?;
        let checksum = u32::from_str_radix(checksum, 16).ok()?;
        if line_count == 0 || line_count > MAX_PARTS || part == 0 || part > line_count {
            return None;
        }
        if chunk.len() > MAX_LINE_CHARS || !chunk.is_ascii() {
            return None;
        }
        if count.is_some_and(|existing| existing != line_count)
            || crc.is_some_and(|existing| existing != checksum)
        {
            return None;
        }
        count = Some(line_count);
        crc = Some(checksum);
        if parts.is_empty() {
            parts.resize(line_count, None);
        }
        let slot = parts.get_mut(part.checked_sub(1)?)?;
        if slot.replace(chunk).is_some() {
            return None;
        }
    }
    let count = count?;
    if parts.len() != count || parts.iter().any(Option::is_none) {
        return None;
    }
    let encoded: String = parts.into_iter().flatten().collect();
    let payload = base64_decode(&encoded)?;
    if payload.len() > MAX_PAYLOAD_BYTES || crc32fast::hash(&payload) != crc? {
        return None;
    }
    let json = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&payload, MAX_PAYLOAD_BYTES)
        .ok()?;
    (!json.is_empty()).then_some(json)
}

/// How many base64 characters fit on a line when the block has `count` parts.
fn chunk_length(base64_len: usize) -> Option<usize> {
    // The number of parts depends on the chunk size and vice versa; a couple of rounds
    // settle it (only the number of digits of `count` changes the prefix).
    let mut count = 1_usize;
    for _ in 0..4 {
        // `::GS1:<part>/<count>:<crc>:`: the marker, the version, three `:`/`/`
        // separators, two numbers and eight hex digits.
        let digits = decimal_digits(count);
        let prefix = MARKER
            .len()
            .checked_add(VERSION.len())?
            .checked_add(12)?
            .checked_add(digits)?
            .checked_add(digits)?;
        let chunk = MAX_LINE_CHARS.checked_sub(prefix)?;
        if chunk == 0 {
            return None;
        }
        let needed = base64_len.div_ceil(chunk);
        if needed == count {
            return Some(chunk);
        }
        count = needed.max(1);
        if count > MAX_PARTS {
            return None;
        }
    }
    // The loop above converges in one or two rounds for any state we can write.
    None
}

/// The number of decimal digits of `value`.
fn decimal_digits(mut value: usize) -> usize {
    let mut digits: usize = 1;
    while value >= 10 {
        value /= 10;
        digits = digits.saturating_add(1);
    }
    digits
}

/// A plain decimal number without sign, whitespace or leading `+`.
fn parse_number(text: &str) -> Option<usize> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

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
fn base64_encode(data: &[u8]) -> String {
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
fn base64_decode(text: &str) -> Option<Vec<u8>> {
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
            let text = base64_encode(&data);
            assert!(text.is_ascii(), "{text}");
            assert_eq!(text.len() % 4, 0, "{text}");
            assert_eq!(base64_decode(&text), Some(data), "length {length}");
        }
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
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
            assert!(base64_decode(text).is_none(), "{text}");
        }
        // Padding only at the end, and only as much as the data needs.
        assert_eq!(base64_decode("Zg=="), Some(b"f".to_vec()));
        assert_eq!(base64_decode("Zm8="), Some(b"fo".to_vec()));
    }

    #[test]
    fn a_payload_round_trips_from_many_lines_to_one() {
        // Small enough for one line.
        let small = br#"{"tracks":[]}"#;
        let lines = encode(small).expect("fits");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("::GS1:1/1:"));
        assert_eq!(
            decode(lines.iter().map(String::as_str)),
            Some(small.to_vec())
        );

        // A long, poorly compressible payload takes several lines, and every line fits.
        let long: Vec<u8> = (0..4000_u32)
            .map(|index| b'a' + (index.wrapping_mul(2654435761) % 26) as u8)
            .collect();
        let lines = encode(&long).expect("fits");
        assert!(lines.len() > 1, "{} lines", lines.len());
        for line in &lines {
            assert!(line.len() <= MAX_LINE_CHARS, "{} chars: {line}", line.len());
            assert!(line.is_ascii());
        }
        assert_eq!(decode(lines.iter().map(String::as_str)), Some(long));
    }

    #[test]
    fn a_missing_doubled_edited_or_future_line_drops_the_whole_block() {
        let payload = b"{\"tracks\":[{\"track\":0,\"comp_send\":96}]}";
        let lines = encode(payload).expect("fits");
        fn text(lines: &[String]) -> Vec<&str> {
            lines.iter().map(String::as_str).collect()
        }
        assert!(decode(text(&lines)).is_some());

        if let Some(first) = lines.first() {
            // Missing: a one-line block without its only line is no block at all.
            assert!(decode(std::iter::empty()).is_none());
            // Doubled.
            let doubled: Vec<String> = lines
                .iter()
                .chain(std::iter::once(first))
                .cloned()
                .collect();
            assert!(decode(text(&doubled)).is_none());
            // Edited chunk: the checksum catches it.
            let mut edited = lines.clone();
            if let Some(line) = edited.first_mut() {
                let last = line.pop().unwrap_or('A');
                line.push(if last == 'A' { 'B' } else { 'A' });
            }
            assert!(decode(text(&edited)).is_none());
            // A future line form is not ours to read.
            let future: Vec<String> = lines
                .iter()
                .map(|line| line.replace("::GS1:", "::GS2:"))
                .collect();
            assert!(decode(text(&future)).is_none());
        }
    }

    #[test]
    fn a_truncated_block_is_not_half_applied() {
        let long: Vec<u8> = (0..2000_u32).map(|index| (index % 251) as u8).collect();
        let lines = encode(&long).expect("fits");
        assert!(lines.len() >= 2);
        for cut in 1..lines.len() {
            let partial: Vec<&str> = lines.iter().take(cut).map(String::as_str).collect();
            assert!(decode(partial).is_none(), "cut at {cut}");
        }
    }
}
