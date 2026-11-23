//! The practice view's drawer: everything but the player, as tiles. The loop, the speed
//! and its trainer, the count-in and the click; the shown track's mute and solo, the
//! transposition and the score's size; the mixer, the shown track's sound chain and its
//! staves; the tuner, settings, saving and opening.
//!
//! A chevron button at the end of the player's row pulls it up and puts it away. On a
//! phone it sits in the bottom bar above the player; from [`FLOATING`] on it floats in a
//! card over the score's corner (and over any window), only as wide as its tiles, with the
//! player at its bottom (shut, only the player is left, as wide). A tile that opens a window (or a
//! file dialog) puts the drawer away first, so the two do not overlap; a second tap on it
//! while its window is open shuts the window again.

use egui::{Align2, Pos2, Rect, Sense, Vec2, vec2};
use gscore_model::TrackStaves;

use crate::app::GuitarScoreApp;
use crate::app::icons::Icon;
use crate::app::style;

use super::PracticeChoice;
use super::TARGET;
use super::tiles::{BADGE_ROOM, Tile, back_button, count_in_row, tile_button};
use super::widgets::{stepper_row, switch_row, trainer_settings};

/// From this screen width on, the drawer floats instead of widening the bottom bar.
pub(super) const FLOATING: f32 = 640.0;
/// Tiles in a row.
const COLUMNS: usize = 5;
/// The largest a tile gets; on a phone they share the width.
const TILE: Vec2 = vec2(72.0, 52.0);
const TILE_GAP: f32 = 8.0;

/// The tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Item {
    Loop,
    Speed,
    Trainer,
    CountIn,
    Click,
    Mute,
    Solo,
    Transpose,
    Mixer,
    Sound,
    Staves,
    Size,
    Tuner,
    Settings,
    Save,
    Open,
    Print,
    ScreenOn,
    FullInterface,
    ByWidth,
    DarkScore,
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
        (Item::Mixer, 1),
        (Item::Sound, 1),
        (Item::Staves, 1),
        (Item::Size, 1),
        (Item::Tuner, 1),
    ],
    &[
        (Item::Settings, 1),
        (Item::Save, 1),
        (Item::Open, 1),
        (Item::Print, 1),
        (Item::FullInterface, 1),
    ],
    &[
        (Item::DarkScore, 1),
        (Item::ScreenOn, 1),
        (Item::ByWidth, 1),
    ],
];

impl GuitarScoreApp {
    /// The drawer inside the bottom bar (a phone): the tiles while it is open, above the
    /// player (whose row holds the drawer's button).
    pub(super) fn practice_drawer_inline(&mut self, ui: &mut egui::Ui) {
        if self.practice.drawer {
            let width = ui.available_width();
            self.practice_tiles(ui, width);
            ui.add_space(8.0);
        }
    }

