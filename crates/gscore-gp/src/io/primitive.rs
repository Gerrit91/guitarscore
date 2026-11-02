//! Reading and writing the binary formats' primitives: integers, strings, colors.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::error::{GpError, GpResult, ToPrimitiveGp};
use encoding_rs::*;

// Reading: every read takes its bytes through `take`, which checks the bounds, so no read
// can index past the data or overflow the position.

/// The next `count` bytes at `seek`, moving `seek` past them.
fn take<'a>(data: &'a [u8], seek: &mut usize, count: usize) -> GpResult<&'a [u8]> {
    let eof = GpError::UnexpectedEof {
        offset: *seek,
        needed: count,
    };
    let end = seek.checked_add(count).ok_or_else(|| eof_clone(&eof))?;
    let bytes = data.get(*seek..end).ok_or(eof)?;
    *seek = end;
    Ok(bytes)
}

/// The same end-of-data error again (`GpError` is not `Clone`: it holds an I/O error).
fn eof_clone(error: &GpError) -> GpError {
    match error {
        GpError::UnexpectedEof { offset, needed } => GpError::UnexpectedEof {
            offset: *offset,
            needed: *needed,
        },
        _ => GpError::FormatError(error.to_string()),
    }
}

/// The next `N` bytes as an array.
fn take_array<const N: usize>(data: &[u8], seek: &mut usize) -> GpResult<[u8; N]> {
    let bytes = take(data, seek, N)?;
    let mut array = [0_u8; N];
    array.copy_from_slice(bytes);
    Ok(array)
}

/// Skips `count` bytes without checking them (as Guitar Pro skips blank bytes); a later
/// read past the end fails.
pub(crate) fn skip(seek: &mut usize, count: usize) {
    *seek = seek.saturating_add(count);
}

/// `value` clamped to 0..=255.
pub(crate) fn clamp_u8(value: i32) -> u8 {
    u8::try_from(value.clamp(0, 255)).unwrap_or_default()
}

/// The low byte of `value` as a signed byte (what a truncating cast gives).
pub(crate) fn low_i8(value: i32) -> i8 {
    let [low, ..] = value.to_le_bytes();
    i8::from_le_bytes([low])
}

/// The low 16 bits of `value` as a signed integer (what a truncating cast gives).
pub(crate) fn low_i16(value: i32) -> i16 {
    let [a, b, ..] = value.to_le_bytes();
    i16::from_le_bytes([a, b])
}

/// `value` rounded and clamped to `i8`; NaN gives 0.
pub(crate) fn round_to_i8(value: f32) -> i8 {
    if value.is_nan() {
        return 0;
    }
    // Clamped to the range first, so the cast cannot truncate.
    #[allow(clippy::cast_possible_truncation)]
    let rounded = value.round().clamp(f32::from(i8::MIN), f32::from(i8::MAX)) as i8;
    rounded
}

/// The low byte of `value` (what a truncating cast gives).
pub(crate) fn low_u8(value: i32) -> u8 {
    let [low, ..] = value.to_le_bytes();
    low
}

/// A byte read as a signed byte (two's complement).
pub(crate) fn byte_as_i8(value: u8) -> i8 {
    i8::from_le_bytes([value])
}

/// A signed byte written as a byte (two's complement).
pub(crate) fn i8_as_byte(value: i8) -> u8 {
    let [byte] = value.to_le_bytes();
    byte
}

/// Reads a byte.
pub(crate) fn read_byte(data: &[u8], seek: &mut usize) -> GpResult<u8> {
    Ok(u8::from_le_bytes(take_array(data, seek)?))
}

/// Reads a signed byte.
pub(crate) fn read_signed_byte(data: &[u8], seek: &mut usize) -> GpResult<i8> {
    Ok(i8::from_le_bytes(take_array(data, seek)?))
}

/// Reads a boolean (any non-zero byte is true).
pub(crate) fn read_bool(data: &[u8], seek: &mut usize) -> GpResult<bool> {
    Ok(read_byte(data, seek)? != 0)
}

