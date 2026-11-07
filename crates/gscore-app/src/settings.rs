//! The app's settings: view state, panel sizes, window, recent files, keymap and autosave.
//! Plain serializable data, stored as JSON in the user's config directory (`gscore-session`),
//! so it can be inspected and restored without a running app.

use std::path::PathBuf;
use std::time::Duration;

use gscore_edit::{ActionId, KeyCombo, Keymap};
use gscore_session::{RecoveryOptions, SNAPSHOT_EVERY};
use serde::{Deserialize, Serialize};

use crate::state::{ViewState, ZOOM_RANGE};

/// How many recent files the File menu remembers.
pub const MAX_RECENT_FILES: usize = 10;
/// Most tabs remembered to reopen at the next start.
pub const MAX_OPEN_FILES: usize = 64;
/// Most changed actions accepted from the settings file, and keys per action.
const MAX_SHORTCUT_OVERRIDES: usize = 1024;
const MAX_KEYS_PER_ACTION: usize = 32;
/// Default interval between recovery snapshots, in seconds.
pub const DEFAULT_AUTOSAVE_SECONDS: u64 = 60;

/// The window's size, position and mode, so it opens the way it was closed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    /// Left edge in screen points; `None` where the system does not tell (Wayland).
    #[serde(default)]
    pub x: Option<f32>,
    /// Top edge in screen points; `None` where the system does not tell (Wayland).
    #[serde(default)]
    pub y: Option<f32>,
    /// Inner width (when not maximized or fullscreen).
    pub width: f32,
    /// Inner height (when not maximized or fullscreen).
    pub height: f32,
    /// Whether it was maximized.
    pub maximized: bool,
    /// Whether it was fullscreen.
    #[serde(default)]
    pub fullscreen: bool,
}

impl WindowState {
    /// Whether the values make sense (finite, positive size, finite position if any).
    pub fn is_valid(self) -> bool {
        self.x.is_none_or(f32::is_finite)
            && self.y.is_none_or(f32::is_finite)
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
    }

    /// The size as egui takes it.
    pub fn size(self) -> [f32; 2] {
        [self.width, self.height]
    }

    /// The position as egui takes it, if known.
    pub fn position(self) -> Option<[f32; 2]> {
        Some([self.x?, self.y?])
    }

    /// The state to remember for a window as egui describes it, or `None` before its size
    /// is known. While it is maximized or fullscreen, the normal size and place from
    /// `previous` are kept, so leaving that mode after a restart gives them back.
    pub fn of_viewport(
        info: &egui::ViewportInfo,
        previous: Option<WindowState>,
    ) -> Option<WindowState> {
        let inner = info.inner_rect?;
        let maximized = info.maximized.unwrap_or(false);
        let fullscreen = info.fullscreen.unwrap_or(false);
        if (maximized || fullscreen)
            && let Some(previous) = previous
        {
            return Some(WindowState {
                maximized,
                fullscreen,
                ..previous
            });
        }
        let state = WindowState {
            x: info.outer_rect.map(|rect| rect.min.x),
            y: info.outer_rect.map(|rect| rect.min.y),
            width: inner.width(),
            height: inner.height(),
            maximized,
            fullscreen,
        };
        state.is_valid().then_some(state)
    }
}

/// One action whose shortcuts the user changed, with all of its keys. Only changed actions
/// are stored, so built-in shortcuts added later still reach the user. Plain data, so the
/// keymap lives in the settings file instead of a separate text file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortcutSetting {
    /// The action id.
    pub action: String,
    /// Its key combinations as written (`ctrl+z`, `Left`, `+`, ...); empty when the action
    /// has no shortcut any more.
    pub keys: Vec<String>,
}

/// Most bend (or whammy) presets kept, and the longest preset name.
pub const MAX_SHAPE_PRESETS: usize = 64;
const MAX_PRESET_NAME: usize = 64;

/// A bend or whammy shape the user saved as a preset in the shape editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapePreset {
    /// The name shown in the preset list.
    pub name: String,
    /// Whether the bend is a release (bend presets only).
    #[serde(default)]
    pub release: bool,
    /// The points as (position 0 to 12, value in quarter tones).
    pub points: Vec<(u8, i8)>,
}

impl ShapePreset {
    /// The points as the model takes them.
    pub fn bend_points(&self) -> Vec<gscore_model::BendPoint> {
        self.points
            .iter()
            .map(|&(position, value)| gscore_model::BendPoint { position, value })
            .collect()
    }

    /// Whether the name and the points are usable (a broken settings file can hold
    /// anything).
    fn is_valid(&self) -> bool {
        let name = self.name.trim();
        !name.is_empty()
            && name.chars().count() <= MAX_PRESET_NAME
            && gscore_model::WhammyShape::new(self.bend_points()).is_ok()
    }
}

/// The pickup of a string sound as the settings file keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PickupSetting {
    /// Not set (a sound saved before pickups existed): its kind's preset decides, see
    /// [`StringSound::for_kind`].
    #[default]
    Unset,
    None,
    SingleCoil,
    Humbucker,
}

