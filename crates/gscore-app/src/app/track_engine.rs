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

/// What plays a track, and why.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TrackPlayer {
    /// What plays it now.
    pub(super) engine: TrackEngine,
    /// What would without the user's choice.
    pub(super) default: TrackEngine,
    /// Whether modelled strings may play it (a string instrument that is not a drum kit).
    pub(super) strings: bool,
    /// The kind of string its preset stands for, if it is a guitar or bass preset.
    pub(super) preset_kind: Option<StringKind>,
    /// Its General MIDI program and name, for the SoundFont line.
    pub(super) program: u8,
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
            .filter(|engine| strings || *engine == TrackEngine::SoundFont)
            .unwrap_or(default);
        Some(TrackPlayer {
            engine,
            default,
            strings,
            preset_kind,
            program: sound.program,
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
        self.sync_track_engines();
    }

    /// The name of `track` of the open document (empty without one).
    pub(super) fn track_name(&self, track: usize) -> String {
        self.tabs
            .get(self.active)
            .and_then(|tab| tab.document.song().track(TrackIndex::new(track)))
            .map(|view| view.name().to_owned())
            .unwrap_or_default()
    }

    /// The tone `track` of the open document plays as `kind`: its preset's, or the kind's
    /// default.
    pub(super) fn track_tone(&self, track: usize, kind: StringKind) -> Tone {
        match self.track_player(track) {
            Some(player) => self.playback.tone(player.program, kind),
            None => self.playback.default_tone(kind),
        }
    }

    /// Gives the preset `track` belongs to `tone`: every track with that preset, in every
    /// song, plays it from now on (it is kept in the settings).
    pub(super) fn set_track_tone(&mut self, track: usize, tone: Tone) {
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

    /// Forgets the tone of the preset `track` belongs to: it plays its built-in default
    /// again (the kind's sound and chain from the settings).
    pub(super) fn reset_track_tone(&mut self, track: usize) {
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
