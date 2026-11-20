//! SF3: a SoundFont whose samples are stored as Ogg Vorbis, about a sixth of the size of
//! the same font as SF2. The browser build ships its SoundFont this way (`cargo xtask web`
//! writes it); the synthesizer only reads SF2, so [`expand`] decodes an SF3 back into one
//! before it is parsed.
//!
//! The layout is MuseScore's SF3, as `cargo xtask web` writes it:
//!
//! - `ifil` (the version in the INFO list) is 3.x;
//! - the `smpl` chunk holds one complete Ogg Vorbis stream (mono) per sample header;
//! - a header's `start` and `end` are the byte range of its stream in `smpl`, `end`
//!   exclusive, and its loop points count frames from the start of the decoded sample;
//! - a compressed sample's type has bit 0x10 set.
//!
//! GuitarScore's packed variant (what `cargo xtask web` writes) stores the Vorbis header
//! pages, which every stream repeats, once: a `vhdr` chunk after `smpl` lists them (each a
//! little-endian `u32` length and the pages), and a sample's bytes in `smpl` are a `u16`
//! index into that list followed by its audio pages. Header and audio pages together are
//! the original stream, byte for byte. In GeneralUser GS this saves 3 of 8.7 MiB.
//!
//! The SF2 that comes out has the decoded samples one after another, each followed by the
//! 46 silent frames the SF2 specification asks for, absolute loop points, the 0x10 bit
//! cleared and `ifil` 2.1. Every other chunk (presets, instruments, names) is copied
//! unchanged.

use std::io::Cursor;

/// A sample header record in `shdr` is this long (SF2 7.10).
const HEADER_BYTES: usize = 46;
/// Silent frames after each sample (SF2 6.1: "at least 46 zero valued sample points").
const PADDING_FRAMES: usize = 46;
/// The sample type bit for an Ogg Vorbis sample.
const VORBIS: u16 = 0x10;
/// A ROM sample has no data in the file.
const ROM: u16 = 0x8000;
/// A damaged or hostile file cannot make the decoder allocate without bound: the SF2 that
/// comes out is at most this large (GeneralUser GS is about 31 MB, FluidR3 GM 142 MB).
const MAX_EXPANDED_BYTES: usize = 512 * 1024 * 1024;

/// Whether `bytes` is a SoundFont with compressed (Ogg Vorbis) samples.
pub fn is_sf3(bytes: &[u8]) -> bool {
    let Ok(font) = Riff::parse(bytes) else {
        return false;
    };
    font.version_major() == Some(3) || font.headers().any(|header| header.kind & VORBIS != 0)
}

/// Decodes an SF3 into the SF2 it was made from (as far as lossy compression allows).
/// Fails with a description for a damaged file; never panics on one.
pub fn expand(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let font = Riff::parse(bytes)?;
    let smpl = font.sub_chunk(b"sdta", b"smpl").unwrap_or(&[]);
    let shared = font
        .sub_chunk(b"sdta", b"vhdr")
        .map(shared_headers)
        .transpose()?;
    let shdr = font
        .sub_chunk(b"pdta", b"shdr")
        .ok_or("the SoundFont has no sample headers (shdr)")?;
    if shdr.is_empty() || !shdr.len().is_multiple_of(HEADER_BYTES) {
        return Err("the SoundFont's sample headers are damaged".to_owned());
    }

    // Decode every sample and rewrite its header, the terminal record included.
    let mut samples: Vec<u8> = Vec::new();
    let mut headers = Vec::with_capacity(shdr.len());
    let count = shdr.len() / HEADER_BYTES;
    for (index, record) in shdr.as_chunks::<HEADER_BYTES>().0.iter().enumerate() {
        let mut header =
            Header::read(record).ok_or("the SoundFont's sample headers are damaged")?;
        let terminal = index + 1 == count;
        if !terminal && header.kind & ROM == 0 {
            if header.kind & VORBIS == 0 {
                return Err(format!("sample {index} is not compressed (a mixed SF3)"));
            }
            let start = usize::try_from(header.start).unwrap_or(usize::MAX);
            let end = usize::try_from(header.end).unwrap_or(usize::MAX);
            let stream = smpl
                .get(start..end)
                .ok_or_else(|| format!("sample {index} lies outside the sample data"))?;
            let frames = match &shared {
                None => decode(stream),
                Some(shared) => unpack(stream, shared).and_then(|stream| decode(&stream)),
            }
            .map_err(|e| format!("sample {index}: {e}"))?;
            let offset = u32::try_from(samples.len() / 2)
                .map_err(|_| "the decoded SoundFont is too large".to_owned())?;
            let length = u32::try_from(frames.len())
                .map_err(|_| "the decoded SoundFont is too large".to_owned())?;
            if samples.len() + (frames.len() + PADDING_FRAMES) * 2 > MAX_EXPANDED_BYTES {
                return Err("the decoded SoundFont is too large".to_owned());
            }
            for frame in &frames {
                samples.extend_from_slice(&frame.to_le_bytes());
            }
            samples.resize(samples.len() + PADDING_FRAMES * 2, 0);
            header.start = offset;
            header.end = offset + length;
            // A loop may end in the silent padding (GeneralUser GS has one that ends a frame
            // after its sample), but not beyond it.
            let limit = length + PADDING_FRAMES as u32;
            header.loop_start = offset + header.loop_start.min(limit);
            header.loop_end = offset + header.loop_end.min(limit);
            header.kind &= !VORBIS;
        }
        // The terminal record ("EOS") is copied as it is; nothing reads its fields.
        header.write(&mut headers);
    }
    // Version 2.1: an SF2 now.
    rebuild(&font, [2, 0, 1, 0], &samples, &headers)
}

