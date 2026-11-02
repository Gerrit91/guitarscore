//! The settings window: the song's information, audio and MIDI output, and editor settings
//! (theme, keymap file, autosave interval). Song changes go through commands, so they are
//! undoable; the other settings apply at once and are remembered between runs.

use egui::{RichText, Ui};
use gscore_edit::Command;
use gscore_model::{MAX_TEMPO, SONG_TEXT_FIELDS, SongInfo, SongTextField};

use super::GuitarScoreApp;
use crate::playback::Output;
use crate::state::ThemeChoice;

/// The window's tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum SettingsTab {
    /// Title, artist, notes, tempo, ...
    #[default]
    Song,
    /// Output, SoundFont, MIDI ports.
    Audio,
    /// Theme, keymap file, autosave.
    Editor,
}

/// The open settings window: which tab, and the song fields being edited.
#[derive(Debug)]
pub(super) struct SettingsWindow {
    tab: SettingsTab,
    /// The fields as typed, not yet applied.
    draft: SongInfo,
    /// The document's fields when the draft was last synced, to notice outside edits.
    base: SongInfo,
}

impl SettingsWindow {
    fn new(info: SongInfo) -> Self {
        SettingsWindow {
            tab: SettingsTab::default(),
            base: info.clone(),
            draft: info,
        }
    }
}

impl GuitarScoreApp {
    /// Opens the settings window (or focuses it), reading the song's current fields.
    pub(super) fn open_settings_window(&mut self) {
        let info = self
            .document
            .as_ref()
            .map_or_else(SongInfo::default, |document| document.song().info());
        self.settings_window = Some(SettingsWindow::new(info));
    }

