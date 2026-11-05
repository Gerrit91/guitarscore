//! A compact, checked binary form of a [`Sequence`], for handing a song to another wasm
//! instance (the browser's AudioWorklet, which cannot share the app's memory).
//!
//! Only what playback needs travels: the events, the tempo map and the end tick. The
//! decoding side builds a [`Sequence`] with no track names or measure spans because it
//! never shows them.

use crate::{Channel, MidiEvent, Sequence, TempoMap, TimedEvent};

/// Identifies the format; bump the last digit when the layout changes.
const MAGIC: &[u8; 4] = b"GSQ2";
/// Caps sized by the input, so a damaged message cannot allocate without bound.
const MAX_EVENTS: usize = 4_000_000;
const MAX_TEMPO_POINTS: usize = 65_536;

/// Encodes the playable part of `sequence`.
pub fn encode_sequence(sequence: &Sequence) -> Vec<u8> {
    let mut out = Vec::with_capacity(sequence.events.len().saturating_mul(12).saturating_add(32));
    out.extend_from_slice(MAGIC);
    put_u32(
        &mut out,
        u32::try_from(sequence.events.len()).unwrap_or(u32::MAX),
    );
    for event in &sequence.events {
        put_u64(&mut out, event.tick);
        put_u32(&mut out, u32::try_from(event.track).unwrap_or(u32::MAX));
        encode_event(&mut out, event.event);
    }
    let points = sequence.tempo.points();
    put_u32(&mut out, u32::try_from(points.len()).unwrap_or(u32::MAX));
    for &(tick, micros) in points {
        put_u64(&mut out, tick);
        put_u32(&mut out, micros);
    }
    let pauses = sequence.tempo.pauses();
    put_u32(&mut out, u32::try_from(pauses.len()).unwrap_or(u32::MAX));
    for &(tick, micros) in pauses {
        put_u64(&mut out, tick);
        put_u64(&mut out, micros);
    }
    put_u64(&mut out, sequence.end);
    out
}

/// Decodes what [`encode_sequence`] wrote. A damaged message is an error, never a panic.
pub fn decode_sequence(bytes: &[u8]) -> Result<Sequence, String> {
    let mut reader = Reader { bytes, at: 0 };
    if reader.take(4)? != MAGIC {
        return Err("not a GuitarScore playback sequence".to_owned());
    }
    let count = reader.u32()? as usize;
    if count > MAX_EVENTS || count > reader.remaining().saturating_div(12) {
        return Err("the playback sequence is damaged (too many events)".to_owned());
    }
    let mut events = Vec::with_capacity(count);
    for _ in 0..count {
        let tick = reader.u64()?;
        let track = usize::try_from(reader.u32()?).map_err(|_| "bad track".to_owned())?;
        events.push(TimedEvent {
            tick,
            track,
            event: decode_event(&mut reader)?,
        });
    }
    let tempo_count = reader.u32()? as usize;
    if tempo_count > MAX_TEMPO_POINTS || tempo_count > reader.remaining().saturating_div(12) {
        return Err("the playback sequence is damaged (too many tempo changes)".to_owned());
    }
    let mut points = Vec::with_capacity(tempo_count);
    for _ in 0..tempo_count {
        points.push((reader.u64()?, reader.u32()?));
    }
    let pause_count = reader.u32()? as usize;
    if pause_count > MAX_TEMPO_POINTS || pause_count > reader.remaining().saturating_div(16) {
        return Err("the playback sequence is damaged (too many pauses)".to_owned());
    }
    let mut pauses = Vec::with_capacity(pause_count);
    for _ in 0..pause_count {
        pauses.push((reader.u64()?, reader.u64()?));
    }
    let end = reader.u64()?;
    let tempo = TempoMap::from_parts(points, pauses)
        .ok_or_else(|| "the playback sequence has no valid tempo".to_owned())?;
    Ok(Sequence {
        tracks: Vec::new(),
        events,
        tempo,
        measures: Vec::new(),
        end,
        skipped_notes: 0,
        degraded_pitch_bends: 0,
    })
}

