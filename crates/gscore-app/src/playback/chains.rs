//! The tones of the modelled tracks in playback: each track's own tone (the sound of its
//! strings and its effect chain) or, without one, its kind's default from the settings.
//! What the synthesizer has been given is remembered per track (by the song's track index,
//! which a beat excerpt keeps too), so a change sends only what differs: a new block or file prepares that track's chain
//! again (on the synthesizer's own thread); a knob only sends its value, so a reverb's
//! tail is not cut while it is turned. Impulse responses are read from disk once.

use std::path::Path;
use std::sync::Arc;

use gscore_guitar::{BLOCKS, BlockKind, BlockSettings, ChainSpec, ImpulseResponse, StringModel};
use gscore_play::{Sequence, StringKind, TrackEngine};

use super::Playback;
use crate::chain_settings::{BlockChoice, ChainSetting, StringChains, Tone, TrackTones};
use crate::settings::{StringSound, StringSounds};

/// A modelled track of the loaded sequence: the song's track it plays (as the
/// synthesizer knows its tone) and its kind of string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ToneTarget {
    track: usize,
    kind: StringKind,
}

/// What the synthesizer was given for one track.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SentTone {
    /// The track's own string sound (`None`: its kind's).
    sound: Option<StringSound>,
    chain: ChainSetting,
}

/// The tone `track` plays as `kind`: its own if it was dialed in on that kind, otherwise
/// the kind's default.
pub(crate) fn tone_of(
    tones: &TrackTones,
    defaults: (&StringSounds, &StringChains),
    track: usize,
    kind: StringKind,
) -> Tone {
    tones
        .get(&track)
        .filter(|tone| tone.kind == kind)
        .cloned()
        .unwrap_or_else(|| Tone {
            kind,
            sound: defaults.0.get(kind),
            chain: defaults.1.get(kind).clone(),
        })
}

/// The modelled tracks of `sequence`.
fn targets(sequence: &Sequence) -> Vec<ToneTarget> {
    sequence
        .performance
        .tracks
        .iter()
        .filter_map(|track| match track.engine {
            TrackEngine::Modelled(kind) => Some(ToneTarget {
                track: track.track.get(),
                kind,
            }),
            TrackEngine::SoundFont => None,
        })
        .collect()
}

/// The tones to render an audio export with: each track's own (with the kind it is for)
/// and each kind's default, resolved against the export's sequence once it is built.
#[derive(Debug, Clone, Default)]
pub(crate) struct ExportTones {
    own: Vec<(
        usize,
        StringKind,
        StringModel,
        ChainSpec,
        [BlockSettings; BLOCKS],
    )>,
    defaults: Vec<(StringKind, ChainSpec, [BlockSettings; BLOCKS])>,
}

impl ExportTones {
    /// The own string models and the chains of `sequence`'s modelled tracks, by the song's
    /// track index.
    pub(crate) fn resolve(
        &self,
        sequence: &Sequence,
    ) -> (Vec<(usize, StringModel)>, Vec<gscore_audio::ChainChoice>) {
        let mut models = Vec::new();
        let mut chains = Vec::new();
        for target in targets(sequence) {
            let own = self
                .own
                .iter()
                .find(|(track, kind, ..)| *track == target.track && *kind == target.kind);
            let chain = match own {
                Some((_, _, model, spec, knobs)) => {
                    models.push((target.track, *model));
                    Some((spec.clone(), *knobs))
                }
                None => self
                    .defaults
                    .iter()
                    .find(|(kind, ..)| *kind == target.kind)
                    .map(|(_, spec, knobs)| (spec.clone(), *knobs)),
            };
            if let Some((spec, knobs)) = chain
                && !gscore_audio::chain_is_empty(&spec)
            {
                chains.push((target.track, spec, knobs));
            }
        }
        (models, chains)
    }
}

impl Playback {
    /// Each kind's default chain (what a track without a tone of its own plays).
    pub fn string_chains(&self) -> &StringChains {
        &self.string_chains
    }

    /// Takes the default chains without sending them (when the settings load).
    pub fn store_string_chains(&mut self, chains: StringChains) {
        self.string_chains = chains;
    }

