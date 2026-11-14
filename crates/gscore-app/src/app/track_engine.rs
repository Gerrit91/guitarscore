//! What plays a track: the SoundFont, or modelled strings of a kind. By default a guitar
//! or bass track's preset decides (with the Settings' switch for modelled guitars); the
//! user can choose for each track in the Sound chain window. The choice belongs to the
//! song: it lives on the track in the document (GuitarScore's own sound state) and is
//! written into the song's notes on save, so a song keeps its sound when it is shared. A
//! track's tone (the sound of its strings and its chain) belongs to its preset unless the
//! song brought one: then it is kept with the song too.
//!
//! Every change goes through [`gscore_edit::Command::SetTrackSound`], so it can be undone
//! and replayed from the recovery journal like any other edit.

use gscore_edit::Command;
use gscore_model::{InstrumentKind, TrackIndex};
use gscore_play::{StringKind, TrackEngine};

use super::GuitarScoreApp;
use super::song_state::{Buses, DrumPartSound, SoundEngine, TrackSound};
use super::tab::Tab;
use super::tone_picker::{PickerChoice, current_tone_choice};
use crate::chain_settings::{ChainSetting, DrumPart, Tone};
use crate::playback::{Heard, MidiOutRoute, TrackDrumPart};
use crate::settings::{ChainPreset, CompressorBus, ReverbBus};

/// What plays a track, and why.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TrackPlayer {
    /// What plays it now.
    pub(super) engine: TrackEngine,
    /// What would without the user's choice.
    pub(super) default: TrackEngine,
    /// Whether modelled strings may play it (a string instrument that is not a drum kit).
    pub(super) strings: bool,
    /// Whether it is a drum kit (its sound is split into the kit's parts).
    pub(super) drums: bool,
    /// The kind of string its preset stands for, if it is a guitar or bass preset.
    pub(super) preset_kind: Option<StringKind>,
    /// Its General MIDI program and name, for the SoundFont line.
    pub(super) program: u8,
    /// The port and channel its hardware synthesizer plays, when it is a MIDI-out track.
    pub(super) midi_out: MidiOutRoute,
}

/// What the track panel's Sound section shows: what plays the track, the chain its sound
/// passes through, the tone it plays now, and the choices its picker offers.
#[derive(Debug, Clone)]
pub(super) struct SoundPanel {
    pub(super) player: TrackPlayer,
    /// The chain the track's sound passes through, as the Sound chain window shows it
    /// (empty for a MIDI-synth track with no tone of its own, which has none yet).
    pub(super) chain: ChainSetting,
    /// The built-in or saved tone that matches `tone` exactly, when one does.
    pub(super) current: Option<PickerChoice>,
    /// The built-in tone the track's program starts on, if it has one.
    pub(super) default_tone: Option<String>,
    /// The user's saved tones, for the picker's own group.
    pub(super) saved: Vec<ChainPreset>,
    /// The MIDI output ports this machine has (for the external source).
    pub(super) ports: Vec<String>,
}

impl GuitarScoreApp {
    /// What plays `track` of the open document, if there is one.
    pub(super) fn track_player(&self, track: usize) -> Option<TrackPlayer> {
        self.track_player_in(self.tabs.get(self.active)?, track)
    }

    /// What plays `track` of `tab`'s document (which need not be the open one: a save may
    /// run for another tab).
    pub(super) fn track_player_in(&self, tab: &Tab, track: usize) -> Option<TrackPlayer> {
        let song = tab.document.song();
        let view = song.track(TrackIndex::new(track))?;
        let sound = song.track_sound(TrackIndex::new(track))?;
        let own = tab.track_sound(track);
        let strings = view.instrument_kind() == InstrumentKind::Strings && !sound.percussion;
        // The kind of string its program stands for.
        let preset_kind = StringKind::for_program(sound.program).filter(|_| strings);
        let default = match preset_kind {
            Some(kind) if self.playback.modelled_guitars() => TrackEngine::Modelled(kind),
            _ => TrackEngine::SoundFont,
        };
        let engine = own
            .engine
            .map(SoundEngine::engine)
            .filter(|engine| match engine {
                TrackEngine::Modelled(_) => strings,
                TrackEngine::SoundFont => true,
                // A browser has no MIDI ports; such a track falls back to the SoundFont.
                TrackEngine::MidiOut => !cfg!(target_arch = "wasm32"),
            })
            .unwrap_or(default);
        Some(TrackPlayer {
            engine,
            default,
            strings,
            drums: sound.percussion,
            preset_kind,
            program: sound.program,
            midi_out: own.midi_out.unwrap_or_default(),
        })
    }