fn encode_event(out: &mut Vec<u8>, event: MidiEvent) {
    match event {
        MidiEvent::NoteOn {
            channel,
            key,
            velocity,
        } => {
            out.push(0);
            out.push(channel.get());
            out.push(key);
            out.push(velocity);
        }
        MidiEvent::NoteOff { channel, key } => {
            out.push(1);
            out.push(channel.get());
            out.push(key);
            out.push(0);
        }
        MidiEvent::ProgramChange { channel, program } => {
            out.push(2);
            out.push(channel.get());
            out.push(program);
            out.push(0);
        }
        MidiEvent::Control {
            channel,
            controller,
            value,
        } => {
            out.push(3);
            out.push(channel.get());
            out.push(controller);
            out.push(value);
        }
        MidiEvent::PitchBend { channel, value } => {
            out.push(4);
            out.push(channel.get());
            put_u16(out, value);
        }
    }
}

fn decode_event(reader: &mut Reader<'_>) -> Result<MidiEvent, String> {
    let kind = reader.u8()?;
    let channel = Channel::new(reader.u8()?).ok_or_else(|| "bad MIDI channel".to_owned())?;
    match kind {
        0 => Ok(MidiEvent::NoteOn {
            channel,
            key: reader.u8()?,
            velocity: reader.u8()?,
        }),
        1 => {
            let key = reader.u8()?;
            let _reserved = reader.u8()?;
            Ok(MidiEvent::NoteOff { channel, key })
        }
        2 => {
            let program = reader.u8()?;
            let _reserved = reader.u8()?;
            Ok(MidiEvent::ProgramChange { channel, program })
        }
        3 => Ok(MidiEvent::Control {
            channel,
            controller: reader.u8()?,
            value: reader.u8()?,
        }),
        4 => Ok(MidiEvent::PitchBend {
            channel,
            value: reader.u16()?,
        }),
        _ => Err("unknown playback event".to_owned()),
    }
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Reads the fields; every read is checked against the end of the message.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.at)
    }

    fn take(&mut self, count: usize) -> Result<&[u8], String> {
        let end = self
            .at
            .checked_add(count)
            .ok_or_else(|| "the playback sequence is damaged".to_owned())?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| "the playback sequence is cut short".to_owned())?;
        self.at = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, String> {
        let slice = self.take(1)?;
        slice
            .first()
            .copied()
            .ok_or_else(|| "the playback sequence is cut short".to_owned())
    }

    fn u16(&mut self) -> Result<u16, String> {
        let slice = self.take(2)?;
        let mut bytes = [0_u8; 2];
        bytes.copy_from_slice(slice);
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let slice = self.take(4)?;
        let mut bytes = [0_u8; 4];
        bytes.copy_from_slice(slice);
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, String> {
        let slice = self.take(8)?;
        let mut bytes = [0_u8; 8];
        bytes.copy_from_slice(slice);
        Ok(u64::from_le_bytes(bytes))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::{PPQ, TimedEvent};

    fn sequence() -> Sequence {
        let mut tempo = TempoMap::new(120);
        tempo.set(u64::from(PPQ), 90);
        tempo.pause(u64::from(PPQ) * 2, 250_000);
        Sequence {
            tracks: Vec::new(),
            events: vec![
                TimedEvent {
                    tick: 0,
                    track: 0,
                    event: MidiEvent::ProgramChange {
                        channel: Channel::new(0).unwrap(),
                        program: 25,
                    },
                },
                TimedEvent {
                    tick: u64::from(PPQ),
                    track: 1,
                    event: MidiEvent::NoteOn {
                        channel: Channel::new(3).unwrap(),
                        key: 60,
                        velocity: 90,
                    },
                },
                TimedEvent {
                    tick: u64::from(PPQ) * 2,
                    track: 1,
                    event: MidiEvent::PitchBend {
                        channel: Channel::new(3).unwrap(),
                        value: 12_000,
                    },
                },
            ],
            tempo,
            measures: Vec::new(),
            end: u64::from(PPQ) * 4,
            skipped_notes: 0,
            degraded_pitch_bends: 0,
        }
    }

    #[test]
    fn a_sequence_round_trips_what_playback_needs() {
        let original = sequence();
        let decoded = decode_sequence(&encode_sequence(&original)).unwrap();
        assert_eq!(decoded.events, original.events);
        assert_eq!(decoded.tempo, original.tempo);
        assert_eq!(decoded.end, original.end);
    }

    #[test]
    fn damaged_messages_are_errors_not_panics() {
        let bytes = encode_sequence(&sequence());
        for cut in 0..bytes.len() {
            assert!(decode_sequence(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        let mut wrong = bytes.clone();
        wrong[0] = b'X';
        assert!(decode_sequence(&wrong).is_err());
        // A huge event count is refused before allocating.
        let mut huge = bytes.clone();
        huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_sequence(&huge).is_err());
    }
}
