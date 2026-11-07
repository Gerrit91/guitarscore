//! The application: menus, toolbar, the page with the edit cursor, and the status bar. It
//! holds state and delegates: opening and saving to [`Tasks`], editing to the document's
//! [`gscore_edit::TabEditor`], layout to [`Document`], painting to the canvas.
//!
//! Split by job: `bars` (menus, toolbar, status), `page` (the page view, cursor and
//! clicks), `editing` (keys and actions), `files` (open, save, unsaved changes).

mod about;
mod alert;
mod amp_knobs;
mod bars;
mod beat_text_editor;
mod chord_editor;
mod circle_of_fifths;
mod compressor_bus_window;
mod edit_dynamics;
mod edit_effects;
mod edit_palette;
mod edit_staff;
mod edit_staff_dialogs;
mod editing;
mod files;
mod grace_editor;
mod grid;
mod harmonic_diagram;
mod harmonic_editor;
mod history;
mod icons;
mod instrument;
mod lyrics_editor;
mod metronome;
mod new_song;
mod new_track;
mod page;
mod page_menu;
mod recovery;
mod reverb_bus_window;
mod score_view;
mod settings_page;
mod settings_window;
mod shape_editor;
mod shortcuts;
mod song_state;
mod sound_chain;
mod staff_pickers;
mod staff_widgets;
mod string_sound;
mod style;
mod swatches;
mod tab;
mod tab_bar;
mod tile_faces;
mod tiles;
mod title_bar;
mod toolbar;
mod track_engine;
mod track_panel;
mod tuner;
mod tuplet_dialog;

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use gscore_edit::Keymap;
use gscore_layout::{Fonts, LayoutOptions};
use gscore_session::FoundSession;

use crate::document::Document;
use crate::keys::KeyReader;
use crate::playback::{Output, Playback};
use crate::settings::{Settings, WindowState};
use crate::state::{ThemeChoice, ViewState};
use crate::tasks::Tasks;
use tab::Tab;

pub(crate) use files::Then;

/// The GuitarScore application.
#[derive(Debug)]
pub struct GuitarScoreApp {
    view: ViewState,
    fonts: Result<Fonts, String>,
    options: LayoutOptions,
    /// The open documents, one per tab, and the active tab's index.
    tabs: Vec<Tab>,
    active: usize,
    tasks: Tasks,
    /// A message for the user (an error); shown until dismissed.
    notice: Option<String>,
    /// A short confirmation (saved, ...) in the status bar.
    status: Option<String>,
    /// The theme applied to egui's own widgets, to apply a change once.
    applied_theme: Option<ThemeChoice>,
    /// The interface size applied to egui, to apply a change once.
    applied_interface_zoom: Option<f32>,
    /// The window title last sent, to send a change once.
    applied_title: String,
    playback: Playback,
    keymap: Keymap,
    keys: KeyReader,
    /// A question for the user (unsaved changes, a lossy save), shown as a window.
    dialog: Option<files::Dialog>,
    /// What to do once the save that is running has succeeded.
    after_save: Option<Then>,
    /// Which tab the save that is running belongs to (to mark it saved). Only one save
    /// runs at a time: its result does not say which tab it was for.
    saving_tab: Option<tab::TabId>,
    /// The id number the next tab gets.
    next_tab_id: u64,
    /// A window close was deferred: each dirty tab is asked about in turn, then the window
    /// may close.
    closing_window: bool,
    /// Unsaved changes were dealt with: the window may close.
    may_close: bool,
    /// Where documents' recovery journals go; `None` keeps no journal.
    recovery_dir: Option<PathBuf>,
    /// Whether the recovery directory was searched for crashed documents yet.
    recovery_searched: bool,
    /// Crashed documents still to ask about.
    recoverable: Vec<FoundSession>,
    /// The shortcut editor, while it is open.
    shortcuts: Option<shortcuts::ShortcutPanel>,
    /// The edit history, while it is open.
    history: Option<history::HistoryPanel>,
    /// The new-song dialog, while it is open.
    new_song_dialog: Option<new_song::NewSongDialog>,
    /// Files still to open when the tabs from last time are being restored.
    pending_opens: VecDeque<PathBuf>,
    /// The file that was active when the tabs were saved, to select after restoring them.
    startup_active: Option<PathBuf>,
    /// The settings window, while it is open.
    settings_window: Option<settings_window::SettingsWindow>,
    /// The About window, while it is open.
    about: Option<about::AboutWindow>,
    /// The tuner window, while it is open.
    tuner: Option<tuner::TunerState>,
    /// The Sound chain window, while it is open.
    sound_chain: Option<sound_chain::SoundChainWindow>,
    /// The reverb bus's window, while it is open.
    reverb_bus_window: Option<reverb_bus_window::ReverbBusWindow>,
    /// The mixer's compressor bus window, while it is open.
    compressor_bus_window: Option<compressor_bus_window::CompressorBusWindow>,
    /// What the app remembers between runs.
    settings: Settings,
    /// Where the settings are stored (`None` keeps them only in memory; a browser keeps
    /// them in its own storage instead).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    settings_path: Option<PathBuf>,
    /// Panel sizes, as last drawn, for the next run.
    panel_sizes: PanelSizes,
    /// The window's last position and size.
    last_window: Option<WindowState>,
    /// The browser's SoundFont state last written to the page (`loaded` or `error: …`).
    #[cfg(target_arch = "wasm32")]
    browser_sound: Option<String>,
}

