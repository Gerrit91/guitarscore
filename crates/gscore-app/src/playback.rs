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
mod chains;

pub(crate) use chains::ExportTones;
mod demo;
mod preview;

pub use preview::Heard;

use gscore_play::{
    Channel, Click, ClickSound, MAX_COUNT_IN_BARS, MidiEvent, PPQ, PlayState, Player, Sequence,
    sound_events,
};

/// How long an entered note sounds.
const AUDITION_LENGTH: Duration = Duration::from_millis(450);
/// The channel entered notes sound on (drums use the percussion channel). Playback is
/// stopped while a note is auditioned, and playing sends every channel's setup first.
const AUDITION_CHANNEL: u8 = 15;

/// A note sounding because it was just entered.
#[derive(Debug, Clone)]
struct Audition {
    channel: Channel,
    key: u8,
    until: Instant,
    /// The MIDI-out port that plays it (`None`: the main output).
    port: Option<Option<String>>,
}

pub(crate) use gscore_audio::SAMPLE_RATES;

/// Every kind of modelled string.
pub(crate) const STRING_KINDS: [gscore_play::StringKind; 4] = [
    gscore_play::StringKind::Nylon,
    gscore_play::StringKind::Steel,
    gscore_play::StringKind::Electric,
    gscore_play::StringKind::Bass,
];

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

/// Where a track's MIDI output goes: the port and channel of a hardware synthesizer.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct MidiOutRoute {
    /// The port's name (or a part of it), or `None` for the first port that makes sound.
    pub port: Option<String>,
    /// The MIDI channel (0-15), or `None` for the sequence's own.
    pub channel: Option<u8>,
}

impl MidiOutRoute {
    /// The route with values a MIDI port can take (a broken settings file may hold
    /// anything).
    pub(crate) fn sanitized(mut self) -> Self {
        self.channel = self.channel.filter(|channel| *channel < 16);
        self
    }

    /// A short name for the Sound chain window and the board.
    pub fn label(&self) -> String {
        let port = self.port.as_deref().unwrap_or("Automatic");
        match self.channel {
            Some(channel) => format!("{port} · {}", channel.saturating_add(1)),
            None => format!("{port} · auto"),
        }
    }
}

/// What each MIDI-out port plays, by the port requested ([`MidiOutRoute::port`]).
type MidiSequences = Vec<(Option<String>, Arc<Sequence>)>;

/// One drum part's own tone in a song: its effect chain and its mixing level inside the
/// drum track.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackDrumPart {
    /// Which part of the kit.
    pub part: crate::chain_settings::DrumPart,
    /// Its blocks and knobs.
    pub chain: crate::chain_settings::ChainSetting,
    /// Its mixing level (0-127, 127 unity).
    pub level: u8,
}

/// One MIDI output port that plays MIDI-out tracks.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
struct MidiOutConnection {
    /// The port requested (`None`: the first that makes sound), which routes match.
    requested: Option<String>,
    player: MidiOutPlayer,
}

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

/// A count-in bar played as one sequence with the song after it ([`gscore_play::lead_in`]),
/// so the song follows the last click without a gap. The player's tick `t` past the bar
/// is the song's tick `t - bar + start`.
#[derive(Debug)]
struct Lead {
    /// The count-in's clicks.
    clicks: Arc<Sequence>,
    /// Where the song starts (its tick).
    start: u64,
    /// How long the count-in is, in the player's ticks.
    bar: u64,
    /// Whether the player has been seen inside the count-in (its position from before
    /// the load is not trusted until then).
    armed: bool,
    /// Whether the count-in is over and the song plays.
    passed: bool,
}

/// The speeds the toolbar offers: 50 % to 120 % of the written tempo.
pub const SPEED_CHOICES: [f32; 8] = [0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2];

/// The fastest and slowest playback speed.
pub const SPEED_RANGE: (f32, f32) = (0.5, 1.2);

/// The speeds a speed trainer may work up to.
pub const TRAINER_TARGETS: [f32; 3] = [1.0, 1.1, 1.2];

/// A playback speed trainer: practise a loop slowly, and raise the speed every few passes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeedTrainer {
    /// Whether it is on.
    pub enabled: bool,
    /// Passes between two speed steps (1 or more).
    pub every: u8,
    /// How much faster each step is, in percent (1 to 20).
    pub step: u8,
    /// The top speed it may reach.
    pub target: f32,
}

impl Default for SpeedTrainer {
    fn default() -> Self {
        SpeedTrainer {
            enabled: false,
            every: 2,
            step: 5,
            target: 1.0,
        }
    }
}

impl SpeedTrainer {
    /// Makes the values safe and usable.
    pub fn sanitize(&mut self) {
        self.every = self.every.clamp(1, 32);
        self.step = self.step.clamp(1, 20);
        self.target = if self.target.is_finite() {
            self.target.clamp(SPEED_RANGE.0, SPEED_RANGE.1)
        } else {
            1.0
        };
    }
}

/// The app's playback state.
#[derive(Debug)]
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
    /// Whether guitar and bass tracks are played by the modelled strings.
    modelled_guitars: bool,
    /// What plays the tracks the user chose for (by track index), over the default.
    track_engines: Vec<(usize, gscore_play::TrackEngine)>,
    /// Each track's MIDI-out port and channel, by the song's track index.
    midi_routes: Vec<(usize, MidiOutRoute)>,
    /// What the main output plays: the song's sequence without its MIDI-out tracks.
    primary_sequence: Option<Arc<Sequence>>,
    /// What each MIDI-out port plays, by the requested port.
    midi_sequences: MidiSequences,
    /// The MIDI-out players that are connected, by the port they were asked for.
    #[cfg(not(target_arch = "wasm32"))]
    midi_outs: Vec<MidiOutConnection>,
    /// A MIDI-out problem not reported to the user yet.
    midi_error: Option<String>,
    /// The sample rate asked of the device; `None`: its own.
    sample_rate: Option<u32>,
    /// How each kind of modelled string sounds on a track without a tone of its own.
    string_sounds: crate::settings::StringSounds,
    /// The effect chain after each kind of string on a track without a tone of its own.
    string_chains: crate::chain_settings::StringChains,
    /// The tones set for track presets (by General MIDI program), local to this machine.
    preset_tones: Vec<crate::settings::PresetTone>,
    /// The tones set for the parts of a drum kit (kick, snare, ...), local to this machine.
    drum_parts: Vec<crate::settings::DrumPartTone>,
    /// The drum parts a song gives a drum track of its own (by the song's track index):
    /// each part's chain and mixing level, over this machine's kit.
    track_drum_parts: std::collections::BTreeMap<usize, Vec<TrackDrumPart>>,
    /// The tones that came with the open song, by its track index (they win over the
    /// preset's: a shared song keeps its sound).
    tones: std::collections::BTreeMap<usize, crate::chain_settings::Tone>,
    /// Each track's send into the compressor bus (0-127), by the song's track index.
    comp_sends: Vec<(usize, u8)>,
    /// The modelled tracks of the sequence the synthesizer has, and what each place of it
    /// was given (see `chains`).
    tone_targets: Vec<chains::ToneTarget>,
    sent_tones: std::collections::BTreeMap<usize, chains::SentTone>,
    /// What each drum track's parts were given (by song track and part), so a part's
    /// chain is prepared again only when its blocks change.
    sent_parts:
        std::collections::BTreeMap<(usize, usize), (crate::chain_settings::ChainSetting, u8)>,
    /// The mixer's reverb bus (a computed reverb or a room, all wet) and its compressor
    /// bus (parallel compression), and what the synthesizer was last given for each.
    reverb_bus: crate::settings::ReverbBus,
    compressor_bus: crate::settings::CompressorBus,
    reverb_sent: chains::SentBus,
    compressor_sent: chains::SentBus,
    /// The impulse responses read so far (by file).
    impulses: std::collections::HashMap<PathBuf, Arc<gscore_guitar::ImpulseResponse>>,
    /// Messages about the chains (an impulse response that cannot be read), shown once.
    chain_notices: Vec<String>,
    /// The metronome's volume, 0 to 1 (1 is full).
    metronome_volume: f32,
    /// The built-in synthesizer's click sound.
    click_sound: ClickSound,
    /// How many bars the count-in clicks (1 to [`MAX_COUNT_IN_BARS`]).
    count_in_bars: u8,
    /// Whether a looped selection is counted in again each time it starts over.
    count_in_loops: bool,
    /// Whether one bar of clicks is played before playback starts.
    count_in: bool,
    /// Playback speed relative to the written tempo (1 is normal).
    speed: f32,
    /// The speed trainer's settings.
    trainer: SpeedTrainer,
    /// Loop passes completed since the trainer last went back to the starting speed.
    passes: u32,
    /// While the player holds a count-in joined to the song (see [`Lead`]).
    lead: Option<Lead>,
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

