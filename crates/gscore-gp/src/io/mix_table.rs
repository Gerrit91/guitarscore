//! Guitar Pro reading and writing: mix table.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;

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
    fn write_mix_table_change(
        &self,
        data: &mut Vec<u8>,
        mix_table_change: &Option<MixTableChange>,
        version: &(u8, u8, u8),
    );
    fn write_mix_table_change_values(
        &self,
        data: &mut Vec<u8>,
        mix_table_change: &MixTableChange,
        version: &(u8, u8, u8),
    );
    fn write_mix_table_change_durations(
        &self,
        data: &mut Vec<u8>,
        mix_table_change: &MixTableChange,
        version: &(u8, u8, u8),
    );
    fn write_mix_table_change_flags_v4(
        &self,
        data: &mut Vec<u8>,
        mix_table_change: &MixTableChange,
    );
    fn write_mix_table_change_flags_v5(
        &self,
        data: &mut Vec<u8>,
        mix_table_change: &MixTableChange,
    );
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

    fn write_mix_table_change(
        &self,
        data: &mut Vec<u8>,
        mix_table_change: &Option<MixTableChange>,
        version: &(u8, u8, u8),
    ) {
        if let Some(mtc) = mix_table_change {
            self.write_mix_table_change_values(data, mtc, version);
            self.write_mix_table_change_durations(data, mtc, version);
            if version.0 == 4 {
                self.write_mix_table_change_flags_v4(data, mtc);
            }
            if version.0 == 5 {
                self.write_mix_table_change_flags_v5(data, mtc);
                if let Some(w) = &mtc.wah {
                    write_signed_byte(data, w.value);
                } else {
                    write_signed_byte(data, WAH_EFFECT_NONE);
                }
                if *version > (5, 0, 0) {
                    self.write_rse_instrument_effect(data, &mtc.rse);
                }
            }
        }
    }

    fn write_mix_table_change_values(
        &self,
        data: &mut Vec<u8>,
        mix_table_change: &MixTableChange,
        version: &(u8, u8, u8),
    ) {
        write_signed_byte(data, value_or_none(mix_table_change.instrument.as_ref()));
        if version.0 >= 5 {
            self.write_rse_instrument(data, &mix_table_change.rse, version);
        }
        if version == &(5, 0, 0) {
            write_placeholder_default(data, 1);
        }
        for item in [
            &mix_table_change.volume,
            &mix_table_change.balance,
            &mix_table_change.chorus,
            &mix_table_change.reverb,
            &mix_table_change.phaser,
            &mix_table_change.tremolo,
        ] {
            write_signed_byte(data, value_or_none(item.as_ref()));
        }
        if version.0 >= 5 {
            write_int_byte_size_string(data, &mix_table_change.tempo_name);
        }
        write_i32(
            data,
            mix_table_change
                .tempo
                .as_ref()
                .map_or(-1, |t| i32::from(t.value)),
        );
    }

    fn write_mix_table_change_durations(
        &self,
        data: &mut Vec<u8>,
        mix_table_change: &MixTableChange,
        version: &(u8, u8, u8),
    ) {
        for item in [
            &mix_table_change.volume,
            &mix_table_change.balance,
            &mix_table_change.chorus,
            &mix_table_change.reverb,
            &mix_table_change.phaser,
            &mix_table_change.tremolo,
        ]
        .into_iter()
        .flatten()
        {
            write_signed_byte(data, signed(item.duration));
        }
        if let Some(i) = &mix_table_change.tempo {
            write_signed_byte(data, signed(i.duration));
            if version >= &(5, 0, 0) {
                write_bool(data, mix_table_change.hide_tempo);
            }
        }
    }

    fn write_mix_table_change_flags_v4(
        &self,
        data: &mut Vec<u8>,
        mix_table_change: &MixTableChange,
    ) {
        let mut flags = 0i8;
        if let Some(i) = &mix_table_change.volume
            && i.all_tracks
        {
            flags |= 0x01;
        }
        if let Some(i) = &mix_table_change.balance
            && i.all_tracks
        {
            flags |= 0x02;
        }
        if let Some(i) = &mix_table_change.chorus
            && i.all_tracks
        {
            flags |= 0x04;
        }
        if let Some(i) = &mix_table_change.reverb
            && i.all_tracks
        {
            flags |= 0x08;
        }
        if let Some(i) = &mix_table_change.phaser
            && i.all_tracks
        {
            flags |= 0x10;
        }
        if let Some(i) = &mix_table_change.tremolo
            && i.all_tracks
        {
            flags |= 0x20;
        }
        write_signed_byte(data, flags);
    }

    fn write_mix_table_change_flags_v5(
        &self,
        data: &mut Vec<u8>,
        mix_table_change: &MixTableChange,
    ) {
        let mut flags = 0u8;
        if let Some(i) = &mix_table_change.volume
            && i.all_tracks
        {
            flags |= 0x01;
        }
        if let Some(i) = &mix_table_change.balance
            && i.all_tracks
        {
            flags |= 0x02;
        }
        if let Some(i) = &mix_table_change.chorus
            && i.all_tracks
        {
            flags |= 0x04;
        }
        if let Some(i) = &mix_table_change.reverb
            && i.all_tracks
        {
            flags |= 0x08;
        }
        if let Some(i) = &mix_table_change.phaser
            && i.all_tracks
        {
            flags |= 0x10;
        }
        if let Some(i) = &mix_table_change.tremolo
            && i.all_tracks
        {
            flags |= 0x20;
        }
        if mix_table_change.use_rse {
            flags |= 0x40;
        }
        if let Some(w) = &mix_table_change.wah
            && w.display
        {
            flags |= 0x80;
        }
        write_byte(data, flags);
    }
}

#[cfg(test)]
mod tests {
    use crate::io::Song;
    use crate::model::{Beat, MixTableChange, MixTableItem, Voice};

    #[test]
    fn tempo_changes_above_255_bpm_survive_gp5() {
        // The reader used to clamp tempo changes to 255 BPM (a u8), losing fast tempos.
        let mut song = Song::default();
        song.tracks.push(Default::default());
        song.measure_headers.push(Default::default());
        let beat = Beat {
            effect: crate::model::BeatEffects {
                mix_table_change: Some(MixTableChange {
                    tempo: Some(MixTableItem {
                        value: 280,
                        ..MixTableItem::default()
                    }),
                    ..MixTableChange::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut measure = crate::model::Measure::default();
        measure.voices = vec![Voice {
            beats: vec![beat],
            ..Voice::default()
        }];
        song.tracks[0].measures.push(measure);

        let bytes = song.write((5, 1, 0), None).expect("written");
        let mut reread = Song::default();
        reread.read_gp5(&bytes).expect("read back");
        let tempo = reread.tracks[0].measures[0].voices[0].beats[0]
            .effect
            .mix_table_change
            .as_ref()
            .and_then(|m| m.tempo.as_ref())
            .map(|t| t.value);
        assert_eq!(tempo, Some(280));
    }
}

/// A negative byte (meaning "none") as 0.
fn non_negative(value: i8) -> u8 {
    u8::try_from(value).unwrap_or_default()
}

/// A duration as the signed byte the file stores (at most 127).
fn signed(value: u8) -> i8 {
    i8::try_from(value).unwrap_or(i8::MAX)
}

/// A mix value as the signed byte the file stores: -1 for no change, at most 127.
fn value_or_none(item: Option<&MixTableItem>) -> i8 {
    item.map_or(-1, |i| i8::try_from(i.value).unwrap_or(i8::MAX))
}
