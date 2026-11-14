//! The chorus bus's window: what the mixer's third bus runs - a chorus, or nothing - with
//! its knobs. In General MIDI a track's Chorus value is a send into one shared chorus, so
//! every track's Chorus knob feeds it after the fader; it runs all wet and returns only the
//! chorused signal, widening the dry tracks. Opened from the bus strip's stomp box.

use egui::{Margin, Pos2, RichText, vec2};

use super::GuitarScoreApp;
use super::sound_chain::block_knobs;
use super::style;
use crate::chain_settings::{BlockChoice, BlockSetting};

/// Width of the window's content.
const WIDTH: f32 = 620.0;
/// The window's [`egui::Area`] id.
const WINDOW_ID: &str = "gscore_chorus_bus";

/// The window while it is open.
#[derive(Debug)]
pub(super) struct ChorusBusWindow {
    pos: Option<Pos2>,
}

impl GuitarScoreApp {
    /// Opens the chorus bus's window (from the mixer's bus strip).
    pub(super) fn open_chorus_bus_window(&mut self) {
        if self.chorus_bus_window.is_none() {
            self.chorus_bus_window = Some(ChorusBusWindow { pos: None });
        }
    }

    /// The chorus bus's window, while it is open.
    pub(super) fn show_chorus_bus_window(&mut self, ctx: &egui::Context) {
        let Some(mut window) = self.chorus_bus_window.take() else {
            return;
        };
        let title = "Chorus bus";
        let size = vec2(WIDTH + 32.0, 360.0);
        let pos = *window
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let bus = self.song_buses().chorus.unwrap_or_default();
        let mut block = bus.block.clone();
        // What the window asked for: another kind of block (the bus is prepared again) or
        // just knobs (sent to the running one).
        let mut rebuilt = false;
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
                                "Every track sends into this chorus with its Chorus knob in \
                                 the mixer (Guitar Pro's per-track Chorus), whether the MIDI \
                                 synth, PM synthesis or a drum kit plays it. It runs all wet: \
                                 only the chorused signal comes back, so the dry tracks stay \
                                 as they are in the middle and the bus widens them.",
                            )
                            .weak(),
                        );
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Chorus").strong());
                            for (choice, label, tip) in [
                                (
                                    BlockChoice::Chorus,
                                    "Stereo chorus",
                                    "Two short delays swinging slowly, one side a quarter \
                                     cycle ahead of the other",
                                ),
                                (
                                    BlockChoice::Empty,
                                    "None",
                                    "No chorus bus: the tracks' Chorus knobs do nothing",
                                ),
                            ] {
                                let selected = block.choice == choice;
                                if ui
                                    .add(egui::Button::selectable(selected, label))
                                    .on_hover_text(tip)
                                    .clicked()
                                    && !selected
                                {
                                    block = BlockSetting::of(choice);
                                    rebuilt = true;
                                }
                            }
                        });
                        ui.add_space(4.0);
                        let moved = block_knobs(ui, &mut block, true, 0.0);
                        rebuilt |= moved.rebuild;
                        ui.add_space(6.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(
                                    "Changes are heard at once; another block is built when \
                                     it is chosen.",
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
        let edited = crate::settings::ChorusBus {
            level: bus.level,
            muted: bus.muted,
            block,
        };
        if rebuilt || edited != bus {
            self.set_song_chorus_bus(edited);
        }
        // Escape closes the window the user is looking at (a popup or a text field gets it
        // first).
        close |= style::escape_closes(ctx, egui::Id::unique(WINDOW_ID));
        if close {
            self.chorus_bus_window = None;
        } else {
            self.chorus_bus_window = Some(window);
        }
    }
}
