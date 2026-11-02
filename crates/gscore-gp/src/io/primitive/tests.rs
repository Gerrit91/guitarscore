//! Tests of the primitive reading and writing.

use super::*;

#[test]
fn test_read_byte_size_string() {
    let data: Vec<u8> = vec![
        0x18, 0x46, 0x49, 0x43, 0x48, 0x49, 0x45, 0x52, 0x20, 0x47, 0x55, 0x49, 0x54, 0x41, 0x52,
        0x20, 0x50, 0x52, 0x4f, 0x20, 0x76, 0x33, 0x2e, 0x30, 0x30,
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
        0x18, 0x46, 0x49, 0x43, 0x48, 0x49, 0x45, 0x52, 0x20, 0x47, 0x55, 0x49, 0x54, 0x41, 0x52,
        0x20, 0x50, 0x52, 0x4f, 0x20, 0x76, 0x33, 0x2e, 0x30, 0x30,
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