    /// The settings window, if it is open.
    pub(super) fn show_settings_window(&mut self, ctx: &egui::Context) {
        let Some(mut window) = self.settings_window.take() else {
            return;
        };
        if let Some(document) = &self.document {
            let current = document.song().info();
            if current != window.base {
                if window.draft == window.base {
                    window.draft = current.clone();
                }
                window.base = current;
            }
        }
        let mut open = true;
        let mut apply = false;
        let mut close = false;
        egui::Window::new("Settings")
            .collapsible(false)
            .resizable(true)
            .default_size([460.0, 420.0])
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (tab, label) in [
                        (SettingsTab::Song, "Song"),
                        (SettingsTab::Audio, "Audio and MIDI"),
                        (SettingsTab::Editor, "Editor"),
                    ] {
                        ui.selectable_value(&mut window.tab, tab, label);
                    }
                });
                ui.separator();
                match window.tab {
                    SettingsTab::Song => {
                        song_tab(ui, &mut window, &mut apply, &mut close);
                    }
                    SettingsTab::Audio => self.audio_tab(ui),
                    SettingsTab::Editor => self.editor_tab(ui),
                }
            });
        if !open || close {
            self.settings_window = None;
            self.save_settings();
            return;
        }
        self.settings_window = Some(window);
        if apply {
            self.apply_song_tab();
        }
    }

    /// Submits the changed song fields as commands: one undo step for the form.
    fn apply_song_tab(&mut self) {
        let Some(window) = &self.settings_window else {
            return;
        };
        let Some(document) = &self.document else {
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
        if let Some(document) = &mut self.document {
            document.editor_mut().break_merge();
        }
        for (index, command) in commands.into_iter().enumerate() {
            self.apply_command(command, index != 0);
        }
        // The draft is now what the song says (unless a command failed).
        if let (Some(window), Some(document)) = (&mut self.settings_window, &self.document) {
            window.base = document.song().info();
        }
    }

    /// The audio and MIDI tab.
    fn audio_tab(&mut self, ui: &mut Ui) {
        ui.heading("Output");
        let soundfont = self.playback.soundfont().map(std::path::Path::to_path_buf);
        if let Some(soundfont) = &soundfont {
            let name = soundfont
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let synth = Output::Synth(soundfont.clone());
            if ui
                .radio(
                    *self.playback.output() == synth,
                    format!("Synthesizer ({name})"),
                )
                .on_hover_text(soundfont.display().to_string())
                .clicked()
            {
                self.playback.choose_output(synth);
            }
        }
        let midi = Output::Midi(None);
        if ui.radio(*self.playback.output() == midi, "MIDI").clicked() {
            self.playback.choose_output(midi);
        }
        ui.horizontal(|ui| {
            let mut chosen = None;
            egui::ComboBox::from_id_salt("settings_midi_port")
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
                        chosen = Some(None);
                    }
                    for port in self.playback.ports() {
                        if ui
                            .selectable_label(
                                self.playback.port() == Some(port),
                                format!("MIDI: {port}"),
                            )
                            .clicked()
                        {
                            chosen = Some(Some(port.clone()));
                        }
                    }
                });
            if let Some(port) = chosen {
                self.playback.choose_port(port);
            }
            if ui.button("Refresh ports").clicked()
                && let Err(error) = self.playback.refresh_ports()
            {
                self.notice = Some(error);
            }
        });
        if let Some(soundfont) = &soundfont {
            ui.label(RichText::new(format!("SoundFont: {}", soundfont.display())).weak());
        } else {
            ui.label(RichText::new("No SoundFont found or chosen").weak());
        }
        if ui.button("Choose SoundFont…").clicked()
            && let Err(error) = self.tasks.pick_soundfont(ui.ctx())
        {
            self.notice = Some(error);
        }
        if let Some(error) = self.playback.new_error() {
            self.notice = Some(error);
        }
        ui.separator();
        ui.label(
            RichText::new(
                "Inputs (a MIDI keyboard or guitar) and a settings file picker come later.",
            )
            .weak(),
        );
    }

    /// The editor tab: theme, keymap file and autosave interval.
    fn editor_tab(&mut self, ui: &mut Ui) {
        ui.heading("Theme");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.view.theme, ThemeChoice::Light, "Light");
            ui.radio_value(&mut self.view.theme, ThemeChoice::Dark, "Dark");
        });

        ui.separator();
        ui.heading("Keymap");
        let path = self.keymap_path();
        ui.label(
            RichText::new(
                path.as_ref()
                    .map_or_else(|| "Built-in keymap".to_owned(), |p| p.display().to_string()),
            )
            .weak(),
        );
        ui.horizontal(|ui| {
            if ui.button("Choose keymap…").clicked()
                && let Err(error) = self.tasks.pick_keymap(ui.ctx())
            {
                self.notice = Some(error);
            }
            if ui
                .add_enabled(path.is_some(), egui::Button::new("Reload"))
                .clicked()
                && let Some(path) = path
            {
                self.load_keymap_from(path);
            }
        });

        ui.separator();
        ui.heading("Autosave");
        ui.horizontal(|ui| {
            ui.label("Recovery snapshot");
            let mut seconds = self.settings.autosave_seconds;
            egui::ComboBox::from_id_salt("settings_autosave")
                .selected_text(autosave_label(seconds))
                .show_ui(ui, |ui| {
                    for choice in [30, 60, 120, 300, 0] {
                        ui.selectable_value(&mut seconds, choice, autosave_label(choice));
                    }
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

    /// The keymap file in use: the user's choice, else the default config path.
    fn keymap_path(&self) -> Option<std::path::PathBuf> {
        self.settings.keymap_path.clone().or_else(|| {
            gscore_session::default_config_dir().map(|dir| dir.join(gscore_session::KEYMAP_FILE))
        })
    }

    /// Loads and uses the keymap at `path`, reporting a problem and remembering the file.
    pub(super) fn load_keymap_from(&mut self, path: std::path::PathBuf) {
        let loaded = gscore_session::load_keymap(&path);
        self.keymap = loaded.keymap;
        self.settings.keymap_path = Some(path);
        if let Some(problem) = loaded.problem {
            self.notice = Some(problem);
        } else {
            self.status = Some("Keymap loaded".to_owned());
        }
        self.save_settings();
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
fn autosave_label(seconds: u64) -> String {
    match seconds {
        0 => "only after many edits".to_owned(),
        seconds if seconds < 60 => format!("every {seconds} seconds"),
        seconds if seconds % 60 == 0 => format!("every {} minutes", seconds / 60),
        seconds => format!("every {seconds} seconds"),
    }
}
