//! The master's sound chain: the effects after the sum of the tracks and buses and before
//! the master fader, laid out like the Sound chain window's pedal board. It is kept with
//! the song (or follows this machine's empty default) and heard at once. Opened from the
//! master strip's stomp box.

use egui::{Margin, Pos2, RichText, vec2};

use super::GuitarScoreApp;
use super::sound_chain::{MASTER_BOARD, SoundClip, board, editor, move_block};
use super::style;
use crate::chain_settings::BlockSetting;

/// Width of the window's content (the same as the Sound chain window, whose board it
/// shows).
const WIDTH: f32 = 700.0;

/// The window while it is open.
#[derive(Debug)]
pub(super) struct MasterChainWindow {
    /// The selected box: 0 the master, 1 to 5 the places after it.
    selected: usize,
    pos: Option<Pos2>,
}

impl GuitarScoreApp {
    /// Opens the master's sound chain window (from the master strip).
    pub(super) fn open_master_chain_window(&mut self) {
        if self.master_chain_window.is_none() {
            self.master_chain_window = Some(MasterChainWindow {
                selected: 0,
                pos: None,
            });
        }
    }

    /// The impulse response chosen for place `place` of the master's chain: read (an error
    /// is shown) and heard at once.
    pub(super) fn use_master_impulse(&mut self, place: usize, path: std::path::PathBuf) {
        if let Err(error) = self.playback.impulse(&path) {
            self.notice = Some(error);
            return;
        }
        let mut chain = self.song_master_chain();
        if let Some(block) = chain.blocks.get_mut(place) {
            block.file = Some(path);
        }
        self.set_song_master_chain(chain);
    }

    /// The master's sound chain window, while it is open.
    pub(super) fn show_master_chain_window(&mut self, ctx: &egui::Context) {
        let Some(mut window) = self.master_chain_window.take() else {
            return;
        };
        let title = "Master chain";
        let size = vec2(WIDTH + 32.0, 320.0);
        let pos = *window
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut chain = self.song_master_chain();
        let mut changed = false;
        let mut load = None;
        let mut close = false;
        let clipboard = self.sound_clipboard.clone();
        let mut copied: Option<SoundClip> = None;
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
                                "The effects on the whole mix: after every track and bus and \
                                 before the master fader. The metronome's click is added \
                                 after it, so a compressor or limiter here never pumps with \
                                 the click. This chain is kept with the song.",
                            )
                            .weak(),
                        );
                        ui.add_space(6.0);
                        let shown = board(
                            ui,
                            ("Master", "After the tracks and buses"),
                            &chain,
                            window.selected,
                            MASTER_BOARD,
                            clipboard.as_ref(),
                        );
                        if let Some(index) = shown.select {
                            window.selected = index;
                        }
                        if let Some(clip) = shown.copy {
                            copied = Some(clip);
                        }
                        if let Some(place) = shown.paste_block
                            && let Some(SoundClip::Block(block)) = &clipboard
                            && let Some(slot) = chain.blocks.get_mut(place)
                        {
                            *slot = block.clone();
                            changed = true;
                            window.selected = place.saturating_add(1);
                        }
                        if shown.paste_chain
                            && let Some(SoundClip::Chain(copied_chain)) = &clipboard
                        {
                            chain = copied_chain.as_ref().clone();
                            changed = true;
                        }
                        if let Some((place, block)) = shown.dropped
                            && let Some(slot) = chain.blocks.get_mut(place)
                        {
                            *slot = block;
                            changed = true;
                            window.selected = place.saturating_add(1);
                        }
                        if let Some(place) = shown.remove
                            && let Some(slot) = chain.blocks.get_mut(place)
                        {
                            *slot = BlockSetting::default();
                            changed = true;
                            window.selected = place.saturating_add(1);
                        }
                        if let Some((place, choice)) = shown.fill {
                            if let Some(slot) = chain.blocks.get_mut(place) {
                                *slot = BlockSetting::of(choice);
                                changed = true;
                            }
                            window.selected = place.saturating_add(1);
                            // A room needs its file; a cab starts on its built-in one.
                            if choice == crate::chain_settings::BlockChoice::Room {
                                load = Some(place);
                            }
                        }
                        if let Some(place) = shown.bypass
                            && let Some(block) = chain.blocks.get(place)
                        {
                            let bypass = !block.bypass;
                            if let Some(slot) = chain.blocks.get_mut(place) {
                                slot.bypass = bypass;
                                changed = true;
                            }
                        }
                        if let Some((from, to)) = shown.moved
                            && move_block(&mut chain.blocks, from, to)
                        {
                            changed = true;
                            window.selected = to.saturating_add(1);
                        }
                        ui.separator();
                        match window.selected.checked_sub(1) {
                            None => {
                                ui.label(
                                    RichText::new(
                                        "The master sum: choose a place on the board to add \
                                         an effect after it.",
                                    )
                                    .weak(),
                                );
                            }
                            Some(place) => {
                                let edited = editor(ui, &chain, place, 0.0);
                                if let Some(edited_chain) = edited.chain {
                                    chain = edited_chain;
                                    changed = true;
                                }
                                if edited.load {
                                    load = Some(place);
                                }
                            }
                        }
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
        if let Some(clip) = copied {
            self.sound_clipboard = Some(clip);
        }
        if changed {
            self.set_song_master_chain(chain);
        }
        if let Some(place) = load
            && let Err(error) = self.tasks.pick_master_impulse(place, ctx)
        {
            self.notice = Some(error);
        }
        if close {
            self.master_chain_window = None;
        } else {
            self.master_chain_window = Some(window);
        }
    }
}