impl PickupSetting {
    /// The pickups a user can choose, in the order the window shows them, with a label.
    pub const CHOICES: [(PickupSetting, &'static str); 3] = [
        (PickupSetting::None, "None"),
        (PickupSetting::SingleCoil, "Single coil"),
        (PickupSetting::Humbucker, "Humbucker"),
    ];

    fn pickup(self) -> gscore_guitar::Pickup {
        match self {
            PickupSetting::Unset | PickupSetting::None => gscore_guitar::Pickup::None,
            PickupSetting::SingleCoil => gscore_guitar::Pickup::SingleCoil,
            PickupSetting::Humbucker => gscore_guitar::Pickup::Humbucker,
        }
    }
}

impl From<gscore_guitar::Pickup> for PickupSetting {
    fn from(pickup: gscore_guitar::Pickup) -> Self {
        match pickup {
            gscore_guitar::Pickup::None => PickupSetting::None,
            gscore_guitar::Pickup::SingleCoil => PickupSetting::SingleCoil,
            gscore_guitar::Pickup::Humbucker => PickupSetting::Humbucker,
        }
    }
}

/// One kind of modelled string as the user dialed it in: the knobs of
/// [`gscore_guitar::StringModel`], kept as plain numbers in the settings file.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StringSound {
    pub sustain: f32,
    pub sustain_exponent: f32,
    pub brilliance: f32,
    pub stiffness: f32,
    pub stiffness_stages: u8,
    pub pick_position: f32,
    pub hardness: f32,
    pub softness: f32,
    pub tone: f32,
    pub resonance: f32,
    pub level: f32,
    pub bridge: f32,
    pub low_cut: f32,
    pub second: f32,
    pub detune: f32,
    /// A magnetic pickup and where it sits (a share of the string from the bridge). A
    /// sound saved before pickups existed has none set (not the struct's default, an
    /// electric's humbucker): its kind decides.
    #[serde(default)]
    pub pickup: PickupSetting,
    #[serde(default)]
    pub pickup_position: f32,
}

impl Default for StringSound {
    fn default() -> Self {
        StringSound::from(gscore_audio::default_string_model(
            gscore_play::StringKind::Electric,
        ))
    }
}

impl From<gscore_guitar::StringModel> for StringSound {
    fn from(model: gscore_guitar::StringModel) -> Self {
        StringSound {
            sustain: model.sustain,
            sustain_exponent: model.sustain_exponent,
            brilliance: model.brilliance,
            stiffness: model.stiffness,
            stiffness_stages: u8::try_from(model.stiffness_stages).unwrap_or(0),
            pick_position: model.pick_position,
            hardness: model.hardness,
            softness: model.softness,
            tone: model.tone,
            resonance: model.resonance,
            level: model.level,
            bridge: model.bridge,
            low_cut: model.low_cut,
            second: model.second,
            detune: model.detune,
            pickup: model.pickup.into(),
            pickup_position: model.pickup_position,
        }
    }
}

impl StringSound {
    /// The model these knobs make, every value in its range.
    pub fn model(&self) -> gscore_guitar::StringModel {
        gscore_guitar::StringModel {
            sustain: self.sustain,
            sustain_exponent: self.sustain_exponent,
            brilliance: self.brilliance,
            stiffness: self.stiffness,
            stiffness_stages: usize::from(self.stiffness_stages),
            pick_position: self.pick_position,
            hardness: self.hardness,
            softness: self.softness,
            tone: self.tone,
            resonance: self.resonance,
            level: self.level,
            bridge: self.bridge,
            low_cut: self.low_cut,
            second: self.second,
            detune: self.detune,
            pickup: self.pickup.pickup(),
            pickup_position: self.pickup_position,
        }
        .sanitized()
    }

    /// This sound for strings of `kind`: a pickup not set comes from the kind's preset
    /// (an electric's humbucker; none for an acoustic or the bass).
    pub fn for_kind(mut self, kind: gscore_play::StringKind) -> Self {
        if self.pickup == PickupSetting::Unset {
            let preset = gscore_audio::default_string_model(kind);
            self.pickup = preset.pickup.into();
            self.pickup_position = preset.pickup_position;
        }
        self
    }
}

/// A chain the user saved under a name (the Sound chain window's presets): the whole effect
/// chain of a kind of string, and the string sound it was saved with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainPreset {
    /// The name shown in the preset list.
    pub name: String,
    /// The kind of string it is for: `nylon`, `steel`, `electric` or `bass`.
    pub kind: String,
    pub sound: StringSound,
    /// The five places after the strings (a preset written before the chain was part of it
    /// has an empty one).
    #[serde(default)]
    pub chain: crate::chain_settings::ChainSetting,
    /// What plays the strings: `soundfont` for the internal synth, or the kind of modelled
    /// string (`nylon`, `steel`, `electric`, `bass`); empty leaves the track's choice.
    #[serde(default)]
    pub engine: String,
}

/// A tone the user set for a track preset (its General MIDI program): what its strings
/// sound like and the chain after them. Kept in the settings, so it plays on every track
/// with that preset, in every song; "Built-in" in the Sound chain window removes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetTone {
    /// The preset's General MIDI program (0 to 127).
    pub program: u8,
    /// The kind of string it is for: `nylon`, `steel`, `electric` or `bass`.
    pub kind: String,
    pub sound: StringSound,
    /// The five places after the strings (a tone written before the chain was part of it
    /// has an empty one).
    #[serde(default)]
    pub chain: crate::chain_settings::ChainSetting,
}

impl PresetTone {
    /// `tone` as the settings keep it, for the preset `program`.
    pub fn of(program: u8, tone: &crate::chain_settings::Tone) -> Self {
        PresetTone {
            program,
            kind: kind_id(tone.kind).to_owned(),
            sound: tone.sound,
            chain: tone.chain.clone(),
        }
    }

    /// The tone it stands for, if its kind is one we know.
    pub fn tone(&self) -> Option<crate::chain_settings::Tone> {
        let kind = kind_from_id(&self.kind)?;
        Some(crate::chain_settings::Tone {
            kind,
            sound: self.sound.for_kind(kind),
            chain: self.chain.clone(),
        })
    }
}

/// The settings name of what plays a track.
pub(crate) fn engine_id(engine: gscore_play::TrackEngine) -> &'static str {
    match engine {
        gscore_play::TrackEngine::SoundFont => "soundfont",
        gscore_play::TrackEngine::Modelled(kind) => kind_id(kind),
        gscore_play::TrackEngine::MidiOut => "midi_out",
    }
}

