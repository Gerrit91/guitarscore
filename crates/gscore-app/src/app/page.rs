//! The page view: the laid-out track, the edit cursor over it, scrolling to the cursor when
//! it moves, and clicks that move the cursor.

use std::path::PathBuf;

use egui::{Align, Layout, Rect, RichText, ScrollArea, Sense, Vec2};
use gscore_edit::{BeatSpot, Cursor, Selection};
use gscore_layout::{Places, Point};
use gscore_model::{BeatIndex, MeasureIndex, Song, TrackIndex, VoiceIndex};
use gscore_play::PlayState;

use super::GuitarScoreApp;
use super::page_menu::{self, MenuChoice};
use crate::canvas::{self, Arrangement, POINTS_PER_SPACE};
use crate::state::ScoreView;

/// How strongly the selection color is laid over the page: the shade the cursor's band and
/// the selection made together, used for the whole selection.
pub(super) const SELECTION_ALPHA: f32 = 0.32;

/// Space around the page inside the view, in points.
const PAGE_MARGIN: f32 = 32.0;
/// Room kept above the playing line when the view turns over during playback, in staff
/// spaces: the bar numbers, section names and tempo marks above the staff stay in view.
const TURN_HEADROOM: f32 = 7.0;

/// How the view scrolls to the cursor's column `column` when `visible` is shown. While
/// playing, nothing moves until the column leaves the view; then the view turns over like
/// a page: the playing line goes to the top (with `headroom` points above it), so what
/// follows fills the screen. Otherwise the view scrolls just enough to show the column.
fn cursor_scroll(
    column: Rect,
    visible: Rect,
    playing: bool,
    headroom: f32,
) -> Option<(Rect, Option<Align>)> {
    if !playing {
        return Some((column, None));
    }
    if visible.contains_rect(column) {
        return None;
    }
    let line = Rect::from_min_max(
        egui::pos2(column.left(), column.top() - headroom),
        column.max,
    );
    Some((line, Some(Align::TOP)))
}

/// How the horizontal screen (one endless line) turns over while playing: nothing moves
/// until the playing `column` leaves `visible`; then it goes to the left edge with `room`
/// points before it, so the next screenful follows. The view keeps its height.
fn screen_turn(column: Rect, visible: Rect, room: f32) -> Option<(Rect, Option<Align>)> {
    if visible.x_range().contains(column.left()) && visible.x_range().contains(column.right()) {
        return None;
    }
    let target = Rect::from_min_max(
        egui::pos2(column.left() - room, visible.top()),
        egui::pos2(column.right(), visible.bottom()),
    );
    Some((target, Some(Align::Min)))
}

/// The selection drawn on the page: none while playing, as it would cover the playing
/// note. It stays selected (a loop plays it) and shows again once playback pauses or stops.
fn shown_selection(selection: Option<Selection>, playing: bool) -> Option<Selection> {
    selection.filter(|_| !playing)
}

/// Where side-by-side pages scroll to show the cursor `column` on `page`: the whole page
/// when it fits in `visible`; else the page's left part with the column's line at the top
/// (`headroom` above it), as the vertical view turns over, so the column never ends up
/// above or below the view. `None` when the column is already in view on its page.
fn page_scroll(
    page: Rect,
    column: Rect,
    visible: Rect,
    headroom: f32,
) -> Option<(Rect, Option<Align>)> {
    let fits = page.height() <= visible.height() && page.width() <= visible.width();
    if fits {
        return (!visible.contains_rect(page)).then_some((page, None));
    }
    if visible.contains_rect(column) {
        return None;
    }
    let right = page.right().min(page.left() + visible.width());
    let target = Rect::from_min_max(
        egui::pos2(page.left(), column.top() - headroom),
        egui::pos2(right, column.bottom()),
    );
    Some((target, Some(Align::TOP)))
}