    /// The Sound section's data for `track` of the open document, if there is one.
    pub(super) fn sound_panel(&self, track: usize) -> Option<SoundPanel> {
        let player = self.track_player(track)?;
        let kind = match player.engine {
            TrackEngine::Modelled(kind) => kind,
            TrackEngine::SoundFont | TrackEngine::MidiOut => {
                player.preset_kind.unwrap_or(StringKind::Steel)
            }
        };
        let mut tone = self.track_tone(track, kind);
        // Only a modelled track (or a track whose song carries a tone) has effects: the
        // MIDI synth's preset has none until the song gives it one.
        let modelled = matches!(player.engine, TrackEngine::Modelled(_));
        let song_tone = self
            .tabs
            .get(self.active)
            .is_some_and(|tab| tab.track_sound(track).tone.is_some());
        if !modelled && !player.drums && !song_tone {
            tone.chain = ChainSetting::default();
        }
        let default_tone = self
            .playback
            .track_default_tone_name(track, player.program, kind);
        Some(SoundPanel {
            player,
            chain: tone.chain.clone(),
            current: current_tone_choice(
                &tone,
                default_tone.as_deref(),
                &self.settings.string_presets,
            ),
            default_tone,
            saved: self.settings.string_presets.clone(),
            ports: self.playback.ports().to_vec(),
        })
    }

    /// Gives `track` `tone` as its own sound (kept with the song, part A), played by
    /// modelled strings, and plays the short Listen phrase unless the song is playing.
    /// This is the picker's choice: it belongs to this track alone.
    pub(super) fn choose_track_tone(&mut self, track: usize, tone: Tone) {
        let Some(player) = self.track_player(track) else {
            return;
        };
        let Some(mut sound) = self.open_track_sound(track) else {
            return;
        };
        sound.tone = Some(crate::settings::PresetTone::of(player.program, &tone));
        let engine = TrackEngine::Modelled(tone.kind);
        sound.engine = (engine != player.default).then(|| SoundEngine::of(engine));
        self.set_track_sound(track, sound, false);
        // The preview plays through the tone's whole chain: give playback the new tone
        // before it starts, not on the next frame.
        self.sync_track_tones();
        if player.strings {
            match self.playback.play_string_demo(track, tone.kind) {
                Ok(Heard::Skipped(why)) if why == "the song is playing" => {}
                Ok(Heard::Skipped(why)) => self.notice = Some(format!("Not played: {why}")),
                Ok(_) => {}
                Err(error) => self.notice = Some(error),
            }
        }
    }

    /// Notes whether the primary pointer button is held, and ends the undo step a sound
    /// drag was merging into when a new press begins (the Sound chain and bus windows'
    /// knobs and sliders do not mark their drags themselves). Called once per frame,
    /// before the windows run.
    pub(super) fn track_pointer(&mut self, ctx: &egui::Context) {
        let (pressed, held) =
            ctx.input(|input| (input.pointer.any_pressed(), input.pointer.primary_down()));
        if pressed && let Some(tab) = self.tabs.get_mut(self.active) {
            tab.document.editor_mut().break_merge();
        }
        self.pointer_held = held;
    }

    /// Whether a sound change asked to `merge` may continue the undo step before it: only
    /// within one drag, never across two drags or a typed value.
    fn sound_merge(&self, merge: bool) -> bool {
        merge && self.pointer_held
    }

    /// The sound state of `track` of the open document (`None` without one).
    fn open_track_sound(&self, track: usize) -> Option<TrackSound> {
        self.tabs.get(self.active).map(|tab| tab.track_sound(track))
    }

    /// Replaces the track's state with `sound`, through the undoable command.
    fn set_track_sound(&mut self, track: usize, sound: TrackSound, merge: bool) {
        let state = sound.json();
        let merge = self.sound_merge(merge);
        self.apply_command(
            Command::SetTrackSound {
                track: TrackIndex::new(track),
                state,
            },
            merge,
        );
    }