/// The engine an `id` names, if it names one.
pub(crate) fn engine_from_id(id: &str) -> Option<gscore_play::TrackEngine> {
    match id {
        "soundfont" => Some(gscore_play::TrackEngine::SoundFont),
        "midi_out" => Some(gscore_play::TrackEngine::MidiOut),
        "nylon" => Some(gscore_play::TrackEngine::Modelled(
            gscore_play::StringKind::Nylon,
        )),
        "steel" => Some(gscore_play::TrackEngine::Modelled(
            gscore_play::StringKind::Steel,
        )),
        "electric" => Some(gscore_play::TrackEngine::Modelled(
            gscore_play::StringKind::Electric,
        )),
        "bass" => Some(gscore_play::TrackEngine::Modelled(
            gscore_play::StringKind::Bass,
        )),
        _ => None,
    }
}

/// The settings name of `kind`.
pub(crate) fn kind_id(kind: gscore_play::StringKind) -> &'static str {
    match kind {
        gscore_play::StringKind::Nylon => "nylon",
        gscore_play::StringKind::Steel => "steel",
        gscore_play::StringKind::Electric => "electric",
        gscore_play::StringKind::Bass => "bass",
    }
}

/// The kind of string an `id` names, if it names one.
pub(crate) fn kind_from_id(id: &str) -> Option<gscore_play::StringKind> {
    match engine_from_id(id) {
        Some(gscore_play::TrackEngine::Modelled(kind)) => Some(kind),
        _ => None,
    }
}

/// The range a remembered panel width may have, and of the song grid's height.
const PANEL_WIDTH_RANGE: (f32, f32) = (200.0, 1200.0);
const GRID_HEIGHT_RANGE: (f32, f32) = (72.0, 2000.0);

/// A remembered size within `range`; anything that is not a number means "not chosen".
fn fit_panel(size: Option<f32>, (low, high): (f32, f32)) -> Option<f32> {
    size.filter(|size| size.is_finite())
        .map(|size| size.clamp(low, high))
}

/// The most string sound presets kept, and the longest name.
pub(crate) const MAX_STRING_PRESETS: usize = 100;
pub(crate) const MAX_STRING_PRESET_NAME: usize = 60;
/// How many track presets (General MIDI programs) there are.
pub(crate) const MAX_PROGRAMS: u8 = 128;

/// The four kinds of modelled string, as dialed in.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StringSounds {
    pub nylon: StringSound,
    pub steel: StringSound,
    pub electric: StringSound,
    pub bass: StringSound,
}

impl Default for StringSounds {
    fn default() -> Self {
        let preset = |kind| StringSound::from(gscore_audio::default_string_model(kind));
        StringSounds {
            nylon: preset(gscore_play::StringKind::Nylon),
            steel: preset(gscore_play::StringKind::Steel),
            electric: preset(gscore_play::StringKind::Electric),
            bass: preset(gscore_play::StringKind::Bass),
        }
    }
}

impl StringSounds {
    /// The knobs of `kind`.
    pub fn get(&self, kind: gscore_play::StringKind) -> StringSound {
        match kind {
            gscore_play::StringKind::Nylon => self.nylon,
            gscore_play::StringKind::Steel => self.steel,
            gscore_play::StringKind::Electric => self.electric,
            gscore_play::StringKind::Bass => self.bass,
        }
        .for_kind(kind)
    }

    /// Sets the knobs of `kind`.
    pub fn set(&mut self, kind: gscore_play::StringKind, sound: StringSound) {
        match kind {
            gscore_play::StringKind::Nylon => self.nylon = sound,
            gscore_play::StringKind::Steel => self.steel = sound,
            gscore_play::StringKind::Electric => self.electric = sound,
            gscore_play::StringKind::Bass => self.bass = sound,
        }
    }
}

/// The mixer's reverb bus: the reverb every modelled track sends into with its mixer reverb
/// amount. Kept in the settings (it is the mixer's, not the song's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReverbBus {
    /// The bus's level, 0-127 (127 plays it as it comes; the track volumes are the sends).
    pub level: u8,
    pub muted: bool,
    /// What the bus runs: the computed reverb, or a room's impulse response (or nothing).
    /// Its knobs apply; the mix is always 1 (the bus runs all wet).
    pub block: crate::chain_settings::BlockSetting,
}

impl Default for ReverbBus {
    fn default() -> Self {
        // A large hall: the bus is where the big, shared reverb lives.
        let mut block =
            crate::chain_settings::BlockSetting::of(crate::chain_settings::BlockChoice::Reverb);
        block.size = 0.9;
        block.decay = 5.0;
        block.pre_delay = 30.0;
        ReverbBus {
            level: 127,
            muted: false,
            block,
        }
    }
}

impl ReverbBus {
    /// Every value in its range; a bus is a reverb or a room, never a cabinet or a place
    /// that holds nothing (empty means it does not run at all).
    pub fn sanitized(mut self) -> Self {
        self.level = self.level.min(127);
        if !matches!(
            self.block.choice,
            crate::chain_settings::BlockChoice::Empty
                | crate::chain_settings::BlockChoice::Room
                | crate::chain_settings::BlockChoice::Reverb
        ) {
            self.block =
                crate::chain_settings::BlockSetting::of(crate::chain_settings::BlockChoice::Reverb);
        }
        self.block.mix = 1.0;
        self
    }
}

/// The mixer's compressor bus: a compressor every track can send into with its Comp knob
/// (parallel, "New York" compression: the compressed sum blended in under the dry tracks).
/// Kept in the settings (it is the mixer's, not the song's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompressorBus {
    /// The bus's level, 0-127 (127 plays it as it comes; the Comp sends decide how much
    /// each track gives it).
    pub level: u8,
    pub muted: bool,
    /// What the bus runs: a compressor, or nothing. Its knobs apply; the mix is the dry
    /// share of the compressed sum (0.5 blends it in under the tracks).
    pub block: crate::chain_settings::BlockSetting,
}