impl GuitarScoreApp {
    /// The page, scrollable, centered horizontally.
    pub(super) fn page_view(&mut self, ui: &mut egui::Ui) {
        if self.tabs.get(self.active).is_none() {
            self.empty_workspace(ui);
            return;
        }
        let fonts = match &self.fonts {
            Ok(fonts) => fonts,
            Err(error) => {
                ui.label(format!("The music font could not be loaded: {error}"));
                return;
            }
        };
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let document = &mut tab.document;
        // The track picker changes the view; the cursor follows it.
        document.editor_mut().set_track(TrackIndex::new(tab.track));
        // The song's page setup shapes both views, so lines break the same in each.
        let options = self
            .options
            .clone()
            .with_page_setup(&document.song().page_setup());
        let score_view = self.view.score_view;
        let zoom = self.view.zoom();
        let options = if score_view.on_pages() {
            options.on_pages()
        } else {
            options
        };
        // The vertical screen view fills the window instead of keeping the paper width;
        // the horizontal one gets its width from the whole score on one line.
        let options = if score_view == ScoreView::ScreenVertical {
            let scale = POINTS_PER_SPACE * zoom;
            // The surface and its margins fill the room left beside the scroll bar
            // exactly, so the view never scrolls sideways and keeps clear of the panels.
            let room =
                ui.available_width() - 2.0 * PAGE_MARGIN - ui.spacing().scroll.allocated_width();
            let width = if scale > 0.0 {
                room / scale
            } else {
                options.page_width
            };
            gscore_layout::LayoutOptions {
                page_width: width.max(20.0),
                margin_left: 2.0,
                margin_right: 2.0,
                ..options
            }
        } else {
            options
        };
        let options = gscore_layout::LayoutOptions {
            single_line: score_view.single_line(),
            ..options
        };
        // Empty staves may be left out, but never the cursor's: it must stay editable.
        let options = gscore_layout::LayoutOptions {
            empty_staves: if self.view.hide_empty_staves {
                gscore_layout::EmptyStaves::HiddenButPrimary
            } else {
                gscore_layout::EmptyStaves::Shown
            },
            ..options
        };
        let shown = tab.score.tracks(tab.track, document.song().track_count());
        let layout = match document.layout(&shown, tab.track, &options, fonts) {
            Ok(layout) => layout,
            Err(error) => {
                tab.page_info = None;
                ui.label(error);
                return;
            }
        };
        let cursor = document.editor().cursor();
        let revision = document.editor().revision();
        // The cursor's page, for the status bar.
        tab.page_info = score_view.on_pages().then(|| {
            let y = layout
                .places
                .column_rect(cursor.measure, cursor.voice, cursor.beat)
                .map_or(0.0, |rect| rect.origin.y);
            let page = layout
                .pages
                .iter()
                .rposition(|page| page.origin.y <= y)
                .unwrap_or(0);
            (page.saturating_add(1), layout.pages.len())
        });
        let scale = POINTS_PER_SPACE * zoom;
        let theme = self.view.theme.palette();
        let mut clicked: Option<Point> = None;
        let mut drag_to: Option<Cursor> = None;
        let mut choices: Vec<MenuChoice> = Vec::new();
        let playing = self.playback.state() == PlayState::Playing;
        // The vertical views turn over while playing: once the playing line leaves the
        // view, it goes to the top. Pages side by side turn page by page, and the
        // horizontal screen screen by screen (`screen_turn`).
        let turn_pages = playing && score_view.turns_pages();
        let shown_selection = shown_selection(document.editor().selection(), playing);
        let in_selection = shown_selection
            .is_some_and(|selection| selection_contains(selection, document.song(), cursor));
        // The grid shows a measure selection; any other selection (or none) ends it there.
        if !matches!(
            document.editor().selection(),
            Some(Selection::Measures { .. })
        ) {
            tab.selection = None;
        }
        // While playing, how far the cursor is through its beat and the bar played next,
        // for the cursor to glide there.
        let glide = if playing && self.settings.flowing_cursor {
            self.playback.sequence().and_then(|sequence| {
                let tick = self.playback.display_position();
                let span = sequence.beat_span_at(document.song(), cursor.position(), tick)?;
                let next = sequence.measure_at(span.end).map(|next| next.measure);
                // The beat playing at this frame's tick, even when the cursor (moved a
                // moment earlier) is still on the one before: no jump back for a frame.
                Some((span.at, span.progress(tick), next))
            })
        } else {
            None
        };
        let scroll_to_cursor = tab.shown_cursor != Some((cursor, revision));
        ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            let available = ui.available_width();
            let arrangement = match score_view {
                ScoreView::PageHorizontal => Arrangement::new(&layout, layout.pages.len()),
                // Zoomed out, as many pages side by side as fit.
                ScoreView::PageGrid => {
                    Arrangement::fitting(&layout, available - 2.0 * PAGE_MARGIN, zoom)
                }
                // One page, endless surface, or one horizontal line.
                ScoreView::PageVertical
                | ScoreView::ScreenVertical
                | ScoreView::ScreenHorizontal => Arrangement::new(&layout, 1),
            };
            let page = canvas::page_size(&layout, &arrangement, zoom);
            let size = egui::vec2(
                (page.x + 2.0 * PAGE_MARGIN).max(available),
                page.y + 2.0 * PAGE_MARGIN,
            );
            let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_label("Score page"));
            let origin = egui::pos2(rect.center().x - page.x / 2.0, rect.top() + PAGE_MARGIN);
            // The screen views are paper from edge to edge: no pages, no canvas around them.
            let surface = if score_view.on_pages() {
                canvas::Surface::Pages
            } else {
                ui.painter()
                    .rect_filled(ui.clip_rect(), 0.0, canvas::color(theme.paper));
                canvas::Surface::Screen
            };
            let painter = ui.painter_at(rect);
            canvas::paint(
                &painter,
                origin,
                &layout,
                &arrangement,
                theme,
                zoom,
                ui.clip_rect(),
                surface,
            );
            // A position on the screen as a point of the layout.
            let to_layout = |pos: egui::Pos2| arrangement.layout_point((pos - origin) / scale);
            // The selection: one solid band per system line it touches.
            if let Some(selection) = shown_selection {
                for band in layout_selection_bands(&layout, document.song(), selection) {
                    let band = arrangement.rect(band);
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
                let r = arrangement.rect(r);
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
            // The cursor's bar, lightly tinted (a selection is marked on its own).
            if shown_selection.is_none()
                && let Some(bar) = places.measure_rect(cursor.measure).map(to_screen)
            {
                canvas::paint_cursor_bar(&painter, bar, theme);
            }
            if let (Some(column), Some(string)) = (column, string) {
                // While playing, the moving column is enough; the string box would only
                // flicker from note to note.
                // Inside a selection the whole beat is marked, so the box is hidden too.
                let string = (!playing && !in_selection).then_some(string);
                // A gliding cursor is drawn on its way to the next beat; the view still
                // follows the beat itself.
                let shown = glide
                    .and_then(|(at, progress, next)| {
                        places.glide_rect(at.measure, at.voice, at.beat, progress, next)
                    })
                    .map_or(column, to_screen);
                // Inside the selection, the cursor's band would make its column darker than
                // the rest; the selection's one shade covers it already.
                let column_band = (!in_selection).then_some(shown);
                canvas::paint_cursor(&painter, column_band, string, theme);
                if scroll_to_cursor {
                    if score_view.pages_side_by_side() {
                        // Side-by-side pages follow page by page: bring the whole page
                        // into view instead of sliding a line to the top and cutting the
                        // page's top and bottom off.
                        let y = layout
                            .places
                            .column_rect(cursor.measure, cursor.voice, cursor.beat)
                            .map_or(0.0, |rect| rect.origin.y);
                        if let Some(page) =
                            layout.pages.iter().rev().find(|page| page.origin.y <= y)
                            && let Some((target, align)) = page_scroll(
                                to_screen(*page),
                                column,
                                ui.clip_rect(),
                                TURN_HEADROOM * scale,
                            )
                        {
                            ui.scroll_to_rect(target, align);
                        }
                    } else if playing && score_view.single_line() {
                        if let Some((target, align)) =
                            screen_turn(column, ui.clip_rect(), TURN_HEADROOM * scale)
                        {
                            ui.scroll_to_rect(target, align);
                        }
                    } else if let Some((target, align)) =
                        cursor_scroll(column, ui.clip_rect(), turn_pages, TURN_HEADROOM * scale)
                    {
                        ui.scroll_to_rect(target, align);
                    }
                }
            }
            // What the pointer is on: the beat and string of whichever track's staves.
            let hit = response
                .interact_pointer_pos()
                .and_then(|pos| layout.hit(to_layout(pos)));
            // A right-click acts on what it hits: the cursor goes there first, unless the
            // click is inside the selection (then the menu acts on the selection).
            if response.secondary_clicked()
                && let Some((track, hit)) = hit
            {
                let target = Cursor {
                    track,
                    measure: hit.measure,
                    voice: hit.voice,
                    beat: hit.beat,
                    string: hit.string,
                };
                let selected = document.editor().selection().is_some_and(|selection| {
                    selection_contains(selection, document.song(), target)
                });
                if !selected {
                    document.editor_mut().set_selection(None);
                    tab.selection = None;
                    tab.track = track.get();
                    document.editor_mut().set_cursor(target);
                }
            }
            let several = shown.len() > 1;
            response.context_menu(|ui| {
                page_menu::with_target(document, several, |target| {
                    page_menu::show(ui, target, &self.keymap, &mut self.view, &mut choices);
                });
            });
            // A drag selects beats of the voice it started in, across bar lines and system
            // lines; the cursor follows the drag so the keyboard can go on from there.
            if response.drag_started()
                && let Some((track, hit)) = hit
            {
                tab.track = track.get();
                let anchor = Cursor {
                    track,
                    measure: hit.measure,
                    voice: hit.voice,
                    beat: hit.beat,
                    string: hit.string,
                };
                tab.drag_anchor = Some(anchor);
                drag_to = Some(anchor);
            }
            if response.dragged()
                && let Some(anchor) = tab.drag_anchor
                && let Some(pos) = response.interact_pointer_pos()
                && let Some(hit) = layout.hit_in_voice(to_layout(pos), anchor.track, anchor.voice)
            {
                drag_to = Some(Cursor {
                    measure: hit.measure,
                    beat: hit.beat,
                    string: hit.string,
                    ..anchor
                });
            }
            if let (Some(anchor), Some(focus)) = (tab.drag_anchor, drag_to) {
                document.editor_mut().set_selection(Some(Selection::Beats {
                    track: anchor.track,
                    voice: anchor.voice,
                    anchor: BeatSpot::of(anchor),
                    focus: BeatSpot::of(focus),
                }));
                document.editor_mut().set_cursor(focus);
            }
            if response.drag_stopped() {
                tab.drag_anchor = None;
            }
            if response.clicked() {
                tab.drag_anchor = None;
                // A click clears a selection and moves the cursor.
                document.editor_mut().set_selection(None);
                tab.selection = None;
                if let Some(pos) = response.interact_pointer_pos() {
                    clicked = Some(to_layout(pos));
                }
            }
        });
        tab.shown_cursor = Some((cursor, revision));
        if let Some(point) = clicked
            && let Some((track, hit)) = layout.hit(point)
        {
            // A click on another track's staves moves the cursor (and the track picker)
            // there.
            tab.track = track.get();
            document.editor_mut().set_cursor(Cursor {
                track,
                measure: hit.measure,
                voice: hit.voice,
                beat: hit.beat,
                string: hit.string,
            });
            // While playing, a click moves playback there too.
            let at = document.editor().cursor().position();
            if let Err(error) = self.playback.jump_to(document.song(), at) {
                self.notice = Some(error);
            }
        }
        let ctx = ui.ctx().clone();
        self.run_menu_choices(choices, &ctx);
    }

    /// What the page shows with no song open: how to open one, and the most recently edited
    /// files as links.
    fn empty_workspace(&mut self, ui: &mut egui::Ui) {
        if self.tasks.busy() {
            ui.centered_and_justified(|ui| {
                ui.add(egui::Spinner::new().size(18.0));
                ui.label("Opening…");
            });
            return;
        }
        let mut open: Option<PathBuf> = None;
        let recent = &self.settings.recent_files;
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.18).max(16.0));
            ui.heading("Open a song");
            ui.add_space(6.0);
            ui.label(
                RichText::new("File › Open… opens a Guitar Pro file; you can also drop one here.")
                    .weak(),
            );
            if recent.is_empty() {
                return;
            }
            ui.add_space(22.0);
            ui.label(RichText::new("Recent files").strong());
            ui.add_space(6.0);
            let width = ui.available_width().min(480.0);
            ui.allocate_ui_with_layout(
                egui::vec2(width, 0.0),
                Layout::top_down(Align::Min),
                |ui| {
                    for path in recent.iter().take(8) {
                        let name = path.file_name().map_or_else(
                            || path.display().to_string(),
                            |n| n.to_string_lossy().into_owned(),
                        );
                        let folder = path
                            .parent()
                            .and_then(|parent| parent.file_name())
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        ui.horizontal(|ui| {
                            if ui
                                .link(name)
                                .on_hover_text(path.display().to_string())
                                .clicked()
                            {
                                open = Some(path.clone());
                            }
                            if !folder.is_empty() {
                                ui.add(
                                    egui::Label::new(RichText::new(folder).weak().small())
                                        .truncate(),
                                );
                            }
                        });
                    }
                },
            );
        });
        if let Some(path) = open {
            let ctx = ui.ctx().clone();
            self.open(path, &ctx);
        }
    }
}