/// Reads a little-endian 16-bit integer.
pub(crate) fn read_short(data: &[u8], seek: &mut usize) -> GpResult<i16> {
    Ok(i16::from_le_bytes(take_array(data, seek)?))
}

/// Reads a little-endian 32-bit integer.
pub(crate) fn read_int(data: &[u8], seek: &mut usize) -> GpResult<i32> {
    Ok(i32::from_le_bytes(take_array(data, seek)?))
}

/// Reads a little-endian 64-bit float.
pub(crate) fn read_double(data: &[u8], seek: &mut usize) -> GpResult<f64> {
    Ok(f64::from_le_bytes(take_array(data, seek)?))
}

/// Reads a string whose length is stored in a 32-bit integer before it.
pub(crate) fn read_int_size_string(data: &[u8], seek: &mut usize) -> GpResult<String> {
    let size = read_int(data, seek)?.to_usize_gp("string size")?;
    read_string(data, seek, size, None)
}

/// Reads a string stored as a 32-bit integer (its length plus one), a byte (its length)
/// and the characters. A length beyond the data gives an empty string.
pub(crate) fn read_int_byte_size_string(data: &[u8], seek: &mut usize) -> GpResult<String> {
    let val = read_int(data, seek)?;
    if val <= 0 {
        return Ok(String::new());
    }
    let s = val
        .checked_sub(1)
        .unwrap_or_default()
        .to_usize_gp("int_byte_size_string length")?;
    let fits = seek
        .checked_add(1)
        .and_then(|after| after.checked_add(s))
        .is_some_and(|end| end <= data.len());
    if !fits {
        return Ok(String::new());
    }
    read_byte_size_string(data, seek, s)
}

/// Reads a string whose length is stored in a byte before it, in a field of `size` bytes.
pub(crate) fn read_byte_size_string(
    data: &[u8],
    seek: &mut usize,
    size: usize,
) -> GpResult<String> {
    let length = usize::from(read_byte(data, seek)?);
    read_string(data, seek, size, Some(length))
}

/// Reads `length` characters (default: `size`) from a field of `size` bytes, as Windows-1252
/// or, if that fails, UTF-8. The position moves past the whole field.
fn read_string(
    data: &[u8],
    seek: &mut usize,
    size: usize,
    length: Option<usize>,
) -> GpResult<String> {
    let length = length.unwrap_or(size);
    let start = *seek;
    let bytes = take(data, &mut start.clone(), length)?;
    let (cow, _encoding_used, had_errors) = WINDOWS_1252.decode(bytes);
    let text = if had_errors {
        std::str::from_utf8(bytes)
            .map_err(|_| GpError::StringDecode { offset: start })?
            .to_owned()
    } else {
        cow.into_owned()
    };
    *seek = start.saturating_add(size);
    Ok(text)
}

/// The version strings: version number, whether it is a clipboard file, and the text.
pub(crate) const VERSIONS: [((u8, u8, u8), bool, &str); 10] = [
    ((3, 0, 0), false, "FICHIER GUITAR PRO v3.00"),
    ((4, 0, 0), false, "FICHIER GUITAR PRO v4.00"),
    ((4, 0, 6), false, "FICHIER GUITAR PRO v4.06"),
    ((4, 0, 6), true, "CLIPBOARD GUITAR PRO 4.0 [c6]"),
    ((5, 0, 0), false, "FICHIER GUITAR PRO v5.00"),
    ((5, 1, 0), false, "FICHIER GUITAR PRO v5.10"),
    ((5, 2, 0), false, "FICHIER GUITAR PRO v5.10"), // sic
    ((5, 0, 0), true, "CLIPBOARD GP 5.0"),
    ((5, 1, 0), true, "CLIPBOARD GP 5.1"),
    ((5, 2, 0), true, "CLIPBOARD GP 5.2"),
];