impl Default for CompressorBus {
    fn default() -> Self {
        // A CA-76 at "All", driven hard and made up: the classic parallel slam.
        let mut block =
            crate::chain_settings::BlockSetting::of(crate::chain_settings::BlockChoice::Compressor);
        block.input_db = 18.0;
        block.gain_db = -12.0;
        block.attack_ms = 0.2;
        block.release_ms = 150.0;
        block.comp_ratio = crate::chain_settings::CompRatioSetting::All;
        block.mix = 1.0;
        CompressorBus {
            level: 127,
            muted: false,
            block,
        }
    }
}

impl CompressorBus {
    /// Every value in its range; a compressor bus runs a compressor or nothing, never a
    /// reverb or a cabinet.
    pub fn sanitized(mut self) -> Self {
        self.level = self.level.min(127);
        if !matches!(
            self.block.choice,
            crate::chain_settings::BlockChoice::Empty
                | crate::chain_settings::BlockChoice::Compressor
        ) {
            self.block = crate::chain_settings::BlockSetting::of(
                crate::chain_settings::BlockChoice::Compressor,
            );
        }
        self
    }
}

/// A drum part's tone the user dialed in (a kick's EQ and compressor, say), kept in the
/// settings: every drum track's kit plays it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrumPartTone {
    /// The part's id (`kick`, `snare`, `hi_hat`, `toms`, `cymbals`, `percussion`).
    pub part: String,
    /// Its blocks and knobs.
    #[serde(default)]
    pub chain: crate::chain_settings::ChainSetting,
}

impl DrumPartTone {
    /// The tone of `part`, as the settings keep it.
    pub fn of(
        part: crate::chain_settings::DrumPart,
        chain: &crate::chain_settings::ChainSetting,
    ) -> Self {
        DrumPartTone {
            part: part.id().to_owned(),
            chain: chain.clone(),
        }
    }

    /// The part it is for.
    pub fn part(&self) -> Option<crate::chain_settings::DrumPart> {
        crate::chain_settings::DrumPart::from_id(&self.part)
    }
}

/// Everything the app remembers between runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Light or dark.
    pub theme: crate::state::ThemeChoice,
    /// Zoom factor; 1 is normal.
    pub zoom: f32,
    /// Size of the whole interface (menus, panels, icons); 1 is normal.
    pub interface_zoom: f32,
    /// Whether the song grid (bottom panel) is shown.
    pub show_track_grid: bool,
    /// Whether the bottom panel shows the mixer instead of the measure grid.
    pub grid_mixer: bool,
    /// Whether the track panel (right panel) is shown.
    pub show_track_panel: bool,
    /// Whether the edit palette (left panel) is shown.
    pub show_edit_panel: bool,
    /// How the score fills the window: pages or an endless surface, vertical or horizontal.
    pub score_view: crate::state::ScoreView,
    /// The old on/off page switch from settings files written before the view modes. Only
    /// read; `None` once this version has saved the file.
    #[serde(rename = "page_view", default, skip_serializing_if = "Option::is_none")]
    pub legacy_page_view: Option<bool>,
    /// With several tracks shown, whether empty staves are left out of a line.
    pub hide_empty_staves: bool,
    /// Height the song grid was dragged to; `None`: it follows the number of tracks. (The
    /// key is new: the old `grid_height_chosen` held whatever the panel happened to be.)
    pub grid_height_chosen: Option<f32>,
    /// Height the mixer view was dragged to; `None` uses its default.
    pub mixer_height_chosen: Option<f32>,
    /// Width of the track panel, as last dragged.
    pub track_panel_width: Option<f32>,
    /// Width of the edit palette, as last dragged.
    pub edit_panel_width: Option<f32>,
    /// The window's last position and size.
    pub window: Option<WindowState>,
    /// Recently opened files, newest first.
    pub recent_files: Vec<PathBuf>,
    /// Reopen the tabs that were open last time at startup (on by default).
    pub reopen_last_file: bool,
    /// The files of the tabs that were open, in order (reopened at startup).
    pub open_files: Vec<PathBuf>,
    /// The file of the tab that was active when they were saved, if any.
    pub active_file: Option<PathBuf>,
    /// The file to reopen at startup (written for older versions; see `open_files`).
    pub last_file: Option<PathBuf>,
    /// The keymap file, if the user chose one.
    pub keymap_path: Option<PathBuf>,
    /// The actions whose shortcuts were changed in the shortcut editor (relative to the
    /// built-in keymap), applied over the built-in keymap or a keymap file.
    pub shortcut_overrides: Vec<ShortcutSetting>,
    /// Whether a fret typed into the score is played back at once.
    pub audition_notes: bool,
    /// Seconds between recovery snapshots; 0 means only by edit count.
    pub autosave_seconds: u64,
    /// The metronome's volume, 0 to 1.
    pub metronome_volume: f32,
    /// The built-in synthesizer's click sound (`wood`, `beep` or `cowbell`).
    pub click_sound: String,
    /// How many bars the count-in clicks (1 to 4).
    pub count_in_bars: u8,
    /// Whether a looped selection is counted in again each time it starts over.
    pub count_in_loops: bool,
    /// Playback speed relative to the written tempo (0.5 to 1.2).
    pub playback_speed: f32,
    /// Whether the speed trainer raises the speed after every few loop passes.
    pub speed_trainer_enabled: bool,
    /// Loop passes between two speed steps.
    pub speed_trainer_every: u8,
    /// How much faster each step is, in percent.
    pub speed_trainer_step: u8,
    /// The top speed the trainer works up to (1.0, 1.1 or 1.2).
    pub speed_trainer_target: f32,
    /// The SoundFont chosen for the built-in synthesizer, if any.
    pub soundfont_path: Option<PathBuf>,
    /// Whether playback prefers the built-in synthesizer over MIDI output.
    pub prefer_synth: bool,
    /// Whether the built-in synthesizer plays guitar and bass tracks with the modelled
    /// strings instead of the SoundFont (experimental, M22).
    pub modelled_guitars: bool,
    /// The sample rate the built-in synthesizer asks the device for (and WAV export uses);
    /// `None`: the device's own.
    pub sample_rate: Option<u32>,
    /// How each kind of modelled string sounds (the Sound chain window's strings).
    pub string_sounds: StringSounds,
    /// Chains (with their string sound) saved under a name, by the Sound chain window.
    pub string_presets: Vec<ChainPreset>,
    /// The tones set for track presets (by General MIDI program), local to this machine:
    /// a track whose preset has one plays it, in every song.
    pub preset_tones: Vec<PresetTone>,
    /// The tones set for the parts of a drum kit (kick, snare, ...), local to this
    /// machine: every drum track's parts play them.
    pub drum_parts: Vec<DrumPartTone>,
    /// The effect chain after each kind of modelled string (the "Sound chain" window).
    pub string_chains: crate::chain_settings::StringChains,
    /// The mixer's reverb bus.
    pub reverb_bus: ReverbBus,
    /// The mixer's compressor bus (parallel compression).
    pub compressor_bus: CompressorBus,
    /// The MIDI port chosen, if any.
    pub midi_port: Option<String>,
    /// Bend shapes saved as presets in the bend editor.
    pub bend_presets: Vec<ShapePreset>,
    /// Whammy shapes saved as presets in the whammy editor.
    pub whammy_presets: Vec<ShapePreset>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: crate::state::ThemeChoice::Light,
            zoom: 1.0,
            interface_zoom: 1.0,
            show_track_grid: true,
            grid_mixer: false,
            show_track_panel: true,
            show_edit_panel: true,
            score_view: crate::state::ScoreView::PageVertical,
            legacy_page_view: None,
            hide_empty_staves: false,
            grid_height_chosen: None,
            mixer_height_chosen: None,
            track_panel_width: None,
            edit_panel_width: None,
            window: None,
            recent_files: Vec::new(),
            reopen_last_file: true,
            open_files: Vec::new(),
            active_file: None,
            last_file: None,
            keymap_path: None,
            shortcut_overrides: Vec::new(),
            audition_notes: true,
            autosave_seconds: DEFAULT_AUTOSAVE_SECONDS,
            metronome_volume: 1.0,
            click_sound: gscore_play::ClickSound::default().id().to_owned(),
            count_in_bars: 1,
            count_in_loops: false,
            playback_speed: 1.0,
            speed_trainer_enabled: false,
            speed_trainer_every: 2,
            speed_trainer_step: 5,
            speed_trainer_target: 1.0,
            soundfont_path: None,
            prefer_synth: true,
            modelled_guitars: true,
            sample_rate: None,
            string_sounds: StringSounds::default(),
            string_chains: crate::chain_settings::StringChains::default(),
            reverb_bus: ReverbBus::default(),
            compressor_bus: CompressorBus::default(),
            string_presets: Vec::new(),
            preset_tones: Vec::new(),
            drum_parts: Vec::new(),
            midi_port: None,
            bend_presets: Vec::new(),
            whammy_presets: Vec::new(),
        }
    }
}