/// A side panel's width: the remembered one (or `default`), in 200..`room`, whatever a
/// stored - or hand-edited - value says.
fn panel_width(stored: Option<f32>, default: f32, room: f32) -> f32 {
    let room = room.max(200.0);
    match stored {
        Some(width) if width.is_finite() => width.clamp(200.0, room),
        _ => default.clamp(200.0, room),
    }
}

/// Panel sizes the app remembers: the song grid's height and the track panel's width.
#[derive(Debug, Clone, Copy, Default)]
struct PanelSizes {
    grid: Option<f32>,
    mixer: Option<f32>,
    track_panel: Option<f32>,
    edit_panel: Option<f32>,
}

impl GuitarScoreApp {
    /// A new app with the built-in keymap; opens `file` in the background if given.
    pub fn new(ctx: &egui::Context, file: Option<PathBuf>) -> Self {
        GuitarScoreApp::with_keymap(ctx, file, Keymap::default())
    }

    /// A new app using `keymap`; opens `file` in the background if given.
    pub fn with_keymap(ctx: &egui::Context, file: Option<PathBuf>, keymap: Keymap) -> Self {
        GuitarScoreApp::with_options(ctx, file, keymap, None)
    }

    /// A new app using `keymap`, journaling edits in `recovery_dir` (and offering, at the
    /// start, to recover what a crash left there); opens `file` in the background if
    /// given.
    pub fn with_options(
        ctx: &egui::Context,
        file: Option<PathBuf>,
        keymap: Keymap,
        recovery_dir: Option<PathBuf>,
    ) -> Self {
        GuitarScoreApp::with_settings(ctx, file, keymap, recovery_dir, Settings::default(), None)
    }

