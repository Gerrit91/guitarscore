//! The practice view's drawer: everything but the player, as tiles. The loop, the speed
//! and its trainer, the count-in and the click; the shown track's mute and solo, the
//! transposition and the score's size; the tuner, settings, saving and opening.
//!
//! A chevron on top pulls it up and puts it away. On a phone it sits in the bottom bar
//! above the player; from [`FLOATING`] on it floats in a card over the score's corner,
//! only as wide as its tiles. A tile that opens a window (or a file dialog) puts the
//! drawer away first, so the two do not overlap; a second tap on it while its window is
//! open shuts the window again.

use egui::{Align2, Pos2, Rect, Sense, Vec2, vec2};

use crate::app::GuitarScoreApp;
use crate::app::icons::Icon;
use crate::app::style;

use super::tiles::{BADGE_ROOM, Tile, count_in_row, handle, popup, tile_button};
use super::widgets::{stepper_row, switch_row, trainer_settings};

/// From this screen width on, the drawer floats instead of widening the bottom bar.
pub(super) const FLOATING: f32 = 640.0;
/// Tiles in a row.
const COLUMNS: usize = 5;
/// The largest a tile gets; on a phone they share the width.
const TILE: Vec2 = vec2(72.0, 56.0);
const TILE_GAP: f32 = 10.0;
/// The handle's height, and its width on the shut floating drawer.
const HANDLE: Vec2 = vec2(72.0, 24.0);

/// The tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    Loop,
    Speed,
    Trainer,
    CountIn,
    Click,
    Mute,
    Solo,
    Transpose,
    Size,
    Tuner,
    Settings,
    Save,
    Open,
}

/// Where each tile sits: its column, its row and how many columns it spans.
const LAYOUT: &[(Item, usize, usize, usize)] = &[
    (Item::Loop, 0, 0, 1),
    (Item::Speed, 1, 0, 2),
    (Item::Trainer, 3, 0, 1),
    (Item::CountIn, 4, 0, 1),
    (Item::Click, 0, 1, 1),
    (Item::Mute, 1, 1, 1),
    (Item::Solo, 2, 1, 1),
    (Item::Transpose, 3, 1, 1),
    (Item::Size, 4, 1, 1),
    (Item::Tuner, 0, 2, 1),
    (Item::Settings, 1, 2, 1),
    (Item::Save, 2, 2, 1),
    (Item::Open, 3, 2, 1),
];
const ROWS: usize = 3;

