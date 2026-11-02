//! GP6 (`.gpx`, a BCFZ-compressed BCFS container) and GP7+ (`.gp`, a zip) files: reading
//! here, writing in [`write`]. Every offset and length comes from the file, so all of them
//! are checked, and unpacked sizes are capped at [`MAX_UNPACKED_BYTES`].

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

mod write;

use std::io::Cursor;

use quick_xml::de::from_str;
use zip::ZipArchive;

use crate::io::error::GpResult;
use crate::io::gpif::Gpif;

#[cfg(test)]
use write::{MAX_BCFS_BLOCKS, pack_bcfs};
pub(crate) use write::{write_gp_bytes, write_gpx_bytes};

/// GuitarScore addition: the most bytes any container may unpack to (score XML, embedded
/// audio, decompressed GPX). Real scores are far smaller; the cap stops "zip bombs" and
/// forged length fields from exhausting memory, which aborts the process (not catchable).
pub(crate) const MAX_UNPACKED_BYTES: usize = 128 * 1024 * 1024;

const BCFZ_MAGIC: &[u8; 4] = b"BCFZ";
const BCFS_MAGIC: &[u8; 4] = b"BCFS";
const SECTOR_SIZE: usize = 0x1000;
/// Where a BCFS directory entry keeps the file size and the block index table.
const ENTRY_FILE_SIZE: usize = 0x8C;
const ENTRY_BLOCKS: usize = 0x94;
/// The longest file name in a directory entry.
const ENTRY_NAME_LENGTH: usize = 127;

/// GuitarScore addition: reads at most [`MAX_UNPACKED_BYTES`], failing beyond that.
fn read_limited(reader: impl std::io::Read, what: &str) -> Result<Vec<u8>, String> {
    read_limited_to(reader, what, MAX_UNPACKED_BYTES)
}

fn read_limited_to(
    reader: impl std::io::Read,
    what: &str,
    limit: usize,
) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let mut buf = Vec::new();
    let take = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    reader
        .take(take)
        .read_to_end(&mut buf)
        .map_err(|e| format!("Read error in {what}: {e}"))?;
    if buf.len() > limit {
        return Err(format!("{what} unpacks to more than {limit} bytes"));
    }
    Ok(buf)
}

/// Reads a .gp (GP7+) file, returning the parsed GPIF plus any embedded
/// backing-track audio found under `Content/Assets/` (e.g. `*.mp3`).
pub(crate) fn read_gp_with_audio(data: &[u8]) -> GpResult<(Gpif, Option<Vec<u8>>)> {
    let mut zip = ZipArchive::new(Cursor::new(data)).map_err(|e| format!("Zip error: {e}"))?;
    let file = zip
        .by_name("Content/score.gpif")
        .map_err(|e| format!("Could not find score.gpif: {e}"))?;
    // GuitarScore fix: bounded read (was `read_to_string` without a limit).
    let contents = String::from_utf8(read_limited(file, "score.gpif")?)
        .map_err(|e| format!("UTF-8 error in score.gpif: {e}"))?;
    let gpif: Gpif = from_str(&contents).map_err(|e| format!("XML Parse error: {e}"))?;

    // Embedded backing track: first audio asset under Content/Assets/.
    // (zip entries can only be read once, so re-open the archive.)
    let mut audio: Option<Vec<u8>> = None;
    if let Ok(mut zip) = ZipArchive::new(Cursor::new(data)) {
        for i in 0..zip.len() {
            let Ok(entry) = zip.by_index(i) else {
                continue;
            };
            let Ok(name) = entry.name().map(|name| name.into_owned()) else {
                continue;
            };
            if name.starts_with("Content/Assets/") && name.ends_with(".mp3") {
                // GuitarScore fix: bounded read (was `read_to_end` without a limit).
                if let Ok(buf) = read_limited(entry, "embedded audio") {
                    audio = Some(buf);
                }
                break;
            }
        }
    }
    Ok((gpif, audio))
}

