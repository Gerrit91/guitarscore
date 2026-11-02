//! The page view: the laid-out track, the edit cursor over it, scrolling to the cursor when
//! it moves, and clicks that move the cursor.

use egui::{Rect, ScrollArea, Sense, Vec2};
use gscore_edit::Cursor;
use gscore_layout::Point;
use gscore_model::{BeatIndex, TrackIndex};

use super::GuitarScoreApp;
use crate::canvas::{self, POINTS_PER_SPACE};

/// Space around the page inside the view, in points.
const PAGE_MARGIN: f32 = 24.0;

impl GuitarScoreApp {
    /// The page, scrollable, centered horizontally.
    pub(super) fn page_view(&mut self, ui: &mut egui::Ui) {
        let fonts = match &self.fonts {
            Ok(fonts) => fonts,
            Err(error) => {
                ui.label(format!("The music font could not be loaded: {error}"));
                return;
            }
        };
        let Some(document) = &mut self.document else {
            ui.centered_and_justified(|ui| {
                ui.label(if self.tasks.busy() {
                    "Opening…"
                } else {
                    "Open a Guitar Pro file with File › Open…"
                });
            });
            return;
        };
        // The track picker changes the view; the cursor follows it.
        document
            .editor_mut()
            .set_track(TrackIndex::new(self.view.track));
        let layout = match document.layout(self.view.track, &self.options, fonts) {
            Ok(layout) => layout,
            Err(error) => {
                ui.label(error);
                return;
            }
        };
        let cursor = document.editor().cursor();
        let revision = document.editor().revision();
        let zoom = self.view.zoom();
        let scale = POINTS_PER_SPACE * zoom;
        let theme = self.view.theme.palette();
        let mut clicked: Option<Point> = None;
        let scroll_to_cursor = self.shown_cursor != Some((cursor, revision));
        ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            let page = canvas::page_size(&layout, zoom);
            let available = ui.available_width();
            let size = egui::vec2(
                (page.x + 2.0 * PAGE_MARGIN).max(available),
                page.y + 2.0 * PAGE_MARGIN,
            );
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let origin = egui::pos2(rect.center().x - page.x / 2.0, rect.top() + PAGE_MARGIN);
            let painter = ui.painter_at(rect);
            canvas::paint(&painter, origin, &layout, theme, zoom, ui.clip_rect());

            let to_screen = |r: gscore_layout::Rect| {
                Rect::from_min_size(
                    origin + Vec2::new(r.origin.x, r.origin.y) * scale,
                    Vec2::new(r.width, r.height) * scale,
                )
            };
            let places = &layout.places;
            let column = places
                .column_rect(cursor.measure, cursor.voice, cursor.beat)
                .map(to_screen);
            let string = places
                .cursor_rect(cursor.measure, cursor.voice, cursor.beat, cursor.string)
                .map(to_screen);
            if let (Some(column), Some(string)) = (column, string) {
                canvas::paint_cursor(&painter, column, string, theme);
                if scroll_to_cursor {
                    ui.scroll_to_rect(column, None);
                }
            }
            response.context_menu(|ui| {
                ui.label("Panels");
                ui.checkbox(&mut self.view.show_track_grid, "Track grid");
                ui.checkbox(&mut self.view.show_track_panel, "Track panel");
            });
            if response.clicked()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let at = (pos - origin) / scale;
                clicked = Some(Point::new(at.x, at.y));
            }
        });
        self.shown_cursor = Some((cursor, revision));
        if let Some(point) = clicked
            && let Some(hit) = layout.places.hit(point)
        {
            document.editor_mut().set_cursor(Cursor {
                measure: hit.measure,
                voice: hit.voice,
                beat: BeatIndex::new(hit.beat.get()),
                string: hit.string,
                ..cursor
            });
        }
    }
}
