//! The practice view's header parts: the song and where playback is, and the shown
//! track's chip. Everything else is in the drawer.

use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, Sense, Stroke, vec2};

use super::TARGET;
use crate::app::GuitarScoreApp;
use crate::app::style;

impl GuitarScoreApp {
    /// The song's title over where playback is, in `rect`.
    pub(super) fn practice_title_block(&self, ui: &mut egui::Ui, rect: Rect) {
        let tokens = style::tokens(ui);
        let (title, detail) = self.practice_title();
        let painter = ui.painter().with_clip_rect(rect);
        let bold = FontId::new(15.0, FontFamily::Name(crate::fonts::BOLD.into()));
        painter.text(
            Pos2::new(rect.left(), rect.center().y - 1.0),
            Align2::LEFT_BOTTOM,
            title,
            bold,
            tokens.text,
        );
        painter.text(
            Pos2::new(rect.left(), rect.center().y + 1.0),
            Align2::LEFT_TOP,
            detail,
            FontId::proportional(12.0),
            tokens.text_weak,
        );
    }

    /// The header's two lines: the song's title, and where playback (or the cursor) is,
    /// with the loop or the bar waiting for a second tap.
    pub(super) fn practice_title(&self) -> (String, String) {
        let Some(tab) = self.tabs.get(self.active) else {
            return (
                "GuitarScore".to_owned(),
                "Open a song to practise".to_owned(),
            );
        };
        let song = tab.document.song();
        let measure = self.playback.current_measure().unwrap_or_else(|| {
            tab.document
                .editor()
                .cursor()
                .measure
                .get()
                .saturating_add(1)
        });
        let mut detail = format!("Bar {measure} of {}", song.measure_count());
        if self.playback.loop_selection()
            && let Some(selection) = tab.loop_measures()
        {
            let (first, last) = (selection.start() + 1, selection.end() + 1);
            if first == last {
                detail.push_str(&format!(" · loop bar {first}"));
            } else {
                detail.push_str(&format!(" · loop {first}–{last}"));
            }
        }
        (tab.document.display_title(), detail)
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
        let tracks: Vec<(String, Color32)> = song
            .tracks()
            .map(|track| {
                let (red, green, blue) = track.color().parts();
                (track.name().to_owned(), Color32::from_rgb(red, green, blue))
            })
            .collect();
        let Some(shown) = tracks.len().checked_sub(1).map(|last| tab.track.min(last)) else {
            return;
        };
        let Some((name, color)) = tracks.get(shown).cloned() else {
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
        let dot = Rect::from_center_size(
            Pos2::new(rect.left() + 16.0, rect.center().y),
            vec2(9.0, 9.0),
        );
        painter.rect_filled(dot, 2.0, color);
        let arrow_x = rect.right() - 14.0;
        let text_room = Rect::from_min_max(
            Pos2::new(dot.right() + 7.0, rect.top()),
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
            for (index, (name, color)) in tracks.iter().enumerate() {
                let row = ui.add_sized(
                    vec2(ui.available_width(), TARGET),
                    egui::Button::selectable(index == shown, format!("    {name}")),
                );
                let dot = Rect::from_center_size(
                    Pos2::new(row.rect.left() + 12.0, row.rect.center().y),
                    vec2(9.0, 9.0),
                );
                ui.painter().rect_filled(dot, 2.0, *color);
                if row.clicked() {
                    chosen = Some(index);
                }
            }
        });
        if let Some(index) = chosen {
            self.set_shown_track(index);
        }
    }
}