/// Bit-level reader for BCFZ decompression. Reads bits MSB-first within each byte; past
/// the end every bit is 0.
struct BitStream<'a> {
    data: &'a [u8],
    bit_position: usize,
}

impl<'a> BitStream<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitStream {
            data,
            bit_position: 0,
        }
    }

    fn is_eof(&self) -> bool {
        self.bit_position / 8 >= self.data.len()
    }

    /// Read a single bit (MSB-first within the current byte).
    fn read_bit(&mut self) -> u8 {
        let Some(byte) = self.data.get(self.bit_position / 8) else {
            return 0;
        };
        let shift = 7_usize.saturating_sub(self.bit_position % 8);
        self.bit_position = self.bit_position.saturating_add(1);
        (byte >> shift) & 1
    }

    /// Read `count` bits, accumulated big-endian (MSB first).
    fn read_bits(&mut self, count: usize) -> u32 {
        (0..count).fold(0_u32, |result, _| {
            (result << 1) | u32::from(self.read_bit())
        })
    }

    /// Read `count` bits, accumulated little-endian (LSB first / "reversed").
    fn read_bits_reversed(&mut self, count: usize) -> u32 {
        (0..count).fold(0_u32, |result, i| {
            result
                | u32::from(self.read_bit())
                    .checked_shl(u32::try_from(i).unwrap_or(32))
                    .unwrap_or(0)
        })
    }
}

/// Decompress a BCFZ-compressed buffer into raw BCFS data.
fn decompress_bcfz(data: &[u8]) -> Result<Vec<u8>, String> {
    let (Some(magic), Some(length), Some(rest)) = (data.get(0..4), data.get(4..8), data.get(8..))
    else {
        return Err("BCFZ data too short".to_owned());
    };
    if magic != BCFZ_MAGIC {
        return Err(format!("Expected BCFZ magic, got {magic:?}"));
    }
    let raw_len = i32::from_le_bytes(<[u8; 4]>::try_from(length).map_err(|e| e.to_string())?);
    let expected_len = usize::try_from(raw_len)
        .map_err(|_| format!("BCFZ: negative expected length {raw_len}"))?;
    // GuitarScore fix: the length comes from the file; refuse absurd values instead of
    // reserving (or producing, via back-references) up to 2 GB.
    if expected_len > MAX_UNPACKED_BYTES {
        return Err(format!(
            "BCFZ: declared size {expected_len} exceeds the limit of {MAX_UNPACKED_BYTES} bytes"
        ));
    }
    let mut output = Vec::with_capacity(expected_len.min(data.len().saturating_mul(4)));
    let mut bits = BitStream::new(rest);
    // A width and a size are at most 15 bits; these conversions cannot fail on any target
    // GuitarScore runs on.
    let size_of = |bits: u32| usize::try_from(bits).unwrap_or(usize::MAX);

    while output.len() < expected_len && !bits.is_eof() {
        if bits.read_bit() == 1 {
            // Back-reference (LZ77-style)
            let word_size = size_of(bits.read_bits(4));
            let offset = size_of(bits.read_bits_reversed(word_size));
            let size = size_of(bits.read_bits_reversed(word_size));
            let Some(source_start) = output.len().checked_sub(offset).filter(|_| offset > 0) else {
                return Err(format!(
                    "BCFZ: invalid back-reference offset {offset} (output len {})",
                    output.len()
                ));
            };
            // LZ77 overlapping copy: when size > offset the source overlaps the
            // destination, so copy byte by byte, repeating the last `offset` bytes.
            for i in 0..size {
                let byte = output
                    .get(source_start.saturating_add(i.checked_rem(offset).unwrap_or(0)))
                    .copied()
                    .unwrap_or_default();
                output.push(byte);
            }
        } else {
            // Literal bytes
            let size = size_of(bits.read_bits_reversed(2));
            for _ in 0..size {
                let [byte, ..] = bits.read_bits(8).to_le_bytes();
                output.push(byte);
            }
        }
    }

    output.truncate(expected_len);
    Ok(output)
}

