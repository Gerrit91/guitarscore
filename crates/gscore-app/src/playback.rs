//! Playback in the app: the MIDI output port, the player, and the sequence of the open
//! song. The player runs on its own thread (`gscore-audio`); this only sends it commands,
//! so the UI never waits for the device.

use std::sync::Arc;

use gscore_audio::{MidiOutPlayer, midi_output_ports};
use gscore_model::Song;
use gscore_play::{PlayState, Player, Sequence};

/// The app's playback state.
#[derive(Debug, Default)]
pub struct Playback {
    /// Output ports, as last listed.
    ports: Vec<String>,
    /// The chosen port; `None` picks the first port that makes sound.
    port: Option<String>,
    /// Connected on the first play, dropped when the port changes.
    player: Option<MidiOutPlayer>,
    /// The open song's sequence, built on the first play.
    sequence: Option<Arc<Sequence>>,
    /// The output error last reported to the user, so it is reported once.
    reported: Option<String>,
}

impl Playback {
    /// Lists the output ports again.
    pub fn refresh_ports(&mut self) -> Result<(), String> {
        self.ports = midi_output_ports().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// The output ports, as last listed.
    pub fn ports(&self) -> &[String] {
        &self.ports
    }

    /// The chosen port, if one was chosen.
    pub fn port(&self) -> Option<&str> {
        self.port.as_deref()
    }

    /// The port the player is connected to, if it is.
    pub fn connected_port(&self) -> Option<&str> {
        self.player.as_ref().map(MidiOutPlayer::port)
    }

    /// Chooses the output port; the player reconnects on the next play.
    pub fn choose_port(&mut self, port: Option<String>) {
        if port != self.port {
            self.player = None;
            self.sequence = None;
            self.port = port;
        }
    }

    /// A different song is open: stop and forget the old sequence.
    pub fn song_changed(&mut self) {
        if let Some(player) = &mut self.player {
            let _ = player.stop();
        }
        self.sequence = None;
    }

    /// Plays `song` from where it was paused or stopped, or pauses it if it is playing.
    pub fn play_pause(&mut self, song: &Song) -> Result<(), String> {
        if self.state() == PlayState::Playing {
            return self.player_mut()?.pause().map_err(|e| e.to_string());
        }
        if self.player.is_none() {
            self.player =
                Some(MidiOutPlayer::connect(self.port.as_deref()).map_err(|e| e.to_string())?);
            self.sequence = None;
        }
        if self.sequence.is_none() {
            let sequence = Arc::new(Sequence::new(song, None).map_err(|e| e.to_string())?);
            self.player_mut()?
                .load(Arc::clone(&sequence))
                .map_err(|e| e.to_string())?;
            self.sequence = Some(sequence);
        }
        self.player_mut()?.play().map_err(|e| e.to_string())
    }

    /// Stops and goes back to the start.
    pub fn stop(&mut self) -> Result<(), String> {
        match &mut self.player {
            Some(player) => player.stop().map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }

    /// Playing, paused or stopped.
    pub fn state(&self) -> PlayState {
        self.player
            .as_ref()
            .map_or(PlayState::Stopped, Player::state)
    }

    /// The measure being played (1-based), while playing or paused.
    pub fn current_measure(&self) -> Option<usize> {
        let (player, sequence) = (self.player.as_ref()?, self.sequence.as_ref()?);
        if player.state() == PlayState::Stopped {
            return None;
        }
        let span = sequence.measure_at(player.position())?;
        Some(span.measure.get().saturating_add(1))
    }

    /// The last output error, if playback stopped because of one.
    pub fn error(&self) -> Option<String> {
        self.player.as_ref().and_then(MidiOutPlayer::error)
    }

    /// An output error that has not been reported yet.
    pub fn new_error(&mut self) -> Option<String> {
        let error = self.error();
        if error == self.reported {
            return None;
        }
        self.reported.clone_from(&error);
        error
    }

    fn player_mut(&mut self) -> Result<&mut MidiOutPlayer, String> {
        self.player
            .as_mut()
            .ok_or_else(|| "no MIDI output is connected".to_owned())
    }
}
