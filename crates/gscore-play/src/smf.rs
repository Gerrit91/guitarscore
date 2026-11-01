//! Writes a [`Sequence`] as a Standard MIDI File (format 1): a conductor track with the
//! title, tempo changes and time signatures, then one track per song track.

use crate::event::MidiEvent;
use crate::sequence::Sequence;
use crate::tempo::PPQ;

/// Longest delta time a variable-length quantity can hold (four bytes).
const MAX_DELTA: u64 = 0x0FFF_FFFF;
/// Longest text written into a meta event (track and song names).
const MAX_TEXT_BYTES: usize = 255;

/// Why a MIDI file could not be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmfError(pub String);

impl std::fmt::Display for SmfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot write the MIDI file: {}", self.0)
    }
}

impl std::error::Error for SmfError {}

/// The sequence as the bytes of a `.mid` file, named `title`.
pub fn to_smf(sequence: &Sequence, title: &str) -> Result<Vec<u8>, SmfError> {
    let track_count = u16::try_from(sequence.tracks.len().saturating_add(1))
        .map_err(|_| SmfError("too many tracks".to_owned()))?;
    let ppq = u16::try_from(PPQ).map_err(|_| SmfError("tick resolution".to_owned()))?;
    let mut out = Vec::new();
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6_u32.to_be_bytes());
    out.extend_from_slice(&1_u16.to_be_bytes());
    out.extend_from_slice(&track_count.to_be_bytes());
    out.extend_from_slice(&ppq.to_be_bytes());

    // Conductor track: name, time signatures and tempo, merged by tick.
    let mut conductor: Vec<(u64, Vec<u8>)> = vec![(0, text_meta(0x03, title))];
    let mut previous = None;
    for span in &sequence.measures {
        if previous != Some(span.signature) {
            conductor.push((span.start, time_signature_meta(span.signature)));
            previous = Some(span.signature);
        }
    }
    for &(tick, micros) in sequence.tempo.points() {
        let [_, a, b, c] = micros.min(0x00FF_FFFF).to_be_bytes();
        conductor.push((tick, vec![0xFF, 0x51, 0x03, a, b, c]));
    }
    conductor.sort_by_key(|(tick, _)| *tick);
    write_track(&mut out, &conductor, sequence.end)?;

    for (number, track) in sequence.tracks.iter().enumerate() {
        let mut events: Vec<(u64, Vec<u8>)> = vec![(0, text_meta(0x03, &track.name))];
        events.extend(
            sequence
                .events
                .iter()
                .filter(|e| e.track == number)
                .map(|e| (e.tick, MidiEvent::bytes(&e.event))),
        );
        write_track(&mut out, &events, sequence.end)?;
    }
    Ok(out)
}

/// Appends an `MTrk` chunk with `events` (sorted by tick) and the end-of-track at `end`.
fn write_track(out: &mut Vec<u8>, events: &[(u64, Vec<u8>)], end: u64) -> Result<(), SmfError> {
    let mut body = Vec::new();
    let mut last = 0_u64;
    for (tick, bytes) in events {
        write_vlq(&mut body, tick.saturating_sub(last));
        body.extend_from_slice(bytes);
        last = last.max(*tick);
    }
    write_vlq(&mut body, end.saturating_sub(last));
    body.extend_from_slice(&[0xFF, 0x2F, 0x00]);
    let length =
        u32::try_from(body.len()).map_err(|_| SmfError("a track is too long".to_owned()))?;
    out.extend_from_slice(b"MTrk");
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(&body);
    Ok(())
}

/// A meta event with text (UTF-8, cut at a character boundary if too long).
fn text_meta(kind: u8, text: &str) -> Vec<u8> {
    let mut end = text.len().min(MAX_TEXT_BYTES);
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    let bytes = text.as_bytes().get(..end).unwrap_or_default();
    let mut meta = vec![0xFF, kind];
    write_vlq(&mut meta, u64::try_from(bytes.len()).unwrap_or(0));
    meta.extend_from_slice(bytes);
    meta
}

/// The time signature meta event (denominator as a power of two).
fn time_signature_meta(signature: gscore_model::TimeSignature) -> Vec<u8> {
    let power = u8::try_from(signature.denominator().trailing_zeros()).unwrap_or(2);
    // 24 MIDI clocks per metronome click, 8 32nd notes per quarter.
    vec![0xFF, 0x58, 0x04, signature.numerator(), power, 24, 8]
}

/// A MIDI variable-length quantity (7 bits per byte, most significant first).
fn write_vlq(out: &mut Vec<u8>, value: u64) {
    let value = value.min(MAX_DELTA);
    let mut groups = [0_u8; 4];
    let mut count = 0;
    let mut rest = value;
    loop {
        if let Some(slot) = groups.get_mut(count) {
            *slot = u8::try_from(rest & 0x7F).unwrap_or(0);
        }
        count = count.saturating_add(1);
        rest >>= 7;
        if rest == 0 || count == groups.len() {
            break;
        }
    }
    for index in (0..count).rev() {
        let group = groups.get(index).copied().unwrap_or(0);
        out.push(if index == 0 { group } else { group | 0x80 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vlq(value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        write_vlq(&mut out, value);
        out
    }

    #[test]
    fn variable_length_quantities_match_the_specification() {
        // Examples from the Standard MIDI File specification.
        assert_eq!(vlq(0), [0x00]);
        assert_eq!(vlq(0x40), [0x40]);
        assert_eq!(vlq(0x7F), [0x7F]);
        assert_eq!(vlq(0x80), [0x81, 0x00]);
        assert_eq!(vlq(0x2000), [0xC0, 0x00]);
        assert_eq!(vlq(0x3FFF), [0xFF, 0x7F]);
        assert_eq!(vlq(0x0FFF_FFFF), [0xFF, 0xFF, 0xFF, 0x7F]);
        assert_eq!(vlq(u64::MAX), [0xFF, 0xFF, 0xFF, 0x7F], "clamped");
    }

    #[test]
    fn long_names_are_cut_at_a_character_boundary() {
        let name = "é".repeat(200); // 400 bytes
        let meta = text_meta(0x03, &name);
        let text = &meta[4..]; // FF 03, two length bytes
        assert!(std::str::from_utf8(text).is_ok());
        assert!(text.len() <= MAX_TEXT_BYTES);
    }
}
