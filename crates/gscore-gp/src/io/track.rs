//! Guitar Pro reading and writing: track.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;

pub trait SongTrackOps {
    fn read_tracks(&mut self, data: &[u8], seek: &mut usize, track_count: usize) -> GpResult<()>;
    fn read_tracks_v5(&mut self, data: &[u8], seek: &mut usize, track_count: usize)
    -> GpResult<()>;
    fn read_track(&mut self, data: &[u8], seek: &mut usize, number: usize) -> GpResult<()>;
    fn read_track_v5(&mut self, data: &[u8], seek: &mut usize, number: usize) -> GpResult<()>;
    fn write_tracks(&self, data: &mut Vec<u8>, version: &(u8, u8, u8)) -> GpResult<()>;
    fn write_track(&self, data: &mut Vec<u8>, number: usize) -> GpResult<()>;
    fn write_track_v5(
        &self,
        data: &mut Vec<u8>,
        number: usize,
        version: &(u8, u8, u8),
    ) -> GpResult<()>;
}

impl SongTrackOps for Song {
    /// Read tracks. The tracks are written one after another, their number having been specified previously in :meth:`GP3File.readSong`.
    /// - `track_count`: number of tracks to expect.
    fn read_tracks(&mut self, data: &[u8], seek: &mut usize, track_count: usize) -> GpResult<()> {
        for i in 0..track_count {
            self.read_track(data, seek, i)?;
        }
        Ok(())
    }

    fn read_tracks_v5(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        track_count: usize,
    ) -> GpResult<()> {
        for i in 0..track_count {
            self.read_track_v5(data, seek, i)?;
        }
        *seek += if self.version.number == (5, 0, 0) {
            2
        } else {
            1
        };
        Ok(())
    }

    /// Read a  track. The first byte is the track's flags. It presides the track's attributes:
    ///
    /// | **bit 7 to 3** | **bit 2**   | **bit 1**                | **bit 0**   |
    /// |----------------|-------------|--------------------------|-------------|
    /// | Blank bits     | Banjo track | 12 stringed guitar track | Drums track |
    ///
    /// Flags are followed by:
    ///
    /// * **Name**: `string`. A 40 characters long string containing the track's name.
    /// * **Number of strings**: `integer`. An integer equal to the number of strings of the track.
    /// * **Tuning of the strings**: Table of integers. The tuning of the strings is stored as a 7-integers table, the "Number of strings" first integers being really used. The strings are stored from the highest to the lowest.
    /// * **Port**: `integer`. The number of the MIDI port used.
    /// * **Channel**: `integer`. The number of the MIDI channel used. The channel 10 is the drums channel.
    /// * **ChannelE**: `integer`. The number of the MIDI channel used for effects.
    /// * **Number of frets**: `integer`. The number of frets of the instrument.
    /// * **Height of the capo**: `integer`. The number of the fret on which a capo is present. If no capo is used, the value is `0x00000000`.
    /// * **Track's color**: `color`. The track's displayed color in Guitar Pro.
    fn read_track(&mut self, data: &[u8], seek: &mut usize, number: usize) -> GpResult<()> {
        let mut track = Track {
            number: number.to_i32_gp("track number")?,
            ..Default::default()
        };
        //read the flag
        let flags = read_byte(data, seek)?;
        track.percussion_track = (flags & 0x01) == 0x01; //Drums track
        track.twelve_stringed_guitar_track = (flags & 0x02) == 0x02; //12 stringed guitar track
        track.banjo_track = (flags & 0x04) == 0x04; //Banjo track

        track.name = read_byte_size_string(data, seek, 40)?;
        // Clamp string_count: valid range is 0..=7 (Guitar Pro caps at 7),
        // but malformed files may store larger ints.
        let string_count = (read_int(data, seek)?.clamp(0, 7)) as u8;
        track.strings.clear();
        for i in 0..7i8 {
            // Truncate rather than fail: MIDI note pitches fit in i8; garbage
            // slots (0x00_00_7F_00, etc.) just mean an unused string.
            let i_tuning = read_int(data, seek)? as i8;
            if string_count as i8 > i {
                track.strings.push((i + 1, i_tuning));
            }
        }
        track.port = read_int(data, seek)?.clamp(0, 255) as u8;
        let index = self.read_channel(data, seek)?;
        if self.channels[index].channel == 9 {
            track.percussion_track = true;
        }
        track.fret_count = read_int(data, seek)?.clamp(0, 255) as u8;
        track.offset = read_int(data, seek)?;
        track.color = read_color(data, seek)?;
        self.tracks.push(track);
        Ok(())
    }

