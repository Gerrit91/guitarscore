//! Writing mix table changes (GP3-5).

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::io::*;

pub(crate) trait SongMixTableWriteOps {
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

impl SongMixTableWriteOps for Song {
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

/// A duration as the signed byte the file stores (at most 127).
fn signed(value: u8) -> i8 {
    i8::try_from(value).unwrap_or(i8::MAX)
}

/// A mix value as the signed byte the file stores: -1 for no change, at most 127.
fn value_or_none(item: Option<&MixTableItem>) -> i8 {
    item.map_or(-1, |i| i8::try_from(i.value).unwrap_or(i8::MAX))
}
