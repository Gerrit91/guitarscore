//! The page view: the laid-out track, the edit cursor over it, scrolling to the cursor when
//! it moves, and clicks that move the cursor.

use egui::{Rect, ScrollArea, Sense, Vec2};
use gscore_edit::{Cursor, Selection};
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
            let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
            let origin = egui::pos2(rect.center().x - page.x / 2.0, rect.top() + PAGE_MARGIN);
            let painter = ui.painter_at(rect);
            canvas::paint(&painter, origin, &layout, theme, zoom, ui.clip_rect());
            // A drag on the page selects a range of beats in the shown track and voice.
            if let Some(Selection::Beats {
                measure,
                voice,
                anchor,
                focus,
                ..
            }) = document.editor().selection()
            {
                let first = anchor.min(focus).get();
                let last = anchor.max(focus).get();
                for beat in first..=last {
                    if let Some(column) =
                        layout
                            .places
                            .column_rect(measure, voice, BeatIndex::new(beat))
                    {
                        let rect = Rect::from_min_size(
                            origin + Vec2::new(column.origin.x, column.origin.y) * scale,
                            Vec2::new(column.width, column.height) * scale,
                        );
                        painter.rect_filled(
                            rect,
                            2.0,
                            canvas::color(theme.selection).gamma_multiply(0.22),
                        );
                    }
                }
            }

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
                ui.checkbox(&mut self.view.show_edit_panel, "Edit palette");
            });
            let hit = response.interact_pointer_pos().and_then(|pos| {
                let at = (pos - origin) / scale;
                layout.places.hit(Point::new(at.x, at.y))
            });
            if response.drag_started()
                && let Some(hit) = hit
            {
                let anchor = Cursor {
                    measure: hit.measure,
                    voice: hit.voice,
                    beat: BeatIndex::new(hit.beat.get()),
                    string: hit.string,
                    ..cursor
                };
                self.drag_anchor = Some(anchor);
                document.editor_mut().set_selection(Some(Selection::Beats {
                    track: anchor.track,
                    measure: anchor.measure,
                    voice: anchor.voice,
                    anchor: anchor.beat,
                    focus: anchor.beat,
                }));
            }
            if response.dragged()
                && let (Some(anchor), Some(hit)) = (self.drag_anchor, hit)
            {
                let focus = if hit.measure == anchor.measure && hit.voice == anchor.voice {
                    BeatIndex::new(hit.beat.get())
                } else {
                    anchor.beat
                };
                document.editor_mut().set_selection(Some(Selection::Beats {
                    track: anchor.track,
                    measure: anchor.measure,
                    voice: anchor.voice,
                    anchor: anchor.beat,
                    focus,
                }));
            }
            if response.drag_stopped() {
                self.drag_anchor = None;
            }
            if response.clicked() {
                self.drag_anchor = None;
                // A click clears a selection and moves the cursor.
                document.editor_mut().set_selection(None);
                self.view.selection = None;
                if let Some(pos) = response.interact_pointer_pos() {
                    let at = (pos - origin) / scale;
                    clicked = Some(Point::new(at.x, at.y));
                }
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
