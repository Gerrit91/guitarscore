//! The settings window: the song's information and page layout, editor settings and audio
//! I/O. Song changes go through commands, so they are undoable; the other settings apply at
//! once and are remembered between runs.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use egui::{Margin, RichText, Ui};
use gscore_edit::Command;
use gscore_model::{MAX_TEMPO, SONG_TEXT_FIELDS, SongInfo, SongTextField};

use super::GuitarScoreApp;
use super::settings_page::{PageDraft, PagePreview, page_tab};
use super::style;
#[cfg(not(target_arch = "wasm32"))]
use crate::playback::Output;
use crate::state::ThemeChoice;

/// The settings window's default size; it opens centered.
const SETTINGS_SIZE: egui::Vec2 = egui::vec2(780.0, 580.0);
/// Smallest size the user may drag it to.
const SETTINGS_MIN_SIZE: egui::Vec2 = egui::vec2(560.0, 400.0);

/// The window's tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum SettingsTab {
    /// Title, artist, notes, tempo, ...
    #[default]
    Song,
    /// Paper, margins, score size, header and footer.
    Page,
    /// Theme, shortcuts, startup, autosave.
    Editor,
    /// The metronome's click and the count-in.
    Metronome,
    /// Output, SoundFont and MIDI ports.
    Audio,
}

/// The open settings window: which tab, and the song fields being edited.
#[derive(Debug)]
pub(super) struct SettingsWindow {
    tab: SettingsTab,
    /// The fields as typed, not yet applied.
    draft: SongInfo,
    /// The document's fields when the draft was last synced, to notice outside edits.
    base: SongInfo,
    /// The page setup being edited.
    page: PageDraft,
    /// Its preview's layout.
    preview: PagePreview,
    /// Where the window is; `None` until it is centered on its first frame.
    pos: Option<egui::Pos2>,
    /// The SoundFonts found on disk, for the Audio I/O tab.
    soundfonts: SoundFontScan,
}

impl SettingsWindow {
    fn new(info: SongInfo, page: gscore_model::PageSetup) -> Self {
        SettingsWindow {
            tab: SettingsTab::default(),
            base: info.clone(),
            draft: info,
            page: PageDraft::new(page),
            preview: PagePreview::default(),
            pos: None,
            soundfonts: SoundFontScan::default(),
        }
    }
}

/// The SoundFonts in the usual folders. Searching reads directories, so it runs on a
/// background thread (never on the UI thread): once when the Audio I/O tab is first shown,
/// and again on "Search again".
#[derive(Debug, Default)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct SoundFontScan {
    /// What the last search found.
    found: Vec<PathBuf>,
    /// The search that is running.
    pending: Option<Receiver<Vec<PathBuf>>>,
    /// Whether a search was started yet.
    started: bool,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl SoundFontScan {
    /// Starts the first search, and takes in a finished one.
    fn poll(&mut self, ctx: &egui::Context) {
        if !self.started {
            self.start(ctx);
        }
        let Some(receiver) = &self.pending else {
            return;
        };
        match receiver.try_recv() {
            Ok(found) => {
                self.found = found;
                self.pending = None;
            }
            Err(TryRecvError::Disconnected) => self.pending = None,
            Err(TryRecvError::Empty) => {}
        }
    }

    /// Searches the folders again in the background.
    fn start(&mut self, ctx: &egui::Context) {
        self.started = true;
        let (sender, receiver) = mpsc::sync_channel(1);
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("gscore-soundfont-search".to_owned())
            .spawn(move || {
                // A panic only means nothing is offered; the file chooser still works.
                let found = catch_unwind(AssertUnwindSafe(gscore_audio::available_soundfonts))
                    .unwrap_or_default();
                let _ = sender.send(found);
                ctx.request_repaint();
            });
        // Without a thread, nothing is listed; the file chooser still works.
        self.pending = spawned.is_ok().then_some(receiver);
    }

    fn searching(&self) -> bool {
        self.pending.is_some()
    }
}

impl GuitarScoreApp {
    /// Opens the settings window (or focuses it), reading the song's current fields.
    pub(super) fn open_settings_window(&mut self) {
        let info = self
            .document()
            .map_or_else(SongInfo::default, |document| document.song().info());
        let page = self
            .document()
            .map_or_else(gscore_model::PageSetup::default, |document| {
                document.song().page_setup()
            });
        self.settings_window = Some(SettingsWindow::new(info, page));
    }

