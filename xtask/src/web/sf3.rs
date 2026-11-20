//! Compresses a SoundFont for the browser build: every sample becomes an Ogg Vorbis stream
//! (SF3, as MuseScore writes it), about a sixth of the SF2's size. The site's synthesizer
//! decodes it again after the download (`crates/gscore-audio/src/sf3.rs`, which documents
//! the layout).
//!
//! Every stream starts with the same few kilobytes of Vorbis headers (codebooks): 920
//! samples would carry 3.4 MiB of them, against 5.1 MiB of sound. So the headers are stored
//! once each in a `vhdr` chunk next to `smpl`, and a sample keeps only its index into it
//! (GuitarScore's packed SF3; the decoder puts each stream back together byte for byte).
//!
//! The encoding is ffmpeg's libvorbis, one process per sample, run in parallel (xtask has
//! no dependencies to do it in-process). GeneralUser GS's 30.6 MiB of samples take a few
//! seconds and become about 5.5 MiB at the chosen quality; the result is cached.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// libvorbis quality (`-q:a`, 0 to 10). 6 (about 192 kbit/s for stereo music) keeps the
/// samples indistinguishable from the originals in playback; 4 saves another quarter.
pub(crate) const QUALITY: &str = "6";
/// A sample header record (SF2 7.10).
const HEADER_BYTES: usize = 46;
/// The sample type bit of an Ogg Vorbis sample.
const VORBIS: u16 = 0x10;
/// A ROM sample has no data in the file.
const ROM: u16 = 0x8000;

/// One sample's Ogg Vorbis stream, or why it could not be made.
type Encoded = Result<Vec<u8>, String>;

/// Compresses an SF2's samples into an SF3. Fails if ffmpeg is missing or refuses one.
pub(crate) fn compress(sf2: &[u8]) -> Result<Vec<u8>, String> {
    let ffmpeg = super::soundfont::which(&["ffmpeg"]).ok_or_else(|| {
        "`ffmpeg` (with libvorbis) is needed to compress the SoundFont for the site; \
         install it, or pass --soundfont PATH to ship a font as it is"
            .to_owned()
    })?;
    let chunks = top_chunks(sf2)?;
    let smpl = sub_chunk(sf2, &chunks, b"sdta", b"smpl").ok_or("the SoundFont has no samples")?;
    let shdr = sub_chunk(sf2, &chunks, b"pdta", b"shdr").ok_or("the SoundFont has no shdr")?;
    if shdr.is_empty() || !shdr.len().is_multiple_of(HEADER_BYTES) {
        return Err("the SoundFont's sample headers are damaged".to_owned());
    }
    let count = shdr.len() / HEADER_BYTES;
    let headers: Vec<&[u8; HEADER_BYTES]> = shdr.as_chunks::<HEADER_BYTES>().0.iter().collect();

    // Encode every sample (not the terminal record), several at a time.
    let streams: Mutex<Vec<Option<Encoded>>> = Mutex::new(vec![None; count]);
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index + 1 >= count {
                        break;
                    }
                    let Some(&header) = headers.get(index) else {
                        break;
                    };
                    if u16_at(header, 44) & ROM != 0 {
                        continue;
                    }
                    let start = to_usize(u32_at(header, 20)).saturating_mul(2);
                    let end = to_usize(u32_at(header, 24)).saturating_mul(2);
                    let result = match smpl.get(start..end) {
                        Some([]) => Err(format!("sample {index} is empty")),
                        Some(pcm) => encode(&ffmpeg, pcm, u32_at(header, 36))
                            .map_err(|e| format!("sample {index}: {e}")),
                        None => Err(format!("sample {index} lies outside the sample data")),
                    };
                    if let Ok(mut streams) = streams.lock()
                        && let Some(slot) = streams.get_mut(index)
                    {
                        *slot = Some(result);
                    }
                }
            });
        }
    });
    let streams = streams
        .into_inner()
        .map_err(|_| "an encoder thread failed".to_owned())?;

    // The streams one after another, and the headers pointing at them.
    let mut data = Vec::new();
    let mut shared: Vec<Vec<u8>> = Vec::new();
    let mut new_headers = Vec::with_capacity(shdr.len());
    for (index, (&header, stream)) in headers.iter().zip(streams).enumerate() {
        let mut record = header.to_vec();
        if index + 1 < count && u16_at(header, 44) & ROM == 0 {
            let stream =
                stream.unwrap_or_else(|| Err(format!("sample {index} was not encoded")))?;
            let start = u32_at(header, 20);
            let end = u32_at(header, 24);
            let length = end.saturating_sub(start);
            // Loop points count from the sample's start in an SF3. One may end in the
            // padding after the sample (the decoder keeps that padding).
            let limit = length.saturating_add(46);
            let loop_start = u32_at(header, 28).saturating_sub(start).min(limit);
            let loop_end = u32_at(header, 32).saturating_sub(start).min(limit);
            let (header_pages, audio_pages) = split_headers(&stream)
                .ok_or_else(|| format!("sample {index}: ffmpeg wrote a damaged Ogg stream"))?;
            let shared_index = match shared.iter().position(|known| known == header_pages) {
                Some(known) => known,
                None => {
                    shared.push(header_pages.to_vec());
                    shared.len() - 1
                }
            };
            let shared_index = u16::try_from(shared_index).map_err(|_| "too many headers")?;
            let byte_start = u32::try_from(data.len()).map_err(|_| "too large")?;
            data.extend_from_slice(&shared_index.to_le_bytes());
            data.extend_from_slice(audio_pages);
            let byte_end = u32::try_from(data.len()).map_err(|_| "too large")?;
            for (at, value) in [
                (20, byte_start),
                (24, byte_end),
                (28, loop_start),
                (32, loop_end),
            ] {
                set(&mut record, at, &value.to_le_bytes());
            }
            let kind = u16_at(header, 44) | VORBIS;
            set(&mut record, 44, &kind.to_le_bytes());
        }
        new_headers.extend_from_slice(&record);
    }
    let mut table = Vec::new();
    for header_pages in &shared {
        let length = u32::try_from(header_pages.len()).map_err(|_| "too large")?;
        table.extend_from_slice(&length.to_le_bytes());
        table.extend_from_slice(header_pages);
    }
    rebuild(sf2, &chunks, &data, &table, &new_headers)
}

