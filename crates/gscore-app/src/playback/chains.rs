//! The effect chains of the modelled strings in playback: kept per kind of string, their
//! impulse responses read from disk once and cached, and sent to the synthesizer. A new
//! block or file prepares the chain again (on the synthesizer's own thread); a knob only
//! sends its value, so a reverb's tail is not cut while it is turned.

use std::path::Path;
use std::sync::Arc;

use gscore_guitar::{BlockKind, ChainSpec, ImpulseResponse};
use gscore_play::{Sequence, StringKind};

use super::{Playback, STRING_KINDS};
use crate::chain_settings::{BlockChoice, ChainSetting, StringChains};

/// The place of `kind` in per-kind arrays.
fn kind_index(kind: StringKind) -> usize {
    STRING_KINDS
        .iter()
        .position(|&known| known == kind)
        .unwrap_or(0)
}

impl Playback {
    /// The chains as they are set.
    pub fn string_chains(&self) -> &StringChains {
        &self.string_chains
    }

    /// Takes the chains without sending them (when the settings load).
    pub fn store_string_chains(&mut self, chains: StringChains) {
        self.string_chains = chains;
    }

    /// Sets the chain of `kind`, heard at once: prepared again if a block or a file
    /// changed, otherwise only the knob values are sent.
    pub fn set_string_chain(&mut self, kind: StringKind, chain: ChainSetting) {
        let same = self.string_chains.get(kind).same_structure(&chain);
        self.string_chains.set(kind, chain);
        if same {
            self.send_chain_knobs(kind);
        } else {
            self.send_chain(kind);
        }
    }

    /// Reads the impulse response at `path` (once; later calls use the copy read).
    pub fn impulse(&mut self, path: &Path) -> Result<Arc<ImpulseResponse>, String> {
        if let Some(response) = self.impulses.get(path) {
            return Ok(Arc::clone(response));
        }
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let bytes = std::fs::read(path).map_err(|e| format!("cannot read {name}: {e}"))?;
        let response =
            Arc::new(ImpulseResponse::from_wav(&bytes).map_err(|e| format!("{name}: {e}"))?);
        self.impulses
            .insert(path.to_path_buf(), Arc::clone(&response));
        Ok(response)
    }

    /// The engine's chain of `kind`: its blocks (an impulse response that cannot be read
    /// leaves its place empty, and says why once) and their knobs.
    fn chain_spec(&mut self, kind: StringKind) -> ChainSpec {
        let setting = self.string_chains.get(kind).clone();
        let mut spec = ChainSpec::default();
        for (place, block) in spec.blocks.iter_mut().zip(&setting.blocks) {
            *place = match (block.choice, &block.file) {
                (BlockChoice::Empty, _) => None,
                (BlockChoice::Reverb, _) => Some(BlockKind::Reverb),
                (BlockChoice::Cabinet | BlockChoice::Room, None) => None,
                (choice, Some(file)) => match self.impulse(file) {
                    Ok(response) if choice == BlockChoice::Room => Some(BlockKind::Room(response)),
                    Ok(response) => Some(BlockKind::Impulse(response)),
                    Err(error) => {
                        self.chain_notices.push(error);
                        None
                    }
                },
            };
        }
        spec
    }