/// Writes `font` again with `version` (`ifil`), the sample data `smpl` and the sample
/// headers `shdr`; every other chunk is copied unchanged.
fn rebuild(font: &Riff<'_>, version: [u8; 4], smpl: &[u8], shdr: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(smpl.len() + 1024 * 1024);
    out.extend_from_slice(b"RIFF\0\0\0\0sfbk");
    for chunk in &font.chunks {
        match (&chunk.id, chunk.list) {
            (b"LIST", Some(b"INFO")) => {
                let start = begin_list(&mut out, b"INFO");
                for sub in chunk.subs() {
                    if &sub.id == b"ifil" {
                        put_chunk(&mut out, b"ifil", &version);
                    } else {
                        put_chunk(&mut out, &sub.id, sub.data);
                    }
                }
                end_list(&mut out, start)?;
            }
            (b"LIST", Some(b"sdta")) => {
                let start = begin_list(&mut out, b"sdta");
                put_chunk(&mut out, b"smpl", smpl);
                end_list(&mut out, start)?;
            }
            (b"LIST", Some(b"pdta")) => {
                let start = begin_list(&mut out, b"pdta");
                for sub in chunk.subs() {
                    if &sub.id == b"shdr" {
                        put_chunk(&mut out, b"shdr", shdr);
                    } else {
                        put_chunk(&mut out, &sub.id, sub.data);
                    }
                }
                end_list(&mut out, start)?;
            }
            _ => put_chunk(&mut out, &chunk.id, chunk.data),
        }
    }
    let size = u32::try_from(out.len() - 8).map_err(|_| "the SoundFont is too large")?;
    set_u32(&mut out, 4, size);
    Ok(out)
}

/// The header pages of a packed SF3's `vhdr` chunk.
fn shared_headers(vhdr: &[u8]) -> Result<Vec<&[u8]>, String> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < vhdr.len() {
        let length = u32_at(vhdr, at).ok_or("the shared Vorbis headers are cut off")?;
        let length = usize::try_from(length).unwrap_or(usize::MAX);
        let pages = at
            .checked_add(4 + length)
            .and_then(|end| vhdr.get(at + 4..end))
            .ok_or("the shared Vorbis headers are cut off")?;
        out.push(pages);
        at += 4 + length;
    }
    Ok(out)
}

/// A packed sample's stream: its shared header pages, then its own audio pages.
fn unpack(sample: &[u8], shared: &[&[u8]]) -> Result<Vec<u8>, String> {
    let (index, audio) = sample
        .split_first_chunk::<2>()
        .ok_or("the sample has no header index")?;
    let headers = shared
        .get(usize::from(u16::from_le_bytes(*index)))
        .ok_or("the sample's header index is out of range")?;
    Ok([headers, audio].concat())
}

