//! The reverb bus's window: what the mixer's bus plays - the computed reverb or a room's
//! impulse response (or nothing) - with its knobs, and the way to load the response. The
//! bus runs all wet: the tracks' Reverb knobs in the mixer are its sends, and its own fader
//! is the return. Opened from the bus strip's "Edit…" button.

use std::path::PathBuf;

use egui::{Margin, Pos2, RichText, vec2};

use super::GuitarScoreApp;
use super::sound_chain::block_knobs;
use super::style;
use crate::chain_settings::{BlockChoice, BlockSetting};

/// Width of the window's content.
const WIDTH: f32 = 620.0;
/// The window's [`egui::Area`] id.
const WINDOW_ID: &str = "gscore_reverb_bus";

/// The window while it is open.
#[derive(Debug)]
pub(super) struct ReverbBusWindow {
    pos: Option<Pos2>,
}

impl GuitarScoreApp {
    /// Opens the reverb bus's window (from the mixer's bus strip).
    pub(super) fn open_reverb_bus_window(&mut self) {
        if self.reverb_bus_window.is_none() {
            self.reverb_bus_window = Some(ReverbBusWindow { pos: None });
        }
    }

    /// The impulse response chosen for the bus: read (an error is shown), then the song's
    /// reverb bus is given the file (an edit, with undo), heard at once.
    pub(super) fn use_bus_impulse(&mut self, path: PathBuf) {
        if let Err(error) = self.playback.impulse(&path) {
            self.notice = Some(error);
            return;
        }
        let mut bus = self.song_buses().reverb.unwrap_or_default();
        bus.block.file = Some(path);
        self.set_song_reverb_bus(bus);
    }

    /// The reverb bus's window, while it is open.
    pub(super) fn show_reverb_bus_window(&mut self, ctx: &egui::Context) {
        let Some(mut window) = self.reverb_bus_window.take() else {
            return;
        };
        let title = "Reverb bus";
        let size = vec2(WIDTH + 32.0, 420.0);
        let pos = *window
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let bus = self.song_buses().reverb.unwrap_or_default();
        let mut block = bus.block.clone();
        // What the window asked for: another kind of reverb or another file (the bus is
        // prepared again), or just knobs (sent to the running one).
        let mut rebuilt = false;
        let mut load = false;
        let mut close = false;
        egui::Window::new(title)
            .id(egui::Id::unique(WINDOW_ID))
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, title, &mut window.pos) {
                    close = true;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        ui.label(
                            RichText::new(
                                "Every track sends into this reverb with its Reverb knob in \
                                 the mixer, whether the MIDI synth or PM synthesis plays it; \
                                 its fader (and mute) decide how much comes back. It runs all \
                                 wet, so the sends alone set how much each track gives it.",
                            )
                            .weak(),
                        );
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Reverb").strong());
                            for (choice, label, tip) in [
                                (
                                    BlockChoice::Reverb,
                                    "Computed",
                                    "The room the program computes: no file needed, its size, \
                                     damping and width are its knobs",
                                ),
                                (
                                    BlockChoice::Room,
                                    "Room (impulse response)",
                                    "A room recorded in a WAV file: a convolution reverb",
                                ),
                                (
                                    BlockChoice::Empty,
                                    "None",
                                    "No reverb at all: the tracks' Reverb knobs do nothing",
                                ),
                            ] {
                                let selected = block.choice == choice;
                                if ui
                                    .add(egui::Button::selectable(selected, label))
                                    .on_hover_text(tip)
                                    .clicked()
                                    && !selected
                                {
                                    let mut new_block = BlockSetting::of(choice);
                                    new_block.file = block.file.clone();
                                    block = new_block;
                                    rebuilt = true;
                                    load = choice == BlockChoice::Room && block.file.is_none();
                                }
                            }
                        });
                        if block.choice.uses_file() {
                            ui.horizontal(|ui| {
                                let name = block.file.as_ref().map_or_else(
                                    || "No impulse response loaded".to_owned(),
                                    |file| {
                                        file.file_name().map_or_else(
                                            || file.display().to_string(),
                                            |name| name.to_string_lossy().into_owned(),
                                        )
                                    },
                                );
                                ui.label(RichText::new(name).weak());
                                if ui.button("Load WAV…").clicked() {
                                    load = true;
                                }
                            });
                        }
                        ui.add_space(4.0);
                        let moved = block_knobs(ui, &mut block, true, 0.0);
                        rebuilt |= moved.rebuild;
                        ui.add_space(6.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(
                                    "Changes are heard at once; another reverb or another \
                                     file is built when it is chosen.",
                                )
                                .weak(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("Close").clicked() {
                                        close = true;
                                    }
                                },
                            );
                        });
                    });
            });
        let edited = crate::settings::ReverbBus {
            level: bus.level,
            muted: bus.muted,
            block,
        };
        if rebuilt || edited != bus {
            self.set_song_reverb_bus(edited);
        }
        if load && let Err(error) = self.tasks.pick_bus_impulse(ctx) {
            self.notice = Some(error);
        }
        // Escape closes the window the user is looking at (a popup or a text field gets it
        // first).
        close |= style::escape_closes(ctx, egui::Id::unique(WINDOW_ID));
        if close {
            self.reverb_bus_window = None;
        } else {
            self.reverb_bus_window = Some(window);
        }
    }
}