    /// Lets `engine` play `track` (back to the default when it is the default).
    pub(super) fn set_track_engine(&mut self, track: usize, engine: TrackEngine) {
        let Some(player) = self.track_player(track) else {
            return;
        };
        let Some(mut sound) = self.open_track_sound(track) else {
            return;
        };
        sound.engine = (engine != player.default).then(|| SoundEngine::of(engine));
        // A MIDI-out track needs a route; automatic until the user picks a port.
        if engine == TrackEngine::MidiOut {
            sound.midi_out.get_or_insert_with(MidiOutRoute::default);
            // The Sound chain window offers the ports by name, so list them if that has
            // not happened yet.
            if self.playback.ports().is_empty() {
                let _ = self.playback.refresh_ports();
            }
        }
        self.set_track_sound(track, sound, false);
    }

    /// Gives the hardware synthesizer of `track` a port and channel, heard at once.
    pub(super) fn set_midi_out(&mut self, track: usize, route: MidiOutRoute) {
        let Some(mut sound) = self.open_track_sound(track) else {
            return;
        };
        sound.midi_out = Some(route);
        self.set_track_sound(track, sound, false);
    }

    /// The name of `track` of the open document (empty without one).
    pub(super) fn track_name(&self, track: usize) -> String {
        self.tabs
            .get(self.active)
            .and_then(|tab| tab.document.song().track(TrackIndex::new(track)))
            .map(|view| view.name().to_owned())
            .unwrap_or_default()
    }

    /// The tone `track` of the open document plays as `kind`: the one that came with the
    /// song, else its program's default.
    pub(super) fn track_tone(&self, track: usize, kind: StringKind) -> Tone {
        match self.tabs.get(self.active) {
            Some(tab) => self.track_tone_in(tab, track, kind),
            None => self.playback.default_tone_for(0, kind),
        }
    }

    /// [`Self::track_tone`] for `track` of `tab`'s document.
    pub(super) fn track_tone_in(&self, tab: &Tab, track: usize, kind: StringKind) -> Tone {
        // A bass track always plays its program's built-in tone (see `Playback::tone`), so
        // the tone the song carries is not shown as the current one.
        let own = if kind == StringKind::Bass {
            None
        } else {
            tab.track_sound(track)
                .tone
                .and_then(|preset| preset.tone())
                .filter(|tone| tone.kind == kind)
        };
        let player = self.track_player_in(tab, track);
        let mut tone = own.unwrap_or_else(|| match &player {
            Some(player) => self.playback.tone(track, player.program, kind),
            None => self.playback.default_tone_for(0, kind),
        });
        // A drum track with no tone of its own runs the drum bus, as playback does.
        if player.is_some_and(|player| player.drums) {
            tone.chain = self.playback.drum_bus_chain();
        }
        tone
    }

    /// Gives `track` `tone` as its own, kept with the song: chosen from the tone picker,
    /// or edited in the Sound chain window. An edit with
    /// undo, so the change marks the song modified and is replayed after a crash.
    pub(super) fn set_track_tone(&mut self, track: usize, tone: Tone) {
        self.set_track_tone_merged(track, tone, true);
    }

    /// [`Self::set_track_tone`]; `merge` continues the previous undo step (one knob drag).
    fn set_track_tone_merged(&mut self, track: usize, tone: Tone, merge: bool) {
        let Some(player) = self.track_player(track) else {
            return;
        };
        let Some(mut sound) = self.open_track_sound(track) else {
            return;
        };
        // The song keeps what this track now plays, undoable at once.
        sound.tone = Some(crate::settings::PresetTone::of(player.program, &tone));
        self.set_track_sound(track, sound, merge);
    }

    /// Puts `block` into the first empty place of `track`'s chain: a block dropped on the
    /// track's mixer strip or copied onto its board. Hears it at once, undoable, and opens
    /// that chain window so the block is seen.
    pub(super) fn paste_block_into_track(
        &mut self,
        track: usize,
        block: crate::chain_settings::BlockSetting,
    ) {
        let Some(player) = self.track_player(track) else {
            return;
        };
        if player.drums {
            self.notice = Some(
                "A drum kit's own chain is its parts: drop the block on a part's strip (Kick, \
                 Snare, …) or open its sound chain and choose a part."
                    .to_owned(),
            );
            return;
        }
        let kind = match player.engine {
            TrackEngine::Modelled(kind) => kind,
            TrackEngine::SoundFont | TrackEngine::MidiOut => {
                player.preset_kind.unwrap_or(StringKind::Steel)
            }
        };
        let mut tone = self.track_tone(track, kind);
        if let Some(place) = super::sound_chain::first_empty(&tone.chain) {
            if let Some(slot) = tone.chain.blocks.get_mut(place) {
                *slot = block;
            }
            self.set_track_tone_merged(track, tone, false);
            self.open_sound_chain(track);
        } else {
            self.notice = Some(format!(
                "{}'s chain has no empty place: open it and paste the block into a place.",
                self.track_name(track)
            ));
        }
    }