    /// The tone `track` of the open song plays as `kind`.
    pub fn tone(&self, track: usize, kind: StringKind) -> Tone {
        tone_of(
            &self.tones,
            (&self.string_sounds, &self.string_chains),
            track,
            kind,
        )
    }

    /// The open song's own tones (each track's; the others play their kind's default),
    /// heard at once: only what changed is sent.
    pub fn set_track_tones(&mut self, tones: &TrackTones) {
        if &self.tones == tones {
            return;
        }
        self.tones = tones.clone();
        self.send_tones();
    }

    /// Prepares `track`'s chain again with the blocks as they are set now, for a change
    /// that alters a prepared response rather than a knob value: a room's length. The
    /// window calls it when the length knob is let go, so a drag does not rebuild the room
    /// on every step (each rebuilding resamples the whole tail).
    pub fn rebuild_chain(&mut self, track: usize) {
        self.sent_tones.remove(&track);
        self.send_tones();
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

    /// The engine's chain for `setting`: its blocks (an impulse response that cannot be
    /// read leaves its place empty, and says why once).
    fn chain_spec(&mut self, setting: &ChainSetting) -> ChainSpec {
        let mut spec = ChainSpec::default();
        for (place, block) in spec.blocks.iter_mut().zip(&setting.blocks) {
            *place = match (block.choice, &block.file) {
                (BlockChoice::Empty, _) => None,
                (BlockChoice::Reverb, _) => Some(BlockKind::Reverb),
                (BlockChoice::Amp, _) => Some(BlockKind::Amp),
                (BlockChoice::Drive, _) => Some(BlockKind::Drive),
                (BlockChoice::Cabinet | BlockChoice::Room, None) => None,
                (choice, Some(file)) => match self.impulse(file) {
                    Ok(response) if choice == BlockChoice::Room => {
                        Some(BlockKind::room(response, block.room_length()))
                    }
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

    /// Whether the built-in synthesizer is connected (the tones play only there).
    pub(super) fn synth_running(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            matches!(self.player, Some(super::Active::Synth(_)))
        }
        #[cfg(target_arch = "wasm32")]
        {
            false
        }
    }

    /// Follows the sequence the synthesizer has loaded: its modelled tracks get their
    /// tones (only what differs from what they had is sent).
    pub(super) fn sync_chains(&mut self, sequence: &Sequence) {
        if !self.bus_sent {
            self.send_reverb_bus();
        }
        self.tone_targets = targets(sequence);
        self.send_tones();
    }

    /// Forgets what the synthesizer was given (a new one starts with nothing).
    pub(super) fn forget_sent_tones(&mut self) {
        self.sent_tones.clear();
    }

    /// Sends each modelled track of the loaded sequence what differs between its tone and
    /// what it was last given.
    fn send_tones(&mut self) {
        if !self.synth_running() {
            return;
        }
        for target in self.tone_targets.clone() {
            let tone = self.tone(target.track, target.kind);
            let own = self
                .tones
                .get(&target.track)
                .is_some_and(|own| own.kind == target.kind);
            let wanted = SentTone {
                sound: own.then_some(tone.sound),
                chain: tone.chain,
            };
            let sent = self.sent_tones.get(&target.track).cloned();
            if sent.as_ref() == Some(&wanted) {
                continue;
            }
            if sent.as_ref().map(|sent| sent.sound) != Some(wanted.sound) {
                self.send_track_model(target.track, wanted.sound);
            }
            match &sent {
                Some(sent) if sent.chain.same_structure(&wanted.chain) => {
                    self.send_chain_knobs(target.track, &wanted.chain);
                }
                _ => self.send_chain(target.track, &wanted.chain),
            }
            self.sent_tones.insert(target.track, wanted);
        }
    }

    /// Gives `track` its own string sound (`None`: its kind's).
    fn send_track_model(&mut self, track: usize, sound: Option<StringSound>) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(super::Active::Synth(player)) = &self.player
            && let Err(error) = player.set_track_model(track, sound.map(|sound| sound.model()))
        {
            self.chain_notices.push(error.to_string());
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (track, sound);
    }

    /// Sends the chain `setting` for `track`, to be prepared on the synthesizer's thread.
    fn send_chain(&mut self, track: usize, setting: &ChainSetting) {
        let spec = self.chain_spec(setting);
        let knobs = setting.knobs();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(super::Active::Synth(player)) = &self.player
            && let Err(error) = player.set_chain(track, spec, knobs)
        {
            self.chain_notices.push(error.to_string());
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (track, spec, knobs);
    }

    /// Sends the knob values of `setting` for `track`.
    fn send_chain_knobs(&mut self, track: usize, setting: &ChainSetting) {
        let knobs = setting.knobs();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(super::Active::Synth(player)) = &self.player {
            let _ = player.set_chain_settings(track, knobs);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (track, knobs);
    }

    /// The tones to render audio export with.
    pub(crate) fn export_tones(&mut self) -> ExportTones {
        let own: Vec<(usize, Tone)> = self
            .tones
            .iter()
            .map(|(&track, tone)| (track, tone.clone()))
            .collect();
        let own = own
            .into_iter()
            .map(|(track, tone)| {
                let spec = self.chain_spec(&tone.chain);
                (
                    track,
                    tone.kind,
                    tone.sound.model(),
                    spec,
                    tone.chain.knobs(),
                )
            })
            .collect();
        let defaults = super::STRING_KINDS
            .iter()
            .map(|&kind| {
                let chain = self.string_chains.get(kind).clone();
                (kind, self.chain_spec(&chain), chain.knobs())
            })
            .collect();
        ExportTones { own, defaults }
    }

    /// A message about the chains, once.
    pub(super) fn chain_notice(&mut self) -> Option<String> {
        (!self.chain_notices.is_empty()).then(|| self.chain_notices.remove(0))
    }
}

impl Playback {
    /// The mixer's reverb bus as it is set.
    pub fn reverb_bus(&self) -> crate::settings::ReverbBus {
        self.reverb_bus.clone()
    }

    /// Takes the reverb bus without sending it (when the settings load).
    pub fn store_reverb_bus(&mut self, bus: crate::settings::ReverbBus) {
        self.reverb_bus = bus.sanitized();
    }

    /// Sets the reverb bus, heard at once: prepared the first time, otherwise only its
    /// knobs, level and mute are sent (so its tail is not cut while a knob turns).
    pub fn set_reverb_bus(&mut self, bus: crate::settings::ReverbBus) {
        self.reverb_bus = bus.sanitized();
        let structure = (
            self.reverb_bus.block.choice,
            self.reverb_bus.block.file.clone(),
        );
        if self.bus_sent && self.bus_structure.as_ref() == Some(&structure) {
            self.send_reverb_bus_settings();
        } else {
            self.send_reverb_bus();
        }
    }

    /// The bus's chain as it is set: the computed reverb or a room's response, all wet (the
    /// sends decide how much each track gives it, the fader how much comes back).
    fn reverb_bus_parts(&mut self) -> (ChainSpec, [BlockSettings; BLOCKS]) {
        let block = self.reverb_bus.block.clone();
        let mut settings = [BlockSettings::default(); BLOCKS];
        let mut block_settings = block.knobs();
        // An all-wet bus: nothing dry goes through it (the tracks' own signal is the dry).
        block_settings.reverb.mix = 1.0;
        block_settings.room.mix = 1.0;
        if let Some(slot) = settings.first_mut() {
            *slot = block_settings;
        }
        let mut spec = ChainSpec::default();
        let prepared = match block.choice {
            BlockChoice::Empty => None,
            BlockChoice::Room => block
                .file
                .as_ref()
                .and_then(|file| match self.impulse(file) {
                    Ok(response) => Some(BlockKind::room(response, block.room_length())),
                    Err(error) => {
                        self.chain_notices.push(error);
                        None
                    }
                }),
            // The bus is a reverb: `ReverbBus::sanitized` keeps anything else out.
            BlockChoice::Reverb | BlockChoice::Cabinet | BlockChoice::Amp | BlockChoice::Drive => {
                Some(BlockKind::Reverb)
            }
        };
        if let Some(slot) = spec.blocks.first_mut() {
            *slot = prepared;
        }
        (spec, settings)
    }

    /// Prepares the bus on the synthesizer's thread (once per player, or when what it runs
    /// changes: another kind of reverb or another impulse response).
    pub(super) fn send_reverb_bus(&mut self) {
        if !self.synth_running() {
            return;
        }
        let (spec, settings) = self.reverb_bus_parts();
        let structure = (
            self.reverb_bus.block.choice,
            self.reverb_bus.block.file.clone(),
        );
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(super::Active::Synth(player)) = &self.player {
            let bus = self.reverb_bus.clone();
            if let Err(error) = player.set_reverb_bus(spec, settings, bus.level, bus.muted) {
                self.chain_notices.push(error.to_string());
            } else {
                self.bus_sent = true;
                self.bus_structure = Some(structure);
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (spec, settings);
    }

    /// New knob values, level and mute for a running synthesizer.
    fn send_reverb_bus_settings(&mut self) {
        let (_, settings) = self.reverb_bus_parts();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(super::Active::Synth(player)) = &self.player {
            let bus = self.reverb_bus.clone();
            let _ = player.set_reverb_bus_settings(settings, bus.level, bus.muted);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = settings;
    }

    /// The bus to render audio export with.
    pub fn reverb_bus_choice(&mut self) -> gscore_audio::ReverbBusChoice {
        let (spec, settings) = self.reverb_bus_parts();
        gscore_audio::ReverbBusChoice {
            spec,
            settings,
            level: self.reverb_bus.level,
            muted: self.reverb_bus.muted,
        }
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
        chain.blocks[0].file = Some(file.clone());
        chain.blocks[1] = BlockSetting::of(BlockChoice::Room);
        chain.blocks[1].file = Some(broken);
        // A room at three times its recorded length: the chain is prepared with it.
        chain.blocks[2] = BlockSetting::of(BlockChoice::Room);
        chain.blocks[2].file = Some(file);
        chain.blocks[2].length = 3.0;
        // An amp needs no file.
        chain.blocks[3] = BlockSetting::of(BlockChoice::Amp);
        chain.blocks[4] = BlockSetting::of(BlockChoice::Drive);
        // Three steel-string tracks: the second has its own tone with this chain, the
        // first one dialed in on nylon strings (so on steel it plays steel's default), the
        // third none. Only the second gets a chain, and only it its own sound.
        let mut sound = playback.string_sounds().get(StringKind::Steel);
        sound.level *= 0.5;
        let mut tones = TrackTones::new();
        tones.insert(
            1,
            Tone {
                kind: StringKind::Steel,
                sound,
                chain,
            },
        );
        tones.insert(
            0,
            Tone {
                kind: StringKind::Nylon,
                sound,
                chain: ChainSetting::default(),
            },
        );
        playback.set_track_tones(&tones);
        assert_eq!(playback.tone(1, StringKind::Steel).sound, sound);
        assert_eq!(
            playback.tone(0, StringKind::Steel),
            playback.tone(2, StringKind::Steel),
            "a tone of another kind does not play on steel"
        );
        let song = gscore_model::testing::song_with_notes_on_tracks(
            3,
            &[(1, &[gscore_model::testing::TestNote::new(3, 2)])],
        )
        .unwrap();
        let sequence = super::super::with_engines(
            Sequence::new(&song, None).unwrap(),
            false,
            &[0, 1, 2].map(|track| (track, TrackEngine::Modelled(StringKind::Steel))),
        );
        let (models, choices) = playback.export_tones().resolve(&sequence);
        assert_eq!(models.len(), 1, "{models:?}");
        assert_eq!(models[0].0, 1);
        assert_eq!(choices.len(), 1, "only the second track has a chain");
        let (track, spec, _) = &choices[0];
        assert_eq!(*track, 1);
        assert!(matches!(spec.blocks[0], Some(BlockKind::Impulse(_))));
        assert!(
            spec.blocks[1].is_none(),
            "a file that cannot be read leaves its place empty"
        );
        assert!(
            matches!(spec.blocks[2], Some(BlockKind::Room { length, .. }) if (length - 3.0).abs() < 1e-6),
            "{:?}",
            spec.blocks[2]
        );
        assert_eq!(spec.blocks[3], Some(BlockKind::Amp));
        assert_eq!(spec.blocks[4], Some(BlockKind::Drive));
        assert!(playback.chain_notice().unwrap().contains("broken.wav"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