/// Whether `cursor` is inside `selection` (in the selected track, for a one-track bar
/// selection).
fn selection_contains(selection: Selection, song: &Song, cursor: Cursor) -> bool {
    match selection {
        Selection::Beats { .. } => selection.positions(song).contains(&cursor.position()),
        Selection::Measures {
            anchor,
            focus,
            track,
        } => {
            track.is_none_or(|track| track == cursor.track)
                && (anchor.min(focus)..=anchor.max(focus)).contains(&cursor.measure)
        }
    }
}

/// The beat on the line above (`down` false) or below `from` (a beat of `voice`), at the
/// same place across the page: the neighbouring system's measure under the beat's x (or
/// the nearest one), and in it the beat of `voice` nearest that x (the start of a measure
/// without beats). `None` when there is no line there.
pub(super) fn line_spot(
    places: &Places,
    from: BeatSpot,
    voice: VoiceIndex,
    down: bool,
) -> Option<BeatSpot> {
    let (x, _) = places.beat_x(from.measure, voice, from.beat)?;
    let system = places.measure(from.measure)?.system;
    let target = if down {
        system.checked_add(1)?
    } else {
        system.checked_sub(1)?
    };
    // How far `x` is from a measure's extent (0 inside it).
    let distance = |place: &gscore_layout::MeasurePlace| {
        if x < place.left {
            place.left - x
        } else if x > place.right {
            x - place.right
        } else {
            0.0
        }
    };
    let measure = places
        .measures
        .iter()
        .filter(|place| place.system == target)
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))?;
    let beat = measure
        .beats
        .iter()
        .filter(|beat| beat.voice == voice)
        .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
        .map_or(BeatIndex::new(0), |beat| beat.beat);
    Some(BeatSpot {
        measure: measure.measure,
        beat,
    })
}

