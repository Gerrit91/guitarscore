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
use super::song_state::{Buses, SoundEngine, TrackSound};
use crate::chain_settings::Tone;
use crate::playback::MidiOutRoute;
use crate::settings::{CompressorBus, ReverbBus};

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

impl GuitarScoreApp {
    /// What plays `track` of the open document, if there is one.
    pub(super) fn track_player(&self, track: usize) -> Option<TrackPlayer> {
        let tab = self.tabs.get(self.active)?;
        let song = tab.document.song();
        let view = song.track(TrackIndex::new(track))?;
        let sound = song.track_sound(TrackIndex::new(track))?;
        let own = TrackSound::of_song(song, track);
        let strings = view.instrument_kind() == InstrumentKind::Strings && !sound.percussion;
        // The kind its preset's tone was dialed in on (the owner's Clean Guitar plays steel
        // strings), otherwise its program's family.
        let preset_kind = self.playback.preset_kind(sound.program).filter(|_| strings);
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

    /// Replaces the track's state with `sound`, through the undoable command.
    fn set_track_sound(&mut self, track: usize, sound: TrackSound, merge: bool) {
        let state = sound.json();
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
        let Some(song) = self.document().map(|document| document.song().clone()) else {
            return;
        };
        let mut sound = TrackSound::of_song(&song, track);
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
        let Some(song) = self.document().map(|document| document.song().clone()) else {
            return;
        };
        let mut sound = TrackSound::of_song(&song, track);
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
    /// song, else its preset's, else the kind's default.
    pub(super) fn track_tone(&self, track: usize, kind: StringKind) -> Tone {
        let own = self
            .tabs
            .get(self.active)
            .map(|tab| TrackSound::of_song(tab.document.song(), track))
            .and_then(|sound| sound.tone)
            .and_then(|preset| preset.tone())
            .filter(|tone| tone.kind == kind);
        let mut tone = own.unwrap_or_else(|| match self.track_player(track) {
            Some(player) => self.playback.tone(track, player.program, kind),
            None => self.playback.default_tone(kind),
        });
        // A drum track with no tone of its own runs the drum bus, as playback does.
        if self.track_player(track).is_some_and(|player| player.drums)
            && tone.chain == crate::chain_settings::ChainSetting::default()
        {
            tone.chain = self.playback.drum_bus_chain();
        }
        tone
    }

    /// Gives `track` `tone`. A track whose tone came with the song keeps its own (the song
    /// keeps its sound); otherwise the preset's tone changes, so every track with that
    /// preset, in every song, plays it from now on (it is kept in the settings).
    pub(super) fn set_track_tone(&mut self, track: usize, tone: Tone) {
        let Some(song) = self.document().map(|document| document.song().clone()) else {
            return;
        };
        let Some(player) = self.track_player(track) else {
            return;
        };
        let mut sound = TrackSound::of_song(&song, track);
        if sound.tone.is_some() {
            sound.tone = Some(crate::settings::PresetTone::of(player.program, &tone));
            self.set_track_sound(track, sound, true);
            return;
        }
        if let Err(error) = self
            .playback
            .set_preset_tone(&song, player.program, Some(tone))
        {
            self.notice = Some(error);
        }
    }

    /// Forgets the tone of `track`: the one that came with the song and the preset's own,
    /// so it plays its built-in default again (the kind's sound and chain from the
    /// settings).
    pub(super) fn reset_track_tone(&mut self, track: usize) {
        let Some(song) = self.document().map(|document| document.song().clone()) else {
            return;
        };
        let mut sound = TrackSound::of_song(&song, track);
        if sound.tone.take().is_some() {
            self.set_track_sound(track, sound, false);
        }
        let Some(player) = self.track_player(track) else {
            return;
        };
        if let Err(error) = self.playback.set_preset_tone(&song, player.program, None) {
            self.notice = Some(error);
        }
    }

    /// Sets `track`'s send into the mixer's compressor bus (0-127), heard at once. It is
    /// kept with the song (in its notes), not with the settings.
    pub(super) fn set_comp_send(&mut self, track: usize, value: u8) {
        let Some(song) = self.document().map(|document| document.song().clone()) else {
            return;
        };
        let mut sound = TrackSound::of_song(&song, track);
        sound.comp_send = value.min(127);
        // One slider drag is one undo step.
        self.set_track_sound(track, sound, true);
    }

    /// Hands the open document's comp sends to playback, so they are heard.
    pub(super) fn sync_comp_sends(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let song = tab.document.song();
        let sends: Vec<(usize, u8)> = (0..song.track_count())
            .map(|track| (track, TrackSound::of_song(song, track).comp_send))
            .filter(|&(_, value)| value != 0)
            .collect();
        self.playback.set_comp_sends(sends);
    }

    /// Hands the open document's own tones to playback, so a song's sound is heard.
    pub(super) fn sync_track_tones(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let song = tab.document.song();
        let tones: std::collections::BTreeMap<usize, Tone> = (0..song.track_count())
            .filter_map(|track| {
                let tone = TrackSound::of_song(song, track).tone?;
                Some((track, tone.tone()?))
            })
            .collect();
        self.playback.set_track_tones(&tones);
    }

    /// Hands the open document's MIDI-out port and channel choices to playback. A track
    /// played by MIDI out always has a route, automatic until the user picks a port, so
    /// its notes are never routed to an output nobody opened.
    pub(super) fn sync_midi_routes(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let song = tab.document.song();
        let routes: Vec<(usize, MidiOutRoute)> = (0..song.track_count())
            .filter_map(|track| {
                if let Some(route) = TrackSound::of_song(song, track).midi_out {
                    return Some((track, route));
                }
                let player = self.track_player(track)?;
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
        let engines: Vec<(usize, TrackEngine)> = (0..song.track_count())
            .filter_map(|track| {
                let engine = TrackSound::of_song(song, track).engine?;
                Some((track, engine.engine()))
            })
            .collect();
        if let Err(error) = self.playback.set_track_engines(song, engines) {
            self.notice = Some(error);
        }
    }

    /// Hands everything the song says about its sound to playback: what plays each track,
    /// its MIDI route, its tone, its compressor send and the mixer's buses. Called after
    /// every edit; the player drops what did not change.
    pub(super) fn sync_song_sound(&mut self) {
        self.sync_track_engines();
        self.sync_midi_routes();
        self.sync_track_tones();
        self.sync_comp_sends();
        self.sync_song_buses();
    }

    /// The song's own buses, each falling back to this machine's settings when the song
    /// does not set it.
    pub(super) fn song_buses(&self) -> Buses {
        let own = self
            .tabs
            .get(self.active)
            .map(|tab| Buses::of_song(tab.document.song()))
            .unwrap_or_default();
        Buses {
            reverb: own
                .reverb
                .or_else(|| Some(self.settings.reverb_bus.clone())),
            compressor: own
                .compressor
                .or_else(|| Some(self.settings.compressor_bus.clone())),
        }
    }

    /// Gives the open song its own reverb bus (an edit, with undo), heard at once.
    pub(super) fn set_song_reverb_bus(&mut self, bus: ReverbBus) {
        let Some(song) = self.document().map(|document| document.song().clone()) else {
            return;
        };
        let mut buses = Buses::of_song(&song);
        buses.reverb = Some(bus);
        self.apply_command(
            Command::SetSongSound {
                state: buses.json(),
            },
            true,
        );
    }

    /// Gives the open song its own compressor bus (an edit, with undo), heard at once.
    pub(super) fn set_song_compressor_bus(&mut self, bus: CompressorBus) {
        let Some(song) = self.document().map(|document| document.song().clone()) else {
            return;
        };
        let mut buses = Buses::of_song(&song);
        buses.compressor = Some(bus);
        self.apply_command(
            Command::SetSongSound {
                state: buses.json(),
            },
            true,
        );
    }

    /// Hands the open document's buses to playback, so a song's own reverb and compressor
    /// are heard (a song without them follows the settings). The player is sent a bus only
    /// when it differs from the one it runs.
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
    }
}
