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

/// A pitch bend with no offset: the middle of the 14-bit range.
pub(crate) const PITCH_BEND_CENTER: u16 = 8192;

/// MIDI controller numbers used by the sequencer.
pub mod controller {
    /// Channel volume.
    pub const VOLUME: u8 = 7;
    /// Data entry, most significant byte (for RPN values).
    pub const DATA_ENTRY_MSB: u8 = 6;
    /// Pan.
    pub const PAN: u8 = 10;
    /// Expression: a share of the channel volume (a damped note's channel drops it to 0).
    pub const EXPRESSION: u8 = 11;
    /// Reverb send (effects 1 depth).
    pub const REVERB: u8 = 91;
    /// Chorus send (effects 3 depth).
    pub const CHORUS: u8 = 93;
    /// Data entry, least significant byte (for RPN values).
    pub const DATA_ENTRY_LSB: u8 = 38;
    /// Registered parameter number, least significant byte.
    pub const RPN_LSB: u8 = 100;
    /// Registered parameter number, most significant byte.
    pub const RPN_MSB: u8 = 101;
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
    /// Set a controller (volume, pan, RPN, ...).
    Control {
        /// The channel.
        channel: Channel,
        /// The controller number (see [`controller`]).
        controller: u8,
        /// The value.
        value: u8,
    },
    /// Bend the pitch of every note on the channel. 8192 is no bend; the range is set with
    /// RPN 0,0 (pitch bend sensitivity) before a borrowed channel plays.
    PitchBend {
        /// The channel.
        channel: Channel,
        /// The 14-bit bend value (0-16383, [`PITCH_BEND_CENTER`] at the middle).
        value: u16,
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
            MidiEvent::PitchBend { channel, value } => {
                let value = value.min(0x3FFF);
                vec![
                    0xE0 | channel.get(),
                    u8::try_from(value & 0x7F).unwrap_or(0),
                    u8::try_from((value >> 7) & 0x7F).unwrap_or(0),
                ]
            }
        }
    }

    /// Whether this starts or ends a note (as opposed to setup such as program changes).
    pub fn is_note(&self) -> bool {
        matches!(self, MidiEvent::NoteOn { .. } | MidiEvent::NoteOff { .. })
    }

    /// The channel this message goes to.
    pub fn channel(&self) -> Channel {
        match *self {
            MidiEvent::NoteOn { channel, .. }
            | MidiEvent::NoteOff { channel, .. }
            | MidiEvent::ProgramChange { channel, .. }
            | MidiEvent::Control { channel, .. }
            | MidiEvent::PitchBend { channel, .. } => channel,
        }
    }

    /// The messages that silence `channels` at once (for stop, pause and seek) and leave
    /// them ready for a fresh start: no notes, no stale pitch bend or controller values.
    pub fn silence(channels: impl IntoIterator<Item = Channel>) -> Vec<MidiEvent> {
        channels
            .into_iter()
            .flat_map(|channel| {
                [
                    MidiEvent::Control {
                        channel,
                        controller: controller::ALL_NOTES_OFF,
                        value: 0,
                    },
                    MidiEvent::Control {
                        channel,
                        controller: controller::ALL_SOUND_OFF,
                        value: 0,
                    },
                    // Do not reset controllers: a pause keeps program, volume and pan for
                    // the resume. Only a stale bend has to go.
                    MidiEvent::PitchBend {
                        channel,
                        value: PITCH_BEND_CENTER,
                    },
                ]
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
        let silence = MidiEvent::silence([channel]);
        assert_eq!(silence.len(), 3);
        assert_eq!(
            silence.last(),
            Some(&MidiEvent::PitchBend {
                channel,
                value: PITCH_BEND_CENTER
            }),
            "silence also resets a stale bend"
        );
    }

    #[test]
    fn pitch_bend_is_a_fourteen_bit_little_endian_value() {
        let channel = Channel::new(2).unwrap();
        let center = MidiEvent::PitchBend {
            channel,
            value: PITCH_BEND_CENTER,
        };
        assert_eq!(center.bytes(), [0xE2, 0x00, 0x40]);
        assert_eq!(center.channel(), channel);
        let maximum = MidiEvent::PitchBend {
            channel,
            value: 0x3FFF,
        };
        assert_eq!(maximum.bytes(), [0xE2, 0x7F, 0x7F]);
        let clamped = MidiEvent::PitchBend {
            channel,
            value: u16::MAX,
        };
        assert_eq!(clamped.bytes(), [0xE2, 0x7F, 0x7F]);
    }
}
