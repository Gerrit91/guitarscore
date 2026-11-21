//! The practice view's pull-up bar: a handle on top of the bottom bar, and the tiles it
//! shows when pulled up (the shown track's mute and solo, the transposition, the
//! count-in's length, the score's size, and the tuner, settings, saving and opening).
//! Each tile is an icon; a value it stands for sits in a small badge on its corner.

use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, Sense, Vec2, vec2};

use super::TARGET;
use crate::app::GuitarScoreApp;
use crate::app::icons::{self, Icon};
use crate::app::style;

use super::controls::{GAP, HANDLE};
use super::widgets::stepper_row;

/// Tiles in a row.
const COLUMNS: usize = 5;
/// The largest a tile gets; on a phone they share the width.
const TILE: Vec2 = vec2(72.0, 56.0);
const TILE_GAP: f32 = 10.0;
/// Room above a row for the badges that stick out of the tiles' corners.
const BADGE_ROOM: f32 = 8.0;
/// From this width on the tiles float in a card above the bar instead of widening it.
const FLOATING: f32 = 640.0;

/// One tile's look: switched on (filled with its colour, the icon in `ink`), and the
/// value in its badge.
#[derive(Default)]
struct Tile {
    on: Option<(Color32, Color32)>,
    badge: Option<String>,
}

/// What a tap on a tile asked for.
#[derive(Clone, Copy)]
enum Pick {
    Mute,
    Solo,
    Tuner,
    Settings,
    Save,
    Open,
}

