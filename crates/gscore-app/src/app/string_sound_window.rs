//! The String sound window, for one track: what plays it (the SoundFont, or modelled
//! strings of a kind: by default what its preset stands for, but any kind can be chosen),
//! and the knobs of that kind of string, with presets saved under a name. Every change is
//! heard at once (also while the song plays) and kept in the settings; "Listen" plays a
//! short phrase. Opened from the Sound chain window's first box.

use egui::{Margin, Pos2, RichText, vec2};
use gscore_model::program_name;
use gscore_play::{StringKind, TrackEngine};

use super::GuitarScoreApp;
use crate::app::string_sound::{kind_name, knobs};
use crate::app::style;
use crate::playback::{Heard, STRING_KINDS};
use crate::settings::{
    MAX_STRING_PRESET_NAME, MAX_STRING_PRESETS, NamedStringSound, StringSound, kind_id,
};

/// Width of the window's content.
const WIDTH: f32 = 780.0;

/// The window while it is open.
#[derive(Debug)]
pub(super) struct StringSoundDialog {
    /// The track it is for.
    track: usize,
    pos: Option<Pos2>,
    /// The name being typed for a new preset, while saving.
    saving: Option<String>,
}

/// What the user did this frame.
#[derive(Default)]
struct Actions {
    close: bool,
    listen: bool,
    engine: Option<TrackEngine>,
    sound: Option<StringSound>,
    save: Option<String>,
    delete: Option<String>,
}

impl GuitarScoreApp {
    /// Opens the String sound window for `track`.
    pub(super) fn open_string_sound(&mut self, track: usize) {
        match &mut self.string_sound {
            Some(dialog) => dialog.track = track,
            None => {
                self.string_sound = Some(StringSoundDialog {
                    track,
                    pos: None,
                    saving: None,
                });
            }
        }
    }

    /// The String sound window, while it is open.
    pub(super) fn show_string_sound(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.string_sound.take() else {
            return;
        };
        let Some(player) = self.track_player(dialog.track) else {
            // The track (or the document) is gone.
            return;
        };
        let title = "String sound";
        let size = vec2(WIDTH + 32.0, 560.0);
        let pos = *dialog
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut actions = Actions::default();
        let kind = match player.engine {
            TrackEngine::Modelled(kind) => Some(kind),
            TrackEngine::SoundFont => None,
        };
        let presets: Vec<NamedStringSound> = kind.map_or_else(Vec::new, |kind| {
            self.settings
                .string_presets
                .iter()
                .filter(|preset| preset.kind == kind_id(kind))
                .cloned()
                .collect()
        });
        egui::Window::new(title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, title, &mut dialog.pos) {
                    actions.close = true;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        played_by(ui, &player, &mut actions);
                        ui.add_space(6.0);
                        match kind {
                            None => {
                                ui.label(
                                    RichText::new(format!(
                                        "The SoundFont plays this track with preset {}: {}. \
                                         Choose a kind of string above to play it with \
                                         modelled strings.",
                                        player.program,
                                        program_name(player.program)
                                    ))
                                    .weak(),
                                );
                            }
                            Some(kind) => {
                                let mut sound = self.playback.string_sounds().get(kind);
                                preset_row(ui, kind, &presets, &mut dialog, &mut actions);
                                ui.add_space(4.0);
                                if knobs(ui, &mut sound) {
                                    actions.sound = Some(sound);
                                }
                            }
                        }
                        ui.add_space(6.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            if kind.is_some()
                                && ui
                                    .button("Listen")
                                    .on_hover_text("Play a short phrase on these strings")
                                    .clicked()
                            {
                                actions.listen = true;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("Close").clicked() {
                                        actions.close = true;
                                    }
                                },
                            );
                        });
                        ui.label(
                            RichText::new(
                                "Changes are heard at once, also while the song plays. The \
                                 sound of a kind applies to every track played by it.",
                            )
                            .weak(),
                        );
                    });
            });
        self.apply_string_sound_actions(dialog, kind, actions);
    }

    /// Carries out what was done in the window; it stays open unless closed.
    fn apply_string_sound_actions(
        &mut self,
        dialog: StringSoundDialog,
        kind: Option<StringKind>,
        actions: Actions,
    ) {
        if let Some(engine) = actions.engine {
            self.set_track_engine(dialog.track, engine);
        }
        if let (Some(kind), Some(sound)) = (kind, actions.sound) {
            self.playback.set_string_sound(kind, sound);
        }
        if let (Some(kind), Some(name)) = (kind, actions.save) {
            let sound = self.playback.string_sounds().get(kind);
            let presets = &mut self.settings.string_presets;
            presets.retain(|preset| !(preset.kind == kind_id(kind) && preset.name == name));
            if presets.len() < MAX_STRING_PRESETS {
                presets.push(NamedStringSound {
                    name,
                    kind: kind_id(kind).to_owned(),
                    sound,
                });
            }
            self.save_settings();
        }
        if let (Some(kind), Some(name)) = (kind, actions.delete) {
            self.settings
                .string_presets
                .retain(|preset| !(preset.kind == kind_id(kind) && preset.name == name));
            self.save_settings();
        }
        if let Some(kind) = kind.filter(|_| actions.listen) {
            match self.playback.play_string_demo(kind) {
                Ok(Heard::Skipped(why)) => self.notice = Some(format!("Not played: {why}")),
                Ok(_) => {}
                Err(error) => self.notice = Some(error),
            }
        }
        if actions.close {
            // What was dialed in is written now, not only when the program ends.
            self.save_settings();
        } else {
            self.string_sound = Some(dialog);
        }
    }
}

