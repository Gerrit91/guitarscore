//! A drum kit's room window: the one short room every part of a kit can send into (and
//! the kit itself), with its knobs. Opened from the kit's Sound chain window. The room is
//! a send: the parts' Room knobs and the kit's own send decide how much reaches it, so it
//! has no mix knob.

use egui::{Margin, Pos2, RichText, vec2};

use super::GuitarScoreApp;
use super::sound_chain::block_knobs;
use super::style;
use crate::chain_settings::{BlockChoice, default_drum_room};

/// Width of the window's content.
const WIDTH: f32 = 460.0;

/// The window while it is open.
#[derive(Debug)]
pub(super) struct KitRoomWindow {
    /// The drum track whose kit the room belongs to.
    pub(super) track: usize,
    pos: Option<Pos2>,
}

impl GuitarScoreApp {
    /// Opens the kit room's window for `track` (from its Sound chain window).
    pub(super) fn open_kit_room(&mut self, track: usize) {
        self.kit_room = Some(KitRoomWindow { track, pos: None });
    }

    /// The kit room's window, while it is open.
    pub(super) fn show_kit_room(&mut self, ctx: &egui::Context) {
        let Some(mut window) = self.kit_room.take() else {
            return;
        };
        let title = format!("Kit room — {}", self.track_name(window.track));
        let mut room = self.drum_room(window.track);
        // A hand-edited song may hold another kind of block; the room is a reverb.
        if room.choice != BlockChoice::Reverb {
            room = default_drum_room();
        }
        let mut changed = false;
        let mut reset = false;
        let mut close = false;
        let size = vec2(WIDTH + 32.0, 300.0);
        let pos = *window
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        egui::Window::new(&title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, &title, &mut window.pos) {
                    close = true;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        ui.label(
                            RichText::new(
                                "One short room behind the whole kit, as a room's overheads \
                                 glue a kit together: every part's Room knob (in the mixer) and \
                                 the kit's own Room send decide how much of them reaches it. \
                                 It is heard through the kit's chain and fader, and the parts' \
                                 chains come before it.",
                            )
                            .weak(),
                        );
                        ui.add_space(6.0);
                        let moved = block_knobs(ui, &mut room, true, 0.0);
                        changed |= moved.changed;
                        ui.add_space(6.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Changes are heard at once and kept with the song.")
                                    .weak(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("Close").clicked() {
                                        close = true;
                                    }
                                    if ui
                                        .button("Default")
                                        .on_hover_text("Back to the short built-in room")
                                        .clicked()
                                    {
                                        reset = true;
                                    }
                                },
                            );
                        });
                    });
            });
        if reset {
            self.set_track_drum_room(window.track, default_drum_room(), false);
        } else {
            room.mix = 1.0;
            if changed {
                self.set_track_drum_room(window.track, room, true);
            }
        }
        if close {
            self.kit_room = None;
        } else {
            self.kit_room = Some(window);
            // Escape closes it, as the other small windows do.
            if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.kit_room = None;
            }
        }
    }
}
