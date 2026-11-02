//! The page view: the laid-out track, the edit cursor over it, scrolling to the cursor when
//! it moves, and clicks that move the cursor.

use egui::{Rect, ScrollArea, Sense, Vec2};
use gscore_edit::{BeatSpot, Cursor, Selection};
use gscore_layout::{Places, Point};
use gscore_model::{MeasureIndex, Song, TrackIndex};
use gscore_play::PlayState;

use super::GuitarScoreApp;
use crate::canvas::{self, POINTS_PER_SPACE};

/// How strongly the selection color is laid over the page: the shade the cursor's band and
/// the selection made together, used for the whole selection.
const SELECTION_ALPHA: f32 = 0.32;

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
        let mut drag_to: Option<Cursor> = None;
        let playing = self.playback.state() == PlayState::Playing;
        let in_selection = document.editor().selection().is_some_and(|selection| {
            selection
                .positions(document.song())
                .contains(&cursor.position())
                || matches!(selection, Selection::Measures { .. })
                    && (selection.first_measure()..=selection.last_measure())
                        .contains(&cursor.measure)
        });
        // The grid shows a measure selection; any other selection (or none) ends it there.
        if !matches!(
            document.editor().selection(),
            Some(Selection::Measures { .. })
        ) {
            self.view.selection = None;
        }
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
            // The selection: one solid band per system line it touches.
            if let Some(selection) = document.editor().selection() {
                for band in
                    selection_bands(&layout.places, document.song(), selection, cursor.track)
                {
                    let rect = Rect::from_min_size(
                        origin + Vec2::new(band.origin.x, band.origin.y) * scale,
                        Vec2::new(band.width, band.height) * scale,
                    );
                    painter.rect_filled(
                        rect,
                        2.0,
                        canvas::color(theme.selection).gamma_multiply(SELECTION_ALPHA),
                    );
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
                // While playing, the moving column is enough; the string box would only
                // flicker from note to note.
                let string = (!playing).then_some(string);
                // Inside the selection, the cursor's band would make its column darker than
                // the rest; the selection's one shade covers it already.
                let column_band = (!in_selection).then_some(column);
                canvas::paint_cursor(&painter, column_band, string, theme);
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
            // A drag selects beats of the voice it started in, across bar lines and system
            // lines; the cursor follows the drag so the keyboard can go on from there.
            if response.drag_started()
                && let Some(hit) = hit
            {
                let anchor = Cursor {
                    measure: hit.measure,
                    voice: hit.voice,
                    beat: hit.beat,
                    string: hit.string,
                    ..cursor
                };
                self.drag_anchor = Some(anchor);
                drag_to = Some(anchor);
            }
            if response.dragged()
                && let Some(anchor) = self.drag_anchor
                && let Some(pos) = response.interact_pointer_pos()
            {
                let at = (pos - origin) / scale;
                if let Some(hit) = layout
                    .places
                    .hit_in_voice(Point::new(at.x, at.y), anchor.voice)
                {
                    drag_to = Some(Cursor {
                        measure: hit.measure,
                        beat: hit.beat,
                        string: hit.string,
                        ..anchor
                    });
                }
            }
            if let (Some(anchor), Some(focus)) = (self.drag_anchor, drag_to) {
                document.editor_mut().set_selection(Some(Selection::Beats {
                    track: anchor.track,
                    voice: anchor.voice,
                    anchor: BeatSpot::of(anchor),
                    focus: BeatSpot::of(focus),
                }));
                document.editor_mut().set_cursor(focus);
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
                beat: hit.beat,
                string: hit.string,
                ..cursor
            });
            // While playing, a click moves playback there too.
            let at = document.editor().cursor().position();
            if let Err(error) = self.playback.jump_to(document.song(), at) {
                self.notice = Some(error);
            }
        }
    }
}