    /// The drawer floating over the score's bottom right corner, above `bottom` (the
    /// mixer's top, or the screen's bottom), over every window: the player (whose row holds
    /// the drawer's button), with the tiles above it while open. Returns where it is.
    pub(super) fn practice_drawer_floating(&mut self, ctx: &egui::Context, bottom: f32) -> Rect {
        let tokens = style::context_tokens(ctx);
        let anchor = Pos2::new(ctx.content_rect().right() - 12.0, bottom - 12.0);
        let open = self.practice.drawer;
        // Shut, it is as wide as open: only the tiles are left out.
        let margin = egui::Margin {
            left: 14,
            right: 14,
            top: if open { 6 } else { 8 },
            bottom: 8,
        };
        egui::Area::new(egui::Id::unique("practice_drawer"))
            // Above the windows (a Sound chain sheet), which are in the middle.
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
                        // As wide as the tiles, or as the player needs.
                        let width = grid_width(f32::INFINITY).max(super::controls::player_width());
                        ui.set_width(width);
                        if open {
                            self.practice_tiles(ui, width);
                            ui.add_space(8.0);
                        }
                        self.practice_player(ui);
                    });
            })
            .response
            .rect
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
            | Item::Mixer
            | Item::Sound
            | Item::Staves
            | Item::Size
            | Item::Tuner
            | Item::Settings
            | Item::Save
            | Item::Open
            | Item::Print
            | Item::FullInterface
            | Item::DarkScore => true,
        }
    }

    /// The tiles, in rows across `width`, each row centred; or, while a tile's page is
    /// open, that page in their place.
    fn practice_tiles(&mut self, ui: &mut egui::Ui, width: f32) {
        if let Some(item) = self.practice.page {
            self.practice_tile_page(ui, item, width);
            return;
        }
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
                }
            }
            Item::Speed => self.practice_speed(ui, rect),
            Item::Trainer => {
                let look = Tile::switch(ui, self.playback.trainer().enabled);
                if tile_button(ui, rect, Icon::SpeedTrainer, "Speed trainer", &look).clicked() {
                    self.practice.page = Some(item);
                }
            }
            Item::CountIn => {
                let (on, bars) = (self.playback.count_in(), self.playback.count_in_bars());
                let look = Tile::switch(ui, on).badge(on.then(|| bars.to_string()));
                if tile_button(ui, rect, Icon::CountIn, "Count-in", &look).clicked() {
                    self.practice.page = Some(item);
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
                let (transposition, value) = self.practice_transposition();
                let look = Tile::default().badge((transposition != 0).then_some(value));
                if tile_button(ui, rect, Icon::TuningFork, "Transpose", &look).clicked() {
                    self.practice.page = Some(item);
                }
            }
            Item::Mixer => {
                let open = self.practice.mixer;
                let look = Tile::switch(ui, open);
                if tile_button(ui, rect, Icon::Mixer, "Mixer", &look).clicked() {
                    self.practice.drawer = false;
                    self.practice.mixer = !open;
                }
            }
            Item::Sound => {
                let Some(track) = self.tabs.get(self.active).map(|tab| tab.track) else {
                    return;
                };
                let open = self.sound_chain.is_some();
                let look = Tile::switch(ui, open);
                let label = format!("Sound chain of {}", self.track_name(track));
                if tile_button(ui, rect, Icon::StompBox, &label, &look).clicked() {
                    self.practice.drawer = false;
                    if open {
                        self.sound_chain = None;
                    } else {
                        self.open_sound_chain(track);
                    }
                }
            }
            Item::Staves => {
                if tile_button(ui, rect, Icon::Eye, "Staves", &Tile::default()).clicked() {
                    self.practice.page = Some(item);
                }
            }
            Item::Size => {
                let zoom = self.settings.practice_zoom;
                let look = Tile::default().badge(Some(format!("{:.0}%", zoom * 100.0)));
                if tile_button(ui, rect, Icon::ZoomIn, "Score size", &look).clicked() {
                    self.practice.page = Some(item);
                }
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
            Item::DarkScore => {
                let dark = self.view.theme == crate::state::ThemeChoice::Dark;
                let look = Tile::switch(ui, dark);
                if tile_button(ui, rect, Icon::Moon, "Dark score", &look).clicked() {
                    self.view.theme = if dark {
                        crate::state::ThemeChoice::Light
                    } else {
                        crate::state::ThemeChoice::Dark
                    };
                }
            }
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

    /// A tile's page in the drawer, in place of the tiles, across `width`: a row with the
    /// way back and its title, then its settings.
    fn practice_tile_page(&mut self, ui: &mut egui::Ui, item: Item, width: f32) {
        let ctx = ui.ctx().clone();
        let (row, _) = ui.allocate_exact_size(vec2(width, TARGET), Sense::hover());
        let back = Rect::from_min_size(row.min, vec2(TARGET, TARGET));
        if back_button(ui, back).clicked() {
            self.practice.page = None;
        }
        let tokens = style::tokens(ui);
        ui.painter().text(
            Pos2::new(back.right() + 6.0, row.center().y),
            Align2::LEFT_CENTER,
            page_title(item),
            egui::FontId::new(16.0, egui::FontFamily::Name(crate::fonts::BOLD.into())),
            tokens.text,
        );
        ui.add_space(4.0);
        ui.scope(|ui| {
            ui.set_width(width);
            match item {
                Item::Trainer => {
                    let trainer = self.playback.trainer();
                    let mut changed = trainer;
                    trainer_settings(ui, &mut changed, self.playback.trainer_passes());
                    if changed != trainer {
                        self.set_speed_trainer(changed);
                    }
                }
                Item::CountIn => {
                    let (on, bars) = (self.playback.count_in(), self.playback.count_in_bars());
                    let (mut switched, mut chosen) = (on, None);
                    switch_row(ui, &mut switched, "Count in before playing");
                    ui.add_enabled_ui(switched, |ui| chosen = count_in_row(ui, bars));
                    if switched != on {
                        self.set_count_in(switched);
                    }
                    if let Some(bars) = chosen {
                        self.playback.set_count_in_bars(bars);
                    }
                }
                Item::Transpose => {
                    let (_, value) = self.practice_transposition();
                    let mut step = None;
                    stepper_row(ui, "Transpose (semitones)", &value, |up| step = Some(up));
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
                    let mut zoomed = zoom;
                    stepper_row(ui, "Score size", &format!("{:.0} %", zoom * 100.0), |up| {
                        let factor = if up { 1.1 } else { 1.0 / 1.1 };
                        zoomed = (zoom * factor)
                            .clamp(crate::state::ZOOM_RANGE.0, crate::state::ZOOM_RANGE.1);
                    });
                    self.settings.practice_zoom = zoomed;
                }
                Item::Staves => self.practice_staves(ui),
                _ => {
                    self.practice.page = None;
                }
            }
        });
        ui.add_space(4.0);
    }

    /// The song's transposition in semitones, and as the tile and its page write it.
    fn practice_transposition(&self) -> (i32, String) {
        let transposition = self
            .document()
            .and_then(|document| document.song().transposition())
            .unwrap_or(0);
        let value = match transposition {
            0 => "0".to_owned(),
            step => format!("{step:+}"),
        };
        (transposition, value)
    }

    /// Which staves the shown track's score has: standard notation, tablature and slash
    /// notation, switched as in the track panel (at least one stays on, notation needs a
    /// tuning; a drum kit switches only its drum tab). Each switch is an edit.
    fn practice_staves(&mut self, ui: &mut egui::Ui) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let track = gscore_model::TrackIndex::new(tab.track);
        let Some(shown) = tab.document.song().track(track) else {
            return;
        };
        let staves = shown.staves();
        let percussion = shown.is_percussion();
        let has_pitches = shown.note_tuning().ok().flatten().is_some();
        let mut next = staves;
        if percussion {
            let mut on = staves.tablature();
            switch_row(ui, &mut on, "Drum tablature");
            if on != staves.tablature() {
                // The percussion staff always shows.
                next = if on {
                    staves.with_tablature(true)
                } else {
                    TrackStaves::NOTATION_ONLY
                };
            }
        } else {
            let count = staves.count();
            for (label, on, possible, toggled) in [
                (
                    "Standard notation",
                    staves.notation(),
                    has_pitches,
                    staves.with_notation(!staves.notation()),
                ),
                (
                    "Tablature",
                    staves.tablature(),
                    true,
                    staves.with_tablature(!staves.tablature()),
                ),
                (
                    "Slash notation",
                    staves.slash(),
                    true,
                    staves.with_slash(!staves.slash()),
                ),
            ] {
                // The last staff that is on stays on.
                let enabled = possible && !(on && count == 1);
                let mut switched = on;
                ui.add_enabled_ui(enabled, |ui| switch_row(ui, &mut switched, label));
                if switched != on && enabled {
                    next = toggled;
                }
            }
        }
        if next != staves {
            self.apply_command(
                gscore_edit::Command::SetTrackStaves {
                    track,
                    staves: next,
                },
                false,
            );
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

/// The title of `item`'s page in the drawer.
fn page_title(item: Item) -> &'static str {
    match item {
        Item::Trainer => "Speed trainer",
        Item::CountIn => "Count-in",
        Item::Transpose => "Transpose",
        Item::Size => "Score size",
        Item::Staves => "Staves",
        _ => "",
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
        assert!(tile_width(374.0) >= TARGET);
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