    /// A new app with the user's `settings` (theme, zoom, panels, recent files, autosave),
    /// saved back to `settings_path` on exit; opens `file` in the background if given.
    pub fn with_settings(
        ctx: &egui::Context,
        file: Option<PathBuf>,
        keymap: Keymap,
        recovery_dir: Option<PathBuf>,
        mut settings: Settings,
        settings_path: Option<PathBuf>,
    ) -> Self {
        crate::fonts::install(ctx);
        style::install(ctx);
        // egui's own Ctrl+-/Ctrl+=/Ctrl+0 zoom the whole interface, and part of them reached
        // it past the score's zoom keys. The app handles both: Ctrl zooms the score,
        // Ctrl+Shift the interface (`interface_zoom_keys`).
        ctx.options_mut(|options| options.zoom_with_keyboard = false);
        settings.sanitize();
        // What to open at startup: a file from the command line, or the tabs that were open
        // last time (each skipped if it is no longer there).
        let (startup_files, startup_active): (Vec<PathBuf>, Option<PathBuf>) =
            if let Some(file) = file {
                (vec![file], None)
            } else if settings.reopen_last_file {
                let mut files = settings.open_files.clone();
                // Settings written by an older version only know the last file.
                if files.is_empty() {
                    files.extend(settings.last_file.clone());
                }
                files.retain(|path| path.is_file());
                (files, settings.active_file.clone())
            } else {
                (Vec::new(), None)
            };
        let mut view = ViewState::default();
        settings.apply_to_view(&mut view);
        let last_window = settings.window;
        let panel_sizes = PanelSizes {
            grid: settings.grid_height_chosen,
            mixer: settings.mixer_height_chosen,
            track_panel: settings.track_panel_width,
            edit_panel: settings.edit_panel_width,
        };
        let mut app = GuitarScoreApp {
            view,
            fonts: Fonts::bundled().map_err(|e| e.to_string()),
            options: LayoutOptions::default(),
            tabs: Vec::new(),
            active: 0,
            tasks: Tasks::default(),
            notice: None,
            status: None,
            applied_theme: None,
            applied_interface_zoom: None,
            applied_title: String::new(),
            playback: Playback::default(),
            keymap,
            keys: KeyReader::default(),
            dialog: None,
            after_save: None,
            saving_tab: None,
            next_tab_id: 0,
            closing_window: false,
            may_close: false,
            recovery_dir,
            recovery_searched: false,
            recoverable: Vec::new(),
            shortcuts: None,
            history: None,
            new_song_dialog: None,
            pending_opens: VecDeque::new(),
            startup_active: None,
            settings_window: None,
            about: None,
            tuner: None,
            sound_chain: None,
            reverb_bus_window: None,
            compressor_bus_window: None,
            settings,
            settings_path,
            panel_sizes,
            last_window,
            #[cfg(target_arch = "wasm32")]
            browser_sound: None,
        };
        app.playback
            .store_metronome_volume(app.settings.metronome_volume);
        app.playback.store_click_sound(
            gscore_play::ClickSound::from_id(&app.settings.click_sound).unwrap_or_default(),
        );
        app.playback
            .store_modelled_guitars(app.settings.modelled_guitars);
        app.playback.set_sample_rate(app.settings.sample_rate);
        app.playback.store_string_sounds(app.settings.string_sounds);
        app.playback
            .store_string_chains(app.settings.string_chains.clone());
        app.playback
            .store_preset_tones(app.settings.preset_tones.clone());
        app.playback
            .store_drum_parts(app.settings.drum_parts.clone());
        app.playback
            .store_reverb_bus(app.settings.reverb_bus.clone());
        app.playback
            .store_compressor_bus(app.settings.compressor_bus.clone());
        app.playback.set_count_in_bars(app.settings.count_in_bars);
        app.playback.set_count_in_loops(app.settings.count_in_loops);
        let _ = app.playback.set_speed(app.settings.playback_speed);
        app.playback.set_trainer(crate::playback::SpeedTrainer {
            enabled: app.settings.speed_trainer_enabled,
            every: app.settings.speed_trainer_every,
            step: app.settings.speed_trainer_step,
            target: app.settings.speed_trainer_target,
        });
        app.restore_tabs(startup_files, startup_active, ctx);
        app
    }

