//! The effect chains of the modelled strings as the settings file keeps them: per kind of
//! string, five places after the strings, each empty or holding a block with its knobs
//! (and, for an impulse response, the WAV file it was loaded from).

use std::path::PathBuf;

use gscore_guitar::{BLOCKS, BlockSettings, ImpulseKnobs, ReverbKnobs, RoomKnobs};
use gscore_play::StringKind;
use serde::{Deserialize, Serialize};

/// What a place holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlockChoice {
    #[default]
    Empty,
    /// An impulse response of a cabinet or a guitar body.
    Cabinet,
    /// An impulse response of a room (a convolution reverb).
    Room,
    /// The computed reverb.
    Reverb,
}

impl BlockChoice {
    /// The name the chain window shows.
    pub fn label(self) -> &'static str {
        match self {
            BlockChoice::Empty => "Empty",
            BlockChoice::Cabinet => "Cabinet / body",
            BlockChoice::Room => "Room",
            BlockChoice::Reverb => "Reverb",
        }
    }

    /// Whether it plays an impulse response from a file.
    pub fn uses_file(self) -> bool {
        matches!(self, BlockChoice::Cabinet | BlockChoice::Room)
    }
}

/// One place of a chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlockSetting {
    pub choice: BlockChoice,
    /// The impulse response's WAV file (a cabinet, body or room).
    pub file: Option<PathBuf>,
    pub bypass: bool,
    pub mix: f32,
    pub gain_db: f32,
    pub low_cut: f32,
    pub high_cut: f32,
    pub width: f32,
    pub size: f32,
    pub damping: f32,
}

impl Default for BlockSetting {
    fn default() -> Self {
        BlockSetting::of(BlockChoice::Empty)
    }
}

impl BlockSetting {
    /// A fresh block of `choice` with its usual knob values.
    pub fn of(choice: BlockChoice) -> Self {
        let (mix, gain_db, low_cut, high_cut, width) = match choice {
            BlockChoice::Room => {
                let room = RoomKnobs::default();
                (
                    room.mix,
                    room.gain_db,
                    room.low_cut,
                    room.high_cut,
                    room.width,
                )
            }
            BlockChoice::Reverb => {
                let reverb = ReverbKnobs::default();
                (
                    reverb.mix,
                    0.0,
                    reverb.low_cut,
                    reverb.high_cut,
                    reverb.width,
                )
            }
            BlockChoice::Cabinet | BlockChoice::Empty => {
                let impulse = ImpulseKnobs::default();
                (
                    impulse.mix,
                    impulse.gain_db,
                    impulse.low_cut,
                    impulse.high_cut,
                    1.0,
                )
            }
        };
        let reverb = ReverbKnobs::default();
        BlockSetting {
            choice,
            file: None,
            bypass: false,
            mix,
            gain_db,
            low_cut,
            high_cut,
            width,
            size: reverb.size,
            damping: reverb.damping,
        }
    }

    /// The engine's knob values for this place.
    pub fn knobs(&self) -> BlockSettings {
        BlockSettings {
            bypass: self.bypass,
            impulse: ImpulseKnobs {
                mix: self.mix,
                gain_db: self.gain_db,
                low_cut: self.low_cut,
                high_cut: self.high_cut,
            },
            room: RoomKnobs {
                mix: self.mix,
                gain_db: self.gain_db,
                low_cut: self.low_cut,
                high_cut: self.high_cut,
                width: self.width,
            },
            reverb: ReverbKnobs {
                size: self.size,
                damping: self.damping,
                width: self.width,
                mix: self.mix,
                low_cut: self.low_cut,
                high_cut: self.high_cut,
            },
        }
        .sanitized()
    }

    /// Whether `other` needs the chain prepared again (another block or file), rather
    /// than only new knob values.
    pub fn same_structure(&self, other: &BlockSetting) -> bool {
        self.choice == other.choice && (!self.choice.uses_file() || self.file == other.file)
    }
}

/// One kind's chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ChainSetting {
    pub blocks: [BlockSetting; BLOCKS],
}

impl ChainSetting {
    /// The engine's knob values of every place.
    pub fn knobs(&self) -> [BlockSettings; BLOCKS] {
        std::array::from_fn(|index| {
            self.blocks
                .get(index)
                .map(BlockSetting::knobs)
                .unwrap_or_default()
        })
    }

    /// Whether `other` needs the chain prepared again.
    pub fn same_structure(&self, other: &ChainSetting) -> bool {
        self.blocks
            .iter()
            .zip(&other.blocks)
            .all(|(a, b)| a.same_structure(b))
    }
}

/// The four kinds' chains.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StringChains {
    pub nylon: ChainSetting,
    pub steel: ChainSetting,
    pub electric: ChainSetting,
    pub bass: ChainSetting,
}

impl StringChains {
    /// The chain of `kind`.
    pub fn get(&self, kind: StringKind) -> &ChainSetting {
        match kind {
            StringKind::Nylon => &self.nylon,
            StringKind::Steel => &self.steel,
            StringKind::Electric => &self.electric,
            StringKind::Bass => &self.bass,
        }
    }

    /// Sets the chain of `kind`.
    pub fn set(&mut self, kind: StringKind, chain: ChainSetting) {
        match kind {
            StringKind::Nylon => self.nylon = chain,
            StringKind::Steel => self.steel = chain,
            StringKind::Electric => self.electric = chain,
            StringKind::Bass => self.bass = chain,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    #[test]
    fn a_knob_change_keeps_the_structure_and_a_new_file_does_not() {
        let mut chain = ChainSetting::default();
        chain.blocks[0] = BlockSetting::of(BlockChoice::Cabinet);
        chain.blocks[0].file = Some(PathBuf::from("body.wav"));
        let mut knob = chain.clone();
        knob.blocks[0].mix = 0.3;
        assert!(chain.same_structure(&knob));
        let mut file = chain.clone();
        file.blocks[0].file = Some(PathBuf::from("other.wav"));
        assert!(!chain.same_structure(&file));
        let mut moved = chain.clone();
        moved.blocks[1] = BlockSetting::of(BlockChoice::Reverb);
        assert!(!chain.same_structure(&moved));
    }

    #[test]
    fn chains_round_trip_through_the_settings_file() {
        let mut chains = StringChains::default();
        let mut room = BlockSetting::of(BlockChoice::Room);
        room.file = Some(PathBuf::from("/irs/hall.wav"));
        chains.steel.blocks[2] = room;
        let text = serde_json::to_string(&chains).unwrap();
        let back: StringChains = serde_json::from_str(&text).unwrap();
        assert_eq!(back, chains);
        // Older settings without chains read as empty chains.
        let empty: StringChains = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, StringChains::default());
    }
}
