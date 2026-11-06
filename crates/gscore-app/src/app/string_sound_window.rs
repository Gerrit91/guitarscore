//! The String sound window, for one track: what plays it (chosen in the Sound chain
//! window's first box) and the knobs of that kind of string. Every change is heard at once
//! (also while the song plays) and kept in the settings; "Listen" plays a short phrase.
//! Saved presets (the string sound with its whole effect chain) live in the Sound chain
//! window, which opens this one.

use egui::{Margin, Pos2, RichText, vec2};
use gscore_model::program_name;
use gscore_play::{StringKind, TrackEngine};

use super::GuitarScoreApp;
use crate::app::string_sound::{kind_name, knobs};
use crate::app::style;
use crate::playback::{Heard, STRING_KINDS};

/// Width of the window's content.
const WIDTH: f32 = 780.0;

/// The window while it is open.
#[derive(Debug)]
pub(super) struct StringSoundDialog {
    /// The track it is for.
    track: usize,
    pos: Option<Pos2>,
}

/// What the user did this frame.
#[derive(Default)]
struct Actions {
    close: bool,
    listen: bool,
    /// Another kind of modelled string plays the track.
    engine: Option<TrackEngine>,
    sound: Option<crate::settings::StringSound>,
}

impl GuitarScoreApp {
    /// Opens the String sound window for `track`.
    pub(super) fn open_string_sound(&mut self, track: usize) {
        match &mut self.string_sound {
            Some(dialog) => dialog.track = track,
            None => {
                self.string_sound = Some(StringSoundDialog { track, pos: None });
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
                        if let Some(kind) = kind {
                            // Which strings: the chain window picks the engine, the kind of
                            // string is chosen here, where its knobs are.
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("Strings").color(style::tokens(ui).text_weak),
                                );
                                for candidate in STRING_KINDS {
                                    let selected = candidate == kind;
                                    if ui
                                        .add(egui::Button::selectable(
                                            selected,
                                            kind_name(candidate),
                                        ))
                                        .clicked()
                                        && !selected
                                    {
                                        actions.engine = Some(TrackEngine::Modelled(candidate));
                                    }
                                }
                            });
                            ui.add_space(4.0);
                        }
                        match kind {
                            None => {
                                ui.label(
                                    RichText::new(format!(
                                        "The SoundFont plays this track with preset {}: {}. \
                                         Its first box in the Sound chain window chooses \
                                         modelled strings instead.",
                                        player.program,
                                        program_name(player.program)
                                    ))
                                    .weak(),
                                );
                            }
                            Some(kind) => {
                                let mut sound = self.playback.string_sounds().get(kind);
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
                                 sound of a kind applies to every track played by it; whole \
                                 chains are saved as presets in the Sound chain window.",
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