    /// [`Self::paste_block_into_track`] for a drum part's chain (also what a drop on the
    /// part's strip or its button does).
    pub(super) fn paste_block_into_part(
        &mut self,
        track: usize,
        part: DrumPart,
        block: crate::chain_settings::BlockSetting,
    ) {
        let mut own = self.track_drum_part(track, part);
        if let Some(place) = super::sound_chain::first_empty(&own.chain) {
            if let Some(slot) = own.chain.blocks.get_mut(place) {
                *slot = block;
            }
            self.set_track_drum_part(track, part, Some(own), false);
            self.open_sound_chain_part(track, part);
        } else {
            self.notice = Some(format!(
                "The {}'s chain has no empty place: open it and paste the block into a \
                 place.",
                part.label()
            ));
        }
    }

    /// The kit's room as the song has it (its own, or the built-in short one).
    pub(super) fn drum_room(&self, track: usize) -> crate::chain_settings::BlockSetting {
        self.open_track_sound(track)
            .and_then(|sound| sound.drum_room)
            .unwrap_or_else(crate::chain_settings::default_drum_room)
    }

    /// Gives the kit `track` its room (a reverb, always all wet), heard at once and kept
    /// with the song.
    pub(super) fn set_track_drum_room(
        &mut self,
        track: usize,
        room: crate::chain_settings::BlockSetting,
        merge: bool,
    ) {
        let Some(mut sound) = self.open_track_sound(track) else {
            return;
        };
        sound.drum_room = Some(room);
        self.set_track_sound(track, sound, merge);
    }

    /// Sets the kit `track`'s own send into its room (0-127), heard at once.
    pub(super) fn set_track_drum_room_send(&mut self, track: usize, value: u8) {
        let Some(mut sound) = self.open_track_sound(track) else {
            return;
        };
        sound.drum_room_send = value.min(127);
        // One slider drag is one undo step.
        self.set_track_sound(track, sound, true);
    }

    /// Hands the open document's kits' rooms and own sends to playback.
    pub(super) fn sync_drum_rooms(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let song = tab.document.song();
        let rooms: std::collections::BTreeMap<usize, crate::playback::DrumRoomChoice> = (0..song
            .track_count())
            .filter(|&track| {
                song.track(TrackIndex::new(track))
                    .is_some_and(|view| view.is_percussion())
            })
            .map(|track| {
                let sound = tab.track_sound(track);
                (
                    track,
                    crate::playback::DrumRoomChoice {
                        room: sound
                            .drum_room
                            .unwrap_or_else(crate::chain_settings::default_drum_room),
                        send: sound.drum_room_send,
                    },
                )
            })
            .collect();
        self.playback.set_drum_rooms(rooms);
    }

    /// Forgets the tone that came with the song for `track`, so it plays its program's
    /// default again.
    pub(super) fn reset_track_tone(&mut self, track: usize) {
        let Some(mut sound) = self.open_track_sound(track) else {
            return;
        };
        if sound.tone.take().is_some() {
            self.set_track_sound(track, sound, false);
        }
    }

    /// Sets `track`'s send into the mixer's compressor bus (0-127), heard at once. It is
    /// kept with the song (in its notes), not with the settings.
    pub(super) fn set_comp_send(&mut self, track: usize, value: u8) {
        let Some(mut sound) = self.open_track_sound(track) else {
            return;
        };
        sound.comp_send = value.min(127);
        // One slider drag is one undo step.
        self.set_track_sound(track, sound, true);
    }

