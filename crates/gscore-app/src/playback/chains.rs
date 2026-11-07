//! The tones of the modelled tracks in playback: the tone its preset (General MIDI
//! program) has, else its kind's default from the settings.
//! What the synthesizer has been given is remembered per track (by the song's track index,
//! which a beat excerpt keeps too), so a change sends only what differs: a new block or file prepares that track's chain
//! again (on the synthesizer's own thread); a knob only sends its value, so a reverb's
//! tail is not cut while it is turned. Impulse responses are read from disk once.

use std::path::Path;
use std::sync::Arc;

use gscore_guitar::{BLOCKS, BlockKind, BlockSettings, ChainSpec, ImpulseResponse, StringModel};
use gscore_play::{Sequence, StringKind, TrackEngine};

use super::Playback;
use crate::chain_settings::{BlockChoice, BlockSetting, ChainSetting, StringChains, Tone};
use crate::settings::{PresetTone, StringSound};

/// A modelled track of the loaded sequence: the song's track it plays (as the
/// synthesizer knows its tone), its preset and its kind of string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ToneTarget {
    track: usize,
    /// The kind of string that plays it; `None`: the SoundFont plays it (the mixer runs
    /// its chain too).
    kind: Option<StringKind>,
    /// The General MIDI program of its preset, whose tone it plays.
    program: u8,
}

/// What the synthesizer was given for one mixer bus: whether it was prepared, and what it
/// runs (another kind of block or another impulse response prepares it again).
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct SentBus {
    sent: bool,
    structure: Option<(BlockChoice, Option<std::path::PathBuf>)>,
}

/// What the synthesizer was given for one track.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SentTone {
    /// The track's own string sound (`None`: its kind's).
    sound: Option<StringSound>,
    chain: ChainSetting,
}

/// The kind of string the preset `program` starts from: the kind its tone was dialed in on
/// (the owner's "Natural Steel 1" makes a Clean Guitar play steel strings), otherwise the
/// family of its program.
pub(crate) fn preset_kind_of(tones: &[PresetTone], program: u8) -> Option<StringKind> {
    tones
        .iter()
        .filter(|tone| tone.program == program)
        .find_map(PresetTone::tone)
        .map(|tone| tone.kind)
        .or_else(|| StringKind::for_program(program))
}

/// The tracks of `sequence`, with what plays them.
fn targets(sequence: &Sequence) -> Vec<ToneTarget> {
    sequence
        .performance
        .tracks
        .iter()
        .map(|track| ToneTarget {
            track: track.track.get(),
            kind: match track.engine {
                TrackEngine::Modelled(kind) => Some(kind),
                TrackEngine::SoundFont => None,
            },
            program: track.sound.program,
        })
        .collect()
}