/// The selection's highlight on the page of `track`, in page units: one rectangle per
/// system line, from the first selected column (or measure) on it to the last, so the band
/// is solid between the notes and across bar lines.
fn selection_bands(
    places: &Places,
    song: &Song,
    selection: Selection,
    track: TrackIndex,
) -> Vec<gscore_layout::Rect> {
    let rects: Vec<gscore_layout::Rect> = match selection {
        Selection::Beats {
            track: selected,
            voice,
            ..
        } => {
            if selected != track {
                return Vec::new();
            }
            selection
                .positions(song)
                .into_iter()
                .filter_map(|at| places.column_rect(at.measure, voice, at.beat))
                .collect()
        }
        Selection::Measures { anchor, focus } => (anchor.min(focus).get()
            ..=anchor.max(focus).get())
            .filter_map(|measure| places.measure_rect(MeasureIndex::new(measure)))
            .collect(),
    };
    let mut bands: Vec<gscore_layout::Rect> = Vec::new();
    for rect in rects {
        // Rectangles of one system share their top; the selection runs in page order.
        match bands.last_mut() {
            Some(band) if (band.origin.y - rect.origin.y).abs() < f32::EPSILON => {
                let left = band.origin.x.min(rect.origin.x);
                let right = (band.origin.x + band.width).max(rect.origin.x + rect.width);
                band.origin.x = left;
                band.width = right - left;
            }
            _ => bands.push(rect),
        }
    }
    bands
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use gscore_edit::{BeatSpot, Selection};
    use gscore_layout::{BeatPlace, MeasurePlace, Places, SystemPlace};
    use gscore_model::testing::{self, GUITAR_6};
    use gscore_model::{BeatIndex, MeasureIndex, TrackIndex, VoiceIndex};

    use super::selection_bands;

    /// Two systems with one measure each; measure 0 has beats at 10, 20, 30, 40.
    fn places() -> Places {
        let system = |top: f32| SystemPlace {
            top,
            tab_top: top + 10.0,
            string_spacing: 1.5,
            strings: 6,
        };
        let measure = |index: usize, system: usize| MeasurePlace {
            measure: MeasureIndex::new(index),
            system,
            left: 5.0,
            right: 45.0,
            content: 10.0,
            beats: (0..4)
                .map(|beat| BeatPlace {
                    voice: VoiceIndex::new(0),
                    beat: BeatIndex::new(beat),
                    x: 10.0 * (beat as f32 + 1.0),
                })
                .collect(),
        };
        Places {
            systems: vec![system(0.0), system(30.0)],
            measures: vec![measure(0, 0), measure(1, 1)],
        }
    }

    fn beats(from: (usize, usize), to: (usize, usize)) -> Selection {
        let spot = |(measure, beat): (usize, usize)| BeatSpot {
            measure: MeasureIndex::new(measure),
            beat: BeatIndex::new(beat),
        };
        Selection::Beats {
            track: TrackIndex::new(0),
            voice: VoiceIndex::new(0),
            anchor: spot(from),
            focus: spot(to),
        }
    }

    #[test]
    fn selected_beats_make_one_solid_band_per_system() {
        let song = testing::song(&[GUITAR_6], 2).unwrap();
        let bands = selection_bands(&places(), &song, beats((0, 1), (0, 3)), TrackIndex::new(0));
        assert_eq!(bands.len(), 1, "no gaps between the columns");
        let band = bands[0];
        // From the left edge of beat 2's column to the right edge of beat 4's.
        assert!((band.origin.x - 19.1).abs() < 1e-4, "{band:?}");
        assert!((band.origin.x + band.width - 40.9).abs() < 1e-4, "{band:?}");

        // Over a line break: one band on each system.
        let bands = selection_bands(&places(), &song, beats((1, 1), (0, 2)), TrackIndex::new(0));
        assert_eq!(bands.len(), 2);
        assert!(bands[0].origin.y < bands[1].origin.y);

        // Another track's selection is not drawn on this page.
        let bands = selection_bands(&places(), &song, beats((0, 0), (0, 1)), TrackIndex::new(1));
        assert!(bands.is_empty());
    }

    #[test]
    fn selected_measures_cover_the_whole_measure() {
        let song = testing::song(&[GUITAR_6], 2).unwrap();
        let selection = Selection::Measures {
            anchor: MeasureIndex::new(0),
            focus: MeasureIndex::new(0),
        };
        let bands = selection_bands(&places(), &song, selection, TrackIndex::new(0));
        assert_eq!(bands.len(), 1);
        assert!((bands[0].origin.x - 5.0).abs() < 1e-4);
        assert!((bands[0].width - 40.0).abs() < 1e-4);
    }
}