/// A file extracted from a BCFS virtual filesystem.
struct BcfsFile {
    name: String,
    data: Vec<u8>,
}

/// Read the integer at the given offset (little-endian i32).
///
/// GuitarScore fix: returns 0 past the end of `data` (was an index panic on truncated
/// files). Callers read 0 as "no entry" or "end of block list".
fn read_le_i32(data: &[u8], offset: usize) -> i32 {
    offset
        .checked_add(4)
        .and_then(|end| data.get(offset..end))
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map_or(0, i32::from_le_bytes)
}

/// Parse the BCFS virtual filesystem and extract all files.
///
/// The BCFS format starts with a 4-byte magic ("BCFS"), followed by sector-based data.
/// The Java reference implementation (TuxGuitar) strips the 4-byte magic and then treats
/// the remaining data as a virtual disk with 0x1000-byte sectors.
fn parse_bcfs(data: &[u8]) -> Result<Vec<BcfsFile>, String> {
    let (Some(magic), Some(disk)) = (data.get(0..4), data.get(4..)) else {
        return Err("BCFS data too short".to_owned());
    };
    if magic != BCFS_MAGIC {
        return Err(format!("Expected BCFS magic, got {magic:?}"));
    }
    // All sector offsets are relative to the data after the magic. Sector 0 is the header.
    let mut files = Vec::new();
    for sector in disk.chunks(SECTOR_SIZE).skip(1) {
        if sector.len() < 4 || read_le_i32(sector, 0) != 2 {
            continue;
        }
        // A file directory entry needs at least 0x98 bytes.
        if sector.len() < ENTRY_BLOCKS.saturating_add(4) {
            continue;
        }
        let name_bytes = sector
            .get(4..4_usize.saturating_add(ENTRY_NAME_LENGTH))
            .unwrap_or_default();
        let name_len = name_bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(name_bytes.len());
        let name =
            String::from_utf8_lossy(name_bytes.get(..name_len).unwrap_or_default()).into_owned();

        // GuitarScore fix: a negative size used to become a huge `usize`, and the capacity
        // was reserved before checking; file data can never exceed the disk.
        let file_size = usize::try_from(read_le_i32(sector, ENTRY_FILE_SIZE))
            .unwrap_or(0)
            .min(disk.len());

        // Block index table at +0x94: sector numbers, terminated by 0. GuitarScore fix: a
        // negative or huge sector number is the end of the list (it overflowed before).
        let mut file_data = Vec::with_capacity(file_size);
        for offset in (ENTRY_BLOCKS..sector.len().saturating_sub(3)).step_by(4) {
            let block = read_le_i32(sector, offset);
            let Some(start) = usize::try_from(block)
                .ok()
                .filter(|&b| b != 0)
                .and_then(|b| b.checked_mul(SECTOR_SIZE))
            else {
                break;
            };
            let end = start.saturating_add(SECTOR_SIZE).min(disk.len());
            if let Some(block) = disk.get(start..end) {
                file_data.extend_from_slice(block);
            }
        }
        file_data.truncate(file_size);
        if !name.is_empty() {
            files.push(BcfsFile {
                name,
                data: file_data,
            });
        }
    }
    Ok(files)
}

/// Reads a .gpx (GP6) file which is a BCFZ/BCFS container holding 'score.gpif'.
pub(crate) fn read_gpx(data: &[u8]) -> GpResult<Gpif> {
    // GuitarScore fix: also accept uncompressed BCFS containers.
    let decompressed;
    let container = if data.starts_with(BCFS_MAGIC) {
        data
    } else {
        decompressed = decompress_bcfz(data)?;
        &decompressed
    };
    let files = parse_bcfs(container)?;
    let score_file = files
        .iter()
        .find(|f| f.name == "score.gpif")
        .ok_or_else(|| {
            let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
            format!("score.gpif not found in GPX archive. Files found: {names:?}")
        })?;
    let xml_str = std::str::from_utf8(&score_file.data)
        .map_err(|e| format!("UTF-8 error in score.gpif: {e}"))?;
    let gpif: Gpif =
        from_str(xml_str).map_err(|e| format!("XML parse error in score.gpif: {e}"))?;
    Ok(gpif)
}

