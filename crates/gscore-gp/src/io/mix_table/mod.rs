//! Guitar Pro reading and writing: mix table.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

mod write;

use crate::io::*;

pub(crate) use write::SongMixTableWriteOps;

pub(crate) trait SongMixTableOps {
    fn read_mix_table_change(&mut self, data: &[u8], seek: &mut usize) -> GpResult<MixTableChange>;
    fn read_mix_table_change_values(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        mtc: &mut MixTableChange,
    ) -> GpResult<()>;
    fn read_mix_table_change_durations(
        &self,
        data: &[u8],
        seek: &mut usize,
        mtc: &mut MixTableChange,
    ) -> GpResult<()>;
    fn read_mix_table_change_flags(
        &self,
        data: &[u8],
        seek: &mut usize,
        mtc: &mut MixTableChange,
    ) -> GpResult<i8>;
    fn read_wah_effect(&self, data: &[u8], seek: &mut usize, flags: i8) -> GpResult<WahEffect>;
}

impl SongMixTableOps for Song {
    fn read_mix_table_change(&mut self, data: &[u8], seek: &mut usize) -> GpResult<MixTableChange> {
        let mut tc = MixTableChange::default();
        self.read_mix_table_change_values(data, seek, &mut tc)?;
        self.read_mix_table_change_durations(data, seek, &mut tc)?;
        if self.version.number >= (4, 0, 0) {
            let flags = self.read_mix_table_change_flags(data, seek, &mut tc)?;
            if self.version.number >= (5, 0, 0) {
                tc.wah = Some(self.read_wah_effect(data, seek, flags)?);
                self.read_rse_instrument_effect(data, seek, &mut tc.rse)?;
            }
        }
        Ok(tc)
    }

    fn read_mix_table_change_values(
        &mut self,
        data: &[u8],
        seek: &mut usize,
        mtc: &mut MixTableChange,
    ) -> GpResult<()> {
        let b = read_signed_byte(data, seek)?;
        if b >= 0 {
            mtc.instrument = Some(MixTableItem {
                value: u16::from(b.to_u8_gp("mix table instrument")?),
                ..Default::default()
            });
        }
        if self.version.number.0 == 5 {
            mtc.rse = self.read_rse_instrument(data, seek)?;
        }
        if self.version.number == (5, 0, 0) {
            skip(seek, 1);
        }
        let b = read_signed_byte(data, seek)?;
        if b >= 0 {
            mtc.volume = Some(MixTableItem {
                value: u16::from(b.to_u8_gp("mix table volume")?),
                ..Default::default()
            });
        }
        let b = read_signed_byte(data, seek)?;
        if b >= 0 {
            mtc.balance = Some(MixTableItem {
                value: u16::from(b.to_u8_gp("mix table balance")?),
                ..Default::default()
            });
        }
        let b = read_signed_byte(data, seek)?;
        if b >= 0 {
            mtc.chorus = Some(MixTableItem {
                value: u16::from(b.to_u8_gp("mix table chorus")?),
                ..Default::default()
            });
        }
        let b = read_signed_byte(data, seek)?;
        if b >= 0 {
            mtc.reverb = Some(MixTableItem {
                value: u16::from(b.to_u8_gp("mix table reverb")?),
                ..Default::default()
            });
        }
        let b = read_signed_byte(data, seek)?;
        if b >= 0 {
            mtc.phaser = Some(MixTableItem {
                value: u16::from(b.to_u8_gp("mix table phaser")?),
                ..Default::default()
            });
        }
        let b = read_signed_byte(data, seek)?;
        if b >= 0 {
            mtc.tremolo = Some(MixTableItem {
                value: u16::from(b.to_u8_gp("mix table tremolo")?),
                ..Default::default()
            });
        }
        if self.version.number >= (5, 0, 0) {
            mtc.tempo_name = read_int_byte_size_string(data, seek)?;
        }
        let b = read_int(data, seek)?;
        if b >= 0 {
            mtc.tempo = Some(MixTableItem {
                // Tempos above 255 BPM are common; `b >= 0` here, so this cannot fail.
                value: u16::try_from(b).unwrap_or(u16::MAX),
                ..Default::default()
            });
        }
        Ok(())
    }

    fn read_mix_table_change_durations(
        &self,
        data: &[u8],
        seek: &mut usize,
        mtc: &mut MixTableChange,
    ) -> GpResult<()> {
        if let Some(ref mut item) = mtc.volume {
            item.duration = non_negative(read_signed_byte(data, seek)?);
        }
        if let Some(ref mut item) = mtc.balance {
            item.duration = non_negative(read_signed_byte(data, seek)?);
        }
        if let Some(ref mut item) = mtc.chorus {
            item.duration = non_negative(read_signed_byte(data, seek)?);
        }
        if let Some(ref mut item) = mtc.reverb {
            item.duration = non_negative(read_signed_byte(data, seek)?);
        }
        if let Some(ref mut item) = mtc.phaser {
            item.duration = non_negative(read_signed_byte(data, seek)?);
        }
        if let Some(ref mut item) = mtc.tremolo {
            item.duration = non_negative(read_signed_byte(data, seek)?);
        }
        if let Some(ref mut item) = mtc.tempo {
            item.duration = non_negative(read_signed_byte(data, seek)?);
            mtc.hide_tempo = false;
            if self.version.number >= (5, 0, 0) {
                mtc.hide_tempo = read_bool(data, seek)?;
            }
        }
        Ok(())
    }

    fn read_mix_table_change_flags(
        &self,
        data: &[u8],
        seek: &mut usize,
        mtc: &mut MixTableChange,
    ) -> GpResult<i8> {
        let flags = read_signed_byte(data, seek)?;
        if let Some(mut e) = mtc.volume.take() {
            e.all_tracks = (flags & 0x01) == 0x01;
            mtc.volume = Some(e);
        }
        if let Some(mut e) = mtc.balance.take() {
            e.all_tracks = (flags & 0x01) == 0x01;
            mtc.balance = Some(e);
        }
        if let Some(mut e) = mtc.chorus.take() {
            e.all_tracks = (flags & 0x01) == 0x01;
            mtc.chorus = Some(e);
        }
        if let Some(mut e) = mtc.reverb.take() {
            e.all_tracks = (flags & 0x01) == 0x01;
            mtc.reverb = Some(e);
        }
        if let Some(mut e) = mtc.phaser.take() {
            e.all_tracks = (flags & 0x01) == 0x01;
            mtc.phaser = Some(e);
        }
        if let Some(mut e) = mtc.tremolo.take() {
            e.all_tracks = (flags & 0x01) == 0x01;
            mtc.tremolo = Some(e);
        }
        if self.version.number >= (5, 0, 0) {
            mtc.use_rse = (flags & 0x40) == 0x40;
        }
        Ok(flags)
    }

    fn read_wah_effect(&self, data: &[u8], seek: &mut usize, flags: i8) -> GpResult<WahEffect> {
        Ok(WahEffect {
            value: read_signed_byte(data, seek)?,
            display: (flags & -0x80) == -0x80,
        })
    }
}

/// A negative byte (meaning "none") as 0.
fn non_negative(value: i8) -> u8 {
    u8::try_from(value).unwrap_or_default()
}

#[cfg(test)]
mod tests;
