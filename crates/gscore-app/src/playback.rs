//! Playback in the app: the output (built-in synthesizer or a MIDI port), the player, and
//! the sequence of the open song. The player runs on its own thread (`gscore-audio`); this
//! only sends it commands, so the UI never waits for the device or the SoundFont.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gscore_audio::{MidiOutPlayer, SynthPlayer, midi_output_ports};
use gscore_model::{BeatPosition, Song};
use gscore_play::{PlayState, Player, Sequence};

/// Where playback goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// The built-in synthesizer, with this SoundFont.
    Synth(PathBuf),
    /// A MIDI output port; `None` picks the first port that makes sound.
    Midi(Option<String>),
}

impl Default for Output {
    fn default() -> Self {
        Output::Midi(None)
    }
}

/// The connected player.
#[derive(Debug)]
enum Active {
    Midi(MidiOutPlayer),
    Synth(SynthPlayer),
}

impl Active {
    fn player(&self) -> &dyn Player {
        match self {
            Active::Midi(p) => p,
            Active::Synth(p) => p,
        }
    }

    fn player_mut(&mut self) -> &mut dyn Player {
        match self {
            Active::Midi(p) => p,
            Active::Synth(p) => p,
        }
    }

    fn error(&self) -> Option<String> {
        match self {
            Active::Midi(p) => p.error(),
            Active::Synth(p) => p.error(),
        }
    }
}

/// The app's playback state.
#[derive(Debug, Default)]
pub struct Playback {
    /// MIDI output ports, as last listed.
    ports: Vec<String>,
    /// The chosen output.
    output: Output,
    /// The SoundFont found or chosen, offered in the menu even while MIDI is chosen.
    soundfont: Option<PathBuf>,
    /// Connected on the first play, dropped when the output changes.
    player: Option<Active>,
    /// The open song's sequence, built on the first play.
    sequence: Option<Arc<Sequence>>,
    /// The output error last reported to the user, so it is reported once.
    reported: Option<String>,
    /// Where the cursor was when playback paused: playing on from the same place resumes,
    /// from anywhere else starts there.
    paused_at: Option<BeatPosition>,
}

