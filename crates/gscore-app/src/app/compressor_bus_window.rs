//! The compressor bus's window: what the mixer's second bus runs - a CA-76 compressor, or
//! nothing - with its knobs and its gain-reduction meter. The bus is parallel ("New York")
//! compression: each track's Comp send feeds it, and its fader blends the compressed sum
//! back in under the dry tracks. Opened from the bus strip's "Edit…" button.

use egui::{Margin, Pos2, RichText, vec2};

use super::GuitarScoreApp;
use super::sound_chain::block_knobs;
use super::style;
use crate::chain_settings::{BlockChoice, BlockSetting};

/// Width of the window's content.
const WIDTH: f32 = 620.0;

/// The window while it is open.
#[derive(Debug)]
pub(super) struct CompressorBusWindow {
    pos: Option<Pos2>,
}

impl GuitarScoreApp {
    /// Opens the compressor bus's window (from the mixer's bus strip).
    pub(super) fn open_compressor_bus_window(&mut self) {
        if self.compressor_bus_window.is_none() {
            self.compressor_bus_window = Some(CompressorBusWindow { pos: None });
        }
    }

    /// The compressor bus's window, while it is open.
    pub(super) fn show_compressor_bus_window(&mut self, ctx: &egui::Context) {
        let Some(mut window) = self.compressor_bus_window.take() else {
            return;
        };
        let title = "Compressor bus";
        let size = vec2(WIDTH + 32.0, 420.0);
        let pos = *window
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let bus = self.song_buses().compressor.unwrap_or_default();
        let mut block = bus.block.clone();
        // How far the bus turns its sum down, for the meter (and a repaint while it moves).
        let meters = self.playback.meters();
        let reduction = meters.as_ref().map_or(0.0, |meters| meters.bus_reduction());
        if meters.as_ref().is_some_and(|meters| meters.moving()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        // What the window asked for: another kind of block (the bus is prepared again) or
        // just knobs (sent to the running one).
        let mut rebuilt = false;
        let mut close = false;
        egui::Window::new(title)
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
                                "Parallel (\"New York\") compression: the tracks send into \
                                 this bus with their Comp knob in the mixer, it squashes the \
                                 sum hard, and its fader blends the result in under the dry \
                                 tracks. Its meter shows how far it works.",
                            )
                            .weak(),
                        );
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Compressor").strong());
                            for (choice, label, tip) in [
                                (
                                    BlockChoice::Compressor,
                                    "CA-76",
                                    "A FET compressor after the 1176 family: fast, punchy, a \
                                     little colored",
                                ),
                                (
                                    BlockChoice::Empty,
                                    "None",
                                    "No compressor bus: the tracks' Comp knobs do nothing",
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
                        let moved = block_knobs(ui, &mut block, false, reduction);
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
        let edited = crate::settings::CompressorBus {
            level: bus.level,
            muted: bus.muted,
            block,
        };
        if rebuilt || edited != bus {
            self.set_song_compressor_bus(edited);
        }
        if close {
            self.compressor_bus_window = None;
        } else {
            self.compressor_bus_window = Some(window);
        }
    }
}