// GuitarScore regression tests for the allocation limits (see docs/guitarpro.md).
#[cfg(test)]
mod limit_tests {
    use super::*;

    #[test]
    fn bcfz_with_absurd_declared_size_is_rejected() {
        let mut data = b"BCFZ".to_vec();
        data.extend_from_slice(&i32::MAX.to_le_bytes());
        data.extend_from_slice(&[0xFF; 16]);
        let err = decompress_bcfz(&data).unwrap_err();
        assert!(err.contains("exceeds the limit"), "{err}");
    }

    #[test]
    fn bcfs_with_negative_or_huge_block_numbers_ends_the_block_list() {
        // A directory entry whose first block number is -1, then i32::MAX: no overflow.
        for block in [-1_i32, i32::MAX] {
            let mut disk = vec![0u8; 4 + 2 * SECTOR_SIZE];
            disk[..4].copy_from_slice(BCFS_MAGIC);
            let entry = 4 + SECTOR_SIZE;
            disk[entry..entry + 4].copy_from_slice(&2i32.to_le_bytes());
            disk[entry + 4..entry + 14].copy_from_slice(b"score.gpif");
            disk[entry + 0x8C..entry + 0x90].copy_from_slice(&100i32.to_le_bytes());
            disk[entry + 0x94..entry + 0x98].copy_from_slice(&block.to_le_bytes());
            let files = parse_bcfs(&disk).expect("parses");
            assert!(files[0].data.is_empty(), "{block}");
        }
    }

    #[test]
    fn bcfs_with_negative_file_size_does_not_allocate_wildly() {
        // Magic, then sector 0, then a directory entry with file size -1.
        let mut disk = vec![0u8; 4 + 2 * SECTOR_SIZE];
        disk[..4].copy_from_slice(BCFS_MAGIC);
        let entry = 4 + SECTOR_SIZE;
        disk[entry..entry + 4].copy_from_slice(&2i32.to_le_bytes());
        disk[entry + 4..entry + 14].copy_from_slice(b"score.gpif");
        disk[entry + 0x8C..entry + 0x90].copy_from_slice(&(-1i32).to_le_bytes());
        let files = parse_bcfs(&disk).expect("parses");
        assert_eq!(files.len(), 1);
        assert!(files[0].data.is_empty());
    }

    #[test]
    fn limited_reads_stop_at_the_limit() {
        let data = [7u8; 100];
        assert_eq!(read_limited_to(&data[..], "x", 100).unwrap().len(), 100);
        let err = read_limited_to(&data[..], "x", 99).unwrap_err();
        assert!(err.contains("more than 99 bytes"), "{err}");
    }

    #[test]
    fn files_too_large_for_one_directory_sector_are_refused() {
        let fits = vec![b' '; MAX_BCFS_BLOCKS * SECTOR_SIZE];
        let packed = pack_bcfs("score.gpif", &fits).unwrap();
        let files = parse_bcfs(&packed[..]).unwrap();
        assert_eq!(files[0].data.len(), fits.len());

        let too_large = vec![b' '; MAX_BCFS_BLOCKS * SECTOR_SIZE + 1];
        assert!(pack_bcfs("score.gpif", &too_large).is_err());
    }

    #[test]
    fn uncompressed_bcfs_is_accepted() {
        // A BCFS container without score.gpif: must fail with "not found", not "magic".
        let disk = pack_bcfs("other.xml", b"<x/>").unwrap();
        let err = read_gpx(&disk).unwrap_err().to_string();
        assert!(err.contains("score.gpif not found"), "{err}");
    }
}