    /// The state to keep in a song's notes: each track's sound settings that are not at
    /// their defaults. A track plays its song's tone, else the preset tone of this
    /// machine, else the built-in one; the song gets the tone it actually plays whenever
    /// that is not built-in, so it sounds the same on another machine.
    fn song_state(&self, tab: &Tab) -> Option<song_state::SongState> {
        let song = tab.document.song();
        let tracks: Vec<song_state::TrackState> = (0..song.track_count())
            .filter_map(|index| {
                let mut sound = song_state::TrackSound::of_song(song, index);
                if let Some(player) = self.track_player(index) {
                    // What plays it: SoundFont is the built-in default; anything else
                    // must travel with the song.
                    if sound.engine.is_none()
                        && player.engine != gscore_play::TrackEngine::SoundFont
                    {
                        sound.engine = Some(song_state::SoundEngine::of(player.engine));
                    }
                    // The tone it plays: the song's own, the preset's on this machine, or
                    // the built-in one. Only the last needs no entry.
                    let kind = match player.engine {
                        gscore_play::TrackEngine::Modelled(kind) => Some(kind),
                        gscore_play::TrackEngine::SoundFont | gscore_play::TrackEngine::MidiOut => {
                            player.preset_kind
                        }
                    };
                    if sound.tone.is_none()
                        && let Some(kind) = kind
                    {
                        let tone = self.track_tone(index, kind);
                        if tone != self.playback.default_tone(kind) {
                            sound.tone =
                                Some(crate::settings::PresetTone::of(player.program, &tone));
                        }
                    }
                    if player.engine == gscore_play::TrackEngine::MidiOut
                        && sound.midi_out.is_none()
                    {
                        sound.midi_out = Some(player.midi_out);
                    }
                }
                (!sound.is_default()).then(|| song_state::TrackState {
                    track: index,
                    identity: song_state::TrackIdentity::of(song, index),
                    sound,
                })
            })
            .collect();
        let buses = song_state::Buses::of_song(song);
        song_state::SongState::new(tracks, buses)
    }

