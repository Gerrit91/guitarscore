//! MIDI input (M23 step 3), the part without a device: reading incoming messages and
//! gathering the notes of a chord. Every byte comes from outside (a keyboard, a MIDI
//! guitar, any program that connects to the port) and is treated as hostile: whatever
//! arrives gives a message or nothing, never a panic.
//!
//! Step entry only needs note-ons; note-offs matter for monitoring (the note heard while
//! the key is held). Everything else (controllers, clock, system exclusive) is ignored.

/// Most notes one chord gathers (a keyboard player's two hands, a guitar's strings).
pub const MAX_CHORD_NOTES: usize = 16;
/// How long after a chord's first note its other notes may still start, by default.
pub const DEFAULT_CHORD_WINDOW_MS: u64 = 40;

/// A note message from a MIDI input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MidiInput {
    /// A key was struck.
    NoteOn {
        /// The channel, 0 to 15.
        channel: u8,
        /// The key (MIDI note number), 0 to 127.
        key: u8,
        /// How hard, 1 to 127.
        velocity: u8,
    },
    /// A key was released (also a note-on with velocity 0, as running status sends it).
    NoteOff {
        /// The channel, 0 to 15.
        channel: u8,
        /// The key.
        key: u8,
    },
}

impl MidiInput {
    /// Reads one message as a MIDI input port delivers it (status byte first). Anything
    /// that is not a complete, well-formed note message gives `None`.
    pub fn parse(bytes: &[u8]) -> Option<MidiInput> {
        let (&status, data) = bytes.split_first()?;
        let [key, velocity, ..] = *data else {
            return None;
        };
        // Data bytes have the top bit clear; a status byte where data belongs is broken.
        if key >= 0x80 || velocity >= 0x80 {
            return None;
        }
        let channel = status & 0x0F;
        match status & 0xF0 {
            0x90 if velocity > 0 => Some(MidiInput::NoteOn {
                channel,
                key,
                velocity,
            }),
            0x90 | 0x80 => Some(MidiInput::NoteOff { channel, key }),
            _ => None,
        }
    }

    /// The channel it came on.
    pub fn channel(&self) -> u8 {
        match *self {
            MidiInput::NoteOn { channel, .. } | MidiInput::NoteOff { channel, .. } => channel,
        }
    }
}

/// A note that is part of a chord: its channel (a MIDI guitar's string) and key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChordNote {
    /// The channel, 0 to 15.
    pub channel: u8,
    /// The key.
    pub key: u8,
}

/// Gathers the notes that start within a short window into one chord: a player never
/// strikes a chord's keys at exactly the same moment. Times are milliseconds on any
/// monotonic clock the caller has.
#[derive(Debug, Clone)]
pub struct ChordGatherer {
    window_ms: u64,
    notes: Vec<ChordNote>,
    started_ms: u64,
}

impl ChordGatherer {
    /// A gatherer whose chords take the notes starting within `window_ms` of the first.
    pub fn new(window_ms: u64) -> Self {
        ChordGatherer {
            window_ms,
            notes: Vec::with_capacity(MAX_CHORD_NOTES),
            started_ms: 0,
        }
    }

    /// A note started. Returns the chord before it when this note comes too late to join
    /// it (the new note then starts the next chord).
    pub fn note_on(&mut self, note: ChordNote, now_ms: u64) -> Option<Vec<ChordNote>> {
        let finished = self.poll(now_ms);
        if self.notes.is_empty() {
            self.started_ms = now_ms;
        }
        // The same key twice is one note; beyond the cap, notes are dropped.
        if !self.notes.contains(&note) && self.notes.len() < MAX_CHORD_NOTES {
            self.notes.push(note);
        }
        finished
    }

    /// The gathered chord, once its window has ended.
    pub fn poll(&mut self, now_ms: u64) -> Option<Vec<ChordNote>> {
        let due = self
            .deadline_ms()
            .is_some_and(|deadline| now_ms >= deadline);
        due.then(|| std::mem::take(&mut self.notes))
    }

    /// When the chord being gathered is complete, if one is.
    pub fn deadline_ms(&self) -> Option<u64> {
        (!self.notes.is_empty()).then(|| self.started_ms.saturating_add(self.window_ms))
    }

    /// Forgets the chord being gathered (the input was switched off).
    pub fn clear(&mut self) {
        self.notes.clear();
    }
}

impl Default for ChordGatherer {
    fn default() -> Self {
        ChordGatherer::new(DEFAULT_CHORD_WINDOW_MS)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]

    use super::*;

    #[test]
    fn note_messages_are_read_and_the_rest_ignored() {
        assert_eq!(
            MidiInput::parse(&[0x93, 60, 100]),
            Some(MidiInput::NoteOn {
                channel: 3,
                key: 60,
                velocity: 100
            })
        );
        assert_eq!(
            MidiInput::parse(&[0x80, 60, 64]),
            Some(MidiInput::NoteOff {
                channel: 0,
                key: 60
            })
        );
        // A note-on without velocity is a note-off.
        assert_eq!(
            MidiInput::parse(&[0x9F, 40, 0]),
            Some(MidiInput::NoteOff {
                channel: 15,
                key: 40
            })
        );
        for bytes in [
            &[][..],
            &[0x90],
            &[0x90, 60],
            &[0x90, 0x80, 1],
            &[0x90, 60, 0xFF],
            &[0xB0, 7, 100],
            &[0xF0, 1, 2, 3, 0xF7],
            &[0xF8],
            &[60, 100, 0x90],
        ] {
            assert_eq!(MidiInput::parse(bytes), None, "{bytes:?}");
        }
    }

    /// Arbitrary bytes give a message or nothing, never a panic: every message of up to
    /// three bytes, and random longer ones.
    #[test]
    fn arbitrary_bytes_never_panic() {
        for a in 0..=255_u8 {
            let _ = MidiInput::parse(&[a]);
            for b in 0..=255_u8 {
                let _ = MidiInput::parse(&[a, b]);
                for c in [0_u8, 1, 64, 127, 128, 255] {
                    if let Some(message) = MidiInput::parse(&[a, b, c]) {
                        assert!(message.channel() < 16);
                    }
                }
            }
        }
        let mut seed = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..20_000 {
            let length = (next() % 40) as usize;
            let bytes: Vec<u8> = (0..length).map(|_| next() as u8).collect();
            if let Some(MidiInput::NoteOn { key, velocity, .. }) = MidiInput::parse(&bytes) {
                assert!(key < 128 && (1..128).contains(&velocity));
            }
        }
    }

    #[test]
    fn notes_within_the_window_make_one_chord() {
        let mut chords = ChordGatherer::new(40);
        let note = |key| ChordNote { channel: 0, key };
        assert_eq!(chords.note_on(note(60), 1_000), None);
        assert_eq!(chords.note_on(note(64), 1_020), None);
        assert_eq!(chords.note_on(note(64), 1_025), None);
        assert_eq!(chords.deadline_ms(), Some(1_040));
        assert_eq!(chords.poll(1_039), None);
        assert_eq!(chords.poll(1_040), Some(vec![note(60), note(64)]));
        assert_eq!(chords.poll(5_000), None);
        // A note after the window starts the next chord and hands back the one before.
        chords.note_on(note(50), 2_000);
        assert_eq!(chords.note_on(note(52), 2_100), Some(vec![note(50)]));
        assert_eq!(chords.poll(2_140), Some(vec![note(52)]));
        // Never more than the cap.
        for key in 0..100 {
            chords.note_on(note(key), 3_000);
        }
        assert_eq!(chords.poll(4_000).unwrap().len(), MAX_CHORD_NOTES);
    }
}