    /// Read track. If it's Guitar Pro 5.0 format and track is first then one blank byte is read. Then go track's flags. It presides the track's attributes:
    /// - *0x01*: drums track
    /// - *0x02*: 12 stringed guitar track
    /// - *0x04*: banjo track
    /// - *0x08*: track visibility
    /// - *0x10*: track is soloed
    /// - *0x20*: track is muted
    /// - *0x40*: RSE is enabled
    /// - *0x80*: show tuning in the header of the sheet.
    ///
    /// Flags are followed by:
    /// - Name: `String`. A 40 characters long string containing the track's name.
    /// - Number of strings: :ref:`int`. An integer equal to the number of strings of the track.
    /// - Tuning of the strings: `Table of integers`. The tuning of the strings is stored as a 7-integers table, the "Number of strings" first integers being really used. The strings are stored from the highest to the lowest.
    /// - Port: :ref:`int`. The number of the MIDI port used.
    /// - Channel. See `GP3File.readChannel`.
    /// - Number of frets: :ref:`int`. The number of frets of the instrument.
    /// - Height of the capo: :ref:`int`. The number of the fret on which a capo is set. If no capo is used, the value is 0.
    /// - Track's color. The track's displayed color in Guitar Pro.
    ///
    /// The properties are followed by second set of flags stored in a :ref:`short`:
    /// - *0x0001*: show tablature
    /// - *0x0002*: show standard notation
    /// - *0x0004*: chord diagrams are below standard notation
    /// - *0x0008*: show rhythm with tab
    /// - *0x0010*: force horizontal beams
    /// - *0x0020*: force channels 11 to 16
    /// - *0x0040*: diagram list on top of the score
    /// - *0x0080*: diagrams in the score
    /// - *0x0200*: auto let-ring
    /// - *0x0400*: auto brush
    /// - *0x0800*: extend rhythmic inside the tab
    ///
    /// Then follow:
    /// - Auto accentuation: :ref:`byte`. See :class:`guitarpro.models.Accentuation`.
    /// - MIDI bank: :ref:`byte`.
    /// - Track RSE. See `readTrackRSE`.
    fn read_track_v5(&mut self, data: &[u8], seek: &mut usize, number: usize) -> GpResult<()> {
        let mut track = Track {
            number: number.to_i32_gp("track number")?,
            ..Default::default()
        };
        if number == 0 || self.version.number == (5, 0, 0) {
            *seek += 1;
        } //always 0 //missing 3 skips?
        let flags1 = read_byte(data, seek)?;
        track.percussion_track = (flags1 & 0x01) == 0x01;
        track.banjo_track = (flags1 & 0x02) == 0x02;
        track.visible = (flags1 & 0x04) == 0x04;
        track.solo = (flags1 & 0x10) == 0x10;
        track.mute = (flags1 & 0x20) == 0x20;
        track.use_rse = (flags1 & 0x40) == 0x40;
        track.indicate_tuning = (flags1 & 0x80) == 0x80;
        track.name = read_byte_size_string(data, seek, 40)?;
        let sc = read_int(data, seek)?;
        let string_count = sc.to_u8_gp("string count")?;
        track.strings.clear();
        for i in 0i8..7i8 {
            let i_tuning = read_int(data, seek)?.to_i8_gp("string tuning")?;
            if string_count.to_i8_gp("string count")? > i {
                track.strings.push((i + 1, i_tuning));
            }
        }
        track.port = read_int(data, seek)?.clamp(0, 255) as u8;
        self.read_channel(data, seek)?;
        if self.channels[number].channel == 9 {
            track.percussion_track = true;
        }
        track.fret_count = read_int(data, seek)?.clamp(0, 255) as u8;
        track.offset = read_int(data, seek)?;
        track.color = read_color(data, seek)?;

        let flags2 = read_short(data, seek)?;
        track.settings.tablature = (flags2 & 0x0001) == 0x0001;
        track.settings.notation = (flags2 & 0x0002) == 0x0002;
        track.settings.diagram_are_below = (flags2 & 0x0004) == 0x0004;
        track.settings.show_rhythm = (flags2 & 0x0008) == 0x0008;
        track.settings.force_horizontal = (flags2 & 0x0010) == 0x0010;
        track.settings.force_channels = (flags2 & 0x0020) == 0x0020;
        track.settings.diagram_list = (flags2 & 0x0040) == 0x0040;
        track.settings.diagram_in_score = (flags2 & 0x0080) == 0x0080;
        //0x0100 ???
        track.settings.auto_let_ring = (flags2 & 0x0200) == 0x0200;
        track.settings.auto_brush = (flags2 & 0x0400) == 0x0400;
        track.settings.extend_rhythmic = (flags2 & 0x0800) == 0x0800;

        track.rse.auto_accentuation = get_accentuation(read_byte(data, seek)?)?;
        self.channels[number].bank = read_byte(data, seek)?;
        self.read_track_rse(data, seek, &mut track)?;
        self.tracks.push(track);
        Ok(())
    }