    /// Sets `track`'s input trim before its chain, in dB, heard at once. It is kept with
    /// the song, not with the settings.
    pub(super) fn set_track_trim(&mut self, track: usize, db: f32) {
        let Some(mut sound) = self.open_track_sound(track) else {
            return;
        };
        sound.trim_db = Some(if db.is_finite() {
            db.clamp(-24.0, 24.0)
        } else {
            self.default_trim_db(track)
        });
        // One slider drag is one undo step.
        self.set_track_sound(track, sound, true);
    }

    /// Hands the open document's input trims to playback, so they are heard.
    pub(super) fn sync_trims(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let trims: Vec<(usize, f32)> = (0..tab.sound().tracks.len())
            .map(|track| (track, self.track_trim_db_in(tab, track)))
            .collect();
        self.playback.set_trims(trims);
    }

    /// The input trim `track` of `tab`'s document plays with: the song's own, else where
    /// its player starts (the modelled strings lower than a SoundFont or MIDI-out track).
    pub(super) fn track_trim_db_in(&self, tab: &Tab, track: usize) -> f32 {
        tab.track_sound(track)
            .trim_db
            .unwrap_or_else(|| self.default_trim_db_in(tab, track))
    }

    /// Where `track` of the open document's input trim starts, for what plays it now.
    pub(super) fn default_trim_db(&self, track: usize) -> f32 {
        self.tabs
            .get(self.active)
            .map_or(0.0, |tab| self.default_trim_db_in(tab, track))
    }

    /// The same for `tab`'s document.
    fn default_trim_db_in(&self, tab: &Tab, track: usize) -> f32 {
        self.track_player_in(tab, track).map_or(0.0, |player| {
            crate::chain_settings::default_trim_db(player.engine)
        })
    }

    /// Hands the open document's comp sends to playback, so they are heard.
    pub(super) fn sync_comp_sends(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let sends: Vec<(usize, u8)> = tab
            .sound()
            .tracks
            .iter()
            .enumerate()
            .map(|(track, sound)| (track, sound.comp_send))
            .filter(|&(_, value)| value != 0)
            .collect();
        self.playback.set_comp_sends(sends);
    }

    /// Hands the open document's own tones to playback, so a song's sound is heard.
    pub(super) fn sync_track_tones(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let tones: std::collections::BTreeMap<usize, Tone> = tab
            .sound()
            .tracks
            .iter()
            .enumerate()
            .filter_map(|(track, sound)| Some((track, sound.tone.as_ref()?.tone()?)))
            .collect();
        // A track made with a Guitar Pro sound the tone library maps to a tone starts on
        // that tone instead of its program's.
        let song = tab.document.song();
        let rse_tones: std::collections::BTreeMap<usize, String> = (0..song.track_count())
            .filter_map(|track| {
                let key = crate::rse_sound::of_track(song, track)?;
                Some((track, crate::tone_library::for_rse(&key)?.name.clone()))
            })
            .collect();
        self.playback.set_track_tones(&tones);
        self.playback.set_track_rse_tones(rse_tones);
    }

    /// Hands the open document's MIDI-out port and channel choices to playback. A track
    /// played by MIDI out always has a route, automatic until the user picks a port, so
    /// its notes are never routed to an output nobody opened.
    pub(super) fn sync_midi_routes(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let song = tab.document.song();
        let sound = tab.sound();
        let routes: Vec<(usize, MidiOutRoute)> = (0..song.track_count())
            .filter_map(|track| {
                if let Some(route) = sound.tracks.get(track).and_then(|own| own.midi_out.clone()) {
                    return Some((track, route));
                }
                let player = self.track_player_in(tab, track)?;
                (player.engine == TrackEngine::MidiOut).then_some((track, player.midi_out))
            })
            .collect();
        if let Err(error) = self.playback.set_midi_routes(song, routes) {
            self.notice = Some(error);
        }
    }

    /// Hands the open document's engine choices to playback (a change rebuilds the
    /// sequence; nothing happens when they are the same).
    pub(super) fn sync_track_engines(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let song = tab.document.song();
        let engines: Vec<(usize, TrackEngine)> = tab
            .sound()
            .tracks
            .iter()
            .enumerate()
            .filter_map(|(track, sound)| Some((track, sound.engine?.engine())))
            .collect();
        if let Err(error) = self.playback.set_track_engines(song, engines) {
            self.notice = Some(error);
        }
    }

