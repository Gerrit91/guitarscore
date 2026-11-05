//! Playback in the app: the output (built-in synthesizer or a MIDI port), the player, and
//! the sequence of the open song. The player runs on its own thread (`gscore-audio`); this
//! only sends it commands, so the UI never waits for the device or the SoundFont.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::Instant;

#[cfg(target_arch = "wasm32")]
use gscore_audio::WebPlayer;
#[cfg(not(target_arch = "wasm32"))]
use gscore_audio::{MidiOutPlayer, SynthPlayer, midi_output_ports};
use gscore_model::{
    BeatIndex, BeatPosition, MeasureIndex, NoteKind, Song, StringNumber, TrackIndex, VoiceIndex,
};
mod preview;

pub use preview::Heard;

use gscore_play::{Channel, MidiEvent, PPQ, PlayState, Player, Sequence, sound_events};

/// How long an entered note sounds.
const AUDITION_LENGTH: Duration = Duration::from_millis(450);
/// The channel entered notes sound on (drums use the percussion channel). Playback is
/// stopped while a note is auditioned, and playing sends every channel's setup first.
const AUDITION_CHANNEL: u8 = 15;

/// A note sounding because it was just entered.
#[derive(Debug, Clone, Copy)]
struct Audition {
    channel: Channel,
    key: u8,
    until: Instant,
}

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
        // A browser has no MIDI ports; the site ships (or its owner chose) a SoundFont for
        // the built-in synthesizer, so that is the output. The path is only a label there.
        #[cfg(target_arch = "wasm32")]
        {
            Output::Synth(PathBuf::from(BROWSER_SOUNDFONT_NAME))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Output::Midi(None)
        }
    }
}

/// The bundled browser SoundFont's name, as the toolbar and the settings show it.
#[cfg(target_arch = "wasm32")]
pub const BROWSER_SOUNDFONT_NAME: &str = "GeneralUser GS 2.0.3";

/// The connected player.
#[derive(Debug)]
enum Active {
    #[cfg(not(target_arch = "wasm32"))]
    Midi(MidiOutPlayer),
    #[cfg(not(target_arch = "wasm32"))]
    Synth(SynthPlayer),
    /// The browser's AudioWorklet synthesizer.
    #[cfg(target_arch = "wasm32")]
    Web(WebPlayer),
}

impl Active {
    fn player(&self) -> &dyn Player {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Active::Midi(p) => p,
            #[cfg(not(target_arch = "wasm32"))]
            Active::Synth(p) => p,
            #[cfg(target_arch = "wasm32")]
            Active::Web(p) => p,
        }
    }

    fn player_mut(&mut self) -> &mut dyn Player {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Active::Midi(p) => p,
            #[cfg(not(target_arch = "wasm32"))]
            Active::Synth(p) => p,
            #[cfg(target_arch = "wasm32")]
            Active::Web(p) => p,
        }
    }

    fn error(&self) -> Option<String> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Active::Midi(p) => p.error(),
            #[cfg(not(target_arch = "wasm32"))]
            Active::Synth(p) => p.error(),
            #[cfg(target_arch = "wasm32")]
            Active::Web(p) => p.error(),
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
    /// Whether the metronome clicks on every beat while playing.
    metronome: bool,
    /// Whether one bar of clicks is played before playback starts.
    count_in: bool,
    /// While the count-in plays: where the song starts when it ends.
    count_in_from: Option<Option<BeatPosition>>,
    /// The song's tick when the count-in ends, for the time readout while counting in.
    count_in_start: Option<u64>,
    /// The song's sequence built for after the count-in (the player holds the clicks).
    count_in_sequence: Option<Arc<Sequence>>,
    /// Whether playback loops the selected measures.
    loop_selection: bool,
    /// The entered note that is sounding, until its note-off is due.
    audition: Option<Audition>,
    /// While a beat excerpt plays (a palette click is heard): the position the song was
    /// at before, reported meanwhile, as the song itself is stopped.
    preview: Option<u64>,
    /// The beat excerpt last played.
    preview_sequence: Option<Arc<Sequence>>,
    /// A beat to play once the synthesizer has loaded its SoundFont (it was chosen while
    /// it loaded).
    pending_preview: Option<Arc<Sequence>>,
}