impl Playback {
    /// Lists the MIDI output ports again.
    pub fn refresh_ports(&mut self) -> Result<(), String> {
        self.ports = midi_output_ports().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// The MIDI output ports, as last listed.
    pub fn ports(&self) -> &[String] {
        &self.ports
    }

    /// The chosen output.
    pub fn output(&self) -> &Output {
        &self.output
    }

    /// The chosen MIDI port, if a port was chosen.
    pub fn port(&self) -> Option<&str> {
        match &self.output {
            Output::Midi(port) => port.as_deref(),
            Output::Synth(_) => None,
        }
    }

    /// The MIDI port the player is connected to, if it is.
    pub fn connected_port(&self) -> Option<&str> {
        match &self.player {
            Some(Active::Midi(p)) => Some(p.port()),
            _ => None,
        }
    }

    /// The SoundFont the built-in synthesizer uses when chosen.
    pub fn soundfont(&self) -> Option<&Path> {
        self.soundfont.as_deref()
    }

    /// Chooses the output; the player reconnects on the next play.
    pub fn choose_output(&mut self, output: Output) {
        if output != self.output {
            if let Some(active) = &mut self.player {
                let _ = active.player_mut().stop();
            }
            self.player = None;
            self.sequence = None;
            self.reported = None;
            self.output = output;
        }
    }

    /// Chooses a MIDI port (`None`: the first that makes sound).
    pub fn choose_port(&mut self, port: Option<String>) {
        self.choose_output(Output::Midi(port));
    }

    /// Plays with the built-in synthesizer and `soundfont` from now on.
    pub fn use_soundfont(&mut self, soundfont: PathBuf) {
        self.soundfont = Some(soundfont.clone());
        self.choose_output(Output::Synth(soundfont));
    }

    /// Whether the built-in synthesizer is still loading its SoundFont.
    pub fn is_loading(&self) -> bool {
        matches!(&self.player, Some(Active::Synth(p)) if p.is_loading())
    }

    /// A short description of the chosen output, for the toolbar.
    pub fn output_label(&self) -> String {
        match &self.output {
            Output::Synth(path) => format!("Synthesizer ({})", file_name(path)),
            Output::Midi(port) => match self.connected_port().or(port.as_deref()) {
                Some(port) => format!("MIDI: {port}"),
                None => "MIDI (automatic)".to_owned(),
            },
        }
    }

    /// A different song is open: stop and forget the old sequence.
    pub fn song_changed(&mut self) {
        if let Some(active) = &mut self.player {
            let _ = active.player_mut().stop();
        }
        self.sequence = None;
        self.paused_at = None;
    }

    /// Plays `song` from where it was paused or stopped, or pauses it if it is playing.
    ///
    /// Playing starts at `from` (the cursor's beat, at its measure's first pass), unless
    /// playback is paused and the cursor has not moved since: then it resumes. Without
    /// `from` it resumes, or starts at the beginning.
    pub fn play_pause(&mut self, song: &Song, from: Option<BeatPosition>) -> Result<(), String> {
        if self.state() == PlayState::Playing {
            self.paused_at = from;
            return self.player_mut()?.pause().map_err(|e| e.to_string());
        }
        // A synthesizer that failed (SoundFont, device) is started afresh: the problem
        // may have been fixed meanwhile, and it is reported again if not.
        if matches!(&self.player, Some(active @ Active::Synth(_)) if active.error().is_some()) {
            self.player = None;
            self.reported = None;
        }
        if self.player.is_none() {
            let active = match &self.output {
                Output::Synth(path) => {
                    Active::Synth(SynthPlayer::start(path).map_err(|e| e.to_string())?)
                }
                Output::Midi(port) => Active::Midi(
                    MidiOutPlayer::connect(port.as_deref()).map_err(|e| e.to_string())?,
                ),
            };
            self.player = Some(active);
            self.sequence = None;
        }
        let resume =
            self.sequence.is_some() && self.state() == PlayState::Paused && self.paused_at == from;
        self.paused_at = None;
        let sequence = match &self.sequence {
            Some(sequence) => Arc::clone(sequence),
            None => {
                let sequence = Arc::new(Sequence::new(song, None).map_err(|e| e.to_string())?);
                self.player_mut()?
                    .load(Arc::clone(&sequence))
                    .map_err(|e| e.to_string())?;
                self.sequence = Some(Arc::clone(&sequence));
                sequence
            }
        };
        if !resume && let Some(tick) = from.and_then(|at| sequence.tick_of(song, at)) {
            self.player_mut()?.seek(tick).map_err(|e| e.to_string())?;
        }
        self.player_mut()?.play().map_err(|e| e.to_string())
    }

    /// Stops and goes back to the start.
    pub fn stop(&mut self) -> Result<(), String> {
        self.paused_at = None;
        match &mut self.player {
            Some(active) => active.player_mut().stop().map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }

    /// Playing, paused or stopped.
    pub fn state(&self) -> PlayState {
        self.player
            .as_ref()
            .map_or(PlayState::Stopped, |a| a.player().state())
    }

    /// The position in ticks (0 without a player).
    pub fn position(&self) -> u64 {
        self.player.as_ref().map_or(0, |a| a.player().position())
    }

    /// The measure being played (1-based), while playing or paused.
    pub fn current_measure(&self) -> Option<usize> {
        let (active, sequence) = (self.player.as_ref()?, self.sequence.as_ref()?);
        let player = active.player();
        if player.state() == PlayState::Stopped {
            return None;
        }
        let span = sequence.measure_at(player.position())?;
        Some(span.measure.get().saturating_add(1))
    }

    /// The last output error, if playback stopped because of one.
    pub fn error(&self) -> Option<String> {
        self.player.as_ref().and_then(Active::error)
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

    fn player_mut(&mut self) -> Result<&mut dyn Player, String> {
        self.player
            .as_mut()
            .map(Active::player_mut)
            .ok_or_else(|| "no output is connected".to_owned())
    }
}

/// The file name of `path`, or the whole path if it has none.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}