/// The tones to render an audio export with: the presets' tones (by General MIDI program)
/// and each kind's default, resolved against the export's sequence once it is built.
#[derive(Debug, Clone, Default)]
pub(crate) struct ExportTones {
    /// The tones that came with the song, by track.
    own: Vec<(
        usize,
        StringKind,
        StringModel,
        ChainSpec,
        [BlockSettings; BLOCKS],
    )>,
    presets: Vec<(
        u8,
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
            let chain = match target.kind {
                // The SoundFont plays it: the chain the song gave it, else its preset's own
                // (the kinds' defaults are the modelled strings').
                None => self
                    .own
                    .iter()
                    .find(|(track, ..)| *track == target.track)
                    .map(|(_, _, _, spec, knobs)| (spec.clone(), *knobs))
                    .or_else(|| {
                        self.presets
                            .iter()
                            .find(|(program, ..)| *program == target.program)
                            .map(|(_, _, _, spec, knobs)| (spec.clone(), *knobs))
                    }),
                Some(kind) => {
                    // The song's own tone for this track first, then its preset's.
                    let own = self
                        .own
                        .iter()
                        .find(|(track, own, ..)| *track == target.track && *own == kind)
                        .map(|(_, _, model, spec, knobs)| (*model, spec.clone(), *knobs))
                        .or_else(|| {
                            self.presets
                                .iter()
                                .find(|(program, preset_kind, ..)| {
                                    *program == target.program && *preset_kind == kind
                                })
                                .map(|(_, _, model, spec, knobs)| (*model, spec.clone(), *knobs))
                        });
                    match own {
                        Some((model, spec, knobs)) => {
                            models.push((target.track, model));
                            Some((spec, knobs))
                        }
                        None => self
                            .defaults
                            .iter()
                            .find(|(default, ..)| *default == kind)
                            .map(|(_, spec, knobs)| (spec.clone(), *knobs)),
                    }
                }
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

    /// The tones set for track presets (by General MIDI program), local to this machine.
    pub fn preset_tones(&self) -> &[PresetTone] {
        &self.preset_tones
    }

    /// Takes the preset tones without sending them (when the settings load).
    pub fn store_preset_tones(&mut self, tones: Vec<PresetTone>) {
        self.preset_tones = tones;
    }

    /// The tone of the preset `program`, whatever kind of string it was dialed in on.
    pub fn preset_tone(&self, program: u8) -> Option<Tone> {
        self.preset_tones
            .iter()
            .filter(|tone| tone.program == program)
            .find_map(PresetTone::tone)
    }

    /// The kind of string the preset `program` starts from (its tone's, else its
    /// program's family).
    pub fn preset_kind(&self, program: u8) -> Option<StringKind> {
        preset_kind_of(&self.preset_tones, program)
    }

    /// The tone the song's track `track` (whose preset is `program`) plays as `kind`: the
    /// tone that came with the song, else the preset's if it was dialed in on that kind of
    /// string, otherwise the kind's default.
    pub fn tone(&self, track: usize, program: u8, kind: StringKind) -> Tone {
        self.tones
            .get(&track)
            .filter(|tone| tone.kind == kind)
            .cloned()
            .or_else(|| self.preset_tone(program).filter(|tone| tone.kind == kind))
            .unwrap_or_else(|| self.default_tone(kind))
    }

    /// The tones that came with the open song (by its track index), heard at once: only
    /// what changed is sent.
    pub fn set_track_tones(&mut self, tones: &std::collections::BTreeMap<usize, Tone>) {
        if &self.tones == tones {
            return;
        }
        self.tones = tones.clone();
        self.send_tones();
    }

    /// The tones that came with the open song (by its track index).
    pub fn track_tones(&self) -> &std::collections::BTreeMap<usize, Tone> {
        &self.tones
    }

    /// The tracks' sends into the compressor bus (by the song's track index), heard at
    /// once.
    pub fn set_comp_sends(&mut self, sends: Vec<(usize, u8)>) {
        if self.comp_sends == sends {
            return;
        }
        self.comp_sends = sends;
        self.send_comp_sends();
    }

    /// The tracks' sends into the compressor bus (by the song's track index).
    pub fn comp_sends(&self) -> &[(usize, u8)] {
        &self.comp_sends
    }

    /// Gives the running synthesizer every track's comp send (a new one starts with none).
    pub(super) fn send_comp_sends(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(super::Active::Synth(player)) = &self.player {
            for &(track, value) in &self.comp_sends {
                let _ = player.set_comp_send(track, value);
            }
        }
    }

    /// The kind's default tone: the sound and chain from the settings (what a preset
    /// without a tone of its own plays).
    pub fn default_tone(&self, kind: StringKind) -> Tone {
        Tone {
            kind,
            sound: self.string_sounds.get(kind),
            chain: self.string_chains.get(kind).clone(),
        }
    }

    /// Gives the preset `program` a tone of its own (heard at once, and on every track with
    /// that preset from now on), or forgets it with `None`: the built-in default plays
    /// again. A tone of another kind of string changes which strings its tracks play, so
    /// the sequence is built again then.
    pub fn set_preset_tone(
        &mut self,
        song: &gscore_model::Song,
        program: u8,
        tone: Option<Tone>,
    ) -> Result<(), String> {
        let before = self.preset_kind(program);
        self.preset_tones.retain(|old| old.program != program);
        if let Some(tone) = tone {
            self.preset_tones.push(PresetTone::of(program, &tone));
        }
        if self.preset_kind(program) == before {
            self.send_tones();
            Ok(())
        } else {
            self.rebuild_sequence(song)
        }
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
                (BlockChoice::Compressor, _) => Some(BlockKind::Compressor),
                (BlockChoice::Eq, _) => Some(BlockKind::Eq),
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
        if !self.reverb_sent.sent {
            self.send_reverb_bus();
        }
        if !self.compressor_sent.sent {
            self.send_compressor_bus();
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
            // The tone that came with the song wins over the preset's; a preset tone only
            // applies on the kind of string it was dialed in on.
            let own = self
                .tones
                .get(&target.track)
                .filter(|tone| Some(tone.kind) == target.kind)
                .cloned();
            let preset = self
                .preset_tone(target.program)
                .filter(|tone| Some(tone.kind) == target.kind);
            let wanted = match target.kind {
                // The SoundFont plays it: the chain the song gave it, else its preset's own
                // (the kinds' defaults are the modelled strings'); no string sound.
                None => SentTone {
                    sound: None,
                    chain: self
                        .tones
                        .get(&target.track)
                        .map(|tone| tone.chain.clone())
                        .or_else(|| self.preset_tone(target.program).map(|own| own.chain))
                        .unwrap_or_default(),
                },
                Some(kind) => {
                    let tone = own.or(preset).unwrap_or_else(|| self.default_tone(kind));
                    let dialed_in = self.tones.contains_key(&target.track)
                        || self
                            .preset_tone(target.program)
                            .is_some_and(|own| own.kind == kind);
                    SentTone {
                        sound: dialed_in.then_some(tone.sound),
                        chain: tone.chain,
                    }
                }
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
        let track_tones: Vec<(usize, Tone)> = self
            .tones
            .iter()
            .map(|(&track, tone)| (track, tone.clone()))
            .collect();
        let own = track_tones
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
        let preset_tones = self.preset_tones.clone();
        let presets = preset_tones
            .iter()
            .filter_map(|preset| {
                let tone = preset.tone()?;
                let spec = self.chain_spec(&tone.chain);
                Some((
                    preset.program,
                    tone.kind,
                    tone.sound.model(),
                    spec,
                    tone.chain.knobs(),
                ))
            })
            .collect();
        let defaults = super::STRING_KINDS
            .iter()
            .map(|&kind| {
                let chain = self.string_chains.get(kind).clone();
                (kind, self.chain_spec(&chain), chain.knobs())
            })
            .collect();
        ExportTones {
            own,
            presets,
            defaults,
        }
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
        self.send_reverb_bus();
    }

    /// The mixer's compressor bus as it is set.
    pub fn compressor_bus(&self) -> crate::settings::CompressorBus {
        self.compressor_bus.clone()
    }

    /// Takes the compressor bus without sending it (when the settings load).
    pub fn store_compressor_bus(&mut self, bus: crate::settings::CompressorBus) {
        self.compressor_bus = bus.sanitized();
    }

    /// Sets the compressor bus, heard at once (a new block prepares it, knobs go straight
    /// to the running one).
    pub fn set_compressor_bus(&mut self, bus: crate::settings::CompressorBus) {
        self.compressor_bus = bus.sanitized();
        self.send_compressor_bus();
    }

    /// The chain a bus runs, as the engine takes it, and its knob values. A bus runs one
    /// block: a reverb or a room, a compressor, or nothing.
    fn bus_parts(&mut self, block: &BlockSetting) -> (ChainSpec, [BlockSettings; BLOCKS]) {
        let mut settings = [BlockSettings::default(); BLOCKS];
        if let Some(slot) = settings.first_mut() {
            *slot = block.knobs();
        }
        let mut spec = ChainSpec::default();
        let prepared = match block.choice {
            BlockChoice::Empty => None,
            BlockChoice::Reverb => Some(BlockKind::Reverb),
            BlockChoice::Compressor => Some(BlockKind::Compressor),
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
            BlockChoice::Cabinet => block
                .file
                .as_ref()
                .and_then(|file| match self.impulse(file) {
                    Ok(response) => Some(BlockKind::Impulse(response)),
                    Err(error) => {
                        self.chain_notices.push(error);
                        None
                    }
                }),
            // A bus never runs an amp, a drive pedal or an EQ: `sanitized` keeps them out.
            BlockChoice::Amp | BlockChoice::Drive | BlockChoice::Eq => None,
        };
        if let Some(slot) = spec.blocks.first_mut() {
            *slot = prepared;
        }
        (spec, settings)
    }

    /// Prepares `which` on the synthesizer's thread (once per player, or when what it runs
    /// changes), otherwise sends only its knob values, level and mute.
    fn send_bus(&mut self, which: gscore_audio::Bus, block: &BlockSetting, level: u8, muted: bool) {
        if !self.synth_running() {
            return;
        }
        let structure = (block.choice, block.file.clone());
        let (spec, settings) = self.bus_parts(block);
        let sent = match which {
            gscore_audio::Bus::Reverb => &mut self.reverb_sent,
            gscore_audio::Bus::Compressor => &mut self.compressor_sent,
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(super::Active::Synth(player)) = &self.player {
            if sent.sent && sent.structure.as_ref() == Some(&structure) {
                let _ = player.set_bus_settings(which, settings, level, muted);
            } else if let Err(error) = player.set_bus(which, spec, settings, level, muted) {
                self.chain_notices.push(error.to_string());
            } else {
                sent.sent = true;
                sent.structure = Some(structure);
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (which, spec, settings, level, muted);
    }

    /// Prepares the reverb bus on the synthesizer's thread (or sends its knobs).
    pub(super) fn send_reverb_bus(&mut self) {
        let bus = self.reverb_bus.clone();
        self.send_bus(gscore_audio::Bus::Reverb, &bus.block, bus.level, bus.muted);
    }

    /// Prepares the compressor bus on the synthesizer's thread (or sends its knobs).
    pub(super) fn send_compressor_bus(&mut self) {
        let bus = self.compressor_bus.clone();
        self.send_bus(
            gscore_audio::Bus::Compressor,
            &bus.block,
            bus.level,
            bus.muted,
        );
    }

    /// The buses to render audio export with.
    pub fn export_buses(&mut self) -> gscore_audio::BusChoices {
        let reverb = self.reverb_bus.clone();
        let compressor = self.compressor_bus.clone();
        let (reverb_spec, reverb_settings) = self.bus_parts(&reverb.block);
        let (comp_spec, comp_settings) = self.bus_parts(&compressor.block);
        gscore_audio::BusChoices {
            reverb: Some(gscore_audio::BusChoice {
                spec: reverb_spec,
                settings: reverb_settings,
                level: reverb.level,
                muted: reverb.muted,
            }),
            compressor: Some(gscore_audio::BusChoice {
                spec: comp_spec,
                settings: comp_settings,
                level: compressor.level,
                muted: compressor.muted,
            }),
            // Each track's Comp send, once the mixer has them (step 5).
            comp_sends: Vec::new(),
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
        // Three steel-string tracks whose presets are 27, 28 and 29: 27's preset has this
        // chain and a sound of its own, 28's was dialed in on nylon strings (so on steel it
        // plays steel's default), 29's none. Only the first gets a chain, and only it its
        // own sound.
        let mut sound = playback.string_sounds().get(StringKind::Steel);
        sound.level *= 0.5;
        playback.store_preset_tones(vec![
            PresetTone::of(
                27,
                &Tone {
                    kind: StringKind::Steel,
                    sound,
                    chain,
                },
            ),
            PresetTone::of(
                28,
                &Tone {
                    kind: StringKind::Nylon,
                    sound,
                    chain: ChainSetting::default(),
                },
            ),
        ]);
        assert_eq!(playback.tone(0, 27, StringKind::Steel).sound, sound);
        assert_eq!(
            playback.tone(0, 28, StringKind::Steel),
            playback.tone(0, 29, StringKind::Steel),
            "a tone of another kind does not play on steel"
        );
        let song = gscore_model::testing::song_with_notes_on_tracks(
            3,
            &[(1, &[gscore_model::testing::TestNote::new(3, 2)])],
        )
        .unwrap();
        let mut sequence = Sequence::new(&song, None).unwrap();
        {
            let performance = Arc::make_mut(&mut sequence.performance);
            for (index, track) in performance.tracks.iter_mut().enumerate() {
                track.sound.program = 27_u8.saturating_add(u8::try_from(index).unwrap_or(0));
            }
        }
        let sequence = super::super::with_engines(
            sequence,
            false,
            &[0, 1, 2].map(|track| (track, TrackEngine::Modelled(StringKind::Steel))),
            &[],
        );
        let (models, choices) = playback.export_tones().resolve(&sequence);
        assert_eq!(models.len(), 1, "{models:?}");
        assert_eq!(models[0].0, 0);
        assert_eq!(choices.len(), 1, "only the first track has a chain");
        let (track, spec, _) = &choices[0];
        assert_eq!(*track, 0);
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