    /// Sends the chain of `kind` to a running synthesizer, to be prepared there.
    pub(super) fn send_chain(&mut self, kind: StringKind) {
        if !self.synth_running() {
            return;
        }
        let spec = self.chain_spec(kind);
        let knobs = self.string_chains.get(kind).knobs();
        let tracks = self
            .chain_tracks
            .get(kind_index(kind))
            .copied()
            .unwrap_or(1)
            .max(1);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(super::Active::Synth(player)) = &self.player
            && let Err(error) = player.set_chain(kind, spec, knobs, tracks)
        {
            self.chain_notices.push(error.to_string());
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (spec, knobs, tracks);
    }

    /// Sends the knob values of `kind`'s chain to a running synthesizer.
    fn send_chain_knobs(&mut self, kind: StringKind) {
        let knobs = self.string_chains.get(kind).knobs();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(super::Active::Synth(player)) = &self.player {
            let _ = player.set_chain_settings(kind, knobs);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = knobs;
    }

    /// Whether the built-in synthesizer is connected (the chains play only there).
    fn synth_running(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            matches!(self.player, Some(super::Active::Synth(_)))
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }

    /// Makes sure the synthesizer's chains are made for as many tracks of each kind as
    /// `sequence` plays (they are only ever made for more, never fewer).
    pub(super) fn sync_chains(&mut self, sequence: &Sequence) {
        for kind in STRING_KINDS {
            let needed = gscore_audio::tracks_of_kind(sequence, kind);
            let index = kind_index(kind);
            let made = self.chain_tracks.get(index).copied().unwrap_or(1);
            if needed > made {
                if let Some(slot) = self.chain_tracks.get_mut(index) {
                    *slot = needed;
                }
                let empty = self
                    .string_chains
                    .get(kind)
                    .blocks
                    .iter()
                    .all(|block| block.choice == BlockChoice::Empty);
                if !empty {
                    self.send_chain(kind);
                }
            }
        }
    }

    /// The chains to render audio export with (the ones with blocks).
    pub fn chain_choices(&mut self) -> Vec<gscore_audio::ChainChoice> {
        STRING_KINDS
            .iter()
            .filter_map(|&kind| {
                let spec = self.chain_spec(kind);
                (!gscore_audio::ChainSet::is_empty(&spec))
                    .then(|| (kind, spec, self.string_chains.get(kind).knobs()))
            })
            .collect()
    }

    /// A message about the chains, once.
    pub(super) fn chain_notice(&mut self) -> Option<String> {
        (!self.chain_notices.is_empty()).then(|| self.chain_notices.remove(0))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;
    use crate::chain_settings::BlockSetting;

    /// A short mono 16-bit WAV file.
    fn wav() -> Vec<u8> {
        let samples: Vec<i16> = (0..400)
            .map(|i| ((i as f32 * 0.3).sin() * (-(i as f32) / 80.0).exp() * 30_000.0) as i16)
            .collect();
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut out = Vec::new();
        out.extend(b"RIFF");
        out.extend((36 + data.len() as u32).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16_u32.to_le_bytes());
        out.extend(1_u16.to_le_bytes());
        out.extend(1_u16.to_le_bytes());
        out.extend(48_000_u32.to_le_bytes());
        out.extend(96_000_u32.to_le_bytes());
        out.extend(2_u16.to_le_bytes());
        out.extend(16_u16.to_le_bytes());
        out.extend(b"data");
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
        out
    }

    #[test]
    fn impulse_files_are_read_once_and_used_for_export() {
        let dir = std::env::temp_dir().join(format!("gscore-chains-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("body.wav");
        std::fs::write(&file, wav()).unwrap();
        let broken = dir.join("broken.wav");
        std::fs::write(&broken, b"RIFF....WAVE").unwrap();

        let mut playback = Playback::default();
        let first = playback.impulse(&file).unwrap();
        // The copy read is used again, even once the file is gone.
        std::fs::remove_file(&file).unwrap();
        assert!(Arc::ptr_eq(&first, &playback.impulse(&file).unwrap()));
        assert!(
            playback
                .impulse(&broken)
                .unwrap_err()
                .contains("broken.wav")
        );

        let mut chain = ChainSetting::default();
        chain.blocks[0] = BlockSetting::of(BlockChoice::Cabinet);
        chain.blocks[0].file = Some(file);
        chain.blocks[1] = BlockSetting::of(BlockChoice::Room);
        chain.blocks[1].file = Some(broken);
        playback.set_string_chain(StringKind::Steel, chain);
        let choices = playback.chain_choices();
        assert_eq!(choices.len(), 1, "only the steel strings have a chain");
        let (kind, spec, _) = &choices[0];
        assert_eq!(*kind, StringKind::Steel);
        assert!(matches!(spec.blocks[0], Some(BlockKind::Impulse(_))));
        assert!(
            spec.blocks[1].is_none(),
            "a file that cannot be read leaves its place empty"
        );
        assert!(playback.chain_notice().unwrap().contains("broken.wav"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