/// Splits an Ogg Vorbis stream into its header pages (granule position 0: identification,
/// comment and setup) and the audio pages after them.
fn split_headers(stream: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut at = 0;
    while stream.get(at..at + 4) == Some(b"OggS") {
        let granule = stream.get(at + 6..at + 14)?;
        if granule.iter().any(|byte| *byte != 0) {
            break;
        }
        let segments = usize::from(*stream.get(at + 26)?);
        let lacing = stream.get(at + 27..at + 27 + segments)?;
        at += 27 + segments + lacing.iter().map(|n| usize::from(*n)).sum::<usize>();
    }
    (at > 0 && at < stream.len()).then(|| stream.split_at(at))
}

/// Encodes 16-bit mono PCM at `rate` as one Ogg Vorbis stream.
fn encode(ffmpeg: &std::ffi::OsStr, pcm: &[u8], rate: u32) -> Result<Vec<u8>, String> {
    let rate = rate.to_string();
    let mut child = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "s16le",
            "-ar",
            &rate,
        ])
        .args([
            "-ac",
            "1",
            "-i",
            "pipe:0",
            "-c:a",
            "libvorbis",
            "-q:a",
            QUALITY,
        ])
        // No encoder tag or other metadata: smaller, and the same bytes on every run.
        .args([
            "-map_metadata",
            "-1",
            "-fflags",
            "+bitexact",
            "-flags:a",
            "+bitexact",
        ])
        .args(["-f", "ogg", "pipe:1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start ffmpeg: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("ffmpeg has no input")?;
    let mut stdout = child.stdout.take().ok_or("ffmpeg has no output")?;
    // Written from its own thread: ffmpeg's output pipe would fill up and block it.
    let (written, output) = std::thread::scope(|scope| {
        let writer = scope.spawn(move || stdin.write_all(pcm));
        let mut output = Vec::new();
        let read = stdout.read_to_end(&mut output);
        (writer.join(), read.map(|_| output))
    });
    let status = child.wait().map_err(|e| format!("ffmpeg: {e}"))?;
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    if !status.success() {
        return Err(format!("ffmpeg failed: {}", stderr.trim()));
    }
    match written {
        Ok(Ok(())) => {}
        _ => return Err("cannot send the sample to ffmpeg".to_owned()),
    }
    let output = output.map_err(|e| format!("cannot read ffmpeg's output: {e}"))?;
    if !output.starts_with(b"OggS") {
        return Err("ffmpeg did not write an Ogg stream".to_owned());
    }
    Ok(output)
}

/// A RIFF chunk: id, list type (for `LIST`) and the data's byte range in the file.
struct Chunk {
    id: [u8; 4],
    list: Option<[u8; 4]>,
    range: std::ops::Range<usize>,
}

fn top_chunks(bytes: &[u8]) -> Result<Vec<Chunk>, String> {
    if !super::soundfont::looks_like_soundfont(bytes) {
        return Err("not a SoundFont (no RIFF/sfbk header)".to_owned());
    }
    chunks(bytes, 12..bytes.len())
}

fn chunks(bytes: &[u8], range: std::ops::Range<usize>) -> Result<Vec<Chunk>, String> {
    let mut out = Vec::new();
    let mut at = range.start;
    while at + 8 <= range.end {
        let id = four(bytes, at).ok_or("damaged chunk")?;
        let size = to_usize(u32_at(bytes, at + 4));
        let start = at + 8;
        let end = start
            .checked_add(size)
            .filter(|end| *end <= range.end)
            .ok_or("a chunk of the SoundFont is cut off")?;
        let (list, start) = match four(bytes, start) {
            Some(kind) if &id == b"LIST" && size >= 4 => (Some(kind), start + 4),
            _ => (None, start),
        };
        out.push(Chunk {
            id,
            list,
            range: start..end,
        });
        at = end + (size & 1);
    }
    Ok(out)
}

fn sub_chunk<'a>(bytes: &'a [u8], top: &[Chunk], list: &[u8; 4], id: &[u8; 4]) -> Option<&'a [u8]> {
    let parent = top.iter().find(|chunk| chunk.list.as_ref() == Some(list))?;
    let subs = chunks(bytes, parent.range.clone()).ok()?;
    subs.into_iter()
        .find(|sub| &sub.id == id)
        .and_then(|sub| bytes.get(sub.range))
}