/// Read the file version. It is on the first 31 bytes (1st byte is the real length, the following 30 bytes contain the version string) of the file.
/// * `data` - array of bytes
/// * `seek` - cursor that will be incremented
/// * returns version
pub(crate) fn read_version_string(data: &[u8], seek: &mut usize) -> GpResult<crate::io::Version> {
    let mut v = crate::io::Version {
        data: read_byte_size_string(data, seek, 30)?,
        number: (5, 2, 0),
        clipboard: false,
    };
    //get the version
    for x in VERSIONS {
        if v.data == x.2 {
            v.number = x.0;
            v.clipboard = x.1;
            break;
        }
    }
    Ok(v)
}

/// Read a color. Colors are used by `Marker` and `Track`. They consist of 3 consecutive bytes and one blank byte.
pub(crate) fn read_color(data: &[u8], seek: &mut usize) -> GpResult<i32> {
    let r = read_byte(data, seek)?;
    let g = read_byte(data, seek)?;
    let b = read_byte(data, seek)?;
    // The blank byte is skipped without a check, as Guitar Pro reads it.
    *seek = seek.saturating_add(1);
    Ok(i32::from_be_bytes([0, r, g, b]))
}

// Writing.

fn write_placeholder(data: &mut Vec<u8>, count: usize, byte: u8) {
    data.extend(std::iter::repeat_n(byte, count));
}

pub(crate) fn write_placeholder_default(data: &mut Vec<u8>, count: usize) {
    write_placeholder(data, count, 0x00);
}
pub(crate) fn write_byte(data: &mut Vec<u8>, value: u8) {
    data.push(value);
}
pub(crate) fn write_signed_byte(data: &mut Vec<u8>, value: i8) {
    data.extend(value.to_le_bytes());
}
pub(crate) fn write_bool(data: &mut Vec<u8>, value: bool) {
    data.push(u8::from(value));
}
pub(crate) fn write_i32(data: &mut Vec<u8>, value: i32) {
    data.extend(value.to_le_bytes());
}
pub(crate) fn write_i16(data: &mut Vec<u8>, value: i16) {
    data.extend(value.to_le_bytes());
}
pub(crate) fn write_f64(data: &mut Vec<u8>, value: f64) {
    data.extend(value.to_le_bytes());
}
pub(crate) fn write_color(data: &mut Vec<u8>, value: i32) {
    let [_, r, g, b] = value.to_be_bytes();
    write_byte(data, r);
    write_byte(data, g);
    write_byte(data, b);
    write_placeholder_default(data, 1);
}
pub(crate) fn encode_windows1252(value: &str) -> std::borrow::Cow<'_, [u8]> {
    let (encoded, _, _) = WINDOWS_1252.encode(value);
    encoded
}

/// Writes a string as a length byte and at most 255 bytes.
pub(crate) fn write_byte_size_string(data: &mut Vec<u8>, value: &str) {
    let encoded = encode_windows1252(value);
    let count = u8::try_from(encoded.len()).unwrap_or(u8::MAX);
    write_byte(data, count);
    data.extend(encoded.iter().take(usize::from(count)));
}

/// Writes a string as a length byte and a field of `size` bytes: cut to fit, padded with
/// zeros (the binary formats' fixed-size names).
pub(crate) fn write_fixed_string(data: &mut Vec<u8>, value: &str, size: u8) {
    let encoded = encode_windows1252(value);
    let count = u8::try_from(encoded.len()).unwrap_or(u8::MAX).min(size);
    write_byte(data, count);
    data.extend(encoded.iter().take(usize::from(count)));
    write_placeholder_default(data, usize::from(size.saturating_sub(count)));
}

/// Writes a string as a 32-bit length and the bytes (cut at `i32::MAX` bytes).
pub(crate) fn write_int_size_string(data: &mut Vec<u8>, value: &str) {
    let encoded = encode_windows1252(value);
    let count = i32::try_from(encoded.len()).unwrap_or(i32::MAX);
    write_i32(data, count);
    data.extend(
        encoded
            .iter()
            .take(usize::try_from(count).unwrap_or_default()),
    );
}