    /// The settings window, if it is open.
    pub(super) fn show_settings_window(&mut self, ctx: &egui::Context) {
        let Some(mut window) = self.settings_window.take() else {
            return;
        };
        if let Some(document) = self.document() {
            let current = document.song().info();
            if current != window.base {
                if window.draft == window.base {
                    window.draft = current.clone();
                }
                window.base = current;
            }
            window.page.sync(document.song().page_setup());
        }
        if window.pos.is_none() {
            window.pos = Some(style::centered_window_pos(ctx, SETTINGS_SIZE));
        }
        let pos = window
            .pos
            .unwrap_or_else(|| style::centered_window_pos(ctx, SETTINGS_SIZE));
        // The preview is laid out only on the Page layout tab, and only for a valid draft.
        let preview = match (self.document(), &self.fonts) {
            (Some(document), Ok(fonts))
                if window.tab == SettingsTab::Page && window.page.draft.check().is_ok() =>
            {
                window.preview.layout(
                    document.song(),
                    document.editor().revision(),
                    self.shown_track(),
                    window.page.draft,
                    &self.options,
                    fonts,
                )
            }
            _ => None,
        };
        let mut close = false;
        let mut apply = false;
        let mut apply_page = false;
        egui::Window::new("Settings")
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size(SETTINGS_SIZE)
            .min_size(SETTINGS_MIN_SIZE)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, "Settings", &mut window.pos) {
                    close = true;
                }
                let escape = style::dialog_body(ui, Margin::symmetric(16, 12), |ui| {
                    ui.horizontal(|ui| {
                        for (tab, label) in [
                            (SettingsTab::Song, "Song"),
                            (SettingsTab::Page, "Page layout"),
                            (SettingsTab::Editor, "Editor"),
                            (SettingsTab::Metronome, "Metronome"),
                            (SettingsTab::Audio, "Audio I/O"),
                        ] {
                            ui.selectable_value(&mut window.tab, tab, label);
                        }
                    });
                    ui.separator();
                    match window.tab {
                        SettingsTab::Song => {
                            song_tab(ui, &mut window, &mut apply, &mut close);
                        }
                        SettingsTab::Page => {
                            let has_song = self.document().is_some();
                            page_tab(
                                ui,
                                &mut window.page,
                                preview.as_deref(),
                                has_song,
                                &mut apply_page,
                                &mut close,
                            );
                        }
                        SettingsTab::Editor => self.editor_tab(ui),
                        SettingsTab::Metronome => self.metronome_tab(ui),
                        SettingsTab::Audio => self.audio_tab(ui, &mut window.soundfonts),
                    }
                });
                close |= escape;
            });
        if close {
            self.settings_window = None;
            self.save_settings();
            return;
        }
        let page_command = self
            .document()
            .and_then(|document| window.page.command(document.song().page_setup()));
        self.settings_window = Some(window);
        if apply {
            self.apply_song_tab();
        }
        if apply_page && let Some(command) = page_command {
            self.apply_command(command, false);
        }
    }

    /// Submits the changed song fields as commands: one undo step for the form.
    fn apply_song_tab(&mut self) {
        let Some(window) = &self.settings_window else {
            return;
        };
        let Some(document) = self.document() else {
            return;
        };
        let current = document.song().info();
        let mut commands: Vec<Command> = Vec::new();
        for field in SONG_TEXT_FIELDS {
            let text = field.get(&window.draft);
            if text != field.get(&current) {
                commands.push(Command::SetSongText {
                    field: *field,
                    text,
                });
            }
        }
        if window.draft.tempo != current.tempo {
            commands.push(Command::SetTempo {
                tempo: window.draft.tempo,
            });
        }
        if commands.is_empty() {
            return;
        }
        // One Apply is one undo step, even when several fields changed.
        if let Some(document) = self.document_mut() {
            document.editor_mut().break_merge();
        }
        for (index, command) in commands.into_iter().enumerate() {
            self.apply_command(command, index != 0);
        }
        // The draft is now what the song says (unless a command failed).
        let info = self.document().map(|document| document.song().info());
        if let (Some(window), Some(info)) = (&mut self.settings_window, info) {
            window.base = info;
        }
    }

    /// The audio I/O tab: the output, a SoundFont for the synthesizer, and MIDI ports.
    #[cfg(not(target_arch = "wasm32"))]
    fn audio_tab(&mut self, ui: &mut Ui, soundfonts: &mut SoundFontScan) {
        soundfonts.poll(ui.ctx());
        ui.heading("Output");
        let current = self.playback.soundfont().map(std::path::Path::to_path_buf);
        let mut available = soundfonts.found.clone();
        if let Some(current) = &current
            && !available.contains(current)
        {
            available.insert(0, current.clone());
        }
        let synth = matches!(self.playback.output(), Output::Synth(_));
        let chosen = current.clone().or_else(|| available.first().cloned());
        if ui
            .add_enabled(
                chosen.is_some(),
                egui::RadioButton::new(synth, "Synthesizer"),
            )
            .clicked()
            && let Some(soundfont) = chosen.clone()
        {
            self.playback.use_soundfont(soundfont);
        }
        if ui.radio(!synth, "MIDI").clicked() {
            self.playback.choose_output(Output::Midi(None));
        }
        if synth {
            ui.horizontal(|ui| {
                ui.label("SoundFont");
                let selected = current
                    .as_deref()
                    .and_then(std::path::Path::file_name)
                    .map_or_else(
                        || "Choose…".to_owned(),
                        |name| name.to_string_lossy().into_owned(),
                    );
                let mut pick = None;
                let response = egui::ComboBox::from_id_salt("settings_soundfont")
                    .selected_text(selected)
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        for path in &available {
                            let name = path.file_name().map_or_else(
                                || path.display().to_string(),
                                |name| name.to_string_lossy().into_owned(),
                            );
                            let is_current = current.as_deref() == Some(path.as_path());
                            if ui
                                .selectable_label(is_current, name)
                                .on_hover_text(path.display().to_string())
                                .clicked()
                            {
                                pick = Some(path.clone());
                                ui.close();
                            }
                        }
                        if soundfonts.searching() {
                            ui.label("Searching…");
                        } else if available.is_empty() {
                            ui.label("No SoundFont files found");
                        }
                    });
                ui.ctx()
                    .accesskit_node_builder(response.response.id, |node| {
                        node.set_label("SoundFont file");
                    });
                if ui.button("Choose SoundFont…").clicked()
                    && let Err(error) = self.tasks.pick_soundfont(ui.ctx())
                {
                    self.notice = Some(error);
                }
                if ui
                    .add_enabled(!soundfonts.searching(), egui::Button::new("Search again"))
                    .on_hover_text("Look for new SoundFont files in the usual folders")
                    .clicked()
                {
                    soundfonts.start(ui.ctx());
                }
                if let Some(path) = pick {
                    self.playback.use_soundfont(path);
                }
            });
            // The full path directly under the dropdown.
            if let Some(soundfont) = &current {
                ui.label(RichText::new(soundfont.display().to_string()).weak());
            } else {
                ui.label(RichText::new("No SoundFont chosen").weak());
            }
        } else {
            if chosen.is_none()
                && ui.button("Choose SoundFont…").clicked()
                && let Err(error) = self.tasks.pick_soundfont(ui.ctx())
            {
                self.notice = Some(error);
            }
            ui.horizontal(|ui| {
                let mut chosen_port = None;
                let response = egui::ComboBox::from_id_salt("settings_midi_port")
                    .selected_text(
                        self.playback
                            .port()
                            .map_or_else(|| "Automatic port".to_owned(), str::to_owned),
                    )
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.playback.port().is_none(), "Automatic port")
                            .clicked()
                        {
                            chosen_port = Some(None);
                        }
                        for port in self.playback.ports() {
                            if ui
                                .selectable_label(
                                    self.playback.port() == Some(port),
                                    format!("MIDI: {port}"),
                                )
                                .clicked()
                            {
                                chosen_port = Some(Some(port.clone()));
                            }
                        }
                    });
                ui.ctx()
                    .accesskit_node_builder(response.response.id, |node| {
                        node.set_label("MIDI port");
                    });
                if let Some(port) = chosen_port {
                    self.playback.choose_port(port);
                }
                if ui.button("Refresh ports").clicked()
                    && let Err(error) = self.playback.refresh_ports()
                {
                    self.notice = Some(error);
                }
            });
        }
        if let Some(error) = self.playback.new_error() {
            self.notice = Some(error);
        }
        self.guitar_sound(ui);
        ui.separator();
        ui.heading("Input");
        ui.label(RichText::new("MIDI inputs (a keyboard or guitar) come later.").weak());
    }

    /// The audio I/O tab in a browser: the synthesizer with the site's SoundFont is the
    /// only output (no MIDI ports, no folders to search).
    #[cfg(target_arch = "wasm32")]
    fn audio_tab(&mut self, ui: &mut Ui, soundfonts: &mut SoundFontScan) {
        let _ = soundfonts;
        ui.heading("Output");
        ui.label("Synthesizer");
        ui.horizontal(|ui| {
            ui.label("SoundFont");
            // The name the site's manifest gives, once the font is loaded.
            let name = self
                .playback
                .soundfont_name()
                .unwrap_or_else(|| crate::BROWSER_SOUNDFONT_NAME.to_owned());
            ui.label(RichText::new(format!("{name} (bundled with this site)")).weak());
        });
        if let Some(error) = self.playback.new_error() {
            self.notice = Some(error);
        }
        self.guitar_sound(ui);
        ui.separator();
        ui.heading("Input");
        ui.label(RichText::new("MIDI inputs (a keyboard or guitar) come later.").weak());
    }

    /// The choice between the SoundFont and the modelled strings for guitars and basses.
    fn guitar_sound(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        let mut modelled = self.playback.modelled_guitars();
        if ui
            .checkbox(&mut modelled, "Modelled guitars and basses (experimental)")
            .changed()
        {
            self.set_modelled_guitars(modelled);
        }
        ui.label(
            RichText::new(
                "The built-in synthesizer plays guitar and bass tracks with physically \
                 modelled strings instead of the SoundFont; drums and other instruments keep \
                 the SoundFont. Also used for audio export. MIDI output is unchanged.",
            )
            .weak(),
        );
    }

    /// The editor tab: theme, keymap file and autosave interval.
    /// The metronome's volume and sound, and the count-in.
    fn metronome_tab(&mut self, ui: &mut Ui) {
        ui.heading("Click");
        ui.horizontal(|ui| {
            ui.label("Volume");
            let mut percent = (self.playback.metronome_volume() * 100.0).round();
            let response = ui.add(egui::Slider::new(&mut percent, 0.0..=100.0).suffix(" %"));
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_label("Click volume"));
            if response.changed() {
                self.set_metronome_volume(percent / 100.0);
            }
        });
        ui.add_space(4.0);
        ui.label("Sound");
        let chosen = self.playback.click_sound();
        for sound in gscore_play::ClickSound::ALL {
            ui.horizontal(|ui| {
                if ui.radio(chosen == sound, sound.label()).clicked() {
                    self.set_click_sound(sound);
                }
                let listen = ui
                    .small_button("Listen")
                    .on_hover_text("Play one bar of this click");
                ui.ctx().accesskit_node_builder(listen.id, |node| {
                    node.set_label(format!("Listen to {}", sound.label()));
                });
                if listen.clicked() {
                    self.set_click_sound(sound);
                    self.preview_click();
                }
            });
        }
        ui.label(
            RichText::new(
                "The built-in synthesizer plays this click; a MIDI output plays its own \
                 metronome sounds. The volume is also on the metronome's fader in the mixer.",
            )
            .weak(),
        );

        ui.separator();
        ui.heading("Count-in");
        let mut count_in = self.playback.count_in();
        if ui
            .checkbox(&mut count_in, "Count in before playback starts")
            .changed()
        {
            self.set_count_in(count_in);
        }
        ui.horizontal(|ui| {
            ui.label("Length");
            let mut bars = self.playback.count_in_bars();
            let response = egui::ComboBox::from_id_salt("settings_count_in_bars")
                .selected_text(bars_label(bars))
                .show_ui(ui, |ui| {
                    for choice in 1..=gscore_play::MAX_COUNT_IN_BARS {
                        ui.selectable_value(&mut bars, choice, bars_label(choice));
                    }
                });
            ui.ctx()
                .accesskit_node_builder(response.response.id, |node| {
                    node.set_label("Count-in length");
                });
            if bars != self.playback.count_in_bars() {
                self.playback.set_count_in_bars(bars);
            }
        });
        let mut loops = self.playback.count_in_loops();
        if ui
            .checkbox(&mut loops, "Count in again each time a loop starts over")
            .changed()
        {
            self.playback.set_count_in_loops(loops);
        }
        ui.label(
            RichText::new(
                "The count-in clicks whether or not the metronome clicks during the song.",
            )
            .weak(),
        );
    }

    fn editor_tab(&mut self, ui: &mut Ui) {
        ui.heading("Score");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.view.theme, ThemeChoice::Light, "Light page");
            ui.radio_value(&mut self.view.theme, ThemeChoice::Dark, "Dark page");
        });
        ui.checkbox(
            &mut self.settings.audition_notes,
            "Play notes as they are entered",
        )
        .on_hover_text(
            "A fret typed into the score sounds briefly with the track's instrument (not \
             while the song plays)",
        );

        ui.separator();
        ui.heading("Shortcuts");
        ui.label(
            RichText::new(
                "Keys are typed characters or key names, so the same bindings work on \
                 QWERTY, QWERTZ and AZERTY. Changed shortcuts are stored with the app \
                 settings.",
            )
            .weak(),
        );
        if ui.button("Edit shortcuts…").clicked() {
            self.open_shortcut_editor();
        }

        ui.separator();
        ui.heading("Startup");
        ui.checkbox(
            &mut self.settings.reopen_last_file,
            "Reopen the tabs from last time at startup",
        );
        ui.label(RichText::new("Files that are no longer there are skipped.").weak());

        ui.separator();
        ui.heading("Autosave");
        ui.horizontal(|ui| {
            ui.label("Recovery snapshot");
            let mut seconds = self.settings.autosave_seconds;
            let response = egui::ComboBox::from_id_salt("settings_autosave")
                .selected_text(autosave_label(seconds))
                .show_ui(ui, |ui| {
                    for choice in [30, 60, 120, 300, 0] {
                        ui.selectable_value(&mut seconds, choice, autosave_label(choice));
                    }
                });
            ui.ctx()
                .accesskit_node_builder(response.response.id, |node| {
                    node.set_label("Recovery snapshot");
                });
            if seconds != self.settings.autosave_seconds {
                self.settings.autosave_seconds = seconds;
            }
        });
        ui.label(
            RichText::new(
                "The journal records every edit; this is how often a full snapshot is added. \
                 Applies to documents opened afterwards.",
            )
            .weak(),
        );

        if ui.button("Save settings now").clicked() {
            self.save_settings();
        }
    }

    /// The keymap editor's window, if it is open.
    pub(super) fn open_shortcut_editor(&mut self) {
        self.open_shortcuts();
    }
}