/// The selection's highlight over a laid-out score. A beat selection follows its track's
/// places (also when several tracks are shown); a measure selection covers every shown
/// track's staves.
fn layout_selection_bands(
    layout: &gscore_layout::Layout,
    song: &Song,
    selection: Selection,
) -> Vec<gscore_layout::Rect> {
    match selection {
        Selection::Beats { track, .. } => layout
            .places_of(track)
            .map(|places| selection_bands(places, song, selection, track))
            .unwrap_or_default(),
        Selection::Measures {
            track: Some(track), ..
        } => layout
            .places_of(track)
            .map(|places| selection_bands(places, song, selection, track))
            .unwrap_or_default(),
        Selection::Measures { track: None, .. } => layout
            .tracks
            .iter()
            .flat_map(|track| selection_bands(&track.places, song, selection, track.track))
            .collect(),
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
            anchor,
            focus,
        } => {
            if selected != track {
                return Vec::new();
            }
            let first = anchor.min(focus);
            let last = anchor.max(focus);
            let count = song.measure_count();
            (first.measure.get()..=last.measure.get().min(count.saturating_sub(1)))
                .filter_map(|index| {
                    let measure = MeasureIndex::new(index);
                    let place = places.measure(measure)?;
                    let system = places.systems.get(place.system)?;
                    let top = system.top - 1.0;
                    // A bar without beats in this voice has no column of its own: the
                    // selection covers the whole bar, as it does between two notes.
                    let has_beats = place.beats.iter().any(|beat| beat.voice == voice);
                    let mut left = place.left;
                    let mut right = place.right;
                    if measure == first.measure
                        && let Some(column) = has_beats
                            .then(|| places.column_rect(measure, voice, first.beat))
                            .flatten()
                    {
                        left = column.origin.x;
                    }
                    if measure == last.measure
                        && let Some(column) = has_beats
                            .then(|| places.column_rect(measure, voice, last.beat))
                            .flatten()
                    {
                        right = column.origin.x + column.width;
                    }
                    Some(gscore_layout::Rect {
                        origin: Point::new(left, top),
                        width: (right - left).max(0.0),
                        height: system.tab_bottom() + 1.0 - top,
                    })
                })
                .collect()
        }
        Selection::Measures { anchor, focus, .. } => (anchor.min(focus).get()
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

    use super::{cursor_scroll, page_scroll, screen_turn, shown_selection};
    use egui::Rect;

    #[test]
    fn playback_turns_the_view_over_instead_of_creeping_a_line_at_a_time() {
        let visible = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let column = |top: f32| Rect::from_min_size(egui::pos2(100.0, top), egui::vec2(4.0, 80.0));
        // Editing: always just make the column visible.
        assert_eq!(
            cursor_scroll(column(300.0), visible, false, 30.0),
            Some((column(300.0), None))
        );
        // Playing on a visible line: the view stays.
        assert_eq!(cursor_scroll(column(300.0), visible, true, 30.0), None);
        // The playing line runs off the bottom: it goes to the top, headroom above it.
        let (target, align) = cursor_scroll(column(560.0), visible, true, 30.0).unwrap();
        assert_eq!(align, Some(egui::Align::TOP));
        assert!((target.top() - 530.0).abs() < 1e-3);
        assert!((target.bottom() - column(560.0).bottom()).abs() < 1e-3);
        // A repeat jumps back above the view: the same, that line to the top.
        let (target, align) = cursor_scroll(column(-400.0), visible, true, 30.0).unwrap();
        assert!((target.top() + 430.0).abs() < 1e-3);
        assert_eq!(align, Some(egui::Align::TOP));
    }

    #[test]
    fn the_selection_is_hidden_while_playing() {
        let selection = Some(Selection::Measures {
            anchor: MeasureIndex::new(0),
            focus: MeasureIndex::new(1),
            track: None,
        });
        assert_eq!(shown_selection(selection, false), selection);
        assert_eq!(shown_selection(selection, true), None);
    }

    #[test]
    fn the_horizontal_screen_turns_over_a_screen_at_a_time() {
        let visible = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let column =
            |left: f32| Rect::from_min_size(egui::pos2(left, 100.0), egui::vec2(4.0, 80.0));
        // On screen: the view stays, however far right the column has moved.
        assert_eq!(screen_turn(column(790.0), visible, 50.0), None);
        // Off the right edge: it goes to the left edge, room before it, height kept.
        let (target, align) = screen_turn(column(798.0), visible, 50.0).unwrap();
        assert_eq!(align, Some(egui::Align::Min));
        assert!((target.left() - 748.0).abs() < 1e-3);
        assert!((target.top() - visible.top()).abs() < 1e-3);
        assert!((target.bottom() - visible.bottom()).abs() < 1e-3);
        // A repeat jumps back left of the view: the same.
        let (target, _) = screen_turn(column(-500.0), visible, 50.0).unwrap();
        assert!((target.left() + 550.0).abs() < 1e-3);
    }

    #[test]
    fn side_by_side_pages_keep_the_playing_line_in_view() {
        let visible = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1600.0, 600.0));
        let page = |left: f32| Rect::from_min_size(egui::pos2(left, 0.0), egui::vec2(700.0, 990.0));
        let column =
            |left: f32, top: f32| Rect::from_min_size(egui::pos2(left, top), egui::vec2(4.0, 80.0));
        // The page is taller than the view: the playing line is shown, not the page's top
        // or bottom wherever the last page left the view.
        let (target, align) = page_scroll(page(1500.0), column(1600.0, 60.0), visible, 30.0)
            .expect("the next page is out of view");
        assert_eq!(align, Some(egui::Align::TOP));
        assert!(
            (target.top() - 30.0).abs() < 1e-3,
            "the line goes to the top"
        );
        assert!(
            (target.left() - 1500.0).abs() < 1e-3,
            "the page's left edge"
        );
        // A line further down the same page, already visible: the view stays.
        assert_eq!(
            page_scroll(page(0.0), column(100.0, 400.0), visible, 30.0),
            None
        );
        // Below the view: turned over, that line to the top.
        let (target, _) = page_scroll(page(0.0), column(100.0, 700.0), visible, 30.0).unwrap();
        assert!((target.top() - 670.0).abs() < 1e-3);
        // A page that fits is shown whole.
        let small = Rect::from_min_size(egui::pos2(1700.0, 0.0), egui::vec2(700.0, 500.0));
        assert_eq!(
            page_scroll(small, column(1800.0, 60.0), visible, 30.0),
            Some((small, None))
        );
    }
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
    fn the_line_below_and_above_is_the_beat_at_the_same_place() {
        use super::line_spot;
        let spot = |measure: usize, beat: usize| BeatSpot {
            measure: MeasureIndex::new(measure),
            beat: BeatIndex::new(beat),
        };
        let voice = VoiceIndex::new(0);
        // Beat 3 of the first line (x 30) goes to beat 3 of the second line, and back.
        assert_eq!(
            line_spot(&places(), spot(0, 2), voice, true),
            Some(spot(1, 2))
        );
        assert_eq!(
            line_spot(&places(), spot(1, 2), voice, false),
            Some(spot(0, 2))
        );
        // No line above the first or below the last.
        assert_eq!(line_spot(&places(), spot(0, 1), voice, false), None);
        assert_eq!(line_spot(&places(), spot(1, 1), voice, true), None);
        // A line whose bar has no beats in the voice: its start.
        let mut empty = places();
        empty.measures[1].beats.clear();
        assert_eq!(line_spot(&empty, spot(0, 3), voice, true), Some(spot(1, 0)));
    }

    /// Three measures on one system; measure 1 has no beats (an empty bar).
    fn places_in_a_line() -> Places {
        let system = SystemPlace {
            top: 0.0,
            tab_top: 10.0,
            string_spacing: 1.5,
            strings: 6,
        };
        let measure = |index: usize| MeasurePlace {
            measure: MeasureIndex::new(index),
            system: 0,
            left: index as f32 * 50.0 + 5.0,
            right: index as f32 * 50.0 + 45.0,
            content: index as f32 * 50.0 + 10.0,
            beats: if index == 1 {
                Vec::new()
            } else {
                (0..4)
                    .map(|beat| BeatPlace {
                        voice: VoiceIndex::new(0),
                        beat: BeatIndex::new(beat),
                        x: index as f32 * 50.0 + 10.0 * (beat as f32 + 1.0),
                    })
                    .collect()
            },
        };
        Places {
            systems: vec![system],
            measures: vec![measure(0), measure(1), measure(2)],
        }
    }

    #[test]
    fn a_selection_covers_an_empty_measure_inside_it() {
        let song = testing::song(&[GUITAR_6], 3).unwrap();
        let bands = selection_bands(
            &places_in_a_line(),
            &song,
            beats((0, 1), (2, 1)),
            TrackIndex::new(0),
        );
        assert_eq!(bands.len(), 1, "one band from the first to the last beat");
        let band = bands[0];
        assert!((band.origin.x - 19.1).abs() < 1e-4, "{band:?}");
        assert!(
            (band.origin.x + band.width - 120.9).abs() < 1e-4,
            "{band:?}"
        );
    }

    #[test]
    fn a_selection_ending_in_an_empty_measure_covers_the_whole_bar() {
        let song = testing::song(&[GUITAR_6], 3).unwrap();
        let bands = selection_bands(
            &places_in_a_line(),
            &song,
            beats((0, 2), (1, 0)),
            TrackIndex::new(0),
        );
        assert_eq!(bands.len(), 1);
        let band = bands[0];
        assert!((band.origin.x - 29.1).abs() < 1e-4, "{band:?}");
        assert!((band.origin.x + band.width - 95.0).abs() < 1e-4, "{band:?}");
    }

    #[test]
    fn selected_measures_cover_the_whole_measure() {
        let song = testing::song(&[GUITAR_6], 2).unwrap();
        let selection = Selection::Measures {
            anchor: MeasureIndex::new(0),
            focus: MeasureIndex::new(0),
            track: None,
        };
        let bands = selection_bands(&places(), &song, selection, TrackIndex::new(0));
        assert_eq!(bands.len(), 1);
        assert!((bands[0].origin.x - 5.0).abs() < 1e-4);
        assert!((bands[0].width - 40.0).abs() < 1e-4);
    }
}
