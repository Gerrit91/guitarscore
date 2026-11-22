//! The practice view's parts above the score: what the view is for while no song is
//! open, and the shown track's chip, floating in the top right corner over the score.

use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, Sense, Stroke, vec2};

use super::TARGET;
use crate::app::GuitarScoreApp;
use crate::app::style;

/// A track's colour mark, as the song grid's rows draw it.
const SWATCH: egui::Vec2 = vec2(5.0, 16.0);
/// The instrument's symbol beside it.
const SYMBOL: f32 = 14.0;

/// A track as the chip and its list show it.
#[derive(Debug, Clone)]
struct ChipTrack {
    name: String,
    color: Color32,
    symbol: gscore_model::InstrumentSymbol,
}

/// The track's colour bar with its left edge at `left`, then its instrument's symbol, as
/// the song grid's rows draw them, centred on `y`. Returns where the name starts.
fn mark(
    painter: &egui::Painter,
    left: f32,
    y: f32,
    color: Color32,
    symbol: gscore_model::InstrumentSymbol,
    ink: Color32,
) -> f32 {
    let bar = Rect::from_min_size(Pos2::new(left, y - SWATCH.y / 2.0), SWATCH);
    painter.rect_filled(bar, 1.0, color);
    let icon = Rect::from_center_size(
        Pos2::new(bar.right() + 6.0 + SYMBOL / 2.0, y),
        vec2(SYMBOL, SYMBOL),
    );
    crate::app::instrument::paint(painter, icon, symbol, ink);
    icon.right() + 8.0
}

/// One track in the chip's list, a finger's height: its colour, its instrument and its
/// name, filled while it is the one shown.
fn track_row(ui: &mut egui::Ui, track: &ChipTrack, shown: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), TARGET), Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, shown, &track.name));
    let tokens = style::tokens(ui);
    let visuals = ui.style().interact_selectable(&response, shown);
    if shown || response.hovered() || response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 6.0, visuals.weak_bg_fill);
    }
    let ink = if shown {
        visuals.text_color()
    } else {
        tokens.text
    };
    let name_left = mark(
        ui.painter(),
        rect.left() + 8.0,
        rect.center().y,
        track.color,
        track.symbol,
        ink,
    );
    let room = Rect::from_min_max(
        Pos2::new(name_left, rect.top()),
        Pos2::new(rect.right() - 8.0, rect.bottom()),
    );
    ui.painter().with_clip_rect(room).text(
        Pos2::new(room.left(), rect.center().y),
        Align2::LEFT_CENTER,
        &track.name,
        FontId::proportional(14.0),
        ink,
    );
    response
}

impl GuitarScoreApp {
    /// What the view is for, in `rect` (the header while no song is open).
    pub(super) fn practice_title_block(&self, ui: &mut egui::Ui, rect: Rect) {
        let tokens = style::tokens(ui);
        let painter = ui.painter().with_clip_rect(rect);
        let bold = FontId::new(15.0, FontFamily::Name(crate::fonts::BOLD.into()));
        painter.text(
            Pos2::new(rect.left(), rect.center().y - 1.0),
            Align2::LEFT_BOTTOM,
            "GuitarScore",
            bold,
            tokens.text,
        );
        painter.text(
            Pos2::new(rect.left(), rect.center().y + 1.0),
            Align2::LEFT_TOP,
            "Open a song to practise",
            FontId::proportional(12.0),
            tokens.text_weak,
        );
    }