impl GuitarScoreApp {
    /// The drawer inside the bottom bar (a phone): the handle, then the tiles while open.
    pub(super) fn practice_drawer_inline(&mut self, ui: &mut egui::Ui) {
        let (strip, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), HANDLE.y), Sense::hover());
        self.practice.drawer = handle(ui, strip, self.practice.drawer);
        if self.practice.drawer {
            let width = ui.available_width();
            self.practice_tiles(ui, width);
            ui.add_space(8.0);
        }
    }

    /// The drawer floating over the score's bottom right corner, above `bottom` (the
    /// bottom bar's top, or the screen's bottom): only its handle while shut, the handle
    /// over the tiles while open.
    pub(super) fn practice_drawer_floating(&mut self, ctx: &egui::Context, bottom: f32) {
        let tokens = style::context_tokens(ctx);
        let anchor = Pos2::new(ctx.content_rect().right() - 12.0, bottom - 12.0);
        let open = self.practice.drawer;
        let margin = if open {
            egui::Margin {
                left: 14,
                right: 14,
                top: 2,
                bottom: 14,
            }
        } else {
            egui::Margin::same(2)
        };
        egui::Area::new(egui::Id::unique("practice_drawer"))
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(anchor)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(tokens.chrome)
                    .stroke(egui::Stroke::new(1.0, tokens.line))
                    .corner_radius(14.0)
                    .inner_margin(margin)
                    .show(ui, |ui| {
                        let width = if open {
                            grid_width(f32::INFINITY)
                        } else {
                            HANDLE.x
                        };
                        let (strip, _) =
                            ui.allocate_exact_size(vec2(width, HANDLE.y), Sense::hover());
                        self.practice.drawer = handle(ui, strip, open);
                        if open {
                            self.practice_tiles(ui, width);
                        }
                    });
            });
    }

    /// The tiles, in rows of [`COLUMNS`] across `width` (centred if they do not need it
    /// all).
    fn practice_tiles(&mut self, ui: &mut egui::Ui, width: f32) {
        let tile = vec2(tile_width(width), TILE.y);
        let used = grid_width(width);
        let mut rows = Vec::with_capacity(ROWS);
        for _ in 0..ROWS {
            ui.add_space(BADGE_ROOM);
            let (strip, _) = ui.allocate_exact_size(vec2(width, tile.y), Sense::hover());
            rows.push(strip.left_top() + vec2((width - used).max(0.0) / 2.0, 0.0));
        }
        for &(item, column, row, span) in LAYOUT {
            let Some(&left_top) = rows.get(row) else {
                continue;
            };
            let rect = Rect::from_min_size(
                left_top + vec2(column as f32 * (tile.x + TILE_GAP), 0.0),
                vec2(span as f32 * tile.x + (span - 1) as f32 * TILE_GAP, tile.y),
            );
            self.practice_tile(ui, item, rect);
        }
    }

    /// Draws `item` in `rect` and does what it was asked to.
    fn practice_tile(&mut self, ui: &mut egui::Ui, item: Item, rect: Rect) {
        let ctx = ui.ctx().clone();
        match item {
            Item::Loop => {
                let looping = self.playback.loop_selection();
                let look = Tile::switch(ui, looping);
                if tile_button(ui, rect, Icon::Loop, "Loop", &look).clicked() {
                    self.set_loop_selection(!looping);
                    if looping {
                        self.practice_forget_tap();
                    }
                }
            }
            Item::Speed => self.practice_speed(ui, rect),
            Item::Trainer => {
                let trainer = self.playback.trainer();
                let look = Tile::switch(ui, trainer.enabled);
                let button = tile_button(ui, rect, Icon::SpeedTrainer, "Speed trainer", &look);
                let mut changed = trainer;
                let passes = self.playback.trainer_passes();
                popup(&button, |ui| trainer_settings(ui, &mut changed, passes));
                if changed != trainer {
                    self.set_speed_trainer(changed);
                }
            }
            Item::CountIn => {
                let (on, bars) = (self.playback.count_in(), self.playback.count_in_bars());
                let look = Tile::switch(ui, on).badge(on.then(|| bars.to_string()));
                let button = tile_button(ui, rect, Icon::CountIn, "Count-in", &look);
                let (mut switched, mut chosen) = (on, None);
                popup(&button, |ui| {
                    switch_row(ui, &mut switched, "Count in before playing");
                    ui.add_enabled_ui(switched, |ui| chosen = count_in_row(ui, bars));
                });
                if switched != on {
                    self.set_count_in(switched);
                }
                if let Some(bars) = chosen {
                    self.playback.set_count_in_bars(bars);
                }
            }
            Item::Click => {
                let metronome = self.playback.metronome();
                let look = Tile::switch(ui, metronome);
                if tile_button(ui, rect, Icon::Metronome, "Metronome", &look).clicked() {
                    self.set_metronome(!metronome);
                }
            }
            Item::Mute | Item::Solo => self.practice_mute_solo(ui, item, rect),
            Item::Transpose => {
                let transposition = self
                    .document()
                    .and_then(|document| document.song().transposition())
                    .unwrap_or(0);
                let value = match transposition {
                    0 => "0".to_owned(),
                    step => format!("{step:+}"),
                };
                let look = Tile::default().badge((transposition != 0).then(|| value.clone()));
                let button = tile_button(ui, rect, Icon::TuningFork, "Transpose", &look);
                let mut step = None;
                popup(&button, |ui| {
                    stepper_row(ui, "Transpose (semitones)", &value, |up| step = Some(up));
                });
                if let Some(up) = step {
                    let action = if up {
                        gscore_edit::ActionId::TransposeUpSemitone
                    } else {
                        gscore_edit::ActionId::TransposeDownSemitone
                    };
                    self.perform_action(gscore_edit::Action::Plain(action), &ctx);
                }
            }
            Item::Size => {
                let zoom = self.settings.practice_zoom;
                let look = Tile::default().badge(Some(format!("{:.0}%", zoom * 100.0)));
                let button = tile_button(ui, rect, Icon::ZoomIn, "Score size", &look);
                let mut zoomed = zoom;
                popup(&button, |ui| {
                    stepper_row(ui, "Score size", &format!("{:.0} %", zoom * 100.0), |up| {
                        let factor = if up { 1.1 } else { 1.0 / 1.1 };
                        zoomed = (zoom * factor)
                            .clamp(crate::state::ZOOM_RANGE.0, crate::state::ZOOM_RANGE.1);
                    });
                });
                self.settings.practice_zoom = zoomed;
            }
            Item::Tuner => {
                let open = self.tuner.is_some();
                let look = Tile::switch(ui, open);
                if tile_button(ui, rect, Icon::Tuner, "Tuner", &look).clicked() {
                    self.practice.drawer = false;
                    if open {
                        self.close_tuner();
                    } else {
                        self.open_tuner();
                    }
                }
            }
            Item::Settings => {
                let open = self.settings_window.is_some();
                let look = Tile::switch(ui, open);
                if tile_button(ui, rect, Icon::Settings, "Settings", &look).clicked() {
                    self.practice.drawer = false;
                    if open {
                        self.close_settings_window();
                    } else {
                        self.open_settings_window();
                    }
                }
            }
            Item::Save => {
                let label = if cfg!(target_arch = "wasm32") {
                    "Download (.gp)"
                } else {
                    "Save"
                };
                if tile_button(ui, rect, Icon::Save, label, &Tile::default()).clicked() {
                    self.practice.drawer = false;
                    self.save_song(&ctx);
                }
            }
            Item::Open => {
                if tile_button(ui, rect, Icon::Open, "Open a song", &Tile::default()).clicked() {
                    self.practice.drawer = false;
                    self.ask_open(&ctx);
                }
            }
        }
    }

    /// The shown track's mute (`Item::Mute`) or solo, coloured as in the mixer.
    fn practice_mute_solo(&mut self, ui: &mut egui::Ui, item: Item, rect: Rect) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let track = gscore_model::TrackIndex::new(tab.track);
        let Some(shown) = tab.document.song().track(track) else {
            return;
        };
        let (name, muted, soloed) = (shown.name().to_owned(), shown.is_muted(), shown.is_soloed());
        let tokens = style::tokens(ui);
        let command = if item == Item::Mute {
            let look = Tile {
                on: muted.then_some((tokens.mute, egui::Color32::WHITE)),
                badge: None,
            };
            tile_button(ui, rect, Icon::Mute, &format!("Mute {name}"), &look)
                .clicked()
                .then_some(gscore_edit::Command::SetTrackMute {
                    track,
                    mute: !muted,
                })
        } else {
            let look = Tile {
                on: soloed.then_some((tokens.solo, egui::Color32::from_rgb(0x23, 0x1A, 0x05))),
                badge: None,
            };
            tile_button(ui, rect, Icon::Solo, &format!("Solo {name}"), &look)
                .clicked()
                .then_some(gscore_edit::Command::SetTrackSolo {
                    track,
                    solo: !soloed,
                })
        };
        if let Some(command) = command {
            self.apply_command(command, false);
        }
    }
}