/// The row that chooses what plays the track: the SoundFont or a kind of string.
fn played_by(
    ui: &mut egui::Ui,
    player: &crate::app::track_engine::TrackPlayer,
    actions: &mut Actions,
) {
    ui.label(RichText::new("Played by").strong());
    ui.horizontal_wrapped(|ui| {
        let mut choice = |ui: &mut egui::Ui, engine: TrackEngine, label: &str, enabled: bool| {
            let mut text = label.to_owned();
            if engine == player.default {
                text.push_str(" (preset)");
            }
            let response = ui
                .add_enabled(
                    enabled,
                    egui::Button::selectable(player.engine == engine, text),
                )
                .on_hover_text(match engine {
                    TrackEngine::SoundFont => "The SoundFont plays the track (General MIDI)",
                    TrackEngine::Modelled(_) => "Modelled strings of this kind play the track",
                });
            if response.clicked() {
                actions.engine = Some(engine);
            }
        };
        choice(ui, TrackEngine::SoundFont, "SoundFont", true);
        for kind in STRING_KINDS {
            choice(
                ui,
                TrackEngine::Modelled(kind),
                kind_name(kind),
                player.strings,
            );
        }
    });
    if !player.strings {
        ui.label(
            RichText::new("Only string instruments can be played by modelled strings.").weak(),
        );
    }
}

/// The presets of `kind`: the built-in sound, the saved ones, saving and deleting.
fn preset_row(
    ui: &mut egui::Ui,
    kind: StringKind,
    presets: &[NamedStringSound],
    dialog: &mut StringSoundDialog,
    actions: &mut Actions,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Preset").color(style::tokens(ui).text_weak));
        let response = egui::ComboBox::from_id_salt("string_sound_preset")
            .selected_text("Choose…")
            .show_ui(ui, |ui| {
                if ui.selectable_label(false, "Built-in").clicked() {
                    actions.sound =
                        Some(StringSound::from(gscore_audio::default_string_model(kind)));
                }
                for preset in presets {
                    if ui.selectable_label(false, &preset.name).clicked() {
                        actions.sound = Some(preset.sound);
                    }
                }
            });
        ui.ctx()
            .accesskit_node_builder(response.response.id, |node| node.set_label("Preset"));
        match &mut dialog.saving {
            None => {
                if ui.button("Save as…").clicked() {
                    dialog.saving = Some(String::new());
                }
                if !presets.is_empty() {
                    ui.menu_button("Delete…", |ui| {
                        for preset in presets {
                            if ui.button(&preset.name).clicked() {
                                actions.delete = Some(preset.name.clone());
                                ui.close();
                            }
                        }
                    });
                }
            }
            Some(name) => {
                let field = ui.add(
                    egui::TextEdit::singleline(name)
                        .char_limit(MAX_STRING_PRESET_NAME)
                        .hint_text("Name")
                        .desired_width(140.0),
                );
                ui.ctx()
                    .accesskit_node_builder(field.id, |node| node.set_label("Preset name"));
                let name_ok = !name.trim().is_empty();
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.add_enabled(name_ok, egui::Button::new("Save")).clicked()
                    || (enter && name_ok)
                {
                    actions.save = Some(name.trim().to_owned());
                    dialog.saving = None;
                } else if ui.button("Cancel").clicked() {
                    dialog.saving = None;
                }
            }
        }
    });
}