    /// Hands everything the song says about its sound to playback: what plays each track,
    /// its MIDI route, its tone, its compressor send, its input trim, the drum parts it
    /// gives its own tones, and the mixer's buses. Called after every edit; the player drops what did not
    /// change.
    pub(super) fn sync_song_sound(&mut self) {
        self.sync_track_engines();
        self.sync_midi_routes();
        self.sync_track_tones();
        self.sync_comp_sends();
        self.sync_trims();
        self.sync_track_drum_parts();
        self.sync_drum_rooms();
        self.sync_song_buses();
        self.sound_synced = self.sound_key();
    }

    /// Hands the open song's sound to playback when it is another song or the song changed
    /// since the last time (each frame: another tab may be open now). The buses are always
    /// compared, because a song without its own follows this machine's settings.
    pub(super) fn sync_song_sound_if_changed(&mut self) {
        if self.sound_key() == self.sound_synced {
            self.sync_song_buses();
        } else {
            self.sync_song_sound();
        }
    }

    /// Which song's state playback was last given: the open tab and its revision.
    fn sound_key(&self) -> Option<(super::tab::TabId, u64)> {
        self.tabs
            .get(self.active)
            .map(|tab| (tab.id, tab.document.editor().revision()))
    }

    /// The chain, mixing level, mute and solo `part` of `track` plays: the song's own,
    /// else this machine's kit setting (or the built-in tone) at unity.
    pub(super) fn track_drum_part(&self, track: usize, part: DrumPart) -> TrackDrumPart {
        let own = self.open_track_sound(track).and_then(|sound| {
            sound
                .drum_parts
                .into_iter()
                .find(|own| own.part() == Some(part))
        });
        match own {
            Some(own) => own.track_part().unwrap_or(TrackDrumPart {
                part,
                chain: self.playback.kit_drum_part_chain(part),
                level: 127,
                pan: 64,
                muted: false,
                soloed: false,
                room: 0,
            }),
            None => TrackDrumPart {
                part,
                chain: self.playback.kit_drum_part_chain(part),
                level: 127,
                pan: 64,
                muted: false,
                soloed: false,
                room: 0,
            },
        }
    }

