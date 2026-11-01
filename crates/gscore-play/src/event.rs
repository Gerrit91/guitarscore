//! MIDI channel messages, as the sequencer produces and backends send them.

/// A MIDI channel (0-15; 9 is the General MIDI percussion channel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Channel(u8);

impl Channel {
    /// The General MIDI percussion channel ("channel 10").
    pub const PERCUSSION: Channel = Channel(9);

    /// Channel `index` (0-15).
    pub fn new(index: u8) -> Option<Self> {
        (index < 16).then_some(Channel(index))
    }

    /// The 0-based channel number.
    pub fn get(self) -> u8 {
        self.0
    }
}

/// MIDI controller numbers used by the sequencer.
pub mod controller {
    /// Channel volume.
    pub const VOLUME: u8 = 7;
    /// Pan.
    pub const PAN: u8 = 10;
    /// All sound off: silences the channel at once.
    pub const ALL_SOUND_OFF: u8 = 120;
    /// Reset all controllers.
    pub const RESET_ALL: u8 = 121;
    /// All notes off.
    pub const ALL_NOTES_OFF: u8 = 123;
}

/// One MIDI channel message. Data values are 0-127.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MidiEvent {
    /// Start a note.
    NoteOn {
        /// The channel.
        channel: Channel,
        /// The MIDI note number.
        key: u8,
        /// How hard (1-127).
        velocity: u8,
    },
    /// End a note.
    NoteOff {
        /// The channel.
        channel: Channel,
        /// The MIDI note number.
        key: u8,
    },
    /// Select the instrument (General MIDI program, 0-based).
    ProgramChange {
        /// The channel.
        channel: Channel,
        /// The program.
        program: u8,
    },
    /// Set a controller (volume, pan, ...).
    Control {
        /// The channel.
        channel: Channel,
        /// The controller number (see [`controller`]).
        controller: u8,
        /// The value.
        value: u8,
    },
}

impl MidiEvent {
    /// The message as MIDI bytes (status byte plus one or two data bytes).
    pub fn bytes(&self) -> Vec<u8> {
        let data = |value: u8| value & 0x7F;
        match *self {
            MidiEvent::NoteOn {
                channel,
                key,
                velocity,
            } => vec![0x90 | channel.get(), data(key), data(velocity)],
            MidiEvent::NoteOff { channel, key } => vec![0x80 | channel.get(), data(key), 0],
            MidiEvent::ProgramChange { channel, program } => {
                vec![0xC0 | channel.get(), data(program)]
            }
            MidiEvent::Control {
                channel,
                controller,
                value,
            } => vec![0xB0 | channel.get(), data(controller), data(value)],
        }
    }

    /// Whether this starts or ends a note (as opposed to setup such as program changes).
    pub fn is_note(&self) -> bool {
        matches!(self, MidiEvent::NoteOn { .. } | MidiEvent::NoteOff { .. })
    }

    /// The messages that silence `channels` at once (for stop, pause and seek).
    pub fn silence(channels: impl IntoIterator<Item = Channel>) -> Vec<MidiEvent> {
        channels
            .into_iter()
            .flat_map(|channel| {
                [controller::ALL_NOTES_OFF, controller::ALL_SOUND_OFF].map(|controller| {
                    MidiEvent::Control {
                        channel,
                        controller,
                        value: 0,
                    }
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_encode_status_and_data_bytes() {
        let channel = Channel::new(2).unwrap();
        let on = MidiEvent::NoteOn {
            channel,
            key: 60,
            velocity: 200,
        };
        assert_eq!(on.bytes(), [0x92, 60, 200 & 0x7F]);
        assert_eq!(
            MidiEvent::NoteOff { channel, key: 60 }.bytes(),
            [0x82, 60, 0]
        );
        let program = MidiEvent::ProgramChange {
            channel: Channel::PERCUSSION,
            program: 25,
        };
        assert_eq!(program.bytes(), [0xC9, 25]);
        assert_eq!(Channel::new(16), None);
        assert_eq!(MidiEvent::silence([channel]).len(), 2);
    }
}