    /// The settings as they are now (including the view and window state).
    pub fn settings(&self) -> Settings {
        let mut settings = self.settings.clone();
        settings.update_from_view(&self.view);
        settings.grid_height_chosen = self.panel_sizes.grid;
        settings.mixer_height_chosen = self.panel_sizes.mixer;
        settings.track_panel_width = self.panel_sizes.track_panel;
        settings.edit_panel_width = self.panel_sizes.edit_panel;
        settings.window = self.last_window;
        settings.metronome_volume = self.playback.metronome_volume();
        settings.click_sound = self.playback.click_sound().id().to_owned();
        settings.count_in_bars = self.playback.count_in_bars();
        settings.count_in_loops = self.playback.count_in_loops();
        settings.playback_speed = self.playback.speed();
        let trainer = self.playback.trainer();
        settings.speed_trainer_enabled = trainer.enabled;
        settings.speed_trainer_every = trainer.every;
        settings.speed_trainer_step = trainer.step;
        settings.speed_trainer_target = trainer.target;
        settings.soundfont_path = self.playback.soundfont().map(std::path::Path::to_path_buf);
        settings.prefer_synth = matches!(self.playback.output(), Output::Synth(_));
        settings.modelled_guitars = self.playback.modelled_guitars();
        settings.sample_rate = self.playback.sample_rate();
        settings.string_sounds = self.playback.string_sounds();
        settings.string_chains = self.playback.string_chains().clone();
        settings.preset_tones = self.playback.preset_tones().to_vec();
        settings.drum_parts = self.playback.drum_parts().to_vec();
        settings.midi_port = self.playback.port().map(str::to_owned);
        // Remember every tab backed by a real file, and which one was active. Setting the
        // old single-file key too keeps older versions reopening something sensible.
        settings.open_files = self
            .tabs
            .iter()
            .filter_map(|tab| tab.document.reopen_path().map(std::path::Path::to_path_buf))
            .collect();
        settings.active_file = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.document.reopen_path())
            .map(std::path::Path::to_path_buf);
        settings.last_file = settings
            .active_file
            .clone()
            .or_else(|| settings.open_files.first().cloned());
        settings.store_shortcuts(&self.keymap);
        // A browser cannot reopen a local path: there is nothing to restore at the next
        // start, and recent entries could not be opened either (the user picks the file
        // again).
        #[cfg(target_arch = "wasm32")]
        {
            settings.open_files.clear();
            settings.active_file = None;
            settings.last_file = None;
            settings.recent_files.clear();
            settings.window = None;
            settings.keymap_path = None;
            settings.soundfont_path = None;
        }
        settings
    }

    /// Writes the settings where this platform keeps them (a JSON file on the desktop,
    /// `localStorage` in a browser). Errors go to the notice bar.
    /// Puts the side panels and the song grid back to their usual sizes (the way out of a
    /// layout a stored size made unusable).
    pub(super) fn reset_panel_sizes(&mut self) {
        self.panel_sizes = PanelSizes::default();
        self.save_settings();
    }

    pub fn save_settings(&mut self) {
        let settings = self.settings();
        #[cfg(target_arch = "wasm32")]
        {
            match gscore_session::save_web_json(gscore_session::WebKey::SETTINGS, &settings) {
                Ok(()) => self.settings = settings,
                Err(error) => self.notice = Some(error),
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(path) = self.settings_path.clone() else {
                return;
            };
            if let Err(error) = gscore_session::save_json(&path, &settings) {
                self.notice = Some(error);
            } else {
                self.settings = settings;
            }
        }
    }

    /// Shows `notice` to the user (for example a problem with the keymap file).
    pub fn set_notice(&mut self, notice: impl Into<String>) {
        self.notice = Some(notice.into());
    }

    /// Whether a background job (opening, saving, a file dialog) is running.
    pub fn is_busy(&self) -> bool {
        self.tasks.busy()
    }

    /// The active tab's index, if any.
    pub fn active_tab(&self) -> Option<usize> {
        self.tabs.get(self.active).map(|_| self.active)
    }

    /// The number of open tabs.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// The open document (the active tab's).
    pub fn document(&self) -> Option<&Document> {
        self.tabs.get(self.active).map(|tab| &tab.document)
    }

    /// The document in `index` (for tests and inspection), if it exists.
    pub fn tab_document(&self, index: usize) -> Option<&Document> {
        self.tabs.get(index).map(|tab| &tab.document)
    }

    /// The open document, to change it (its editor and cursor).
    pub fn document_mut(&mut self) -> Option<&mut Document> {
        self.tabs.get_mut(self.active).map(|tab| &mut tab.document)
    }

    /// The shown track of the active tab (0-based).
    pub fn shown_track(&self) -> usize {
        self.tabs.get(self.active).map_or(0, |tab| tab.track)
    }

    /// The tracks the active tab's score shows, top to bottom (the cursor's track among
    /// them); empty without a document.
    pub fn score_tracks(&self) -> Vec<usize> {
        self.tabs
            .get(self.active)
            .map(|tab| tab.shown_tracks())
            .unwrap_or_default()
    }

    /// Sets the active tab's shown track.
    pub fn set_shown_track(&mut self, track: usize) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.track = track;
        }
    }

    /// The selected measure range of the active tab, if any.
    pub fn selection(&self) -> Option<crate::state::MeasureSelection> {
        self.tabs.get(self.active).and_then(|tab| tab.selection)
    }

    /// Sets the active tab's measure selection (the grid and the page share it).
    pub fn set_selection(&mut self, selection: Option<crate::state::MeasureSelection>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.selection = selection;
        }
    }

    /// The current message for the user, if any.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The status bar's last confirmation, if any.
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// Whether a question is waiting for the user's answer.
    pub fn has_dialog(&self) -> bool {
        self.dialog.is_some()
    }

    /// Playback (built-in synthesizer or MIDI output).
    pub fn playback(&self) -> &Playback {
        &self.playback
    }

    /// Playback, to change it.
    pub fn playback_mut(&mut self) -> &mut Playback {
        &mut self.playback
    }

    /// Opens the settings window (also used by the snapshot example).
    pub fn open_settings(&mut self) {
        self.open_settings_window();
    }

    /// The view state (theme, zoom, track).
    pub fn view(&self) -> &ViewState {
        &self.view
    }

    /// The view state, to change it.
    pub fn view_mut(&mut self) -> &mut ViewState {
        &mut self.view
    }

    /// The keymap in use.
    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        // The chrome stays dark; the score's theme only changes how the page is drawn,
        // which reads `view.theme` every frame. A change needs one more frame.
        let interface = self.view.interface_zoom();
        if self.applied_interface_zoom != Some(interface) {
            ctx.set_zoom_factor(interface);
            self.applied_interface_zoom = Some(interface);
        }
        if self.applied_theme != Some(self.view.theme) {
            ctx.request_repaint();
            self.applied_theme = Some(self.view.theme);
        }
    }

    /// Writes the SoundFont's state to the page, so the smoke test can wait for it.
    #[cfg(target_arch = "wasm32")]
    fn mark_browser_sound(&mut self, ctx: &egui::Context) {
        if self.playback.is_loading() {
            // Report the download's progress too, so a stuck one is visible in the page.
            if let Some((done, total)) = self.playback.load_progress()
                && total > 0
                && let Some(window) = web_sys::window()
                && let Some(document) = window.document()
                && let Some(body) = document.body()
            {
                let percent =
                    u16::try_from(done.saturating_mul(100).checked_div(total).unwrap_or(0))
                        .unwrap_or(0)
                        .min(100);
                let _ = body.set_attribute("data-guitarscore-sound-progress", &percent.to_string());
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        // A player that never started (the browser refused audio) has loaded nothing.
        let value = match self.playback.browser_error() {
            Some(error) => format!("error: {error}"),
            None if !self.playback.is_connected() => "error: no audio output".to_owned(),
            None => "loaded".to_owned(),
        };
        if self.browser_sound.as_deref() == Some(value.as_str()) {
            return;
        }
        if let Some(window) = web_sys::window()
            && let Some(document) = window.document()
            && let Some(body) = document.body()
        {
            // A failure to set a debug attribute must not affect the editor.
            let _ = body.set_attribute("data-guitarscore-sound", &value);
            // How long the page took from its script starting to the SoundFont being
            // ready: the build's smoke test reports it (download plus worklet parse).
            if value == "loaded"
                && let Some(started) = js_sys::Reflect::get(
                    window.as_ref(),
                    &wasm_bindgen::JsValue::from_str("__guitarscoreStarted"),
                )
                .ok()
                .and_then(|value| value.as_f64())
                && let Some(performance) = window.performance()
            {
                let millis = (performance.now() - started).max(0.0).round();
                let _ = body.set_attribute("data-guitarscore-sound-ms", &format!("{millis:.0}"));
            }
        }
        self.browser_sound = Some(value);
    }

    /// Saves the settings and closes the tabs (on the desktop, their recovery folders).
    fn finish_exit(&mut self) {
        self.save_settings();
        // A normal exit (unsaved changes were asked about): the recovery folders go.
        for tab in self.tabs.drain(..) {
            tab.document.close(true);
        }
    }

    fn apply_title(&mut self, ctx: &egui::Context) {
        let title = self.document().map_or_else(
            || "GuitarScore".to_owned(),
            |d| {
                let changed = if d.is_dirty() { "• " } else { "" };
                format!("{changed}{} — GuitarScore", d.display_title())
            },
        );
        if title != self.applied_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.applied_title = title;
        }
    }
}