impl Settings {
    /// Settings that show `view`.
    pub fn from_view(view: &ViewState) -> Self {
        let mut settings = Settings::default();
        settings.update_from_view(view);
        settings
    }

    /// Takes the view state's theme, zoom and panel visibility.
    pub fn update_from_view(&mut self, view: &ViewState) {
        self.theme = view.theme;
        self.zoom = view.zoom();
        self.interface_zoom = view.interface_zoom();
        self.show_track_grid = view.show_track_grid;
        self.grid_mixer = view.grid_mixer;
        self.show_track_panel = view.show_track_panel;
        self.show_edit_panel = view.show_edit_panel;
        self.score_view = view.score_view;
        self.legacy_page_view = None;
        self.hide_empty_staves = view.hide_empty_staves;
    }

    /// Applies the stored theme, zoom and panel visibility to `view`.
    pub fn apply_to_view(&self, view: &mut ViewState) {
        view.theme = self.theme;
        view.set_zoom(self.zoom);
        view.set_interface_zoom(self.interface_zoom);
        view.show_track_grid = self.show_track_grid;
        view.grid_mixer = self.grid_mixer;
        view.show_track_panel = self.show_track_panel;
        view.show_edit_panel = self.show_edit_panel;
        view.score_view = match self.legacy_page_view {
            Some(false) => crate::state::ScoreView::ScreenVertical,
            _ => self.score_view,
        };
        view.hide_empty_staves = self.hide_empty_staves;
    }

    /// Puts `path` first in the recent files, without duplicates.
    pub fn add_recent(&mut self, path: PathBuf) {
        self.recent_files.retain(|recent| recent != &path);
        self.recent_files.insert(0, path);
        if gscore_play::ClickSound::from_id(&self.click_sound).is_none() {
            self.click_sound = gscore_play::ClickSound::default().id().to_owned();
        }
        self.count_in_bars = self.count_in_bars.clamp(1, gscore_play::MAX_COUNT_IN_BARS);
        self.recent_files.truncate(MAX_RECENT_FILES);
    }

    /// How often the recovery journal writes a full snapshot.
    pub fn recovery_options(&self) -> RecoveryOptions {
        RecoveryOptions {
            snapshot_every: SNAPSHOT_EVERY,
            snapshot_interval: if self.autosave_seconds == 0 {
                Duration::ZERO
            } else {
                Duration::from_secs(self.autosave_seconds.min(24 * 60 * 60))
            },
        }
    }

