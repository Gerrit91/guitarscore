//! Writing GP6 (`.gpx`) and GP7+ (`.gp`) containers.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use super::{
    BCFS_MAGIC, BCFZ_MAGIC, ENTRY_BLOCKS, ENTRY_FILE_SIZE, ENTRY_NAME_LENGTH, SECTOR_SIZE,
};
use crate::io::GpError;
use crate::io::error::GpResult;

/// Block indices that fit into one BCFS directory sector (after the entry's 0x94-byte
/// header).
pub(super) const MAX_BCFS_BLOCKS: usize = (SECTOR_SIZE - ENTRY_BLOCKS) / 4;

/// Bit-level writer (MSB-first within each byte, matching `BitStream` read order).
struct BitWriter {
    data: Vec<u8>,
    current: u8,
    /// Bits filled (0-7); bit 7 is written first.
    used: u8,
}

impl BitWriter {
    fn new() -> Self {
        BitWriter {
            data: Vec::new(),
            current: 0,
            used: 0,
        }
    }

    fn write_bit(&mut self, bit: bool) {
        let shift = 7_u8.saturating_sub(self.used);
        self.current |= u8::from(bit) << shift;
        self.used = self.used.saturating_add(1);
        if self.used == 8 {
            self.data.push(self.current);
            self.current = 0;
            self.used = 0;
        }
    }

    /// Write `count` bits MSB-first (matches `read_bits`).
    fn write_bits_msb(&mut self, value: u32, count: u32) {
        for i in (0..count).rev() {
            self.write_bit((value >> i) & 1 == 1);
        }
    }

    /// Write `count` bits LSB-first (matches `read_bits_reversed`).
    fn write_bits_lsb(&mut self, value: u32, count: u32) {
        for i in 0..count {
            self.write_bit((value >> i) & 1 == 1);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.used > 0 {
            self.data.push(self.current);
        }
        self.data
    }
}

/// Encode `data` as a valid BCFZ container (store-only, no back-references): literal
/// blocks of up to 3 bytes, each a 0 flag, the size (2 bits LSB-first) and the bytes (8 bits
/// MSB-first).
pub(super) fn compress_bcfz(data: &[u8]) -> GpResult<Vec<u8>> {
    let length = i32::try_from(data.len())
        .map_err(|_| GpError::WriteError(format!("{} bytes do not fit a GPX file", data.len())))?;
    let mut bits = BitWriter::new();
    for block in data.chunks(3) {
        bits.write_bit(false); // literal flag
        bits.write_bits_lsb(u32::try_from(block.len()).unwrap_or(3), 2);
        for &byte in block {
            bits.write_bits_msb(u32::from(byte), 8);
        }
    }
    let compressed = bits.finish();

    let mut out = Vec::with_capacity(compressed.len().saturating_add(8));
    out.extend_from_slice(BCFZ_MAGIC);
    out.extend_from_slice(&length.to_le_bytes());
    out.extend(compressed);
    Ok(out)
}

/// Writes `bytes` into `disk` at `offset`; the caller sized `disk` to hold it.
fn put(disk: &mut [u8], offset: usize, bytes: &[u8]) -> GpResult<()> {
    offset
        .checked_add(bytes.len())
        .and_then(|end| disk.get_mut(offset..end))
        .map(|slot| slot.copy_from_slice(bytes))
        .ok_or_else(|| GpError::WriteError(format!("BCFS: no room at offset {offset}")))
}

/// Pack a single file into a BCFS virtual filesystem (uncompressed BCFZ input).
///
/// Layout (after the 4-byte "BCFS" magic, all in 0x1000-byte sectors):
///   sector 0: blank (header)
///   sector 1: file directory entry for `filename`
///   sectors 2+: file data
///
/// GuitarScore fix: the block index table must fit into the directory sector (at most
/// [`MAX_BCFS_BLOCKS`] blocks, about 4 MB); larger files are refused. Writing the extra
/// indices overwrote the start of the file data and produced an unreadable file.
pub(super) fn pack_bcfs(filename: &str, file_data: &[u8]) -> GpResult<Vec<u8>> {
    let file_size = file_data.len();
    let num_data_sectors = file_size.div_ceil(SECTOR_SIZE);
    if num_data_sectors > MAX_BCFS_BLOCKS {
        return Err(GpError::FormatError(format!(
            "{filename} is {file_size} bytes; a GPX container holds at most {} bytes",
            MAX_BCFS_BLOCKS.saturating_mul(SECTOR_SIZE)
        )));
    }
    // At most MAX_BCFS_BLOCKS + 2 sectors of 4 KiB: none of this can overflow.
    let disk_size = num_data_sectors
        .saturating_add(2)
        .saturating_mul(SECTOR_SIZE);
    let mut disk = vec![0u8; disk_size];

    // Sector 1: the directory entry: type 2, the name (null-terminated), size, blocks.
    let entry = SECTOR_SIZE;
    put(&mut disk, entry, &2i32.to_le_bytes())?;
    let name = filename.as_bytes();
    let name = name
        .get(..name.len().min(ENTRY_NAME_LENGTH))
        .unwrap_or_default();
    put(&mut disk, entry.saturating_add(4), name)?;
    let size = i32::try_from(file_size).unwrap_or(i32::MAX);
    put(
        &mut disk,
        entry.saturating_add(ENTRY_FILE_SIZE),
        &size.to_le_bytes(),
    )?;
    for i in 0..num_data_sectors {
        let index = i32::try_from(i.saturating_add(2)).unwrap_or(i32::MAX);
        let offset = entry
            .saturating_add(ENTRY_BLOCKS)
            .saturating_add(i.saturating_mul(4));
        put(&mut disk, offset, &index.to_le_bytes())?;
    }

    // Sectors 2+: file content
    for (i, chunk) in file_data.chunks(SECTOR_SIZE).enumerate() {
        put(
            &mut disk,
            i.saturating_add(2).saturating_mul(SECTOR_SIZE),
            chunk,
        )?;
    }

    let mut out = Vec::with_capacity(disk_size.saturating_add(4));
    out.extend_from_slice(BCFS_MAGIC);
    out.extend(disk);
    Ok(out)
}

/// Serialize a `Song` to GP6 (.gpx) bytes.
pub(crate) fn write_gpx_bytes(song: &crate::io::Song) -> GpResult<Vec<u8>> {
    use crate::io::gpif_export::SongGpifExportOps;
    let xml = song.write_gpif_xml()?;
    let bcfs = pack_bcfs("score.gpif", xml.as_bytes())?;
    compress_bcfz(&bcfs)
}

/// Serialize a `Song` to GP7+ (.gp) bytes (ZIP archive with `Content/score.gpif`).
pub(crate) fn write_gp_bytes(song: &crate::io::Song) -> GpResult<Vec<u8>> {
    use crate::io::gpif_export::SongGpifExportOps;
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;

    let xml = song.write_gpif_xml()?;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("Content/score.gpif", options)
        .map_err(|e| format!("Zip write error: {e}"))?;
    zip.write_all(xml.as_bytes())
        .map_err(|e| format!("Zip data write error: {e}"))?;
    let cursor = zip.finish().map_err(|e| format!("Zip finish error: {e}"))?;
    Ok(cursor.into_inner())
}