    /// The shown track's chip floating in the top right corner of the score, whose top is
    /// at `top` (in a card of its own, like the drawer).
    pub(super) fn practice_chip_floating(&mut self, ctx: &egui::Context, top: f32) {
        let tokens = style::context_tokens(ctx);
        let corner = Pos2::new(ctx.content_rect().right() - 12.0, top + 12.0);
        egui::Area::new(egui::Id::unique("practice_chip_card"))
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(corner)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(tokens.chrome)
                    .stroke(Stroke::new(1.0, tokens.line))
                    .corner_radius(22.0)
                    .inner_margin(egui::Margin::same(4))
                    .show(ui, |ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(super::controls::CHIP, Sense::hover());
                        self.practice_track_chip(ui, rect);
                    });
            });
    }

    /// How far playback (or the cursor) is into the song, "01:23".
    pub(super) fn practice_time(&mut self) -> String {
        self.practice_where()
            .map(|(elapsed, _)| crate::app::toolbar::time_text(elapsed))
            .unwrap_or_default()
    }

    /// The tempo written where playback (or the cursor) is, in beats per minute.
    pub(super) fn practice_tempo(&mut self) -> Option<u32> {
        self.practice_where().map(|(_, bpm)| bpm)
    }

    /// Where playback (or, while stopped, the cursor) is: the time into the song and the
    /// tempo written there.
    fn practice_where(&mut self) -> Option<(u64, u32)> {
        let state = self.playback.state();
        let pending = self.playback.count_in_pending();
        let position = self.playback.position();
        let document = &mut self.tabs.get_mut(self.active)?.document;
        let cursor = document.editor().cursor().position();
        let timeline = document.timeline()?;
        let tick = if state != gscore_play::PlayState::Stopped || pending {
            position
        } else {
            timeline.tick_of(document.song(), cursor).unwrap_or(0)
        };
        let (elapsed, _) = crate::app::toolbar::playback_times(&timeline, tick, None, false);
        Some((elapsed, timeline.bpm_at(tick)))
    }

    /// The shown track: its colour and name; a tap lists the song's tracks.
    pub(super) fn practice_track_chip(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let song = tab.document.song();
        let tracks: Vec<ChipTrack> = song
            .tracks()
            .map(|track| {
                let (red, green, blue) = track.color().parts();
                ChipTrack {
                    name: track.name().to_owned(),
                    color: Color32::from_rgb(red, green, blue),
                    symbol: crate::app::instrument::derived(
                        &track,
                        song.track_sound(track.index()),
                    ),
                }
            })
            .collect();
        let Some(shown) = tracks.len().checked_sub(1).map(|last| tab.track.min(last)) else {
            return;
        };
        let Some(ChipTrack {
            name,
            color,
            symbol,
        }) = tracks.get(shown).cloned()
        else {
            return;
        };
        let response = ui.interact(rect, ui.scope_id().with("practice_chip"), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Track: {name}"))
        });
        let tokens = style::tokens(ui);
        let visuals = ui.style().interact(&response);
        let painter = ui.painter();
        painter.rect_filled(rect, rect.height() / 2.0, visuals.weak_bg_fill);
        // The track's colour and instrument as the song grid marks them.
        let name_left = mark(
            painter,
            rect.left() + 14.0,
            rect.center().y,
            color,
            symbol,
            tokens.text,
        );
        let arrow_x = rect.right() - 14.0;
        let text_room = Rect::from_min_max(
            Pos2::new(name_left, rect.top()),
            Pos2::new(arrow_x - 8.0, rect.bottom()),
        );
        painter.with_clip_rect(text_room).text(
            Pos2::new(text_room.left(), rect.center().y),
            Align2::LEFT_CENTER,
            &name,
            FontId::proportional(13.0),
            tokens.text,
        );
        let c = Pos2::new(arrow_x, rect.center().y);
        painter.add(egui::Shape::convex_polygon(
            vec![
                c + vec2(-4.0, -2.0),
                c + vec2(4.0, -2.0),
                c + vec2(0.0, 3.0),
            ],
            tokens.text,
            Stroke::NONE,
        ));
        let mut chosen = None;
        egui::Popup::menu(&response).show(|ui| {
            ui.set_min_width(220.0);
            for (index, track) in tracks.iter().enumerate() {
                if track_row(ui, track, index == shown).clicked() {
                    chosen = Some(index);
                }
            }
        });
        if let Some(index) = chosen {
            self.set_shown_track(index);
        }
    }
}