impl GuitarScoreApp {
    /// The handle at the top of the bottom bar: a tap, or a swipe up, pulls the bar up to
    /// show more; a tap or a swipe down puts it back.
    pub(super) fn practice_handle(&mut self, ui: &mut egui::Ui) {
        let (strip, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), HANDLE), Sense::click_and_drag());
        let open = self.practice.drawer;
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, true, open, "More controls")
        });
        if response.clicked() {
            self.practice.drawer = !open;
        } else if response.dragged() {
            let moved = response.drag_delta().y;
            if moved < -2.0 {
                self.practice.drawer = true;
            } else if moved > 2.0 {
                self.practice.drawer = false;
            }
        }
        let visuals = ui.style().interact(&response);
        let grip = Rect::from_center_size(strip.center(), vec2(40.0, 5.0));
        ui.painter()
            .rect_filled(grip, 2.5, visuals.fg_stroke.color.gamma_multiply(0.6));
    }

    /// The pulled-up bar's tiles: under the bar's controls on a phone; on a wider screen
    /// in a card floating above the bar, only as wide as the tiles, so the score keeps its
    /// height.
    pub(super) fn practice_drawer(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        if width < FLOATING {
            ui.add_space(6.0);
            self.practice_tiles(ui, width);
            return;
        }
        let ctx = ui.ctx().clone();
        let tokens = style::tokens(ui);
        // The bar's top edge: what it shows so far, plus its frame's margin.
        let bar = ui.min_rect();
        let anchor = Pos2::new(bar.center().x, bar.top() - 8.0 - GAP);
        egui::Area::new(ui.scope_id().with("practice_drawer"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(anchor)
            .constrain(true)
            .show(&ctx, |ui| {
                egui::Frame::new()
                    .fill(tokens.chrome)
                    .stroke(egui::Stroke::new(1.0, tokens.line))
                    .corner_radius(14.0)
                    .inner_margin(egui::Margin::same(14))
                    .show(ui, |ui| self.practice_tiles(ui, grid_width(f32::INFINITY)));
            });
    }

    /// The tiles, in rows of [`COLUMNS`] across `width` (centred if they do not need it
    /// all).
    fn practice_tiles(&mut self, ui: &mut egui::Ui, width: f32) {
        let tile = vec2(
            ((width - (COLUMNS - 1) as f32 * TILE_GAP) / COLUMNS as f32).min(TILE.x),
            TILE.y,
        );
        let used = grid_width(width);
        let tokens = style::tokens(ui);
        let ctx = ui.ctx().clone();

        let track = self.tabs.get(self.active).and_then(|tab| {
            let index = gscore_model::TrackIndex::new(tab.track);
            let track = tab.document.song().track(index)?;
            Some((
                index,
                track.name().to_owned(),
                track.is_muted(),
                track.is_soloed(),
            ))
        });
        let transposition = self
            .document()
            .and_then(|document| document.song().transposition())
            .unwrap_or(0);
        let bars = self.playback.count_in_bars();
        let zoom = self.settings.practice_zoom;

        let mut pick = None;
        let row = |ui: &mut egui::Ui| -> Rect {
            ui.add_space(BADGE_ROOM);
            let (strip, _) = ui.allocate_exact_size(vec2(width, tile.y), Sense::hover());
            Rect::from_min_size(
                Pos2::new(strip.left() + (width - used).max(0.0) / 2.0, strip.top()),
                vec2(used.min(width), tile.y),
            )
        };
        let cell = |strip: Rect, column: usize| {
            Rect::from_min_size(
                strip.min + vec2(column as f32 * (tile.x + TILE_GAP), 0.0),
                tile,
            )
        };

        // The sound: the shown track's mute and solo, the transposition, the count-in.
        let strip = row(ui);
        if let Some((_, name, muted, soloed)) = &track {
            let look = Tile {
                on: muted.then_some((tokens.mute, Color32::WHITE)),
                badge: None,
            };
            if tile_button(
                ui,
                cell(strip, 0),
                Icon::Mute,
                &format!("Mute {name}"),
                &look,
            )
            .clicked()
            {
                pick = Some(Pick::Mute);
            }
            let look = Tile {
                on: soloed.then_some((tokens.solo, Color32::from_rgb(0x23, 0x1A, 0x05))),
                badge: None,
            };
            if tile_button(
                ui,
                cell(strip, 1),
                Icon::Solo,
                &format!("Solo {name}"),
                &look,
            )
            .clicked()
            {
                pick = Some(Pick::Solo);
            }
        }
        let look = Tile {
            on: None,
            badge: (transposition != 0).then(|| format!("{transposition:+}")),
        };
        let button = tile_button(ui, cell(strip, 2), Icon::TuningFork, "Transpose", &look);
        let mut step = None;
        let value = match transposition {
            0 => "0".to_owned(),
            step => format!("{step:+}"),
        };
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
        let look = Tile {
            on: None,
            badge: Some(bars.to_string()),
        };
        let button = tile_button(ui, cell(strip, 3), Icon::CountIn, "Count-in length", &look);
        let mut chosen = None;
        popup(&button, |ui| chosen = count_in_row(ui, bars));
        if let Some(bars) = chosen {
            self.playback.set_count_in_bars(bars);
        }
        let look = Tile {
            on: None,
            badge: Some(format!("{:.0}%", zoom * 100.0)),
        };
        let button = tile_button(ui, cell(strip, 4), Icon::ZoomIn, "Score size", &look);
        let mut zoomed = zoom;
        popup(&button, |ui| {
            stepper_row(ui, "Score size", &format!("{:.0} %", zoom * 100.0), |up| {
                let factor = if up { 1.1 } else { 1.0 / 1.1 };
                zoomed =
                    (zoom * factor).clamp(crate::state::ZOOM_RANGE.0, crate::state::ZOOM_RANGE.1);
            });
        });
        self.settings.practice_zoom = zoomed;

        // The rest: the tuner, settings, saving and opening.
        let strip = row(ui);
        let save = if cfg!(target_arch = "wasm32") {
            "Download (.gp)"
        } else {
            "Save"
        };
        let rest = [
            (Icon::Tuner, "Tuner", Pick::Tuner),
            (Icon::Settings, "Settings", Pick::Settings),
            (Icon::Save, save, Pick::Save),
            (Icon::Open, "Open a song", Pick::Open),
        ];
        for (column, (icon, label, choice)) in rest.into_iter().enumerate() {
            if tile_button(ui, cell(strip, column), icon, label, &Tile::default()).clicked() {
                pick = Some(choice);
            }
        }

        match (pick, track) {
            (Some(Pick::Mute), Some((track, _, muted, _))) => self.apply_command(
                gscore_edit::Command::SetTrackMute {
                    track,
                    mute: !muted,
                },
                false,
            ),
            (Some(Pick::Solo), Some((track, _, _, soloed))) => self.apply_command(
                gscore_edit::Command::SetTrackSolo {
                    track,
                    solo: !soloed,
                },
                false,
            ),
            (Some(Pick::Tuner), _) => self.open_tuner(),
            (Some(Pick::Settings), _) => self.open_settings_window(),
            (Some(Pick::Save), _) => self.save_song(&ctx),
            (Some(Pick::Open), _) => self.ask_open(&ctx),
            _ => {}
        }
    }
}

/// How wide a row of tiles is when `width` is there for it.
fn grid_width(width: f32) -> f32 {
    let gaps = (COLUMNS - 1) as f32 * TILE_GAP;
    let tile = ((width - gaps) / COLUMNS as f32).min(TILE.x);
    COLUMNS as f32 * tile + gaps
}

/// A tile's settings in a popup under it, sized for a finger.
fn popup(button: &egui::Response, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Popup::menu(button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(280.0);
            contents(ui);
        });
}