/// The Song tab: one field per row, the notes below, then Apply/Revert.
fn song_tab(ui: &mut Ui, window: &mut SettingsWindow, apply: &mut bool, close: &mut bool) {
    if window.base == SongInfo::default() && window.draft == SongInfo::default() {
        ui.label(RichText::new("Open a song to edit its information.").weak());
    }
    egui::Grid::new("song_fields")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for field in SONG_TEXT_FIELDS {
                if *field == SongTextField::Notes {
                    continue;
                }
                ui.label(field.label());
                let mut text = field.get(&window.draft);
                let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(280.0));
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label(format!("{} field", field.label()));
                });
                if response.changed() {
                    field.set(&mut window.draft, &text);
                }
                ui.end_row();
            }
            ui.label("Tempo");
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::DragValue::new(&mut window.draft.tempo)
                        .range(1..=MAX_TEMPO)
                        .suffix(" BPM"),
                );
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_label("Tempo"));
            });
            ui.end_row();
        });
    ui.label(SongTextField::Notes.label());
    let mut notes = SongTextField::Notes.get(&window.draft);
    let response = ui.add(
        egui::TextEdit::multiline(&mut notes)
            .desired_rows(4)
            .desired_width(f32::INFINITY),
    );
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_label("Notes field"));
    if response.changed() {
        SongTextField::Notes.set(&mut window.draft, &notes);
    }
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Apply").clicked() {
            *apply = true;
        }
        if ui.button("Revert").clicked() {
            window.draft = window.base.clone();
        }
        if ui.button("Done").clicked() {
            *close = true;
        }
    });
}

/// The autosave choices, as the combo box shows them.
/// "1 bar", "2 bars", ...
fn bars_label(bars: u8) -> String {
    if bars == 1 {
        "1 bar".to_owned()
    } else {
        format!("{bars} bars")
    }
}

fn autosave_label(seconds: u64) -> String {
    match seconds {
        0 => "only after many edits".to_owned(),
        seconds if seconds < 60 => format!("every {seconds} seconds"),
        seconds if seconds % 60 == 0 => format!("every {} minutes", seconds / 60),
        seconds => format!("every {seconds} seconds"),
    }
}