    /// Gives `part` of `track` settings of the song's own (a chain, level, pan, mute and
    /// solo),
    /// or `None` to follow this machine's kit again. An edit, with undo, heard at once.
    pub(super) fn set_track_drum_part(
        &mut self,
        track: usize,
        part: DrumPart,
        own: Option<TrackDrumPart>,
        merge: bool,
    ) {
        let Some(mut sound) = self.open_track_sound(track) else {
            return;
        };
        let was_soloed = sound
            .drum_parts
            .iter()
            .find(|entry| entry.part() == Some(part))
            .is_some_and(|entry| entry.soloed);
        let now_soloed = own.as_ref().is_some_and(|own| own.soloed);
        sound.drum_parts.retain(|entry| entry.part() != Some(part));
        if let Some(own) = own {
            sound
                .drum_parts
                .push(DrumPartSound::of(&TrackDrumPart { part, ..own }));
        }
        // The kit's order, so the payload does not depend on the order parts were edited.
        sound
            .drum_parts
            .sort_by_key(|entry| entry.part().map_or(usize::MAX, DrumPart::index));
        // Soloing a part of a kit means hearing that kit: the track is soloed with it, so
        // the rest of the song falls silent. Un-soloing the last part un-soloes the track
        // again. One step with the part's own change.
        let any_soloed = sound
            .drum_parts
            .iter()
            .any(|entry| entry.soloed && entry.part().is_some());
        let track_soloed = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.document.song().track(TrackIndex::new(track)))
            .is_some_and(|view| view.is_soloed());
        if now_soloed != was_soloed && any_soloed != track_soloed {
            let state = sound.json();
            let merge = self.sound_merge(merge);
            self.apply_command(
                Command::Batch {
                    commands: vec![
                        Command::SetTrackSound {
                            track: TrackIndex::new(track),
                            state,
                        },
                        Command::SetTrackSolo {
                            track: TrackIndex::new(track),
                            solo: any_soloed,
                        },
                    ],
                },
                merge,
            );
        } else {
            self.set_track_sound(track, sound, merge);
        }
    }

    /// Hands the drum parts the open document gives its tracks to playback.
    pub(super) fn sync_track_drum_parts(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let parts: std::collections::BTreeMap<usize, Vec<TrackDrumPart>> = tab
            .sound()
            .tracks
            .iter()
            .enumerate()
            .filter_map(|(track, sound)| {
                let own: Vec<TrackDrumPart> = sound
                    .drum_parts
                    .iter()
                    .filter_map(DrumPartSound::track_part)
                    .collect();
                (!own.is_empty()).then_some((track, own))
            })
            .collect();
        self.playback.set_track_drum_parts(parts);
    }

    /// The song's own buses, each falling back to this machine's settings when the song
    /// does not set it.
    pub(super) fn song_buses(&self) -> Buses {
        let own = self
            .tabs
            .get(self.active)
            .map(|tab| tab.sound().buses.clone())
            .unwrap_or_default();
        Buses {
            reverb: own
                .reverb
                .or_else(|| Some(self.settings.reverb_bus.clone())),
            compressor: own
                .compressor
                .or_else(|| Some(self.settings.compressor_bus.clone())),
            chorus: own
                .chorus
                .or_else(|| Some(self.settings.chorus_bus.clone())),
            master: own
                .master
                .or_else(|| Some(self.settings.master_chain.clone())),
        }
    }

    /// The chain on the master: the song's own, else this machine's (empty by default).
    pub(super) fn song_master_chain(&self) -> ChainSetting {
        self.song_buses()
            .master
            .unwrap_or_else(|| self.settings.master_chain.clone())
    }

    /// Gives the open song its own reverb bus (an edit, with undo), heard at once.
    pub(super) fn set_song_reverb_bus(&mut self, bus: ReverbBus) {
        let Some(mut buses) = self
            .tabs
            .get(self.active)
            .map(|tab| tab.sound().buses.clone())
        else {
            return;
        };
        buses.reverb = Some(bus);
        let merge = self.sound_merge(true);
        self.apply_command(
            Command::SetSongSound {
                state: buses.json(),
            },
            merge,
        );
    }

    /// Gives the open song its own compressor bus (an edit, with undo), heard at once.
    pub(super) fn set_song_compressor_bus(&mut self, bus: CompressorBus) {
        let Some(mut buses) = self
            .tabs
            .get(self.active)
            .map(|tab| tab.sound().buses.clone())
        else {
            return;
        };
        buses.compressor = Some(bus);
        let merge = self.sound_merge(true);
        self.apply_command(
            Command::SetSongSound {
                state: buses.json(),
            },
            merge,
        );
    }

    /// Gives the open song its own chorus bus (an edit, with undo), heard at once.
    pub(super) fn set_song_chorus_bus(&mut self, bus: crate::settings::ChorusBus) {
        let Some(mut buses) = self
            .tabs
            .get(self.active)
            .map(|tab| tab.sound().buses.clone())
        else {
            return;
        };
        buses.chorus = Some(bus);
        let merge = self.sound_merge(true);
        self.apply_command(
            Command::SetSongSound {
                state: buses.json(),
            },
            merge,
        );
    }

    /// Gives the open song its own master chain (an edit, with undo), heard at once.
    pub(super) fn set_song_master_chain(&mut self, chain: ChainSetting) {
        let Some(mut buses) = self
            .tabs
            .get(self.active)
            .map(|tab| tab.sound().buses.clone())
        else {
            return;
        };
        buses.master = Some(chain);
        let merge = self.sound_merge(true);
        self.apply_command(
            Command::SetSongSound {
                state: buses.json(),
            },
            merge,
        );
    }

    /// Hands the open document's buses and master chain to playback, so a song's own
    /// reverb, compressor and chorus are heard (a song without them follows the
    /// settings). The player is sent a bus only when it differs from the one it runs.
    pub(super) fn sync_song_buses(&mut self) {
        let buses = self.song_buses();
        let reverb = buses.reverb.unwrap_or_default();
        if self.playback.reverb_bus() != reverb {
            self.playback.set_reverb_bus(reverb);
        }
        let compressor = buses.compressor.unwrap_or_default();
        if self.playback.compressor_bus() != compressor {
            self.playback.set_compressor_bus(compressor);
        }
        let chorus = buses.chorus.unwrap_or_default();
        if self.playback.chorus_bus() != chorus {
            self.playback.set_chorus_bus(chorus);
        }
        let master = buses.master.unwrap_or_default();
        if self.playback.master_chain() != master {
            self.playback.set_master_chain(master);
        }
    }
}
