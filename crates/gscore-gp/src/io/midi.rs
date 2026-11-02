//! Guitar Pro reading and writing: midi.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;

pub(crate) trait SongMidiOps {
    fn read_midi_channels(&mut self, data: &[u8], seek: &mut usize) -> GpResult<()>;
    fn read_midi_channel(
        &self,
        data: &[u8],
        seek: &mut usize,
        channel: u8,
    ) -> GpResult<MidiChannel>;
    fn read_channel(&mut self, data: &[u8], seek: &mut usize) -> GpResult<usize>;
    fn write_midi_channels(&self, data: &mut Vec<u8>);
}

impl SongMidiOps for Song {
    /// Read all the MIDI channels
    fn read_midi_channels(&mut self, data: &[u8], seek: &mut usize) -> GpResult<()> {
        for i in 0u8..64u8 {
            self.channels.push(self.read_midi_channel(data, seek, i)?);
        }
        Ok(())
    }
    /// Read MIDI channels. Guitar Pro format provides 64 channels (4 MIDI ports by 16 hannels), the channels are stored in this order:
    ///`port1/channel1`, `port1/channel2`, ..., `port1/channel16`, `port2/channel1`, ..., `port4/channel16`.
    ///
    /// Each channel has the following form:
    ///
    /// * **Instrument**: `int`
    /// * **Volume**: `byte`
    /// * **Balance**: `byte`
    /// * **Chorus**: `byte`
    /// * **Reverb**: `byte`
    /// * **Phaser**: `byte`
    /// * **Tremolo**: `byte`
    /// * **blank1**: `byte` => Backward compatibility with version 3.0
    /// * **blank2**: `byte` => Backward compatibility with version 3.0
    fn read_midi_channel(
        &self,
        data: &[u8],
        seek: &mut usize,
        channel: u8,
    ) -> GpResult<MidiChannel> {
        let instrument = read_int(data, seek)?;
        let mut c = MidiChannel {
            channel,
            effect_channel: channel,
            ..Default::default()
        };
        c.volume = read_signed_byte(data, seek)?;
        c.balance = read_signed_byte(data, seek)?;
        c.chorus = read_signed_byte(data, seek)?;
        c.reverb = read_signed_byte(data, seek)?;
        c.phaser = read_signed_byte(data, seek)?;
        c.tremolo = read_signed_byte(data, seek)?;
        c.set_instrument(instrument);
        skip(seek, 2); // Backward compatibility with version 3.0
        Ok(c)
    }

    /// Read MIDI channel. MIDI channel in Guitar Pro is represented by two integers. First is zero-based number of channel, second is zero-based number of channel used for effects.
    fn read_channel(&mut self, data: &[u8], seek: &mut usize) -> GpResult<usize> {
        // The file counts channels from 1.
        let index = read_int(data, seek)?.saturating_sub(1);
        let effect_channel = read_int(data, seek)?.saturating_sub(1);
        let Some(idx) = usize::try_from(index)
            .ok()
            .filter(|&i| i < self.channels.len())
        else {
            // Malformed file: the index is out of range. Use the first channel, which
            // always exists when this is called.
            return Ok(0);
        };
        let channel = item_mut(&mut self.channels, idx, "MIDI channel")?;
        if channel.instrument < 0 {
            channel.instrument = 0;
        }
        if !channel.is_percussion_channel() {
            // Some legacy files store junk in the second int of the channel pair (GP2
            // upgraded to GP3, byte misalignment). Clamp to a byte rather than fail.
            channel.effect_channel = clamp_u8(effect_channel);
        }
        Ok(idx)
    }

    fn write_midi_channels(&self, data: &mut Vec<u8>) {
        for channel in &self.channels {
            if channel.is_percussion_channel() && channel.instrument == 0 {
                write_i32(data, -1);
            } else {
                write_i32(data, channel.instrument);
            }
            for value in [
                channel.volume,
                channel.balance,
                channel.chorus,
                channel.reverb,
                channel.phaser,
                channel.tremolo,
            ] {
                write_signed_byte(data, value);
            }
            write_placeholder_default(data, 2); // Backward compatibility with version 3.0
        }
    }
}
