//! Guitar Pro reading and writing: midi.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;

pub trait SongMidiOps {
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
        *seek += 2; //Backward compatibility with version 3.0
        Ok(c)
    }

    /// Read MIDI channel. MIDI channel in Guitar Pro is represented by two integers. First is zero-based number of channel, second is zero-based number of channel used for effects.
    fn read_channel(&mut self, data: &[u8], seek: &mut usize) -> GpResult<usize> {
        //TODO: fixme for writing
        let index = read_int(data, seek)? - 1;
        let effect_channel = read_int(data, seek)? - 1;
        let channels_len = self.channels.len().to_i32_gp("channels length")?;
        if 0 <= index && index < channels_len {
            let idx = index.to_usize_gp("channel index")?;
            if self.channels[idx].instrument < 0 {
                self.channels[idx].instrument = 0;
            }
            if !self.channels[idx].is_percussion_channel() {
                // Some legacy files store junk in the second int of the
                // channel pair (GP2 upgraded to GP3, byte-mis-alignment). Cap
                // to u8 rather than fail.
                self.channels[idx].effect_channel = effect_channel.clamp(0, 255) as u8;
            }
            return Ok(idx);
        }
        // Malformed file: raw index is out of range. Clamp to a safe slot so
        // callers indexing `self.channels[…]` don't panic. `channels` is
        // always initialised with at least one entry before this is called.
        Ok(0)
    }

    fn write_midi_channels(&self, data: &mut Vec<u8>) {
        for i in 0..self.channels.len() {
            if self.channels[i].is_percussion_channel() && self.channels[i].instrument == 0 {
                write_i32(data, -1);
            } else {
                write_i32(data, self.channels[i].instrument);
            }
            write_signed_byte(data, self.channels[i].volume);
            write_signed_byte(data, self.channels[i].balance);
            write_signed_byte(data, self.channels[i].chorus);
            write_signed_byte(data, self.channels[i].reverb);
            write_signed_byte(data, self.channels[i].phaser);
            write_signed_byte(data, self.channels[i].tremolo);
            write_placeholder_default(data, 2); //Backward compatibility with version 3.0
        }
    }
}