impl Playback {
    /// Lists the MIDI output ports again.
    pub fn refresh_ports(&mut self) -> Result<(), String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.ports = midi_output_ports().map_err(|e| e.to_string())?;
        }
        // A browser has no MIDI port list; the worklet is the only output.
        #[cfg(target_arch = "wasm32")]
        {
            self.ports.clear();
        }
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
            #[cfg(not(target_arch = "wasm32"))]
            Some(Active::Midi(p)) => Some(p.port()),
            _ => None,
        }
    }

    /// The SoundFont the built-in synthesizer uses when chosen.
    pub fn soundfont(&self) -> Option<&Path> {
        self.soundfont.as_deref()
    }

    /// Downloaded and announced SoundFont bytes, while the browser loads it.
    pub fn load_progress(&self) -> Option<(u64, u64)> {
        #[cfg(target_arch = "wasm32")]
        {
            match &self.player {
                Some(Active::Web(player)) => player.load_progress(),
                _ => None,
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            None
        }
    }

    /// Why the browser's audio output failed, if it did (not consumed like `new_error`).
    pub fn browser_error(&self) -> Option<String> {
        #[cfg(target_arch = "wasm32")]
        {
            match &self.player {
                Some(Active::Web(player)) => player.error(),
                _ => None,
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            None
        }
    }

    /// The loaded browser SoundFont's name, if it is known.
    pub fn soundfont_name(&self) -> Option<String> {
        #[cfg(target_arch = "wasm32")]
        {
            match &self.player {
                Some(Active::Web(player)) => player.soundfont_name(),
                _ => None,
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.soundfont
                .as_deref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
        }
    }

    /// Whether the metronome clicks on every beat.
    pub fn metronome(&self) -> bool {
        self.metronome
    }

    /// Whether one bar of clicks is played before playback starts.
    pub fn count_in(&self) -> bool {
        self.count_in
    }

    /// Whether the count-in is playing right now.
    pub fn count_in_pending(&self) -> bool {
        self.count_in_from.is_some()
    }

    /// Turns the metronome on or off. While a sequence is loaded it is rebuilt and swapped
    /// in where playback is, so the change is heard without a stop.
    pub fn set_metronome(&mut self, song: &Song, enabled: bool) -> Result<(), String> {
        if self.metronome == enabled {
            return Ok(());
        }
        self.metronome = enabled;
        self.rebuild_sequence(song)
    }

    /// Turns the count-in on or off. Turning it off during the count-in starts the song at
    /// once instead of waiting for the bar of clicks to finish.
    pub fn set_count_in(&mut self, song: &Song, enabled: bool) -> Result<(), String> {
        self.count_in = enabled;
        if !enabled && let Some(from) = self.count_in_from.take() {
            return self.start_music(song, from);
        }
        Ok(())
    }

    /// Starts the built-in synthesizer now, so its SoundFont is already loaded when the
    /// first note sounds: a song opening calls this. Nothing happens for MIDI output. A
    /// synthesizer that failed before (a SoundFont or device problem) is started afresh,
    /// since the problem may have been fixed meanwhile; a failure is not reported here
    /// (pressing Play reports it, and tries again).
    pub fn preload_synth(&mut self) {
        if !matches!(self.output, Output::Synth(_)) {
            return;
        }
        if matches!(&self.player, Some(active) if active.error().is_some()) {
            self.player = None;
            self.reported = None;
        }
        if self.player.is_none() {
            let _ = self.connect();
        }
    }

    /// Whether an output is connected (the synthesizer has been started and may still be
    /// loading its SoundFont, or a MIDI port is open).
    pub fn is_connected(&self) -> bool {
        self.player.is_some()
    }

    /// Connects the output if nothing is connected yet (the synthesizer then loads its
    /// SoundFont in the background).
    fn connect(&mut self) -> Result<(), String> {
        if self.player.is_none() {
            #[cfg(not(target_arch = "wasm32"))]
            let active = match &self.output {
                Output::Synth(path) => {
                    Active::Synth(SynthPlayer::start(path).map_err(|e| e.to_string())?)
                }
                Output::Midi(port) => Active::Midi(
                    MidiOutPlayer::connect(port.as_deref()).map_err(|e| e.to_string())?,
                ),
            };
            // A browser always plays through the AudioWorklet; the chosen output only
            // names the SoundFont to use once the worklet is started.
            #[cfg(target_arch = "wasm32")]
            let active = {
                let _ = &self.output;
                Active::Web(WebPlayer::start().map_err(|e| e.to_string())?)
            };
            self.player = Some(active);
            self.sequence = None;
        }
        Ok(())
    }

    /// Sounds the note on `string` of the beat at `at`, as just entered: briefly, with the
    /// track's sound, on its own channel. Nothing happens while the song plays, is paused
    /// or counts in (the music is not interrupted), for a muted track, or while the
    /// synthesizer is still loading (a note queued behind the SoundFont would sound late);
    /// the first entered note starts the output, so the next ones are heard.
    pub fn audition(
        &mut self,
        song: &Song,
        at: BeatPosition,
        string: StringNumber,
    ) -> Result<(), String> {
        if self.state() != PlayState::Stopped || self.count_in_pending() {
            return Ok(());
        }
        let Some(track) = song.track(at.track) else {
            return Ok(());
        };
        if track.is_muted() {
            return Ok(());
        }
        let note = track
            .measure(at.measure)
            .and_then(|measure| measure.voice(at.voice))
            .and_then(|voice| voice.beat(at.beat))
            .and_then(|beat| beat.notes().find(|note| note.string().ok() == Some(string)));
        let Some(note) =
            note.filter(|note| matches!(note.kind(), NoteKind::Normal | NoteKind::Dead))
        else {
            return Ok(());
        };
        let (Ok(pitch), Some(sound)) = (track.sounding_pitch(&note), song.played_sound(at.track))
        else {
            return Ok(());
        };
        let velocity = note.velocity();
        // Auditioning is a convenience: an output that cannot be started (no MIDI port,
        // no usable device) leaves the note silent instead of raising the same error on
        // every typed note. Pressing Play reports it.
        if self.connect().is_err() {
            return Ok(());
        }
        if self.is_loading() || self.error().is_some() {
            return Ok(());
        }
        self.end_audition();
        let channel = if sound.percussion {
            Channel::PERCUSSION
        } else {
            Channel::new(AUDITION_CHANNEL).unwrap_or(Channel::PERCUSSION)
        };
        let player = self.player_mut()?;
        for event in sound_events(channel, sound) {
            player.send_event(event).map_err(|e| e.to_string())?;
        }
        player
            .send_event(MidiEvent::NoteOn {
                channel,
                key: pitch.get(),
                velocity,
            })
            .map_err(|e| e.to_string())?;
        self.audition = Some(Audition {
            channel,
            key: pitch.get(),
            until: Instant::now() + AUDITION_LENGTH,
        });
        Ok(())
    }

    /// The key of the entered note that is sounding, if one is.
    pub fn auditioning(&self) -> Option<u8> {
        self.audition.map(|audition| audition.key)
    }

    /// Ends an auditioned note whose time is up. Returns how long the sounding one still
    /// has, so the UI can wake up for it.
    pub fn poll_audition(&mut self) -> Option<Duration> {
        let audition = self.audition?;
        let left = audition.until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            self.end_audition();
            return None;
        }
        Some(left)
    }

    /// Stops the auditioned note now, if one sounds.
    fn end_audition(&mut self) {
        let Some(audition) = self.audition.take() else {
            return;
        };
        if let Some(active) = &mut self.player {
            // A failed note-off only means the output is gone; nothing is left sounding.
            let _ = active.player_mut().send_event(MidiEvent::NoteOff {
                channel: audition.channel,
                key: audition.key,
            });
        }
    }

    /// Whether playback loops the selected measures.
    pub fn loop_selection(&self) -> bool {
        self.loop_selection
    }

    /// Turns looping of the selected measures on or off.
    pub fn set_loop_selection(&mut self, enabled: bool) {
        self.loop_selection = enabled;
    }

    /// Keeps playback inside `range` (first and last tick) when the loop is on: a position
    /// outside it jumps to its start, and reaching its end starts it again. Does nothing
    /// while paused or stopped (except to restart at the song's end, where playback stops).
    pub fn poll_loop(&mut self, range: Option<(u64, u64)>) -> Result<(), String> {
        if !self.loop_selection || self.count_in_pending() {
            return Ok(());
        }
        let Some((start, end)) = range else {
            return Ok(());
        };
        if end <= start {
            return Ok(());
        }
        let position = self.position();
        match self.state() {
            PlayState::Playing if position < start || position >= end => {
                self.player_mut()?.seek(start).map_err(|e| e.to_string())
            }
            PlayState::Stopped if position >= end => {
                // The song ended at the loop's end: play the loop again.
                self.player_mut()?.seek(start).map_err(|e| e.to_string())?;
                self.player_mut()?.play().map_err(|e| e.to_string())
            }
            _ => Ok(()),
        }
    }

    /// Moves the player to `tick` while playing or paused, keeping its state.
    pub fn seek_tick(&mut self, tick: u64) -> Result<(), String> {
        if self.count_in_pending() || self.state() == PlayState::Stopped {
            return Ok(());
        }
        self.player_mut()?.seek(tick).map_err(|e| e.to_string())
    }

    /// Forgets a count-in that is running or queued.
    fn clear_count_in(&mut self) {
        self.count_in_from = None;
        self.count_in_start = None;
        self.count_in_sequence = None;
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
            self.clear_count_in();
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
        match &self.player {
            #[cfg(not(target_arch = "wasm32"))]
            Some(Active::Synth(p)) => p.is_loading(),
            #[cfg(target_arch = "wasm32")]
            Some(Active::Web(p)) => p.is_loading(),
            _ => false,
        }
    }

    /// Stops waiting for a SoundFont that is loading: the player is dropped and the next
    /// play starts it again.
    pub fn cancel_loading(&mut self) {
        if self.is_loading() {
            self.player = None;
            self.sequence = None;
            self.reported = None;
            self.clear_count_in();
        }
    }

    /// A short description of the chosen output, for the toolbar.
    pub fn output_label(&self) -> String {
        match &self.output {
            // In a browser, the name the site's manifest gives once it is known.
            Output::Synth(path) => format!(
                "Synthesizer ({})",
                if cfg!(target_arch = "wasm32") {
                    self.soundfont_name().unwrap_or_else(|| file_name(path))
                } else {
                    file_name(path)
                }
            ),
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
        self.clear_count_in();
    }

    /// The song's mix (volume, pan, mute, solo) changed: rebuild the sequence. While it is
    /// playing or paused, the new mix takes over at the same position, so a slider is heard
    /// at once.
    pub fn mix_changed(&mut self, song: &Song) -> Result<(), String> {
        if self.end_preview() {
            return Ok(());
        }
        if self.count_in_pending() {
            // The song has not started yet: keep the waiting sequence in step.
            self.count_in_sequence = Some(self.build_sequence(song)?);
            return Ok(());
        }
        let sequence = self.build_sequence(song)?;
        let Some(active) = &mut self.player else {
            // Nothing is connected: the next play builds the sequence again.
            self.sequence = None;
            return Ok(());
        };
        let state = active.player().state();
        let position = active.player().position();
        active.player_mut().stop().map_err(|e| e.to_string())?;
        if let Err(error) = active.player_mut().load(Arc::clone(&sequence)) {
            self.sequence = None;
            return Err(error.to_string());
        }
        self.sequence = Some(sequence);
        if state != PlayState::Stopped {
            active
                .player_mut()
                .seek(position)
                .map_err(|e| e.to_string())?;
        }
        if state == PlayState::Playing {
            active.player_mut().play().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Plays `song` from where it was paused or stopped, or pauses it if it is playing.
    ///
    /// Playing starts at `from` (the cursor's beat, at its measure's first pass), unless
    /// playback is paused and the cursor has not moved since: then it resumes. Without
    /// `from` it resumes, or starts at the beginning.
    pub fn play_pause(&mut self, song: &Song, from: Option<BeatPosition>) -> Result<(), String> {
        // Ending an excerpt stops the player, but it may still report Playing for a moment
        // (the stop is on its way to the audio thread): then this is a fresh start.
        let ended_preview = self.end_preview();
        if !ended_preview && self.state() == PlayState::Playing {
            self.paused_at = from;
            return self.player_mut()?.pause().map_err(|e| e.to_string());
        }
        // A synthesizer that failed (SoundFont, device) is started afresh: the problem
        // may have been fixed meanwhile, and it is reported again if not.
        if self
            .player
            .as_ref()
            .is_some_and(|active| active.error().is_some())
        {
            self.player = None;
            self.reported = None;
        }
        self.end_audition();
        self.connect()?;
        // A paused count-in resumes where it paused; one that has ended starts the song.
        if self.count_in_from.is_some() {
            if self.is_loading() {
                // The count-in is on its way to the output; it has not started yet.
                return Ok(());
            }
            if self.state() == PlayState::Paused {
                return self.player_mut()?.play().map_err(|e| e.to_string());
            }
            // A stopped count-in that never started has a position of 0 (its commands are
            // still queued); one that played to the end stays at its last tick.
            if self.output_position() == 0 {
                return Ok(());
            }
            let at = self.count_in_from.take().unwrap_or(None);
            return self.start_music(song, at);
        }
        let resume =
            self.sequence.is_some() && self.state() == PlayState::Paused && self.paused_at == from;
        self.paused_at = None;
        // A fresh start with the count-in on: one bar of clicks, then the song.
        if !resume && self.count_in {
            let at = from.unwrap_or_else(first_beat);
            let clicks = gscore_play::count_in(song, at).map_err(|e| e.to_string())?;
            // Build the song's sequence now: its start tick is what the readout shows while
            // the count-in plays.
            let sequence = match self.sequence.take() {
                Some(sequence) => sequence,
                None => self.build_sequence(song)?,
            };
            let start = from.and_then(|at| sequence.tick_of(song, at)).unwrap_or(0);
            self.player_mut()?
                .load(Arc::new(clicks))
                .map_err(|e| e.to_string())?;
            self.count_in_start = Some(start);
            self.count_in_sequence = Some(sequence);
            self.count_in_from = Some(from);
            return self.player_mut()?.play().map_err(|e| e.to_string());
        }
        let sequence = match &self.sequence {
            Some(sequence) => Arc::clone(sequence),
            None => {
                let sequence = self.build_sequence(song)?;
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

    /// When the count-in has finished, starts the song where it was asked to. Call it every
    /// frame while [`Playback::count_in_pending`] is set.
    pub fn poll_count_in(&mut self, song: &Song) -> Result<(), String> {
        if self.count_in_from.is_none() || self.is_loading() {
            return Ok(());
        }
        if self.error().is_some() {
            // The output failed; there is nothing to lead into. The error is reported.
            self.clear_count_in();
            return Ok(());
        }
        if self.state() == PlayState::Stopped && self.output_position() > 0 {
            let from = self.count_in_from.take().unwrap_or(None);
            return self.start_music(song, from);
        }
        Ok(())
    }

    /// Builds the song's sequence with the current metronome setting.
    fn build_sequence(&self, song: &Song) -> Result<Arc<Sequence>, String> {
        Sequence::with_metronome(song, None, self.metronome)
            .map(Arc::new)
            .map_err(|e| e.to_string())
    }

    /// Swaps in a sequence rebuilt with the current settings at the position playback is,
    /// without stopping or silencing.
    fn rebuild_sequence(&mut self, song: &Song) -> Result<(), String> {
        if self.count_in_pending() {
            // The song waits behind the count-in: rebuild the sequence it will start with.
            self.count_in_sequence = Some(self.build_sequence(song)?);
            return Ok(());
        }
        if self.player.is_none() || self.sequence.is_none() {
            self.sequence = None;
            return Ok(());
        }
        let sequence = self.build_sequence(song)?;
        let Some(active) = &mut self.player else {
            return Ok(());
        };
        active
            .player_mut()
            .replace(Arc::clone(&sequence))
            .map_err(|e| e.to_string())?;
        self.sequence = Some(sequence);
        Ok(())
    }

    /// Loads (or reuses) the song's sequence and starts it at `from`.
    fn start_music(&mut self, song: &Song, from: Option<BeatPosition>) -> Result<(), String> {
        self.paused_at = None;
        let count_in = self.count_in_sequence.take();
        let sequence = match count_in.or_else(|| self.sequence.take()) {
            Some(sequence) => {
                // The player holds the count-in's clicks: load the song's sequence.
                self.player_mut()?
                    .load(Arc::clone(&sequence))
                    .map_err(|e| e.to_string())?;
                sequence
            }
            None => {
                let sequence = self.build_sequence(song)?;
                self.player_mut()?
                    .load(Arc::clone(&sequence))
                    .map_err(|e| e.to_string())?;
                sequence
            }
        };
        let tick = self
            .count_in_start
            .take()
            .or_else(|| from.and_then(|at| sequence.tick_of(song, at)));
        if let Some(tick) = tick {
            self.player_mut()?.seek(tick).map_err(|e| e.to_string())?;
        }
        self.count_in_from = None;
        self.sequence = Some(sequence);
        self.player_mut()?.play().map_err(|e| e.to_string())
    }

    /// While playing, continues from `at` (a beat the user clicked), at its measure's first
    /// pass, without stopping. Paused or stopped, nothing happens here: the next play
    /// starts at the cursor anyway.
    pub fn jump_to(&mut self, song: &Song, at: BeatPosition) -> Result<(), String> {
        if self.state() != PlayState::Playing {
            return Ok(());
        }
        let Some(tick) = self
            .sequence
            .as_ref()
            .and_then(|sequence| sequence.tick_of(song, at))
        else {
            return Ok(());
        };
        self.player_mut()?.seek(tick).map_err(|e| e.to_string())
    }

    /// Stops and goes back to the start.
    pub fn stop(&mut self) -> Result<(), String> {
        let _ = self.end_preview();
        self.paused_at = None;
        self.clear_count_in();
        match &mut self.player {
            Some(active) => active.player_mut().stop().map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }

    /// Playing, paused or stopped.
    pub fn state(&self) -> PlayState {
        if self.preview.is_some() {
            // A heard beat is not the song playing.
            return PlayState::Stopped;
        }
        self.player
            .as_ref()
            .map_or(PlayState::Stopped, |a| a.player().state())
    }

    /// The position in the song, in ticks: during the count-in, where the song will start.
    pub fn position(&self) -> u64 {
        if let Some(tick) = self.preview.or(self.count_in_start) {
            return tick;
        }
        self.output_position()
    }

    /// The player's own position in ticks (the count-in's bar while it runs).
    fn output_position(&self) -> u64 {
        self.player.as_ref().map_or(0, |a| a.player().position())
    }

    /// The measure being played (1-based), while playing or paused.
    pub fn current_measure(&self) -> Option<usize> {
        if self.state() == PlayState::Stopped {
            return None;
        }
        let sequence = self.sequence()?;
        let span = sequence.measure_at(self.position())?;
        Some(span.measure.get().saturating_add(1))
    }

    /// An approximate output level of `track` (0.0 to 1.0) for the mixer meter: recent
    /// note velocities scaled by the channel gain of the track's played volume (master
    /// included) and decaying over a quarter note. This
    /// is note activity, not measured audio, so it also works with MIDI output.
    pub fn track_level(&self, track: TrackIndex) -> f32 {
        if self.state() != PlayState::Playing {
            return 0.0;
        }
        let (Some(sequence), position) = (&self.sequence, self.position()) else {
            return 0.0;
        };
        let Some((entry_index, entry)) = sequence
            .tracks
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.track == track)
        else {
            return 0.0;
        };
        let window = u64::from(PPQ);
        let start = position.saturating_sub(window);
        let events = &sequence.events;
        let first = events.partition_point(|event| event.tick < start);
        let last = events.partition_point(|event| event.tick <= position);
        let mut peak = 0.0_f32;
        for event in events.get(first..last).unwrap_or_default() {
            if event.track != entry_index {
                continue;
            }
            if let MidiEvent::NoteOn { velocity, .. } = event.event {
                let age = position.saturating_sub(event.tick) as f32 / window as f32;
                let strength = f32::from(velocity) / 127.0 * (1.0 - age).max(0.0);
                peak = peak.max(strength);
            }
        }
        // The synthesizer's channel gain is (volume / 127)²; the sequence's volumes already
        // include the master.
        let gain = f32::from(entry.sound.volume) / 127.0;
        peak * gain * gain
    }

    /// A track's volume, pan or program changed. The new values are sent to the player at
    /// once, so a slider or preset is heard while it changes; then the sequence is rebuilt
    /// and swapped in where playback is (no stop, no silence), so its setup holds the new
    /// values too: playing from the start, seeking, and the borrowed channels of bends and
    /// slides use them.
    pub fn send_track_sound(&mut self, song: &Song, track: TrackIndex) -> Result<(), String> {
        self.send_sounds(song, [track])
    }

    /// The master volume or pan changed: every track's sound is sent, as
    /// [`Playback::send_track_sound`] does for one.
    pub fn send_all_sounds(&mut self, song: &Song) -> Result<(), String> {
        self.send_sounds(song, (0..song.track_count()).map(TrackIndex::new))
    }

    /// Sends the played sound (with the master mix) of `tracks`, then swaps in a rebuilt
    /// sequence.
    fn send_sounds(
        &mut self,
        song: &Song,
        tracks: impl IntoIterator<Item = TrackIndex>,
    ) -> Result<(), String> {
        if self.count_in_pending() {
            // The song has not started yet; only the sequence waiting behind the count-in
            // needs the new mix.
            self.count_in_sequence = Some(self.build_sequence(song)?);
            return Ok(());
        }
        if self.player.is_none() {
            // Nothing is connected: the next play builds the sequence again.
            self.sequence = None;
            return Ok(());
        }
        let rebuilt = if self.sequence.is_some() {
            Some(self.build_sequence(song)?)
        } else {
            None
        };
        let Some(active) = &mut self.player else {
            return Ok(());
        };
        for track in tracks {
            if let (Some(sequence), Some(sound)) = (&self.sequence, song.played_sound(track))
                && let Some(channel) = sequence.channel_of(track)
            {
                for event in sound_events(channel, sound) {
                    active
                        .player_mut()
                        .send_event(event)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        let Some(sequence) = rebuilt else {
            return Ok(());
        };
        active
            .player_mut()
            .replace(Arc::clone(&sequence))
            .map_err(|e| e.to_string())?;
        self.sequence = Some(sequence);
        Ok(())
    }

    /// The sequence the player has, or the one waiting after a count-in.
    pub fn sequence(&self) -> Option<&Arc<Sequence>> {
        self.sequence.as_ref().or(self.count_in_sequence.as_ref())
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

/// The first beat of the song, for a count-in without a cursor position.
fn first_beat() -> BeatPosition {
    BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(0),
    }
}

/// The file name of `path`, or the whole path if it has none.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}
