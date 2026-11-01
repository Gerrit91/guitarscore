//! Guitar Pro song data: mix table.

// Derived from the `guitarpro` crate by slundi (https://github.com/slundi/guitarpro,
// MIT, see crates/gscore-gp/LICENSE-guitarpro). Changes: docs/guitarpro.md.

use crate::model::*;

/// A mix table item describes a mix parameter, e.g. volume or reverb
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MixTableItem {
    pub value: u8,
    pub duration: u8,
    pub all_tracks: bool,
}

#[allow(dead_code)]
pub const WAH_EFFECT_OFF: i8 = -2;

pub const WAH_EFFECT_NONE: i8 = -1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WahEffect {
    pub value: i8,
    pub display: bool,
}

impl Default for WahEffect {
    fn default() -> Self {
        WahEffect {
            value: WAH_EFFECT_NONE,
            display: false,
        }
    }
}

/// A MixTableChange describes a change in mix parameters
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixTableChange {
    pub instrument: Option<MixTableItem>,
    pub rse: crate::model::RseInstrument,
    pub volume: Option<MixTableItem>,
    pub balance: Option<MixTableItem>,
    pub chorus: Option<MixTableItem>,
    pub reverb: Option<MixTableItem>,
    pub phaser: Option<MixTableItem>,
    pub tremolo: Option<MixTableItem>,
    pub tempo_name: String,
    pub tempo: Option<MixTableItem>,
    pub hide_tempo: bool,
    pub wah: Option<WahEffect>,
    pub use_rse: bool,
}

impl Default for MixTableChange {
    fn default() -> Self {
        MixTableChange {
            instrument: None,
            rse: crate::model::RseInstrument::default(),
            volume: None,
            balance: None,
            chorus: None,
            reverb: None,
            phaser: None,
            tremolo: None,
            tempo_name: String::new(),
            tempo: None,
            hide_tempo: true,
            wah: None,
            use_rse: false,
        }
    }
}

impl MixTableChange {
    pub fn is_just_wah(&self) -> bool {
        self.instrument.is_none()
            && self.volume.is_none()
            && self.balance.is_none()
            && self.chorus.is_none()
            && self.reverb.is_none()
            && self.phaser.is_none()
            && self.tremolo.is_none()
            && self.tempo.is_none()
            && self.wah.is_none()
    }
}
