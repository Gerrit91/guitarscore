//! What plays a track: the SoundFont, or modelled strings of a kind. By default a guitar
//! or bass track's preset decides (with the Settings' switch for modelled guitars); the
//! user can choose for each track in the Sound chain window. The choice is kept with the
//! open document (by track index) and sent to playback every frame it changes; so is each
//! track's own tone (the sound of its strings and its chain).

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
        let preset_kind = StringKind::for_program(sound.program).filter(|_| strings);
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

    /// The tone `track` of the open document plays as `kind`: its own, or the kind's
    /// default.
    pub(super) fn track_tone(&self, track: usize, kind: StringKind) -> Tone {
        let sounds = self.playback.string_sounds();
        let defaults = (&sounds, self.playback.string_chains());
        match self.tabs.get(self.active) {
            Some(tab) => crate::playback::tone_of(&tab.track_tones, defaults, track, kind),
            None => crate::playback::tone_of(&Default::default(), defaults, track, kind),
        }
    }

    /// Gives `track` of the open document its own tone (no other track changes), heard at
    /// once.
    pub(super) fn set_track_tone(&mut self, track: usize, tone: Tone) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        tab.track_tones.insert(track, tone);
        self.sync_track_tones();
    }

    /// Hands the open document's tones to playback (only what changed is sent).
    pub(super) fn sync_track_tones(&mut self) {
        if let Some(tab) = self.tabs.get(self.active) {
            self.playback.set_track_tones(&tab.track_tones);
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