    fn write_tracks(&self, data: &mut Vec<u8>, version: &(u8, u8, u8)) -> GpResult<()> {
        for i in 0..self.tracks.len() {
            if version.0 < 5 {
                self.write_track(data, i)?;
            } else {
                self.write_track_v5(data, i, version)?;
            }
        }
        if version.0 == 5 {
            write_placeholder_default(data, if version == &(5, 0, 0) { 2 } else { 1 });
        }
        Ok(())
    }
    fn write_track(&self, data: &mut Vec<u8>, number: usize) -> GpResult<()> {
        let mut flags = 0x00;
        if self.tracks[number].percussion_track {
            flags |= 0x01;
        }
        if self.tracks[number].twelve_stringed_guitar_track {
            flags |= 0x02;
        }
        if self.tracks[number].banjo_track {
            flags |= 0x04;
        }
        write_byte(data, flags);
        write_byte_size_string(data, &self.tracks[number].name);
        write_placeholder_default(data, 40 - self.tracks[number].name.len());
        write_i32(
            data,
            self.tracks[number].strings.len().to_i32_gp("strings len")?,
        );
        for i in 0..7usize {
            let mut tuning = 0i8;
            if i < self.tracks[number].strings.len() {
                tuning = self.tracks[number].strings[i].1;
            }
            write_i32(data, tuning.to_i32_gp("string tuning")?);
        }
        write_i32(data, self.tracks[number].port.to_i32_gp("port")?);
        //write channel
        write_i32(
            data,
            self.channels[self.tracks[number].channel_index]
                .channel
                .to_i32_gp("channel")?
                + 1,
        );
        write_i32(
            data,
            self.channels[self.tracks[number].channel_index]
                .effect_channel
                .to_i32_gp("effect channel")?
                + 1,
        );
        //end write channel
        write_i32(
            data,
            self.tracks[number].fret_count.to_i32_gp("fret count")?,
        );
        write_i32(data, self.tracks[number].offset);
        write_color(data, self.tracks[number].color);
        Ok(())
    }
    fn write_track_v5(
        &self,
        data: &mut Vec<u8>,
        number: usize,
        version: &(u8, u8, u8),
    ) -> GpResult<()> {
        if number == 0 || version == &(5, 0, 0) {
            write_placeholder_default(data, 1);
        }
        let mut flags1 = 0u8;
        if self.tracks[number].percussion_track {
            flags1 |= 0x01;
        }
        if self.tracks[number].twelve_stringed_guitar_track {
            flags1 |= 0x02;
        }
        if self.tracks[number].banjo_track {
            flags1 |= 0x04;
        }
        if self.tracks[number].visible {
            flags1 |= 0x08;
        }
        if self.tracks[number].solo {
            flags1 |= 0x10;
        }
        if self.tracks[number].mute {
            flags1 |= 0x20;
        }
        if self.tracks[number].use_rse {
            flags1 |= 0x40;
        }
        if self.tracks[number].indicate_tuning {
            flags1 |= 0x80;
        }
        write_byte(data, flags1);

        write_byte_size_string(data, &self.tracks[number].name);
        write_placeholder_default(data, 40 - self.tracks[number].name.len());

        write_i32(
            data,
            self.tracks[number].strings.len().to_i32_gp("strings len")?,
        );
        for i in 0..7usize {
            let mut tuning = 0i8;
            if i < self.tracks[number].strings.len() {
                tuning = self.tracks[number].strings[i].1;
            }
            write_i32(data, tuning.to_i32_gp("string tuning")?);
        }
        write_i32(data, self.tracks[number].port.to_i32_gp("port")?);
        //write channel
        write_i32(
            data,
            self.channels[self.tracks[number].channel_index]
                .channel
                .to_i32_gp("channel")?
                + 1,
        );
        write_i32(
            data,
            self.channels[self.tracks[number].channel_index]
                .effect_channel
                .to_i32_gp("effect channel")?
                + 1,
        );
        //end write channel
        write_i32(
            data,
            self.tracks[number].fret_count.to_i32_gp("fret count")?,
        );
        write_i32(data, self.tracks[number].offset);
        write_color(data, self.tracks[number].color);

        let mut flags2 = 0i16;
        if self.tracks[number].settings.tablature {
            flags2 |= 0x0001;
        }
        if self.tracks[number].settings.notation {
            flags2 |= 0x0002;
        }
        if self.tracks[number].settings.diagram_are_below {
            flags2 |= 0x0004;
        }
        if self.tracks[number].settings.show_rhythm {
            flags2 |= 0x0008;
        }
        if self.tracks[number].settings.force_horizontal {
            flags2 |= 0x0010;
        }
        if self.tracks[number].settings.force_channels {
            flags2 |= 0x0020;
        }
        if self.tracks[number].settings.diagram_list {
            flags2 |= 0x0040;
        }
        if self.tracks[number].settings.diagram_in_score {
            flags2 |= 0x0080;
        }
        if self.tracks[number].settings.auto_let_ring {
            flags2 |= 0x0200;
        }
        if self.tracks[number].settings.auto_brush {
            flags2 |= 0x0400;
        }
        if self.tracks[number].settings.extend_rhythmic {
            flags2 |= 0x0800;
        }
        write_i16(data, flags2);

        write_byte(
            data,
            from_accentuation(&self.tracks[number].rse.auto_accentuation),
        );
        write_byte(data, self.channels[self.tracks[number].channel_index].bank);
        self.write_track_rse(data, &self.tracks[number].rse, version);
        Ok(())
    }
}