/// Decodes one mono Ogg Vorbis stream into 16-bit frames.
fn decode(stream: &[u8]) -> Result<Vec<i16>, String> {
    let mut reader = lewton::inside_ogg::OggStreamReader::new(Cursor::new(stream))
        .map_err(|e| format!("not an Ogg Vorbis stream: {e}"))?;
    if reader.ident_hdr.audio_channels != 1 {
        return Err(format!(
            "{} channels; a SoundFont sample is mono",
            reader.ident_hdr.audio_channels
        ));
    }
    let mut frames = Vec::new();
    while let Some(packet) = reader
        .read_dec_packet_itl()
        .map_err(|e| format!("damaged Ogg Vorbis data: {e}"))?
    {
        if frames.len() + packet.len() > MAX_EXPANDED_BYTES / 2 {
            return Err("the sample is too large".to_owned());
        }
        frames.extend_from_slice(&packet);
    }
    // The last packet decodes whole; the stream's final granule position says where the
    // sample really ends (lewton does not trim it).
    if let Some(length) = reader
        .get_last_absgp()
        .and_then(|granule| usize::try_from(granule).ok())
    {
        frames.truncate(length);
    }
    Ok(frames)
}

/// One sample header (`shdr` record).
#[derive(Debug, Default, Clone)]
struct Header {
    name: [u8; 20],
    start: u32,
    end: u32,
    loop_start: u32,
    loop_end: u32,
    rate: u32,
    pitch: u8,
    correction: u8,
    link: u16,
    kind: u16,
}

impl Header {
    /// Reads a record (`None` if it is shorter than [`HEADER_BYTES`]).
    fn read(record: &[u8]) -> Option<Header> {
        Some(Header {
            name: record.get(..20)?.try_into().ok()?,
            start: u32_at(record, 20)?,
            end: u32_at(record, 24)?,
            loop_start: u32_at(record, 28)?,
            loop_end: u32_at(record, 32)?,
            rate: u32_at(record, 36)?,
            pitch: *record.get(40)?,
            correction: *record.get(41)?,
            link: u16_at(record, 42)?,
            kind: u16_at(record, 44)?,
        })
    }

    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.name);
        for value in [
            self.start,
            self.end,
            self.loop_start,
            self.loop_end,
            self.rate,
        ] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.push(self.pitch);
        out.push(self.correction);
        out.extend_from_slice(&self.link.to_le_bytes());
        out.extend_from_slice(&self.kind.to_le_bytes());
    }
}

/// A chunk of the RIFF file: its id, its data and, for a `LIST`, the list's type.
struct Chunk<'a> {
    id: [u8; 4],
    list: Option<&'a [u8; 4]>,
    /// For a `LIST`: the data after the list type.
    data: &'a [u8],
}

impl<'a> Chunk<'a> {
    /// The chunks inside a `LIST` (a damaged tail is ignored).
    fn subs(&self) -> Vec<Chunk<'a>> {
        chunks(self.data).unwrap_or_default()
    }
}

/// The top-level chunks of a SoundFont.
struct Riff<'a> {
    chunks: Vec<Chunk<'a>>,
}

impl<'a> Riff<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Riff<'a>, String> {
        if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"sfbk") {
            return Err("not a SoundFont (no RIFF/sfbk header)".to_owned());
        }
        let size = u32_at(bytes, 4).ok_or("not a SoundFont")?;
        let end = usize::try_from(size)
            .unwrap_or(usize::MAX)
            .saturating_add(8)
            .clamp(12, bytes.len());
        Ok(Riff {
            chunks: chunks(bytes.get(12..end).unwrap_or_default())?,
        })
    }

    fn sub_chunk(&self, list: &[u8; 4], id: &[u8; 4]) -> Option<&'a [u8]> {
        self.chunks
            .iter()
            .filter(|chunk| chunk.list == Some(list))
            .flat_map(|chunk| chunk.subs())
            .find(|sub| &sub.id == id)
            .map(|sub| sub.data)
    }

    fn version_major(&self) -> Option<u16> {
        let ifil = self.sub_chunk(b"INFO", b"ifil")?;
        Some(u16::from_le_bytes([*ifil.first()?, *ifil.get(1)?]))
    }

    fn headers(&self) -> impl Iterator<Item = Header> + '_ {
        self.sub_chunk(b"pdta", b"shdr")
            .unwrap_or(&[])
            .as_chunks::<HEADER_BYTES>()
            .0
            .iter()
            .filter_map(|record| Header::read(record))
    }
}