impl eframe::App for GuitarScoreApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Wayland never tells the position; the size and the mode are kept regardless.
        if let Some(window) =
            ctx.input(|input| WindowState::of_viewport(input.viewport(), self.last_window))
        {
            self.last_window = Some(window);
        }
        self.handle_events(&ctx);
        #[cfg(target_arch = "wasm32")]
        self.mark_browser_sound(&ctx);
        // The page asks before it is left while a song has unsaved edits: the browser
        // build has no recovery journal yet.
        #[cfg(target_arch = "wasm32")]
        crate::web::set_unsaved(self.tabs.iter().any(|tab| tab.document.is_dirty()));
        // A file dropped on the window opens like File > Open (asking about unsaved work).
        // A browser drop is read asynchronously; a native one is a path to read.
        #[cfg(target_arch = "wasm32")]
        if let Some(file) = ctx.input(|input| input.raw.dropped_files.first().cloned())
            && let Err(error) = self.tasks.open_dropped(file, &ctx)
        {
            self.notice = Some(error);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .first()
                .map(|file| file.path().to_path_buf())
        }) {
            self.open(path, &ctx);
        }
        self.search_recoverable(&ctx);
        self.handle_close_request(&ctx);
        self.apply_theme(&ctx);
        // Keys first, before any widget sees them (a focused button would take Space).
        self.interface_zoom_keys(&ctx);
        self.handle_keys(&ctx);
        // A count-in that has finished starts the song it was counting in.
        self.poll_count_in();
        // Another tab may hold other choices of what plays its tracks, their MIDI-out
        // ports and channels, other tones and other sends into the compressor bus, and
        // other mixer buses.
        self.sync_track_engines();
        self.sync_midi_routes();
        self.sync_track_tones();
        self.sync_comp_sends();
        self.sync_song_buses();
        // Keep a loop of the selected measures going; a new selection turns it on.
        self.auto_loop();
        self.poll_loop();
        // A beat chosen while the SoundFont loaded plays once it is ready.
        match self.playback.poll_preview() {
            Ok(true) => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
            Ok(false) => {}
            Err(error) => self.notice = Some(error),
        }
        // An entered note stops after a moment.
        if let Some(left) = self.playback.poll_audition() {
            ctx.request_repaint_after(left);
        }
        // The cursor follows the beat being played; the page scrolls with it.
        self.follow_playback();
        if self.playback.state() == gscore_play::PlayState::Playing {
            // A loop checks its end more often; the position moves every frame otherwise.
            let millis = if self.playback.loop_selection() {
                16
            } else {
                50
            };
            ctx.request_repaint_after(std::time::Duration::from_millis(millis));
        }
        self.apply_title(&ctx);

        egui::Panel::top("title_bar")
            .frame(style::chrome_frame(&ctx, 0))
            .exact_size(title_bar::TITLE_BAR_HEIGHT)
            .resizable(false)
            .show(ui, |ui| self.title_bar(ui));
        egui::Panel::top("toolbar")
            .frame(style::chrome_frame(&ctx, 5))
            .show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status")
            .frame(style::chrome_frame(&ctx, 3))
            .show(ui, |ui| self.status_bar(ui));
        if self.view.show_track_grid && self.document().is_some() {
            // Until the user drags it, the grid is just tall enough for its tracks (up to
            // eight) and the master row, and follows when tracks come and go: a panel id per
            // fitting height makes egui take the new default. The mixer view has its own,
            // taller default and its own remembered height.
            let tracks = self.document().map_or(1, |d| d.song().track_count());
            let mixer = self.view.grid_mixer;
            let (fit, min, max, chosen, id) = if mixer {
                let chosen = self.panel_sizes.mixer;
                let id = match chosen {
                    Some(_) => "mixer_chosen".to_owned(),
                    None => "mixer_fit".to_owned(),
                };
                (
                    grid::MIXER_HEIGHT,
                    grid::MIXER_MIN_HEIGHT,
                    grid::MIXER_MAX_HEIGHT,
                    chosen,
                    id,
                )
            } else {
                let fit = grid::opening_height(tracks);
                let chosen = self.panel_sizes.grid;
                let id = match chosen {
                    Some(_) => "track_grid_chosen".to_owned(),
                    None => format!("track_grid_fit_{}", tracks.min(grid::OPEN_ROWS)),
                };
                (
                    fit,
                    grid::height_for(0),
                    grid::height_for(tracks),
                    chosen,
                    id,
                )
            };
            let panel = egui::Panel::bottom(id)
                .resizable(true)
                .min_size(min)
                .max_size(max)
                .default_size(chosen.unwrap_or(fit))
                .frame(style::panel_frame(&ctx))
                .show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| self.track_grid(ui));
                });
            let height = panel.response.rect.height();
            // A height other than the fitting one means the user dragged the edge.
            if chosen.is_some() || (height - fit).abs() > 1.0 {
                if mixer {
                    self.panel_sizes.mixer = Some(height);
                } else {
                    self.panel_sizes.grid = Some(height);
                }
            }
        }
        if self.notice.is_some() {
            egui::Panel::top("notice").show(ui, |ui| self.notice_bar(ui));
        }
        if self.view.show_track_panel && self.document().is_some() {
            // Never wider than leaves the edit palette and the score their room: a stored
            // width (or a broken one) must not cover the window and put the edge out of
            // reach.
            let width = panel_width(
                self.panel_sizes.track_panel,
                280.0,
                (ui.available_width() - 500.0).max(200.0),
            );
            let panel = egui::Panel::right("track_panel")
                .resizable(true)
                .min_size(200.0)
                .max_size(width)
                .default_size(width)
                .frame(style::panel_frame(&ctx))
                .show(ui, |ui| {
                    self.track_panel(ui);
                });
            self.panel_sizes.track_panel = Some(panel.response.rect.width());
        }
        if self.view.show_edit_panel && self.document().is_some() {
            let width = panel_width(
                self.panel_sizes.edit_panel,
                250.0,
                (ui.available_width() - 300.0).max(200.0),
            );
            let panel = egui::Panel::left("edit_palette")
                .resizable(true)
                .min_size(200.0)
                .max_size(width)
                .default_size(width)
                .frame(style::panel_frame(&ctx))
                .show(ui, |ui| {
                    self.edit_palette(ui);
                });
            self.panel_sizes.edit_panel = Some(panel.response.rect.width());
        }
        // The tab strip sits below the toolbar, between the side panels: because it is
        // created after them, it spans only the central column.
        if !self.tabs.is_empty() {
            egui::Panel::top("tabs")
                .frame(style::panel_frame(&ctx))
                .show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(6, 2))
                        .show(ui, |ui| self.tab_bar(ui));
                });
        }
        egui::CentralPanel::default()
            .frame(style::canvas_frame(&ctx))
            .show(ui, |ui| {
                // A bug in drawing must not take the app down: contain it and tell the user.
                let drawn = catch_unwind(AssertUnwindSafe(|| self.page_view(ui)));
                if drawn.is_err() {
                    self.notice = Some(
                        "Drawing the page failed unexpectedly; a crash report was written"
                            .to_owned(),
                    );
                }
            });
        title_bar::resize_edges(&ctx);
        self.show_dialog(&ctx);
        self.show_new_song_dialog(&ctx);
        self.show_rename_window(&ctx);
        self.show_tuning_dialog(&ctx);
        self.show_sound_chain(&ctx);
        self.show_reverb_bus_window(&ctx);
        self.show_compressor_bus_window(&ctx);
        self.show_settings_window(&ctx);
        self.show_shortcuts(&ctx);
        self.show_history(&ctx);
        self.show_new_track_dialog(&ctx);
        self.show_about(&ctx);
        self.show_tuner(&ctx);
        self.show_shape_editors(&ctx);
        self.show_staff_dialogs(&ctx);
        self.show_tuplet_dialog(&ctx);
        // Every edit of this frame goes to the recovery journal (one per open document).
        let mut journal_error = None;
        for tab in &mut self.tabs {
            if let Some(error) = tab.document.record_changes() {
                journal_error.get_or_insert(error);
            }
        }
        if let Some(error) = journal_error {
            self.notice = Some(error);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn on_exit(&mut self) {
        self.finish_exit();
    }

    // The browser renderer hands the glow context to `on_exit`; it is not needed.
    #[cfg(target_arch = "wasm32")]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.finish_exit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn loaded_settings_restore_the_view() {
        let ctx = egui::Context::default();
        let settings = Settings {
            theme: ThemeChoice::Dark,
            zoom: 1.5,
            show_track_panel: false,
            ..Settings::default()
        };
        let app =
            GuitarScoreApp::with_settings(&ctx, None, Keymap::default(), None, settings, None);
        assert_eq!(app.view().theme, ThemeChoice::Dark);
        assert!((app.view().zoom() - 1.5).abs() < 1e-4);
        assert!(!app.view().show_track_panel);
        assert!(app.document().is_none());
    }

    #[test]
    fn the_window_state_is_only_saved_when_it_makes_sense() {
        let ctx = egui::Context::default();
        let settings = Settings {
            window: Some(WindowState {
                x: Some(f32::NAN),
                y: Some(0.0),
                width: -10.0,
                height: 0.0,
                maximized: false,
                fullscreen: false,
            }),
            ..Settings::default()
        };
        let app =
            GuitarScoreApp::with_settings(&ctx, None, Keymap::default(), None, settings, None);
        assert!(app.last_window.is_none());
    }
}