/// Writes a string as a 32-bit integer (length plus one), a length byte and at most 255
/// bytes.
pub(crate) fn write_int_byte_size_string(data: &mut Vec<u8>, value: &str) {
    let encoded = encode_windows1252(value);
    let count = u8::try_from(encoded.len()).unwrap_or(u8::MAX);
    write_i32(data, i32::from(count).saturating_add(1));
    write_byte(data, count);
    data.extend(encoded.iter().take(usize::from(count)));
}

/// Writes the version string of `version`, padded to 30 bytes.
pub(crate) fn write_version(data: &mut Vec<u8>, version: (u8, u8, u8)) {
    if let Some((_, _, text)) = VERSIONS.iter().find(|v| v.0 == version) {
        write_byte_size_string(data, text);
        write_placeholder_default(data, 30_usize.saturating_sub(text.len()));
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_read_byte_size_string() {
        let data: Vec<u8> = vec![
            0x18, 0x46, 0x49, 0x43, 0x48, 0x49, 0x45, 0x52, 0x20, 0x47, 0x55, 0x49, 0x54, 0x41,
            0x52, 0x20, 0x50, 0x52, 0x4f, 0x20, 0x76, 0x33, 0x2e, 0x30, 0x30,
        ];
        let mut seek = 0usize;
        assert_eq!(
            read_byte_size_string(&data, &mut seek, 30).unwrap(),
            "FICHIER GUITAR PRO v3.00"
        );
    }

    #[test]
    fn test_read_int_size_string() {
        let data: Vec<u8> = vec![
            0x08, 0x00, 0x00, 0x00, 0x25, 0x41, 0x52, 0x54, 0x49, 0x53, 0x54, 0x25,
        ];
        let mut seek = 0usize;
        assert_eq!(read_int_size_string(&data, &mut seek).unwrap(), "%ARTIST%");
    }

    #[test]
    fn test_read_int_byte_size_string() {
        let data: Vec<u8> = vec![
            0x09, 0x00, 0x00, 0x00, 0x08, 0x25, 0x41, 0x52, 0x54, 0x49, 0x53, 0x54, 0x25,
        ];
        let mut seek = 0usize;
        assert_eq!(
            read_int_byte_size_string(&data, &mut seek).unwrap(),
            "%ARTIST%"
        );
    }

    #[test]
    fn test_write_byte_size_string() {
        let mut out: Vec<u8> = Vec::with_capacity(32);
        write_byte_size_string(&mut out, "FICHIER GUITAR PRO v3.00");
        let expected_result: Vec<u8> = vec![
            0x18, 0x46, 0x49, 0x43, 0x48, 0x49, 0x45, 0x52, 0x20, 0x47, 0x55, 0x49, 0x54, 0x41,
            0x52, 0x20, 0x50, 0x52, 0x4f, 0x20, 0x76, 0x33, 0x2e, 0x30, 0x30,
        ];
        assert_eq!(out, expected_result);
    }
    #[test]
    fn test_write_int_size_string() {
        let mut out: Vec<u8> = Vec::with_capacity(16);
        write_int_size_string(&mut out, "%ARTIST%");
        // int_size_string = int(length), then string bytes (no byte length prefix)
        let expected_result: Vec<u8> = vec![
            0x08, 0x00, 0x00, 0x00, 0x25, 0x41, 0x52, 0x54, 0x49, 0x53, 0x54, 0x25,
        ];
        assert_eq!(out, expected_result);
    }
    #[test]
    fn test_write_int_byte_size_string() {
        let mut out: Vec<u8> = Vec::with_capacity(16);
        write_int_byte_size_string(&mut out, "%ARTIST%");
        let expected_result: Vec<u8> = vec![
            0x09, 0x00, 0x00, 0x00, 0x08, 0x25, 0x41, 0x52, 0x54, 0x49, 0x53, 0x54, 0x25,
        ];
        assert_eq!(out, expected_result);
    }
}