/// Splits `bytes` into RIFF chunks (each padded to an even length).
fn chunks(bytes: &[u8]) -> Result<Vec<Chunk<'_>>, String> {
    let mut out = Vec::new();
    let mut at = 0;
    while let (Some(id), Some(size)) = (bytes.get(at..at + 4), u32_at(bytes, at + 4)) {
        let id: [u8; 4] = id.try_into().map_err(|_| "damaged chunk")?;
        let size = usize::try_from(size).unwrap_or(usize::MAX);
        let start = at + 8;
        let end = start.saturating_add(size);
        let data = bytes
            .get(start..end)
            .ok_or_else(|| format!("the chunk {} is cut off", String::from_utf8_lossy(&id)))?;
        let (list, data) = match data.split_first_chunk::<4>() {
            Some((kind, rest)) if &id == b"LIST" => (Some(kind), rest),
            _ => (None, data),
        };
        out.push(Chunk { id, list, data });
        at = end.saturating_add(size & 1);
    }
    Ok(out)
}

fn put_chunk(out: &mut Vec<u8>, id: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(id);
    out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

/// Starts a `LIST` of `kind`; returns where its size goes.
fn begin_list(out: &mut Vec<u8>, kind: &[u8; 4]) -> usize {
    out.extend_from_slice(b"LIST\0\0\0\0");
    let at = out.len() - 4;
    out.extend_from_slice(kind);
    at
}

fn end_list(out: &mut [u8], size_at: usize) -> Result<(), String> {
    let size = u32::try_from(out.len() - size_at - 4).map_err(|_| "the SoundFont is too large")?;
    set_u32(out, size_at, size);
    Ok(())
}

/// The little-endian `u32` at `at`, if the bytes are there.
fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let four = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(four.try_into().ok()?))
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let two = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes(two.try_into().ok()?))
}