/// The SF2 again with `smpl`, the shared headers `vhdr`, `shdr` and version 3.1; every
/// other chunk unchanged.
fn rebuild(
    bytes: &[u8],
    top: &[Chunk],
    smpl: &[u8],
    vhdr: &[u8],
    shdr: &[u8],
) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(smpl.len() + 1024 * 1024);
    out.extend_from_slice(b"RIFF\0\0\0\0sfbk");
    for chunk in top {
        let Some(list) = chunk.list else {
            put_chunk(
                &mut out,
                &chunk.id,
                bytes.get(chunk.range.clone()).unwrap_or_default(),
            );
            continue;
        };
        out.extend_from_slice(b"LIST\0\0\0\0");
        let size_at = out.len() - 4;
        out.extend_from_slice(&list);
        if &list == b"sdta" {
            // Only the 16-bit data: an `sm24` chunk has no meaning for Vorbis samples.
            put_chunk(&mut out, b"smpl", smpl);
            put_chunk(&mut out, b"vhdr", vhdr);
        } else {
            for sub in chunks(bytes, chunk.range.clone())? {
                let data = match (&list, &sub.id) {
                    (b"INFO", b"ifil") => &[3, 0, 1, 0][..],
                    (b"pdta", b"shdr") => shdr,
                    _ => bytes.get(sub.range).unwrap_or_default(),
                };
                put_chunk(&mut out, &sub.id, data);
            }
        }
        let size = u32::try_from(out.len() - size_at - 4).map_err(|_| "too large")?;
        set(&mut out, size_at, &size.to_le_bytes());
    }
    let size = u32::try_from(out.len() - 8).map_err(|_| "too large")?;
    set(&mut out, 4, &size.to_le_bytes());
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

/// The four bytes at `at`, if they are there.
fn four(bytes: &[u8], at: usize) -> Option<[u8; 4]> {
    bytes.get(at..at.checked_add(4)?)?.try_into().ok()
}

/// The little-endian `u32` at `at` (0 past the end: a size there then cuts the chunk off).
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    four(bytes, at).map_or(0, u32::from_le_bytes)
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    bytes
        .get(at..at.saturating_add(2))
        .and_then(|two| two.try_into().ok())
        .map_or(0, u16::from_le_bytes)
}

/// Overwrites the bytes at `at` (a field this module laid out itself).
fn set(out: &mut [u8], at: usize, value: &[u8]) {
    if let Some(slot) = out.get_mut(at..at.saturating_add(value.len())) {
        slot.copy_from_slice(value);
    }
}

fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// Whether the file was written by [`compress`] (or another SF3 writer): version 3.
pub(crate) fn is_compressed(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Ok(top) = top_chunks(&bytes) else {
        return false;
    };
    sub_chunk(&bytes, &top, b"INFO", b"ifil").is_some_and(|ifil| ifil.first() == Some(&3))
}
