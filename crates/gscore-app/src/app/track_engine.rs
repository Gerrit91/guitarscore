//! What plays a track: the SoundFont, or modelled strings of a kind. By default a guitar
//! or bass track's preset decides (with the Settings' switch for modelled guitars); the
//! user can choose for each track in the Sound chain window. The choice is kept with the
//! open document (by track index) and sent to playback every frame it changes. A track's
//! tone (the sound of its strings and its chain) belongs to its preset, not to the track:
//! it is kept in the settings, so every track with that preset, in every song, plays it.

use gscore_model::{InstrumentKind, TrackIndex};
use gscore_play::{StringKind, TrackEngine};

use super::GuitarScoreApp;
use crate::chain_settings::Tone;
use crate::playback::MidiOutRoute;

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
        let strings = view.instrument_kind() == InstrumentKind::Strings && !sound.percussion;
        // The kind its preset's tone was dialed in on (the owner's Clean Guitar plays steel
        // strings), otherwise its program's family.
        let preset_kind = self.playback.preset_kind(sound.program).filter(|_| strings);
        let default = match preset_kind {
            Some(kind) if self.playback.modelled_guitars() => TrackEngine::Modelled(kind),
            _ => TrackEngine::SoundFont,
        };
        let engine = tab
            .track_engines
            .get(&track)
            .copied()
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
            midi_out: tab.midi_outs.get(&track).cloned().unwrap_or_default(),
        })
    }

    /// Lets `engine` play `track` (back to the default when it is the default).
    pub(super) fn set_track_engine(&mut self, track: usize, engine: TrackEngine) {
        let Some(player) = self.track_player(track) else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if engine == player.default {
            tab.track_engines.remove(&track);
        } else {
            tab.track_engines.insert(track, engine);
        }
        // A MIDI-out track needs a route; automatic until the user picks a port.
        if engine == TrackEngine::MidiOut {
            tab.midi_outs.entry(track).or_default();
            // The Sound chain window offers the ports by name, so list them if that has
            // not happened yet.
            if self.playback.ports().is_empty() {
                let _ = self.playback.refresh_ports();
            }
        }
        // The engine first: a MIDI-out track needs its player, then the route splits the
        // rebuilt sequence.
        self.sync_track_engines();
        self.sync_midi_routes();
    }

    /// Gives the hardware synthesizer of `track` a port and channel, heard at once.
    pub(super) fn set_midi_out(&mut self, track: usize, route: MidiOutRoute) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        tab.midi_outs.insert(track, route);
        self.sync_midi_routes();
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
            .and_then(|tab| tab.track_tones.get(&track))
            .filter(|tone| tone.kind == kind)
            .cloned();
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
        if let Some(tab) = self.tabs.get_mut(self.active)
            && tab.track_tones.contains_key(&track)
        {
            tab.track_tones.insert(track, tone);
            self.sync_track_tones();
            return;
        }
        let Some(player) = self.track_player(track) else {
            return;
        };
        let Some(song) = self.document().map(|document| document.song().clone()) else {
            return;
        };
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
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.track_tones.remove(&track);
        }
        self.sync_track_tones();
        let Some(player) = self.track_player(track) else {
            return;
        };
        let Some(song) = self.document().map(|document| document.song().clone()) else {
            return;
        };
        if let Err(error) = self.playback.set_preset_tone(&song, player.program, None) {
            self.notice = Some(error);
        }
    }

    /// Sets `track`'s send into the mixer's compressor bus (0-127), heard at once. It is
    /// kept with the song (in its notes), not with the settings.
    pub(super) fn set_comp_send(&mut self, track: usize, value: u8) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if value == 0 {
            tab.comp_sends.remove(&track);
        } else {
            tab.comp_sends.insert(track, value.min(127));
        }
        self.sync_comp_sends();
    }

    /// Hands the open document's comp sends to playback, so they are heard.
    pub(super) fn sync_comp_sends(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let sends: Vec<(usize, u8)> = tab
            .comp_sends
            .iter()
            .map(|(&track, &value)| (track, value))
            .collect();
        self.playback.set_comp_sends(sends);
    }

    /// Hands the open document's own tones to playback, so a song's sound is heard.
    pub(super) fn sync_track_tones(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        self.playback.set_track_tones(&tab.track_tones);
    }

    /// Hands the open document's MIDI-out port and channel choices to playback.
    pub(super) fn sync_midi_routes(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let routes: Vec<(usize, MidiOutRoute)> = tab
            .midi_outs
            .iter()
            .map(|(&track, route)| (track, route.clone()))
            .collect();
        let song = tab.document.song();
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
        let engines: Vec<(usize, TrackEngine)> = tab
            .track_engines
            .iter()
            .map(|(&track, &engine)| (track, engine))
            .collect();
        let song = tab.document.song();
        if let Err(error) = self.playback.set_track_engines(song, engines) {
            self.notice = Some(error);
        }
    }
}