/// Writes a little-endian `u32` at `at` (a size field this module reserved itself).
fn set_u32(out: &mut [u8], at: usize, value: u32) {
    if let Some(slot) = out.get_mut(at..at.saturating_add(4)) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SoundBank;

    /// A quarter second of A4 at 44 kHz as Ogg Vorbis (made with ffmpeg's libvorbis, q6).
    const SINE_OGG: &[u8] = include_bytes!("../../../testdata/audio/sine-a4.ogg");

    /// The test SoundFont with every sample replaced by [`SINE_OGG`], as an SF3 writer
    /// would store it: byte ranges, relative loops, the Vorbis bit, version 3.
    fn compressed(sf2: &[u8]) -> Vec<u8> {
        compressed_as(sf2, false)
    }

    /// Like [`compressed`]; `packed`, the header pages are stored once in `vhdr`.
    fn compressed_as(sf2: &[u8], packed: bool) -> Vec<u8> {
        // The stream's header pages: the pages before the first with a granule position.
        let header_end = SINE_OGG
            .windows(4)
            .enumerate()
            .filter(|(_, window)| *window == b"OggS")
            .map(|(at, _)| at)
            .find(|at| SINE_OGG[at + 6..at + 14].iter().any(|byte| *byte != 0))
            .unwrap();
        let font = Riff::parse(sf2).unwrap();
        let shdr = font.sub_chunk(b"pdta", b"shdr").unwrap();
        let count = shdr.len() / HEADER_BYTES;
        let mut smpl = Vec::new();
        let mut headers = Vec::new();
        for (index, record) in shdr.as_chunks::<HEADER_BYTES>().0.iter().enumerate() {
            let mut header = Header::read(record).unwrap();
            if index + 1 < count {
                header.loop_start = 100;
                header.loop_end = 200;
                header.start = u32::try_from(smpl.len()).unwrap();
                if packed {
                    smpl.extend_from_slice(&0_u16.to_le_bytes());
                    smpl.extend_from_slice(&SINE_OGG[header_end..]);
                } else {
                    smpl.extend_from_slice(SINE_OGG);
                }
                header.end = u32::try_from(smpl.len()).unwrap();
                header.kind |= VORBIS;
            }
            header.write(&mut headers);
        }
        let mut out = rebuild(&font, [3, 0, 1, 0], &smpl, &headers).unwrap();
        if packed {
            // A `vhdr` chunk after `smpl`, with the one header the samples share.
            let mut vhdr = u32::try_from(header_end).unwrap().to_le_bytes().to_vec();
            vhdr.extend_from_slice(&SINE_OGG[..header_end]);
            let mut chunk = Vec::new();
            put_chunk(&mut chunk, b"vhdr", &vhdr);
            let list = out.windows(4).position(|w| w == b"sdta").unwrap() - 8;
            let size_at = list + 4;
            let size = u32::from_le_bytes(out[size_at..size_at + 4].try_into().unwrap());
            let end = size_at + 4 + usize::try_from(size).unwrap();
            out.splice(end..end, chunk.iter().copied());
            let size = size + u32::try_from(chunk.len()).unwrap();
            out[size_at..size_at + 4].copy_from_slice(&size.to_le_bytes());
            let total = u32::try_from(out.len() - 8).unwrap();
            out[4..8].copy_from_slice(&total.to_le_bytes());
        }
        out
    }

    #[test]
    fn a_packed_sf3_decodes_like_the_plain_one() {
        let sf2 = crate::testing::sine_soundfont("Sine");
        let plain = compressed_as(&sf2, false);
        let packed = compressed_as(&sf2, true);
        assert!(is_sf3(&packed));
        assert_eq!(expand(&packed).unwrap(), expand(&plain).unwrap());
        // A header index past the table is an error.
        let mut broken = packed.clone();
        let at = broken.windows(4).position(|w| w == b"smpl").unwrap() + 8;
        broken[at] = 9;
        assert!(expand(&broken).is_err());
    }

    #[test]
    fn an_sf3_is_decoded_into_an_sf2_the_synthesizer_reads() {
        let sf2 = crate::testing::sine_soundfont("Sine");
        assert!(!is_sf3(&sf2));
        let sf3 = compressed(&sf2);
        assert!(is_sf3(&sf3));

        let expanded = expand(&sf3).unwrap();
        assert!(!is_sf3(&expanded));
        let font = Riff::parse(&expanded).unwrap();
        assert_eq!(font.version_major(), Some(2));
        let smpl = font.sub_chunk(b"sdta", b"smpl").unwrap();
        let headers: Vec<Header> = font.headers().collect();
        let frames = 11_000; // a quarter second at 44 kHz
        for header in &headers[..headers.len() - 1] {
            assert_eq!(
                header.end - header.start,
                frames,
                "the stream's exact length"
            );
            assert_eq!(
                header.loop_start - header.start,
                100,
                "loops become absolute"
            );
            assert_eq!(header.loop_end - header.start, 200);
            assert_eq!(header.kind & VORBIS, 0);
            // Not silence, and the 46 silent frames after it.
            let start = usize::try_from(header.start).unwrap() * 2;
            let end = usize::try_from(header.end).unwrap() * 2;
            assert!(
                smpl[start..end]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .any(|pair| *pair != [0, 0])
            );
            assert!(
                smpl[end..end + PADDING_FRAMES * 2]
                    .iter()
                    .all(|byte| *byte == 0)
            );
        }
        // The presets and names are the original's.
        assert_eq!(
            font.sub_chunk(b"pdta", b"phdr"),
            Riff::parse(&sf2).unwrap().sub_chunk(b"pdta", b"phdr")
        );
        // And the synthesizer takes the SF3 directly.
        let bank = SoundBank::from_bytes(&sf3).unwrap();
        assert_eq!(bank.name(), SoundBank::from_bytes(&sf2).unwrap().name());
    }

    #[test]
    fn a_damaged_sf3_is_an_error_not_a_panic() {
        let sf3 = compressed(&crate::testing::sine_soundfont("Sine"));
        // Cut anywhere, or with the sample streams scribbled over.
        for cut in [12, 100, sf3.len() / 2, sf3.len() - 10] {
            let _ = expand(&sf3[..cut]);
        }
        let mut scribbled = sf3.clone();
        let at = scribbled.windows(4).position(|w| w == b"OggS").unwrap();
        for byte in &mut scribbled[at..at + 400] {
            *byte = byte.wrapping_mul(31).wrapping_add(7);
        }
        assert!(expand(&scribbled).is_err());
        assert!(SoundBank::from_bytes(&scribbled).is_err());
        assert!(expand(b"RIFF\0\0\0\0sfbk").is_err());
    }
}