/// One tile: its icon in the middle, filled while it is on, and its badge on the top right
/// corner.
fn tile_button(
    ui: &mut egui::Ui,
    rect: Rect,
    icon: Icon,
    label: &str,
    look: &Tile,
) -> egui::Response {
    let response = ui.interact(
        rect,
        ui.scope_id().with(("practice_tile", label)),
        Sense::click(),
    );
    response.widget_info(|| match look.on {
        Some(_) => egui::WidgetInfo::selected(egui::Role::Button, true, true, label),
        None => egui::WidgetInfo::labeled(egui::Role::Button, true, label),
    });
    let tokens = style::tokens(ui);
    let visuals = ui.style().interact(&response);
    let pressed = response.is_pointer_button_down_on();
    let (fill, ink) = match look.on {
        Some((fill, ink)) => (fill, ink),
        None if pressed => (tokens.control_active, visuals.fg_stroke.color),
        None if response.hovered() => (tokens.control_hover, visuals.fg_stroke.color),
        None => (tokens.control, visuals.fg_stroke.color),
    };
    ui.painter().rect_filled(rect, 10.0, fill);
    let icon_rect = Rect::from_center_size(rect.center(), vec2(24.0, 24.0));
    icons::paint(ui.painter(), icon_rect, ink, icon);
    if let Some(badge) = &look.badge {
        let font = FontId::new(11.0, FontFamily::Name(crate::fonts::BOLD.into()));
        let text = ui
            .painter()
            .layout_no_wrap(badge.clone(), font, tokens.on_accent);
        let size = vec2((text.size().x + 10.0).max(18.0), 18.0);
        let pill = Rect::from_min_size(
            Pos2::new(rect.right() - size.x + 4.0, rect.top() - BADGE_ROOM),
            size,
        );
        ui.painter().rect_filled(pill, size.y / 2.0, tokens.accent);
        ui.painter()
            .galley(pill.center() - text.size() / 2.0, text, tokens.on_accent);
    }
    response
}

/// The count-in's length, one to four bars, as a row of choices; returns the one tapped.
fn count_in_row(ui: &mut egui::Ui, bars: u8) -> Option<u8> {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), TARGET), Sense::hover());
    let tokens = style::tokens(ui);
    ui.painter().text(
        Pos2::new(row.left(), row.center().y),
        Align2::LEFT_CENTER,
        "Count-in (bars)",
        FontId::proportional(14.0),
        tokens.text,
    );
    let count = gscore_play::MAX_COUNT_IN_BARS;
    let mut chosen = None;
    for choice in 1..=count {
        let from_right = f32::from(count - choice);
        let rect = Rect::from_min_size(
            Pos2::new(row.right() - (from_right + 1.0) * TARGET, row.top()),
            vec2(TARGET, TARGET),
        );
        let response = ui.interact(
            rect,
            ui.scope_id().with(("practice_count_in", choice)),
            Sense::click(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::RadioButton,
                true,
                choice == bars,
                format!("{choice} bars of count-in"),
            )
        });
        let (fill, text) = if choice == bars {
            (tokens.accent, tokens.on_accent)
        } else {
            (ui.style().interact(&response).weak_bg_fill, tokens.text)
        };
        ui.painter().rect_filled(rect.shrink(2.0), 6.0, fill);
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            choice.to_string(),
            FontId::proportional(15.0),
            text,
        );
        if response.clicked() {
            chosen = Some(choice);
        }
    }
    chosen
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
        assert!((phone - 4.0 * TILE_GAP) / COLUMNS as f32 >= TARGET);
    }
}