    /// Stores the shortcuts of `keymap` that differ from the built-in keymap.
    pub fn store_shortcuts(&mut self, keymap: &Keymap) {
        self.shortcut_overrides = keymap
            .overrides(&Keymap::default())
            .into_iter()
            .map(|(action, keys)| ShortcutSetting {
                action: action.id().to_owned(),
                keys: keys.iter().map(ToString::to_string).collect(),
            })
            .collect();
    }

    /// Applies the stored shortcut changes to `keymap`. Unusable entries (an unknown
    /// action or key) are skipped, the rest still apply; the error names what was skipped.
    pub fn apply_shortcuts(&self, keymap: &mut Keymap) -> Result<(), String> {
        let mut overrides = Vec::with_capacity(self.shortcut_overrides.len());
        let mut errors = Vec::new();
        for stored in &self.shortcut_overrides {
            let Some(action) = ActionId::from_id(&stored.action) else {
                errors.push(format!("unknown action `{}`", stored.action));
                continue;
            };
            let mut keys = Vec::with_capacity(stored.keys.len());
            for key in &stored.keys {
                match KeyCombo::parse(key) {
                    Ok(combo) => keys.push(combo),
                    Err(error) => errors.push(format!("`{key}`: {error}")),
                }
            }
            overrides.push((action, keys));
        }
        keymap.apply_overrides(overrides);
        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Some stored shortcuts are not usable and were skipped: {}",
                errors.join("; ")
            ))
        }
    }

    /// Makes the loaded values safe: finite zoom and sizes, a sane autosave interval, and
    /// no nonsense window rectangles.
    pub fn sanitize(&mut self) {
        if self
            .sample_rate
            .is_some_and(|rate| !crate::playback::SAMPLE_RATES.contains(&rate))
        {
            self.sample_rate = None;
        }
        if !self.zoom.is_finite() {
            self.zoom = 1.0;
        }
        self.zoom = self.zoom.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1);
        let mut view = ViewState::default();
        view.set_interface_zoom(self.interface_zoom);
        self.interface_zoom = view.interface_zoom();
        if let Some(height) = self.grid_height_chosen
            && !(height.is_finite() && height > 0.0)
        {
            self.grid_height_chosen = None;
        }
        if let Some(height) = self.mixer_height_chosen
            && !(height.is_finite() && height > 0.0)
        {
            self.mixer_height_chosen = None;
        }
        if let Some(width) = self.track_panel_width
            && !(width.is_finite() && width > 0.0)
        {
            self.track_panel_width = None;
        }
        if let Some(width) = self.edit_panel_width
            && !(width.is_finite() && width > 0.0)
        {
            self.edit_panel_width = None;
        }
        if self.window.is_some_and(|window| !window.is_valid()) {
            self.window = None;
        }
        if self.autosave_seconds > 24 * 60 * 60 {
            self.autosave_seconds = DEFAULT_AUTOSAVE_SECONDS;
        }
        self.metronome_volume = if self.metronome_volume.is_finite() {
            self.metronome_volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.playback_speed = if self.playback_speed.is_finite() {
            self.playback_speed.clamp(
                crate::playback::SPEED_RANGE.0,
                crate::playback::SPEED_RANGE.1,
            )
        } else {
            1.0
        };
        let mut trainer = crate::playback::SpeedTrainer {
            enabled: self.speed_trainer_enabled,
            every: self.speed_trainer_every,
            step: self.speed_trainer_step,
            target: self.speed_trainer_target,
        };
        trainer.sanitize();
        self.speed_trainer_enabled = trainer.enabled;
        self.speed_trainer_every = trainer.every;
        self.speed_trainer_step = trainer.step;
        self.speed_trainer_target = trainer.target;
        self.recent_files.truncate(MAX_RECENT_FILES);
        self.open_files.truncate(MAX_OPEN_FILES);
        // A hostile or broken file cannot make the keymap huge.
        self.shortcut_overrides.truncate(MAX_SHORTCUT_OVERRIDES);
        self.shortcut_overrides
            .retain(|stored| !stored.action.is_empty() && stored.action.len() <= 64);
        for stored in &mut self.shortcut_overrides {
            stored.keys.truncate(MAX_KEYS_PER_ACTION);
            stored.keys.retain(|key| !key.is_empty() && key.len() <= 64);
        }
        self.reverb_bus = std::mem::take(&mut self.reverb_bus).sanitized();
        self.compressor_bus = std::mem::take(&mut self.compressor_bus).sanitized();
        // Panel sizes: a broken or huge value must not take the window over.
        self.track_panel_width = fit_panel(self.track_panel_width, PANEL_WIDTH_RANGE);
        self.edit_panel_width = fit_panel(self.edit_panel_width, PANEL_WIDTH_RANGE);
        self.grid_height_chosen = fit_panel(self.grid_height_chosen, GRID_HEIGHT_RANGE);
        self.mixer_height_chosen = fit_panel(self.mixer_height_chosen, GRID_HEIGHT_RANGE);
        for presets in [&mut self.bend_presets, &mut self.whammy_presets] {
            presets.retain(ShapePreset::is_valid);
            presets.truncate(MAX_SHAPE_PRESETS);
        }
        // Chain presets: a name and a known kind, most recent one per name.
        self.string_presets.retain(|preset| {
            !preset.name.trim().is_empty()
                && preset.name.len() <= MAX_STRING_PRESET_NAME
                && matches!(
                    preset.kind.as_str(),
                    "nylon" | "steel" | "electric" | "bass"
                )
        });
        self.string_presets.truncate(MAX_STRING_PRESETS);
        // Drum part tones: a known part, one per part.
        let mut parts = std::collections::BTreeSet::new();
        self.drum_parts
            .retain(|tone| tone.part().is_some_and(|part| parts.insert(part.index())));
        // Per-preset tones: a program the board has and a known kind, one per program.
        let mut programs = std::collections::BTreeSet::new();
        self.preset_tones.retain(|tone| {
            tone.program < MAX_PROGRAMS
                && kind_from_id(&tone.kind).is_some()
                && programs.insert(tone.program)
        });
        for tone in &mut self.preset_tones {
            if let Some(kind) = kind_from_id(&tone.kind) {
                tone.sound = tone.sound.for_kind(kind);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ThemeChoice;

    #[test]
    fn sounds_saved_before_pickups_get_their_kinds_pickup() {
        use gscore_play::StringKind;
        let mut sounds = StringSounds::default();
        sounds.steel.level = 0.3;
        let text = serde_json::to_string(&sounds).unwrap_or_default();
        // An older settings file: no pickup in any sound.
        let mut value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        for kind in ["nylon", "steel", "electric", "bass"] {
            if let Some(sound) = value.get_mut(kind).and_then(|v| v.as_object_mut()) {
                assert!(sound.remove("pickup").is_some(), "{kind}");
                sound.remove("pickup_position");
            }
        }
        let older: StringSounds = serde_json::from_value(value).unwrap_or_default();
        assert_eq!(older.steel.level, 0.3, "read");
        assert_eq!(older.get(StringKind::Steel).pickup, PickupSetting::None);
        assert_eq!(older.get(StringKind::Bass).pickup, PickupSetting::None);
        let electric = older.get(StringKind::Electric);
        assert_eq!(electric.pickup, PickupSetting::Humbucker);
        assert_eq!(
            electric.model().pickup_position,
            gscore_guitar::pickup_positions::BRIDGE
        );
        // A chosen pickup is kept, also "none" on an electric.
        let mut chosen = sounds.electric;
        chosen.pickup = PickupSetting::None;
        assert_eq!(
            chosen.for_kind(StringKind::Electric).pickup,
            PickupSetting::None
        );
    }

    #[test]
    fn the_reverb_bus_runs_a_reverb_or_a_room_all_wet() {
        use crate::chain_settings::BlockChoice;

        let mut bus = ReverbBus::default();
        assert_eq!(bus.block.choice, BlockChoice::Reverb);
        // Only a reverb, a room or nothing; a cabinet is not a reverb.
        bus.block.choice = BlockChoice::Cabinet;
        assert_eq!(bus.clone().sanitized().block.choice, BlockChoice::Reverb);
        // All wet, whatever the block says, and the level in its range.
        bus.block.choice = BlockChoice::Room;
        bus.block.mix = 0.3;
        bus.level = 200;
        let clean = bus.sanitized();
        assert!((clean.block.mix - 1.0).abs() < 1e-6);
        assert_eq!(clean.level, 127);
        assert_eq!(clean.block.choice, BlockChoice::Room);
    }

    #[test]
    fn preset_tones_round_trip_and_a_broken_one_is_dropped() {
        use gscore_play::StringKind;

        let mut settings = Settings::default();
        // A tone saved before pickups existed: its kind's pickup comes back.
        let mut sound = settings.string_sounds.get(StringKind::Electric);
        sound.pickup = PickupSetting::Unset;
        let tone = |program: u8, kind: &str, sound: StringSound| PresetTone {
            program,
            kind: kind.to_owned(),
            sound,
            chain: crate::chain_settings::ChainSetting::default(),
        };
        settings.preset_tones.push(tone(27, "electric", sound));
        // A broken settings file: a program past the board, an unknown kind, and a second
        // tone for one program (the first one is kept).
        settings.preset_tones.push(tone(200, "electric", sound));
        settings.preset_tones.push(tone(28, "banjo", sound));
        settings.preset_tones.push(tone(27, "bass", sound));
        let text = serde_json::to_string(&settings).unwrap_or_default();
        let mut back: Settings = serde_json::from_str(&text).unwrap_or_default();
        back.sanitize();
        assert_eq!(back.preset_tones.len(), 1, "{:?}", back.preset_tones);
        let kept = back.preset_tones.first().expect("kept");
        assert_eq!((kept.program, kept.kind.as_str()), (27, "electric"));
        let tone = kept.tone().expect("a known kind");
        assert_eq!(tone.kind, StringKind::Electric);
        assert_eq!(tone.sound.pickup, PickupSetting::Humbucker);
        // An older settings file without preset tones reads with none.
        let mut value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        assert!(
            value
                .as_object_mut()
                .and_then(|object| object.remove("preset_tones"))
                .is_some()
        );
        let older: Settings = serde_json::from_value(value).unwrap_or_default();
        assert!(older.preset_tones.is_empty());
    }

    #[test]
    fn recent_files_are_unique_and_bounded() {
        let mut settings = Settings::default();
        for index in 0..MAX_RECENT_FILES.saturating_add(5) {
            settings.add_recent(PathBuf::from(format!("song{index}.gp")));
        }
        assert_eq!(settings.recent_files.len(), MAX_RECENT_FILES);
        assert_eq!(settings.recent_files[0], PathBuf::from("song14.gp"));
        let oldest = settings.recent_files[0].clone();
        settings.add_recent(oldest.clone());
        assert_eq!(settings.recent_files[0], oldest);
        assert_eq!(
            settings
                .recent_files
                .iter()
                .filter(|path| **path == oldest)
                .count(),
            1
        );
    }

    #[test]
    fn a_sample_rate_not_offered_becomes_automatic() {
        let mut settings = Settings {
            sample_rate: Some(1),
            ..Settings::default()
        };
        settings.sanitize();
        assert_eq!(settings.sample_rate, None);
        settings.sample_rate = Some(48_000);
        settings.sanitize();
        assert_eq!(settings.sample_rate, Some(48_000));
    }

    #[test]
    fn view_state_round_trips_through_the_settings() {
        let mut view = ViewState::default();
        view.theme = ThemeChoice::Dark;
        view.set_zoom(1.5);
        view.show_track_grid = false;
        let mut settings = Settings::from_view(&view);
        let mut restored = ViewState::default();
        settings.apply_to_view(&mut restored);
        assert_eq!(restored, view);
        settings.zoom = f32::NAN;
        settings.grid_height_chosen = Some(f32::INFINITY);
        settings.autosave_seconds = u64::MAX;
        settings.sanitize();
        assert_eq!(settings.zoom, 1.0);
        assert_eq!(settings.grid_height_chosen, None);
        assert_eq!(settings.autosave_seconds, DEFAULT_AUTOSAVE_SECONDS);
    }

    #[test]
    fn only_changed_shortcuts_are_stored_and_bad_ones_are_skipped() {
        let mut settings = Settings::default();
        settings.store_shortcuts(&Keymap::default());
        assert!(settings.shortcut_overrides.is_empty(), "nothing changed");

        let mut keymap = Keymap::default();
        let combo = KeyCombo::parse("ctrl+k").expect("a valid combo");
        keymap
            .bind(ActionId::Copy, combo)
            .expect("copy can be bound");
        settings.store_shortcuts(&keymap);
        assert_eq!(
            settings.shortcut_overrides,
            [ShortcutSetting {
                action: "copy".to_owned(),
                keys: vec!["ctrl+k".to_owned()],
            }]
        );
        let mut loaded = Keymap::default();
        settings
            .apply_shortcuts(&mut loaded)
            .expect("valid shortcuts");
        assert_eq!(loaded, keymap);

        // A broken entry is reported; the usable ones still apply.
        settings.shortcut_overrides.push(ShortcutSetting {
            action: "no_such_action".to_owned(),
            keys: vec!["ctrl+j".to_owned()],
        });
        let mut loaded = Keymap::default();
        assert!(settings.apply_shortcuts(&mut loaded).is_err());
        assert_eq!(loaded.keys_for(ActionId::Copy), [combo]);
    }

    #[test]
    fn the_autosave_interval_sets_the_snapshot_interval() {
        let mut settings = Settings::default();
        assert_eq!(
            settings.recovery_options().snapshot_interval,
            Duration::from_secs(DEFAULT_AUTOSAVE_SECONDS)
        );
        settings.autosave_seconds = 0;
        assert!(settings.recovery_options().snapshot_interval.is_zero());
    }

    fn viewport(
        inner: Option<(f32, f32)>,
        outer_at: Option<(f32, f32)>,
        maximized: bool,
        fullscreen: bool,
    ) -> egui::ViewportInfo {
        let size = inner.unwrap_or((0.0, 0.0));
        egui::ViewportInfo {
            inner_rect: inner
                .map(|(w, h)| egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h))),
            outer_rect: outer_at.map(|(x, y)| {
                egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(size.0, size.1))
            }),
            maximized: Some(maximized),
            fullscreen: Some(fullscreen),
            ..egui::ViewportInfo::default()
        }
    }

    #[test]
    fn the_window_mode_is_remembered_without_a_position() {
        // Wayland: a size but no position. The window is still remembered.
        let normal =
            WindowState::of_viewport(&viewport(Some((900.0, 700.0)), None, false, false), None)
                .expect("remembered");
        assert_eq!(normal.position(), None);
        assert_eq!(normal.size(), [900.0, 700.0]);
        // Maximized, then fullscreen: the normal size stays for when the mode is left.
        let maximized = WindowState::of_viewport(
            &viewport(Some((1920.0, 1080.0)), None, true, false),
            Some(normal),
        )
        .expect("remembered");
        assert!(maximized.maximized);
        assert_eq!(maximized.size(), [900.0, 700.0]);
        let fullscreen = WindowState::of_viewport(
            &viewport(Some((1920.0, 1080.0)), None, false, true),
            Some(maximized),
        )
        .expect("remembered");
        assert!(fullscreen.fullscreen && !fullscreen.maximized);
        assert_eq!(fullscreen.size(), [900.0, 700.0]);
        // Where the system tells the position (X11, Windows), it is kept.
        let placed = WindowState::of_viewport(
            &viewport(Some((800.0, 600.0)), Some((40.0, 30.0)), false, false),
            Some(fullscreen),
        )
        .expect("remembered");
        assert_eq!(placed.position(), Some([40.0, 30.0]));
        assert!(!placed.fullscreen);
        // No size yet: nothing to remember.
        assert_eq!(
            WindowState::of_viewport(&viewport(None, None, false, false), None),
            None
        );
    }

    #[test]
    fn window_settings_from_before_fullscreen_still_load() {
        let old = r#"{"window":{"x":10.0,"y":20.0,"width":900.0,"height":700.0,"maximized":true}}"#;
        let settings: Settings = serde_json::from_str(old).expect("deserialize");
        let window = settings.window.expect("window");
        assert_eq!(window.position(), Some([10.0, 20.0]));
        assert!(window.maximized && !window.fullscreen);
    }

    #[test]
    fn a_json_round_trip_keeps_every_field() {
        let mut settings = Settings::default();
        settings.add_recent(PathBuf::from("/tabs/song.gp"));
        settings.keymap_path = Some(PathBuf::from("/keys/keymap.txt"));
        settings.shortcut_overrides = vec![ShortcutSetting {
            action: "copy".to_owned(),
            keys: vec!["ctrl+k".to_owned()],
        }];
        settings.grid_height_chosen = Some(180.0);
        settings.open_files = vec![PathBuf::from("/tabs/a.gp"), PathBuf::from("/tabs/b.gp")];
        settings.active_file = Some(PathBuf::from("/tabs/b.gp"));
        settings.window = Some(WindowState {
            x: Some(10.0),
            y: Some(20.0),
            width: 900.0,
            height: 700.0,
            maximized: false,
            fullscreen: true,
        });
        let text = serde_json::to_string(&settings).expect("serialize");
        let back: Settings = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back, settings);
    }
}