impl Default for Playback {
    fn default() -> Self {
        Playback {
            ports: Vec::new(),
            output: Output::default(),
            soundfont: None,
            player: None,
            sequence: None,
            reported: None,
            paused_at: None,
            metronome: false,
            modelled_guitars: false,
            track_engines: Vec::new(),
            midi_routes: Vec::new(),
            primary_sequence: None,
            midi_sequences: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            midi_outs: Vec::new(),
            midi_error: None,
            sample_rate: None,
            string_sounds: crate::settings::StringSounds::default(),
            string_chains: crate::chain_settings::StringChains::default(),
            preset_tones: Vec::new(),
            drum_parts: Vec::new(),
            track_drum_parts: std::collections::BTreeMap::new(),
            tones: std::collections::BTreeMap::new(),
            comp_sends: Vec::new(),
            tone_targets: Vec::new(),
            sent_tones: std::collections::BTreeMap::new(),
            sent_parts: std::collections::BTreeMap::new(),
            reverb_bus: crate::settings::ReverbBus::default(),
            compressor_bus: crate::settings::CompressorBus::default(),
            reverb_sent: chains::SentBus::default(),
            compressor_sent: chains::SentBus::default(),
            impulses: std::collections::HashMap::new(),
            chain_notices: Vec::new(),
            metronome_volume: 1.0,
            click_sound: ClickSound::default(),
            count_in_bars: 1,
            count_in_loops: false,
            count_in: false,
            speed: 1.0,
            trainer: SpeedTrainer::default(),
            passes: 0,
            lead: None,
            loop_selection: false,
            audition: None,
            preview: None,
            preview_sequence: None,
            pending_preview: None,
        }
    }
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

    /// The metronome's volume, 0 to 1.
    pub fn metronome_volume(&self) -> f32 {
        self.metronome_volume
    }

    /// Sets the metronome's volume without rebuilding a sequence (used when the settings
    /// load, before any song is open).
    pub fn store_metronome_volume(&mut self, volume: f32) {
        self.metronome_volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
    }

    /// Sets the metronome's volume (0 to 1), heard at once while playing.
    pub fn set_metronome_volume(&mut self, song: &Song, volume: f32) -> Result<(), String> {
        let volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        if (self.metronome_volume - volume).abs() < f32::EPSILON {
            return Ok(());
        }
        self.metronome_volume = volume;
        self.rebuild_sequence(song)
    }

    /// The metronome's click: its volume and sound.
    fn click(&self) -> Click {
        Click {
            volume: self.metronome_volume,
            sound: self.click_sound,
        }
    }

    /// The built-in synthesizer's click sound.
    pub fn click_sound(&self) -> ClickSound {
        self.click_sound
    }

    /// Chooses the click sound, heard at once while playing.
    pub fn set_click_sound(&mut self, song: &Song, sound: ClickSound) -> Result<(), String> {
        if self.click_sound == sound {
            return Ok(());
        }
        self.click_sound = sound;
        self.rebuild_sequence(song)
    }

    /// How the modelled strings sound, kind by kind, on tracks without a tone of their own.
    pub fn string_sounds(&self) -> crate::settings::StringSounds {
        self.string_sounds
    }

    /// The string models to render with, for audio export.
    pub fn string_models(&self) -> Vec<(gscore_play::StringKind, gscore_guitar::StringModel)> {
        STRING_KINDS
            .iter()
            .map(|&kind| (kind, self.string_sounds.get(kind).model()))
            .collect()
    }

    /// Makes the modelled strings of `kind` sound as `sound` on tracks without a tone of
    /// their own (the kind's default), heard at once while playing.
    pub fn set_string_sound(
        &mut self,
        kind: gscore_play::StringKind,
        sound: crate::settings::StringSound,
    ) {
        self.string_sounds.set(kind, sound);
        self.send_string_model(kind);
    }

    /// Takes the dialed-in sounds without sending them (when the settings load).
    pub fn store_string_sounds(&mut self, sounds: crate::settings::StringSounds) {
        self.string_sounds = sounds;
    }

