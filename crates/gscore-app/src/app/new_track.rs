//! The "New track" popup: asks for a name, an instrument symbol and a color before the
//! track exists, and creates all of it as one undo step.

use egui::{Margin, RichText, Sense, vec2};
use gscore_edit::Command;
use gscore_model::{TrackColor, TrackIndex};

use super::GuitarScoreApp;
use super::instrument::Instrument;
use super::style;
use super::swatches;

/// The popup's default size.
const DIALOG_SIZE: egui::Vec2 = vec2(450.0, 330.0);

/// The instruments that can be chosen.
const SYMBOLS: [Instrument; 6] = [
    Instrument::Guitar,
    Instrument::Bass,
    Instrument::Drums,
    Instrument::Keyboard,
    Instrument::Voice,
    Instrument::Other,
];

/// The open new-track dialog.
#[derive(Debug)]
pub(super) struct NewTrackDialog {
    /// Where the track is inserted.
    pub(super) at: usize,
    name: String,
    color: TrackColor,
    instrument: Instrument,
    /// Whether the name field has been focused yet.
    focused: bool,
    /// Where the popup is; `None` until it is centered on its first frame.
    pos: Option<egui::Pos2>,
}

impl GuitarScoreApp {
    /// Opens the new-track popup for a track appended at the end (also used by the
    /// snapshot example).
    pub fn open_new_track(&mut self) {
        let at = self
            .tabs
            .get(self.active)
            .map_or(0, |tab| tab.document.song().track_count());
        self.begin_new_track(at);
    }

    /// Opens the popup for a track inserted before `at`.
    pub(super) fn begin_new_track(&mut self, at: usize) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let song = tab.document.song();
        let count = song.track_count();
        let at = at.min(count);
        let template = at
            .checked_sub(1)
            .filter(|index| *index < count)
            .or_else(|| (count > 0).then_some(at.min(count.saturating_sub(1))));
        let instrument = template
            .and_then(|index| {
                let track = TrackIndex::new(index);
                song.track(track)
                    .map(|view| Instrument::of(&view, song.track_sound(track)))
            })
            .unwrap_or(Instrument::Other);
        let mut number = 1_usize;
        let name = loop {
            let candidate = format!("Track {number}");
            if !song.tracks().any(|track| track.name() == candidate) {
                break candidate;
            }
            number = number.saturating_add(1);
        };
        let color = TrackColor::DEFAULT;
        tab.new_track = Some(NewTrackDialog {
            at,
            name,
            color,
            instrument,
            focused: false,
            pos: None,
        });
    }

    /// The new-track popup, if it is open.
    pub(super) fn show_new_track_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self
            .tabs
            .get_mut(self.active)
            .and_then(|tab| tab.new_track.take())
        else {
            return;
        };
        if dialog.pos.is_none() {
            dialog.pos = Some(style::centered_window_pos(ctx, DIALOG_SIZE));
        }
        let pos = dialog
            .pos
            .unwrap_or_else(|| style::centered_window_pos(ctx, DIALOG_SIZE));
        let mut close = false;
        let mut create = false;
        egui::Window::new("New track")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .default_size(DIALOG_SIZE)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, "New track", &mut dialog.pos) {
                    close = true;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.label(RichText::new("Name").weak());
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut dialog.name)
                                .hint_text("Track name")
                                .desired_width(f32::INFINITY),
                        );
                        ui.ctx().accesskit_node_builder(response.id, |node| {
                            node.set_label("Track name")
                        });
                        if !dialog.focused {
                            response.request_focus();
                            dialog.focused = true;
                        }
                        if response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        {
                            create = !dialog.name.trim().is_empty();
                        }
                        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                            close = true;
                        }

                        ui.add_space(8.0);
                        ui.label(RichText::new("Symbol").weak());
                        ui.horizontal_wrapped(|ui| {
                            for instrument in SYMBOLS {
                                let (rect, response) =
                                    ui.allocate_exact_size(vec2(44.0, 34.0), Sense::click());
                                ui.ctx().accesskit_node_builder(response.id, |node| {
                                    node.set_label(instrument.name());
                                    node.set_role(egui::Role::Button);
                                });
                                let chosen = dialog.instrument == instrument;
                                if chosen || response.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        4.0,
                                        if chosen {
                                            ui.visuals().selection.bg_fill.gamma_multiply(0.35)
                                        } else {
                                            ui.visuals().widgets.hovered.weak_bg_fill
                                        },
                                    );
                                }
                                instrument.paint(
                                    ui.painter(),
                                    rect.shrink2(vec2(12.0, 8.0)),
                                    ui.visuals().text_color(),
                                );
                                if response.on_hover_text(instrument.name()).clicked() {
                                    dialog.instrument = instrument;
                                }
                            }
                        });

                        ui.add_space(8.0);
                        ui.label(RichText::new("Color").weak());
                        if let Some(preset) = swatches::presets(ui, dialog.color) {
                            dialog.color = preset;
                        }
                        // Any other color.
                        let (red, green, blue) = dialog.color.parts();
                        let mut rgb = [red, green, blue];
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Custom").weak());
                            let response = ui.color_edit_button_srgb(&mut rgb);
                            ui.ctx().accesskit_node_builder(response.id, |node| {
                                node.set_label("Custom track color");
                            });
                            if response.changed() {
                                let [red, green, blue] = rgb;
                                dialog.color = TrackColor::from_parts(red, green, blue);
                            }
                        });

                        ui.add_space(10.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    !dialog.name.trim().is_empty(),
                                    egui::Button::new("Create track"),
                                )
                                .clicked()
                            {
                                create = true;
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    });
            });
        if create {
            self.create_track(&dialog);
            return;
        }
        if close {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.new_track = None;
            }
            return;
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.new_track = Some(dialog);
        }
    }

    /// Inserts the track from `dialog` with its name, color and symbol, as one undo step.
    fn create_track(&mut self, dialog: &NewTrackDialog) {
        let name = dialog.name.trim();
        if name.is_empty() {
            return;
        }
        let track = TrackIndex::new(dialog.at);
        let mut commands = vec![
            Command::InsertTrack { at: track },
            Command::RenameTrack {
                track,
                name: name.to_owned(),
            },
            Command::SetTrackColor {
                track,
                color: dialog.color,
            },
        ];
        if dialog.instrument == Instrument::Drums {
            commands.push(Command::SetTrackDrums { track, drums: true });
        } else {
            commands.push(Command::SetTrackDrums {
                track,
                drums: false,
            });
            if let Some(program) = program_for(dialog.instrument) {
                commands.push(Command::SetTrackProgram { track, program });
            }
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.new_track = None;
        }
        let revision = self
            .tabs
            .get(self.active)
            .map(|tab| tab.document.editor().revision());
        self.apply_command(Command::Batch { commands }, false);
        if self
            .tabs
            .get(self.active)
            .map(|tab| tab.document.editor().revision())
            != revision
            && let Some(tab) = self.tabs.get_mut(self.active)
        {
            tab.track = dialog.at;
        }
    }
}

/// The General MIDI program that shows `instrument`'s symbol.
fn program_for(instrument: Instrument) -> Option<u8> {
    Some(match instrument {
        Instrument::Guitar => 25,
        Instrument::Bass => 33,
        Instrument::Drums => return None,
        Instrument::Keyboard => 0,
        Instrument::Voice => 52,
        // A violin program, outside the guitar/bass/keyboard/voice families.
        Instrument::Other => 40,
    })
}
