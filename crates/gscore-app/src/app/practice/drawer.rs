//! The practice view's drawer: everything but the player, as tiles. The loop, the speed
//! and its trainer, the count-in and the click; the shown track's mute and solo, the
//! transposition and the score's size; the tuner, settings, saving and opening.
//!
//! A chevron on top pulls it up and puts it away. On a phone it sits in the bottom bar
//! above the player; from [`FLOATING`] on it floats in a card over the score's corner,
//! only as wide as its tiles (shut, only the chevron is left, as wide). A tile that opens a window (or a file dialog) puts the
//! drawer away first, so the two do not overlap; a second tap on it while its window is
//! open shuts the window again.

use egui::{Align2, Pos2, Rect, Sense, Vec2, vec2};

use crate::app::GuitarScoreApp;
use crate::app::icons::Icon;
use crate::app::style;

use super::PracticeChoice;
use super::tiles::{BADGE_ROOM, Tile, count_in_row, handle, popup, tile_button};
use super::widgets::{stepper_row, switch_row, trainer_settings};

/// From this screen width on, the drawer floats instead of widening the bottom bar.
pub(super) const FLOATING: f32 = 640.0;
/// Tiles in a row.
const COLUMNS: usize = 5;
/// The largest a tile gets; on a phone they share the width.
const TILE: Vec2 = vec2(72.0, 52.0);
const TILE_GAP: f32 = 8.0;
/// The handle's height.
const HANDLE: f32 = 24.0;

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
    Print,
    ScreenOn,
    FullInterface,
    ByWidth,
}

/// The rows of tiles, each with how many columns it spans; a row is only as wide as the
/// tiles it shows, and centred.
const LAYOUT: &[&[(Item, usize)]] = &[
    &[(Item::Loop, 1), (Item::Speed, 3), (Item::Trainer, 1)],
    &[
        (Item::CountIn, 1),
        (Item::Click, 1),
        (Item::Mute, 1),
        (Item::Solo, 1),
        (Item::Transpose, 1),
    ],
    &[
        (Item::Size, 1),
        (Item::Tuner, 1),
        (Item::Settings, 1),
        (Item::Save, 1),
        (Item::Open, 1),
    ],
    &[
        (Item::Print, 1),
        (Item::ScreenOn, 1),
        (Item::FullInterface, 1),
        (Item::ByWidth, 1),
    ],
];

impl GuitarScoreApp {
    /// The drawer inside the bottom bar (a phone): the handle, then the tiles while open.
    pub(super) fn practice_drawer_inline(&mut self, ui: &mut egui::Ui) {
        let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), HANDLE), Sense::hover());
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
        // Shut, it is as wide as open: only the tiles are left out.
        let margin = egui::Margin {
            left: 14,
            right: 14,
            top: 2,
            bottom: if open { 14 } else { 2 },
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
                        let width = grid_width(f32::INFINITY);
                        let (strip, _) =
                            ui.allocate_exact_size(vec2(width, HANDLE), Sense::hover());
                        self.practice.drawer = handle(ui, strip, open);
                        if open {
                            self.practice_tiles(ui, width);
                        }
                    });
            });
    }

    /// Whether `item` belongs in the drawer now: the browser's own choices only exist
    /// there, and choosing the practice view by width has nothing to do once it does.
    fn practice_tile_shown(&self, item: Item) -> bool {
        match item {
            Item::ScreenOn => cfg!(target_arch = "wasm32"),
            Item::ByWidth => {
                cfg!(target_arch = "wasm32")
                    && PracticeChoice::parse(&self.settings.practice_view) != PracticeChoice::Auto
            }
            Item::Loop
            | Item::Speed
            | Item::Trainer
            | Item::CountIn
            | Item::Click
            | Item::Mute
            | Item::Solo
            | Item::Transpose
            | Item::Size
            | Item::Tuner
            | Item::Settings
            | Item::Save
            | Item::Open
            | Item::Print
            | Item::FullInterface => true,
        }
    }

    /// The tiles, in rows across `width`, each row centred.
    fn practice_tiles(&mut self, ui: &mut egui::Ui, width: f32) {
        let tile = vec2(tile_width(width), TILE.y);
        for row in LAYOUT {
            let shown: Vec<(Item, usize)> = row
                .iter()
                .copied()
                .filter(|(item, _)| self.practice_tile_shown(*item))
                .collect();
            if shown.is_empty() {
                continue;
            }
            let used = row_width(&shown, tile.x);
            ui.add_space(BADGE_ROOM);
            let (strip, _) = ui.allocate_exact_size(vec2(width, tile.y), Sense::hover());
            let mut x = strip.left() + (width - used).max(0.0) / 2.0;
            for (item, span) in shown {
                let size = vec2(
                    span as f32 * tile.x + (span.saturating_sub(1)) as f32 * TILE_GAP,
                    tile.y,
                );
                let rect = Rect::from_min_size(Pos2::new(x, strip.top()), size);
                self.practice_tile(ui, item, rect);
                x += size.x + TILE_GAP;
            }
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
            Item::Print => {
                let look = Tile::default();
                if tile_button(ui, rect, Icon::Print, "Print (PDF)", &look).clicked() {
                    self.practice.drawer = false;
                    self.ask_export(crate::export::ExportFormat::Pdf, &ctx);
                }
            }
            Item::FullInterface => {
                let look = Tile::default();
                if tile_button(ui, rect, Icon::FullInterface, "Full interface", &look).clicked() {
                    self.practice.drawer = false;
                    self.set_practice_view(PracticeChoice::Off);
                }
            }
            Item::ScreenOn => self.practice_screen_on(ui, rect),
            Item::ByWidth => {
                if tile_button(
                    ui,
                    rect,
                    Icon::FitWidth,
                    "Choose by screen width",
                    &Tile::default(),
                )
                .clicked()
                {
                    self.set_practice_view(PracticeChoice::Auto);
                }
            }
        }
    }

    /// The browser's switch that keeps the screen awake while the song plays.
    fn practice_screen_on(&mut self, ui: &mut egui::Ui, rect: Rect) {
        #[cfg(target_arch = "wasm32")]
        {
            let on = self.settings.keep_screen_on;
            let look = Tile::switch(ui, on);
            if tile_button(ui, rect, Icon::ScreenOn, "Keep the screen on", &look).clicked() {
                self.settings.keep_screen_on = !on;
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (ui, rect);
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

/// How wide `row` is with tiles `tile_width` across: every span is that many columns
/// wide (with the gaps inside it), so a row of spans lines up with the rows beneath it.
fn row_width(row: &[(Item, usize)], tile_width: f32) -> f32 {
    let spans: usize = row.iter().map(|(_, span)| *span).sum();
    spans as f32 * tile_width + spans.saturating_sub(1) as f32 * TILE_GAP
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
    fn every_tile_has_a_place_of_its_own_and_no_row_overflows() {
        let mut seen: Vec<Item> = Vec::new();
        for row in LAYOUT {
            let spans: usize = row.iter().map(|(_, span)| *span).sum();
            assert!(spans <= COLUMNS, "a row wider than the drawer: {row:?}");
            for (item, span) in *row {
                assert!(!seen.contains(item), "two tiles for one control: {item:?}");
                seen.push(*item);
                assert!(*span >= 1, "a tile with no width: {item:?}");
            }
        }
    }
}