    /// Sends the model of `kind` to the running synthesizer, if any.
    fn send_string_model(&self, kind: gscore_play::StringKind) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(Active::Synth(player)) = &self.player {
            let _ = player.set_string_model(kind, self.string_sounds.get(kind).model());
        }
        #[cfg(target_arch = "wasm32")]
        let _ = kind;
    }

    /// The sample rate chosen for the built-in synthesizer (`None`: the device's own).
    pub fn sample_rate(&self) -> Option<u32> {
        self.sample_rate
    }

    /// Chooses the synthesizer's sample rate; a running synthesizer is restarted at the
    /// next play (it stops now).
    pub fn set_sample_rate(&mut self, rate: Option<u32>) {
        let rate = rate.filter(|rate| SAMPLE_RATES.contains(rate));
        if rate == self.sample_rate {
            return;
        }
        self.sample_rate = rate;
        #[cfg(not(target_arch = "wasm32"))]
        if matches!(self.player, Some(Active::Synth(_))) {
            if let Some(active) = &mut self.player {
                let _ = active.player_mut().stop();
            }
            self.stop_midi_outs();
            self.player = None;
            self.sequence = None;
            self.primary_sequence = None;
            self.midi_sequences.clear();
            self.reported = None;
            self.lead = None;
        }
    }

    /// The rate the synthesizer's device plays at, once it runs.
    pub fn device_sample_rate(&self) -> Option<u32> {
        match &self.player {
            #[cfg(not(target_arch = "wasm32"))]
            Some(Active::Synth(player)) => player.sample_rate(),
            _ => None,
        }
    }

    /// The mixer's real meters (every track's level after its fader, the reverb bus's and
    /// the master's), while the built-in synthesizer runs. `None` for MIDI output and in a
    /// browser: the mixer then shows levels estimated from the notes.
    pub fn meters(&self) -> Option<Arc<gscore_audio::Meters>> {
        match &self.player {
            #[cfg(not(target_arch = "wasm32"))]
            Some(Active::Synth(player)) => player.meters(),
            _ => None,
        }
    }

    /// How busy the synthesizer's audio thread is (1: all of each buffer's time), once it
    /// runs. `None` for MIDI output and in a browser.
    pub fn audio_load(&self) -> Option<f32> {
        match &self.player {
            #[cfg(not(target_arch = "wasm32"))]
            Some(Active::Synth(player)) => player.audio_load(),
            _ => None,
        }
    }

    /// Whether guitar and bass tracks are played by the modelled strings.
    pub fn modelled_guitars(&self) -> bool {
        self.modelled_guitars
    }

    /// Plays guitar and bass tracks with the modelled strings (or the SoundFont again),
    /// heard at once while playing.
    pub fn set_modelled_guitars(&mut self, song: &Song, modelled: bool) -> Result<(), String> {
        if self.modelled_guitars == modelled {
            return Ok(());
        }
        self.modelled_guitars = modelled;
        self.rebuild_sequence(song)
    }

    /// Like [`Self::set_modelled_guitars`] without rebuilding (when the settings load).
    pub fn store_modelled_guitars(&mut self, modelled: bool) {
        self.modelled_guitars = modelled;
    }

    /// Chooses the click sound without rebuilding a sequence (when the settings load).
    pub fn store_click_sound(&mut self, sound: ClickSound) {
        self.click_sound = sound;
    }

    /// How many bars the count-in clicks.
    pub fn count_in_bars(&self) -> u8 {
        self.count_in_bars
    }

    /// Sets how many bars the count-in clicks (1 to [`MAX_COUNT_IN_BARS`]); the next
    /// count-in uses it.
    pub fn set_count_in_bars(&mut self, bars: u8) {
        self.count_in_bars = bars.clamp(1, MAX_COUNT_IN_BARS);
    }

    /// Whether a looped selection is counted in again each time it starts over.
    pub fn count_in_loops(&self) -> bool {
        self.count_in_loops
    }

    /// Counts a loop in again each time it starts over (with the count-in on), or not.
    pub fn set_count_in_loops(&mut self, enabled: bool) {
        self.count_in_loops = enabled;
    }

    /// Playback speed relative to the written tempo (1 is normal).
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Slows down or speeds up playback, heard at once while it plays.
    pub fn set_speed(&mut self, speed: f32) -> Result<(), String> {
        let speed = if speed.is_finite() {
            speed.clamp(SPEED_RANGE.0, SPEED_RANGE.1)
        } else {
            1.0
        };
        if (self.speed - speed).abs() < f32::EPSILON {
            return Ok(());
        }
        self.speed = speed;
        self.passes = 0;
        self.transport_all(|player| player.set_tempo_scale(speed))
    }

    /// The speed trainer's settings.
    pub fn trainer(&self) -> SpeedTrainer {
        self.trainer
    }

    /// Sets the speed trainer. Turning it on, or changing its steps, starts the pass count
    /// again at the current speed.
    pub fn set_trainer(&mut self, mut trainer: SpeedTrainer) {
        trainer.sanitize();
        if trainer != self.trainer {
            self.passes = 0;
        }
        self.trainer = trainer;
    }

    /// How many loop passes the trainer has counted since it started.
    pub fn trainer_passes(&self) -> u32 {
        self.passes
    }

    /// Counts one completed loop pass; every `every`th pass, raises the speed by `step`
    /// percent until the trainer's target. Returns whether the speed changed.
    pub fn trainer_pass(&mut self) -> Result<bool, String> {
        if !self.trainer.enabled {
            return Ok(false);
        }
        self.passes = self.passes.saturating_add(1);
        let every = u32::from(self.trainer.every.max(1));
        if !self.passes.is_multiple_of(every) {
            return Ok(false);
        }
        let target = self.trainer.target.max(SPEED_RANGE.0);
        if self.speed >= target - f32::EPSILON {
            return Ok(false);
        }
        let step = f32::from(self.trainer.step.max(1)) / 100.0;
        let next = (self.speed + step).min(target).min(SPEED_RANGE.1);
        self.set_speed(next)?;
        Ok(true)
    }

    /// Starts the speed trainer over at `speed` (a fresh practice run).
    fn reset_trainer(&mut self) {
        self.passes = 0;
    }

    /// Plays one bar of the metronome alone, to hear its sound and volume. Only while the
    /// song is stopped (as a beat preview); `song` gives the tempo.
    pub fn preview_click(&mut self, song: &Song) -> Result<(), String> {
        if self.state() == PlayState::Playing || self.count_in_pending() {
            return Ok(());
        }
        let bar = gscore_play::count_in(song, first_beat(), 1, self.click())
            .map_err(|e| e.to_string())?;
        self.connect()?;
        if self.is_loading() {
            return Ok(());
        }
        self.play_excerpt(Arc::new(bar))
    }

    /// Whether one bar of clicks is played before playback starts.
    pub fn count_in(&self) -> bool {
        self.count_in
    }

    /// Whether the count-in is playing right now.
    pub fn count_in_pending(&self) -> bool {
        self.lead.as_ref().is_some_and(|lead| !lead.passed)
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
    pub fn set_count_in(&mut self, _song: &Song, enabled: bool) -> Result<(), String> {
        self.count_in = enabled;
        if !enabled
            && let Some(lead) = &mut self.lead
            && !lead.passed
        {
            // Skip the rest of the clicks: the song starts now.
            lead.passed = true;
            let bar = lead.bar;
            return self.transport_all(|player| player.seek(bar));
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
                Output::Synth(path) => Active::Synth(
                    SynthPlayer::start_at(path, self.sample_rate).map_err(|e| e.to_string())?,
                ),
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
            self.primary_sequence = None;
            self.midi_sequences.clear();
            self.midi_error = None;
            // A track may be played by a hardware synthesizer: one player per port it
            // needs.
            self.connect_midi_outs();
            // A new synthesizer starts with the presets: give it each kind's default sound;
            // the tracks' tones follow with the first sequence it loads.
            for kind in STRING_KINDS {
                self.send_string_model(kind);
            }
            self.forget_sent_tones();
            // The buses too (a new synthesizer starts without them).
            self.reverb_sent = chains::SentBus::default();
            self.compressor_sent = chains::SentBus::default();
            self.send_reverb_bus();
            self.send_compressor_bus();
            self.send_comp_sends();
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
        let route = self
            .track_is_midi_out(at.track.get())
            .then(|| self.midi_route(at.track.get()));
        let port = route.as_ref().map(|route| route.port.clone());
        let channel = match &route {
            Some(route) => route
                .channel
                .and_then(Channel::new)
                .unwrap_or_else(|| Channel::new(AUDITION_CHANNEL).unwrap_or(Channel::PERCUSSION)),
            None if sound.percussion => Channel::PERCUSSION,
            None => Channel::new(AUDITION_CHANNEL).unwrap_or(Channel::PERCUSSION),
        };
        let events = sound_events(channel, sound)
            .into_iter()
            .chain([MidiEvent::NoteOn {
                channel,
                key: pitch.get(),
                velocity,
            }]);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(port) = &port {
            let Some(player) = self.midi_player_mut(port) else {
                return Ok(());
            };
            for event in events {
                player.send_event(event).map_err(|e| e.to_string())?;
            }
            self.audition = Some(Audition {
                channel,
                key: pitch.get(),
                until: Instant::now() + AUDITION_LENGTH,
                port: Some(port.clone()),
            });
            return Ok(());
        }
        let player = self.player_mut()?;
        for event in events {
            player.send_event(event).map_err(|e| e.to_string())?;
        }
        self.audition = Some(Audition {
            channel,
            key: pitch.get(),
            until: Instant::now() + AUDITION_LENGTH,
            port,
        });
        Ok(())
    }

    /// The key of the entered note that is sounding, if one is.
    pub fn auditioning(&self) -> Option<u8> {
        self.audition.as_ref().map(|audition| audition.key)
    }

    /// Ends an auditioned note whose time is up. Returns how long the sounding one still
    /// has, so the UI can wake up for it.
    pub fn poll_audition(&mut self) -> Option<Duration> {
        let until = self.audition.as_ref()?.until;
        let left = until.saturating_duration_since(Instant::now());
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
        let event = MidiEvent::NoteOff {
            channel: audition.channel,
            key: audition.key,
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(port) = &audition.port {
            if let Some(player) = self.midi_player_mut(port) {
                // A failed note-off only means the output is gone; nothing is left sounding.
                let _ = player.send_event(event);
            }
            return;
        }
        if let Some(active) = &mut self.player {
            // A failed note-off only means the output is gone; nothing is left sounding.
            let _ = active.player_mut().send_event(event);
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
    ///
    /// `first` is the loop's first beat: with the count-in on and
    /// [`Playback::count_in_loops`], each new pass is counted in there.
    pub fn poll_loop(
        &mut self,
        song: &Song,
        range: Option<(u64, u64)>,
        first: BeatPosition,
    ) -> Result<(), String> {
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
            PlayState::Playing if position >= end && self.count_in && self.count_in_loops => {
                let _ = self.trainer_pass()?;
                self.start_count_in(song, Some(first))
            }
            PlayState::Playing if position < start || position >= end => {
                if position >= end {
                    let _ = self.trainer_pass()?;
                }
                self.seek_song(start)
            }
            PlayState::Stopped if position >= end => {
                // The song ended at the loop's end: play the loop again.
                let _ = self.trainer_pass()?;
                self.seek_song(start)?;
                self.transport_all(|player| player.play())
            }
            _ => Ok(()),
        }
    }

    /// Moves the player to `tick` while playing or paused, keeping its state.
    pub fn seek_tick(&mut self, tick: u64) -> Result<(), String> {
        if self.count_in_pending() || self.state() == PlayState::Stopped {
            return Ok(());
        }
        self.seek_song(tick)
    }

    /// Moves the player to the song's `tick`. Behind a count-in, that tick is a bar later
    /// in the player; a tick before the count-in's song start leaves the joined sequence for
    /// the song's own (in the same state).
    fn seek_song(&mut self, tick: u64) -> Result<(), String> {
        let Some(lead) = &self.lead else {
            return self.transport_all(|player| player.seek(tick));
        };
        if tick >= lead.start {
            let at = tick.saturating_sub(lead.start).saturating_add(lead.bar);
            return self.transport_all(|player| player.seek(at));
        }
        let playing = self.state() == PlayState::Playing;
        self.drop_lead()?;
        self.transport_all(|player| player.seek(tick))?;
        if playing {
            self.transport_all(|player| player.play())?;
        }
        Ok(())
    }

    /// Ends a count-in joined to the song: the players get the song's own sequences again
    /// (stopped at their start).
    fn drop_lead(&mut self) -> Result<(), String> {
        if self.lead.take().is_none() {
            return Ok(());
        }
        if self.sequence.is_none() {
            return Ok(());
        }
        let (primary, midi) = self.loading_sequences();
        if self.player.is_some() {
            self.transport_all(|player| player.stop())?;
            self.load_sequences(primary, &midi)?;
        }
        Ok(())
    }

    /// Chooses the output; the player reconnects on the next play.
    pub fn choose_output(&mut self, output: Output) {
        if output != self.output {
            if let Some(active) = &mut self.player {
                let _ = active.player_mut().stop();
            }
            self.stop_midi_outs();
            self.player = None;
            #[cfg(not(target_arch = "wasm32"))]
            self.midi_outs.clear();
            self.sequence = None;
            self.primary_sequence = None;
            self.midi_sequences.clear();
            self.midi_error = None;
            self.reported = None;
            self.lead = None;
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
            self.stop_midi_outs();
            self.player = None;
            self.sequence = None;
            self.primary_sequence = None;
            self.midi_sequences.clear();
            self.reported = None;
            self.lead = None;
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
        self.stop_midi_outs();
        self.sequence = None;
        self.primary_sequence = None;
        self.midi_sequences.clear();
        self.paused_at = None;
        self.lead = None;
        self.reset_trainer();
    }

    /// The song's mix (volume, pan, mute, solo) changed: rebuild the sequence. While it is
    /// playing or paused, the new mix takes over at the same position, so a slider is heard
    /// at once.
    pub fn mix_changed(&mut self, song: &Song) -> Result<(), String> {
        if self.end_preview() {
            return Ok(());
        }
        let sequence = self.build_sequence(song)?;
        let (primary, midi) = self.loading_sequences();
        let Some(active) = &self.player else {
            // Nothing is connected: the next play builds the sequence again.
            self.sequence = None;
            self.primary_sequence = None;
            self.midi_sequences.clear();
            return Ok(());
        };
        let state = active.player().state();
        let position = active.player().position();
        self.transport_all(|player| player.stop())?;
        if let Err(error) = self.load_sequences(primary, &midi) {
            self.sequence = None;
            return Err(error);
        }
        self.sequence = Some(sequence);
        if state != PlayState::Stopped {
            self.transport_all(|player| player.seek(position))?;
        }
        if state == PlayState::Playing {
            self.transport_all(|player| player.play())?;
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
            return self.transport_all(|player| player.pause());
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
        self.connect_midi_outs();
        self.sync_midi_delay();
        // An interrupted count-in starts again from its first click, at the place asked for
        // now (the cursor may have moved): half a bar of clicks is no count-in.
        if self.count_in_pending() && self.state() == PlayState::Paused {
            self.drop_lead()?;
        }
        let resume =
            self.sequence.is_some() && self.state() == PlayState::Paused && self.paused_at == from;
        self.paused_at = None;
        if !resume {
            // A fresh start: whatever count-in came before is over.
            self.drop_lead()?;
            self.reset_trainer();
        }
        // A fresh start with the count-in on: one bar of clicks, then the song, played as
        // one sequence so the song follows the last click exactly.
        if !resume && self.count_in {
            return self.start_count_in(song, from);
        }
        let sequence = match &self.sequence {
            Some(sequence) => Arc::clone(sequence),
            None => {
                let sequence = self.build_sequence(song)?;
                let (primary, midi) = self.loading_sequences();
                self.load_sequences(primary, &midi)?;
                self.sequence = Some(Arc::clone(&sequence));
                sequence
            }
        };
        if !resume && let Some(tick) = from.and_then(|at| sequence.tick_of(song, at)) {
            self.transport_all(|player| player.seek(tick))?;
        }
        let speed = self.speed;
        self.transport_all(|player| player.set_tempo_scale(speed))?;
        self.transport_all(|player| player.play())
    }

    /// Plays one bar of clicks, then the song from `from` (its start without), as one
    /// sequence so the song follows the last click exactly.
    fn start_count_in(&mut self, song: &Song, from: Option<BeatPosition>) -> Result<(), String> {
        let at = from.unwrap_or_else(first_beat);
        let clicks = gscore_play::count_in(song, at, self.count_in_bars, self.click())
            .map(Arc::new)
            .map_err(|e| e.to_string())?;
        let sequence = match &self.sequence {
            Some(sequence) => Arc::clone(sequence),
            None => self.build_sequence(song)?,
        };
        let start = from.and_then(|at| sequence.tick_of(song, at)).unwrap_or(0);
        self.lead = Some(Lead {
            bar: clicks.end,
            clicks,
            start,
            armed: false,
            passed: false,
        });
        self.sequence = Some(sequence);
        let (primary, midi) = self.loading_sequences();
        let speed = self.speed;
        self.transport_all(|player| player.stop())?;
        self.load_sequences(primary, &midi)?;
        self.transport_all(|player| player.set_tempo_scale(speed))?;
        self.transport_all(|player| player.play())
    }

    /// Notes when the count-in has given way to the song (the readout then follows the
    /// player). Call it every frame while [`Playback::count_in_pending`] is set; the song
    /// itself already follows the clicks in the player.
    pub fn poll_count_in(&mut self, _song: &Song) -> Result<(), String> {
        let position = self.output_position();
        let playing = self.state() == PlayState::Playing;
        let loading = self.is_loading();
        let Some(lead) = &mut self.lead else {
            return Ok(());
        };
        if lead.passed || loading {
            return Ok(());
        }
        if !lead.armed {
            lead.armed = playing && position < lead.bar;
        } else if position >= lead.bar {
            lead.passed = true;
        }
        Ok(())
    }

    /// Builds the song's sequence with the current metronome setting and volume (and
    /// makes sure the synthesizer's chains are made for as many tracks as it has). The
    /// MIDI-out tracks are split off: the main output gets the others, each MIDI port its
    /// own tracks.
    fn build_sequence(&mut self, song: &Song) -> Result<Arc<Sequence>, String> {
        let sequence = Sequence::with_click(song, None, self.metronome.then(|| self.click()))
            .map(|sequence| self.engines(sequence))
            .map(Arc::new)
            .map_err(|e| e.to_string())?;
        let (primary, midi) = self.route_sequences(&sequence);
        self.sync_chains(&primary);
        self.primary_sequence = Some(primary);
        self.midi_sequences = midi;
        Ok(sequence)
    }

    /// `sequence` with the engines chosen: the settings' default for guitar and bass
    /// tracks, then each track's own choice (modelled strings only for string tracks).
    pub(crate) fn engines(&self, sequence: Sequence) -> Sequence {
        with_engines(
            sequence,
            self.modelled_guitars,
            &self.track_engines,
            &self.preset_tones,
        )
    }

    /// The tracks' own engine choices (by track index).
    pub(crate) fn track_engines(&self) -> Vec<(usize, gscore_play::TrackEngine)> {
        self.track_engines.clone()
    }

    /// Each track's MIDI-out route (by the song's track index).
    pub fn midi_routes(&self) -> &[(usize, MidiOutRoute)] {
        &self.midi_routes
    }

    /// The MIDI-out route of `track` (automatic when none was chosen).
    pub fn midi_route(&self, track: usize) -> MidiOutRoute {
        self.midi_routes
            .iter()
            .find(|(index, _)| *index == track)
            .map(|(_, route)| route.clone())
            .unwrap_or_default()
    }

    /// Whether `track` of the open song is played by a hardware synthesizer.
    pub fn track_is_midi_out(&self, track: usize) -> bool {
        !cfg!(target_arch = "wasm32")
            && self.track_engines.iter().any(|&(index, engine)| {
                index == track && engine == gscore_play::TrackEngine::MidiOut
            })
    }

    /// The tracks' own engine choices (by track index); a change is heard at once.
    pub fn set_track_engines(
        &mut self,
        song: &Song,
        engines: Vec<(usize, gscore_play::TrackEngine)>,
    ) -> Result<(), String> {
        if self.track_engines == engines {
            return Ok(());
        }
        self.track_engines = engines;
        if self.player.is_some() {
            self.connect_midi_outs();
        }
        self.rebuild_sequence(song)
    }

    /// Each track's MIDI-out port and channel (by the song's track index); a change is
    /// heard at once.
    pub fn set_midi_routes(
        &mut self,
        song: &Song,
        routes: Vec<(usize, MidiOutRoute)>,
    ) -> Result<(), String> {
        let routes: Vec<(usize, MidiOutRoute)> = routes
            .into_iter()
            .map(|(track, route)| (track, route.sanitized()))
            .collect();
        if self.midi_routes == routes {
            return Ok(());
        }
        self.midi_routes = routes;
        if self.player.is_some() {
            self.connect_midi_outs();
        }
        self.rebuild_sequence(song)
    }

    /// What the main output plays (the song without its MIDI-out tracks).
    #[cfg(test)]
    pub(crate) fn primary_sequence(&self) -> Option<&Arc<Sequence>> {
        self.primary_sequence.as_ref()
    }

    /// What each MIDI-out port plays, by the requested port.
    #[cfg(test)]
    pub(crate) fn midi_sequences(&self) -> &[(Option<String>, Arc<Sequence>)] {
        &self.midi_sequences
    }

    /// The song's full sequence, split into what the main output plays and what each MIDI
    /// port plays.
    fn route_sequences(&self, full: &Arc<Sequence>) -> (Arc<Sequence>, MidiSequences) {
        let midi_tracks: Vec<usize> = full
            .performance
            .tracks
            .iter()
            .filter(|track| track.engine == gscore_play::TrackEngine::MidiOut)
            .map(|track| track.track.get())
            .collect();
        if midi_tracks.is_empty() {
            return (Arc::clone(full), Vec::new());
        }
        let primary = Arc::new(full.with_tracks(|track| !midi_tracks.contains(&track.get())));
        let mut groups: Vec<(Option<String>, Vec<TrackIndex>)> = Vec::new();
        for &track in &midi_tracks {
            let port = self.midi_route(track).port;
            match groups.iter_mut().find(|(other, _)| *other == port) {
                Some((_, tracks)) => tracks.push(TrackIndex::new(track)),
                None => groups.push((port, vec![TrackIndex::new(track)])),
            }
        }
        let midi = groups
            .into_iter()
            .map(|(port, tracks)| {
                let mut sequence = full.with_tracks(|track| tracks.contains(&track));
                // The click is the app's, not the hardware synthesizer's.
                sequence
                    .events
                    .retain(|event| event.track != gscore_play::CLICK_TRACK);
                for &track in &tracks {
                    if let Some(channel) =
                        self.midi_route(track.get()).channel.and_then(Channel::new)
                    {
                        set_track_channel(&mut sequence, track, channel);
                    }
                }
                (port, Arc::new(sequence))
            })
            .collect();
        (primary, midi)
    }

    /// The ports the MIDI-out tracks need, each once.
    fn required_midi_ports(&self) -> Vec<Option<String>> {
        let mut ports = Vec::new();
        for &(track, ref route) in &self.midi_routes {
            if self.track_is_midi_out(track) && !ports.contains(&route.port) {
                ports.push(route.port.clone());
            }
        }
        ports
    }

    /// Connects a player for every port the MIDI-out tracks need, and drops those no
    /// track uses any more. A port that cannot be opened is reported once; the other
    /// tracks still play.
    fn connect_midi_outs(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let needed = self.required_midi_ports();
            // A port that failed is opened again: the problem may have been fixed.
            self.midi_outs.retain(|connection| {
                needed.contains(&connection.requested) && connection.player.error().is_none()
            });
            let mut error = None;
            for port in needed {
                if self
                    .midi_outs
                    .iter()
                    .any(|connection| connection.requested == port)
                {
                    continue;
                }
                match MidiOutPlayer::connect(port.as_deref()) {
                    Ok(player) => {
                        if let Some(latency) = self.latency() {
                            player.set_delay(latency);
                        }
                        self.midi_outs.push(MidiOutConnection {
                            requested: port,
                            player,
                        });
                    }
                    Err(failed) => error = Some(failed.to_string()),
                }
            }
            self.midi_error = error;
        }
        // A browser has no MIDI ports; its MIDI-out choices fall back to the SoundFont.
        #[cfg(target_arch = "wasm32")]
        let _ = self.required_midi_ports();
    }

    /// How late the main output's sound reaches the ears, for holding the MIDI-out tracks
    /// back by the same time.
    fn latency(&self) -> Option<Duration> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match &self.player {
                Some(Active::Synth(player)) => player.latency(),
                _ => None,
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            None
        }
    }

    /// Stops every MIDI-out player.
    fn stop_midi_outs(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        for connection in &mut self.midi_outs {
            let _ = connection.player.stop();
        }
    }

    /// Holds the MIDI-out players back by the main output's latency, now that the device
    /// may have reported it.
    fn sync_midi_delay(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(latency) = self.latency() {
            for connection in &mut self.midi_outs {
                connection.player.set_delay(latency);
            }
        }
    }

    /// The MIDI-out player for `port`.
    #[cfg(not(target_arch = "wasm32"))]
    fn midi_player_mut(&mut self, port: &Option<String>) -> Option<&mut MidiOutPlayer> {
        self.midi_outs
            .iter_mut()
            .find(|connection| connection.requested == *port)
            .map(|connection| &mut connection.player)
    }

    /// The sequences to load: the main output's and each MIDI port's, behind a count-in
    /// when one runs (the MIDI outputs hear its timing but not its clicks).
    fn loading_sequences(&self) -> (Option<Arc<Sequence>>, MidiSequences) {
        let Some(primary) = &self.primary_sequence else {
            return (None, Vec::new());
        };
        let (primary, silent) = match &self.lead {
            Some(lead) => (
                Arc::new(gscore_play::lead_in(primary, &lead.clicks, lead.start)),
                Some(silent_clicks(&lead.clicks)),
            ),
            None => (Arc::clone(primary), None),
        };
        let midi = self
            .midi_sequences
            .iter()
            .map(|(port, sequence)| {
                let sequence = match (&self.lead, &silent) {
                    (Some(lead), Some(silent)) => {
                        Arc::new(gscore_play::lead_in(sequence, silent, lead.start))
                    }
                    _ => Arc::clone(sequence),
                };
                (port.clone(), sequence)
            })
            .collect();
        (Some(primary), midi)
    }

    /// Loads the sequences for the main output and the MIDI ports (stopped at their
    /// start).
    fn load_sequences(
        &mut self,
        primary: Option<Arc<Sequence>>,
        midi: &[(Option<String>, Arc<Sequence>)],
    ) -> Result<(), String> {
        let Some(primary) = primary else {
            return Ok(());
        };
        self.player_mut()?
            .load(primary)
            .map_err(|e| e.to_string())?;
        #[cfg(not(target_arch = "wasm32"))]
        for (port, sequence) in midi {
            if let Some(player) = self.midi_player_mut(port) {
                player
                    .load(Arc::clone(sequence))
                    .map_err(|e| e.to_string())?;
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = midi;
        Ok(())
    }

    /// Swaps in rebuilt sequences where playback is, without stopping.
    fn replace_sequences(
        &mut self,
        primary: Option<Arc<Sequence>>,
        midi: &[(Option<String>, Arc<Sequence>)],
    ) -> Result<(), String> {
        let Some(primary) = primary else {
            return Ok(());
        };
        self.player_mut()?
            .replace(primary)
            .map_err(|e| e.to_string())?;
        #[cfg(not(target_arch = "wasm32"))]
        for (port, sequence) in midi {
            if let Some(player) = self.midi_player_mut(port) {
                player
                    .replace(Arc::clone(sequence))
                    .map_err(|e| e.to_string())?;
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = midi;
        Ok(())
    }

    /// Carries out `action` on the main output and every MIDI-out player.
    fn transport_all(
        &mut self,
        mut action: impl FnMut(&mut dyn Player) -> Result<(), gscore_play::PlayError>,
    ) -> Result<(), String> {
        if let Some(active) = &mut self.player {
            action(active.player_mut()).map_err(|e| e.to_string())?;
        }
        #[cfg(not(target_arch = "wasm32"))]
        for connection in &mut self.midi_outs {
            action(&mut connection.player).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Swaps in a sequence rebuilt with the current settings at the position playback is,
    /// without stopping or silencing.
    fn rebuild_sequence(&mut self, song: &Song) -> Result<(), String> {
        if self.player.is_none() || self.sequence.is_none() {
            self.sequence = None;
            self.primary_sequence = None;
            self.midi_sequences.clear();
            return Ok(());
        }
        let sequence = self.build_sequence(song)?;
        let (primary, midi) = self.loading_sequences();
        self.replace_sequences(primary, &midi)?;
        self.sequence = Some(sequence);
        Ok(())
    }

    /// While playing, continues from `at` (a beat the user clicked), at its measure's first
    /// pass, without stopping; with the count-in on, after a new bar of clicks. Paused or stopped, nothing happens here: the next play
    /// starts at the cursor anyway.
    pub fn jump_to(&mut self, song: &Song, at: BeatPosition) -> Result<(), String> {
        if self.state() != PlayState::Playing {
            return Ok(());
        }
        // With the count-in on, a new place is counted in again.
        if self.count_in {
            self.paused_at = None;
            return self.start_count_in(song, Some(at));
        }
        let Some(tick) = self
            .sequence
            .as_ref()
            .and_then(|sequence| sequence.tick_of(song, at))
        else {
            return Ok(());
        };
        self.seek_song(tick)
    }

    /// Stops and goes back to the start.
    pub fn stop(&mut self) -> Result<(), String> {
        let _ = self.end_preview();
        self.paused_at = None;
        self.lead = None;
        self.reset_trainer();
        self.transport_all(|player| player.stop())
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
        if let Some(tick) = self.preview {
            return tick;
        }
        let position = self.output_position();
        match &self.lead {
            Some(lead) if !lead.passed || position < lead.bar => lead.start,
            Some(lead) => position.saturating_sub(lead.bar).saturating_add(lead.start),
            None => position,
        }
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

    /// How loud the metronome sounds right now (0 to 1), for the mixer's meter: its
    /// clicks during the count-in and, with the metronome on, during the song.
    pub fn metronome_level(&self) -> f32 {
        if self.state() != PlayState::Playing {
            return 0.0;
        }
        let (sequence, position) = if let Some(lead) = self.lead.as_ref().filter(|l| !l.passed) {
            (Some(&lead.clicks), self.output_position())
        } else {
            (self.sequence.as_ref(), self.position())
        };
        let Some(sequence) = sequence else {
            return 0.0;
        };
        // A click is short: its meter falls over an eighth note.
        let window = u64::from(PPQ) / 2;
        let start = position.saturating_sub(window);
        let events = &sequence.events;
        let first = events.partition_point(|event| event.tick < start);
        let last = events.partition_point(|event| event.tick <= position);
        events
            .get(first..last)
            .unwrap_or_default()
            .iter()
            .filter(|event| event.track == gscore_play::CLICK_TRACK)
            .filter_map(|event| match event.event {
                MidiEvent::NoteOn { velocity, .. } => {
                    let age = position.saturating_sub(event.tick) as f32 / window as f32;
                    Some(f32::from(velocity) / 127.0 * (1.0 - age).max(0.0))
                }
                _ => None,
            })
            .fold(0.0, f32::max)
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
        if self.player.is_none() {
            // Nothing is connected: the next play builds the sequence again.
            self.sequence = None;
            self.primary_sequence = None;
            self.midi_sequences.clear();
            return Ok(());
        }
        let rebuilt = if self.sequence.is_some() {
            Some(self.build_sequence(song)?)
        } else {
            None
        };
        for track in tracks {
            let Some(sound) = song.played_sound(track) else {
                continue;
            };
            if self.track_is_midi_out(track.get()) {
                let port = self.midi_route(track.get()).port;
                let channel = self
                    .midi_sequences
                    .iter()
                    .find(|(other, _)| *other == port)
                    .and_then(|(_, sequence)| sequence.channel_of(track));
                let Some(channel) = channel else {
                    continue;
                };
                #[cfg(target_arch = "wasm32")]
                let _ = channel;
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(player) = self.midi_player_mut(&port) {
                    for event in sound_events(channel, sound) {
                        player.send_event(event).map_err(|e| e.to_string())?;
                    }
                }
            } else if let Some(channel) = self
                .primary_sequence
                .as_ref()
                .and_then(|sequence| sequence.channel_of(track))
            {
                for event in sound_events(channel, sound) {
                    self.player_mut()?
                        .send_event(event)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        let Some(sequence) = rebuilt else {
            return Ok(());
        };
        let (primary, midi) = self.loading_sequences();
        self.replace_sequences(primary, &midi)?;
        self.sequence = Some(sequence);
        Ok(())
    }

    /// The song's sequence (also while a count-in plays before it).
    pub fn sequence(&self) -> Option<&Arc<Sequence>> {
        self.sequence.as_ref()
    }

    /// The last output error, if playback stopped because of one (a main output's or a
    /// MIDI-out port's).
    pub fn error(&self) -> Option<String> {
        if let Some(error) = self.player.as_ref().and_then(Active::error) {
            return Some(error);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(error) = self
            .midi_outs
            .iter()
            .find_map(|connection| connection.player.error())
        {
            return Some(error);
        }
        self.midi_error.clone()
    }

    /// An output error that has not been reported yet.
    pub fn new_error(&mut self) -> Option<String> {
        if let Some(notice) = self.chain_notice() {
            return Some(notice);
        }
        // A notice (the chosen sample rate was not available) is shown like an error, once.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(Active::Synth(player)) = &self.player
            && let Some(notice) = player.notice()
        {
            return Some(notice);
        }
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

/// Gives `track` of `sequence` `channel`: its own events move there (a borrowed channel
/// stays borrowed, so a bending note keeps its independent pitch).
fn set_track_channel(sequence: &mut Sequence, track: TrackIndex, channel: Channel) {
    let Some(position) = sequence
        .tracks
        .iter()
        .position(|entry| entry.track == track)
    else {
        return;
    };
    let old = sequence
        .tracks
        .get(position)
        .map(|entry| entry.channel)
        .unwrap_or(channel);
    if old == channel {
        return;
    }
    if let Some(entry) = sequence.tracks.get_mut(position) {
        entry.channel = channel;
    }
    for event in &mut sequence.events {
        if event.track == position && event.event.channel() == old {
            event.event = event.event.on_channel(channel);
        }
    }
}

/// The count-in clicks without their sound: its timing (the bar's length and tempo) for a
/// MIDI-out player, which must not make the clicks itself.
fn silent_clicks(clicks: &Sequence) -> Arc<Sequence> {
    let mut silent = clicks.clone();
    silent.events.clear();
    Arc::new(silent)
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

/// `sequence` with its engines: modelled strings for guitar and bass tracks when
/// `modelled`, then each track's own choice in `engines` (by track index; modelled strings
/// only for string tracks).
pub(crate) fn with_engines(
    sequence: Sequence,
    modelled: bool,
    engines: &[(usize, gscore_play::TrackEngine)],
    preset_tones: &[crate::settings::PresetTone],
) -> Sequence {
    let mut sequence = if modelled {
        // A preset the user gave a tone of its own plays that tone's kind of string.
        sequence
            .with_modelled_guitars_where(|program| chains::preset_kind_of(preset_tones, program))
    } else {
        sequence
    };
    if engines.is_empty() {
        return sequence;
    }
    let performance = Arc::make_mut(&mut sequence.performance);
    for track in &mut performance.tracks {
        let chosen = engines
            .iter()
            .find(|(index, _)| *index == track.track.get())
            .map(|&(_, engine)| engine);
        let strings =
            track.kind == gscore_model::InstrumentKind::Strings && !track.sound.percussion;
        match chosen {
            Some(engine @ gscore_play::TrackEngine::Modelled(_)) if strings => {
                track.engine = engine;
            }
            // Any track can be played by a hardware synthesizer; a browser has none, so it
            // plays such a track with the SoundFont.
            Some(gscore_play::TrackEngine::MidiOut) => {
                track.engine = if cfg!(target_arch = "wasm32") {
                    gscore_play::TrackEngine::SoundFont
                } else {
                    gscore_play::TrackEngine::MidiOut
                };
            }
            Some(gscore_play::TrackEngine::SoundFont) => {
                track.engine = gscore_play::TrackEngine::SoundFont;
            }
            _ => {}
        }
    }
    sequence
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_track_can_be_played_by_another_kind_of_string_or_the_soundfont() {
        use gscore_play::{StringKind, TrackEngine};
        let song = gscore_model::testing::song_with_notes(&[(
            1,
            &[gscore_model::testing::TestNote::new(3, 2)],
        )])
        .unwrap();
        let mut playback = Playback::default();
        let engine = |playback: &mut Playback| {
            playback.build_sequence(&song).unwrap().performance.tracks[0].engine
        };
        // A steel-guitar preset, played by nylon strings.
        playback
            .set_track_engines(&song, vec![(0, TrackEngine::Modelled(StringKind::Nylon))])
            .unwrap();
        assert_eq!(
            engine(&mut playback),
            TrackEngine::Modelled(StringKind::Nylon)
        );
        // With modelled guitars on, the SoundFont can still be chosen for it.
        playback.set_modelled_guitars(&song, true).unwrap();
        playback
            .set_track_engines(&song, vec![(0, TrackEngine::SoundFont)])
            .unwrap();
        assert_eq!(engine(&mut playback), TrackEngine::SoundFont);
        playback.set_track_engines(&song, Vec::new()).unwrap();
        assert_eq!(
            engine(&mut playback),
            TrackEngine::Modelled(StringKind::Steel)
        );
    }

    #[test]
    fn midi_out_tracks_are_split_off_with_their_own_port_and_channel() {
        use gscore_play::{StringKind, TrackEngine};
        let song = gscore_model::testing::song_with_notes_on_tracks(
            2,
            &[(1, &[gscore_model::testing::TestNote::new(3, 2)])],
        )
        .unwrap();
        let mut playback = Playback::default();
        playback
            .set_midi_routes(
                &song,
                vec![(
                    0,
                    MidiOutRoute {
                        port: Some("Hardware".to_owned()),
                        channel: Some(5),
                    },
                )],
            )
            .unwrap();
        playback
            .set_track_engines(&song, vec![(0, TrackEngine::MidiOut)])
            .unwrap();
        let full = playback.build_sequence(&song).unwrap();
        assert_eq!(full.tracks.len(), 2);
        assert_eq!(full.performance.tracks[0].engine, TrackEngine::MidiOut);
        // The main output has the other track only.
        let primary = playback.primary_sequence().unwrap();
        assert_eq!(primary.tracks.len(), 1);
        assert_eq!(primary.tracks[0].track, TrackIndex::new(1));
        assert!(
            primary
                .performance
                .tracks
                .iter()
                .all(|track| track.engine != TrackEngine::MidiOut)
        );
        // The hardware synthesizer gets its track, on the chosen channel.
        let midi = playback.midi_sequences();
        assert_eq!(midi.len(), 1);
        assert_eq!(midi[0].0.as_deref(), Some("Hardware"));
        assert_eq!(midi[0].1.tracks.len(), 1);
        assert_eq!(midi[0].1.tracks[0].track, TrackIndex::new(0));
        assert_eq!(midi[0].1.tracks[0].channel.get(), 5);
        assert!(
            midi[0]
                .1
                .events
                .iter()
                .all(|event| event.track != gscore_play::CLICK_TRACK),
            "the hardware synthesizer does not click"
        );
        // A modelled track still works in the main output; the MIDI-out route wins over
        // the string choice.
        playback.set_midi_routes(&song, Vec::new()).unwrap();
        playback
            .set_track_engines(&song, vec![(0, TrackEngine::Modelled(StringKind::Steel))])
            .unwrap();
        playback.build_sequence(&song).unwrap();
        assert_eq!(playback.midi_sequences().len(), 0);
        assert_eq!(playback.primary_sequence().unwrap().tracks.len(), 2);
    }

    #[test]
    fn only_offered_sample_rates_are_kept() {
        let mut playback = Playback::default();
        assert_eq!(playback.sample_rate(), None, "the device's own by default");
        playback.set_sample_rate(Some(96_000));
        assert_eq!(playback.sample_rate(), Some(96_000));
        playback.set_sample_rate(Some(12_345));
        assert_eq!(playback.sample_rate(), None, "an odd rate means automatic");
        assert_eq!(playback.audio_load(), None, "no synthesizer runs");
    }

    #[test]
    fn the_modelled_guitars_setting_chooses_the_engine_of_guitar_tracks() {
        let song = gscore_model::testing::song_with_notes(&[(
            1,
            &[gscore_model::testing::TestNote::new(3, 2)],
        )])
        .unwrap();
        let mut playback = Playback::default();
        let engine = |playback: &mut Playback| {
            playback.build_sequence(&song).unwrap().performance.tracks[0].engine
        };
        assert_eq!(engine(&mut playback), gscore_play::TrackEngine::SoundFont);
        playback.set_modelled_guitars(&song, true).unwrap();
        assert!(playback.modelled_guitars());
        assert!(matches!(
            engine(&mut playback),
            gscore_play::TrackEngine::Modelled(_)
        ));
    }

    #[test]
    fn the_speed_is_clamped_and_kept_between_plays() {
        let mut playback = Playback::default();
        assert_eq!(playback.speed(), 1.0);
        playback.set_speed(0.7).unwrap();
        assert_eq!(playback.speed(), 0.7);
        playback.set_speed(0.1).unwrap();
        assert_eq!(playback.speed(), SPEED_RANGE.0);
        playback.set_speed(9.0).unwrap();
        assert_eq!(playback.speed(), SPEED_RANGE.1);
        playback.set_speed(f32::NAN).unwrap();
        assert_eq!(playback.speed(), 1.0);
    }

    #[test]
    fn the_speed_trainer_raises_the_speed_every_few_passes_up_to_its_target() {
        let mut playback = Playback::default();
        playback.set_speed(0.8).unwrap();
        playback.set_trainer(SpeedTrainer {
            enabled: true,
            every: 2,
            step: 5,
            target: 1.0,
        });
        assert!(!playback.trainer_pass().unwrap(), "the first pass waits");
        assert!(playback.trainer_pass().unwrap());
        assert!((playback.speed() - 0.85).abs() < 1e-6);
        while playback.speed() < 1.0 {
            let _ = playback.trainer_pass().unwrap();
        }
        assert!((playback.speed() - 1.0).abs() < 1e-6);
        assert!(!playback.trainer_pass().unwrap(), "the target is the top");
        assert_eq!(playback.speed(), 1.0);

        // A disabled trainer never changes the speed.
        playback.set_speed(0.6).unwrap();
        playback.set_trainer(SpeedTrainer {
            enabled: false,
            ..playback.trainer()
        });
        for _ in 0..10 {
            assert!(!playback.trainer_pass().unwrap());
        }
        assert!((playback.speed() - 0.6).abs() < 1e-6);
    }
}