/// A tile's width when `width` is there for a row of them.
fn tile_width(width: f32) -> f32 {
    ((width - (COLUMNS - 1) as f32 * TILE_GAP) / COLUMNS as f32).min(TILE.x)
}

/// How wide a row of tiles is when `width` is there for it.
fn grid_width(width: f32) -> f32 {
    COLUMNS as f32 * tile_width(width) + (COLUMNS - 1) as f32 * TILE_GAP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tiles_share_a_phone_but_keep_their_size_on_a_wide_screen() {
        let wide = grid_width(f32::INFINITY);
        assert_eq!(
            wide,
            COLUMNS as f32 * TILE.x + (COLUMNS - 1) as f32 * TILE_GAP
        );
        assert!(
            wide < FLOATING,
            "the floating card is narrower than where it floats"
        );
        let phone = grid_width(374.0);
        assert!(
            (phone - 374.0).abs() < 1e-3,
            "a phone's row fills its width"
        );
        assert!(tile_width(374.0) >= super::super::TARGET);
    }

    #[test]
    fn every_tile_has_a_place_of_its_own() {
        let mut taken = [[false; COLUMNS]; ROWS];
        for &(_, column, row, span) in LAYOUT {
            for cell in &mut taken[row][column..column + span] {
                assert!(!*cell, "two tiles in one place");
                *cell = true;
            }
        }
    }
}
