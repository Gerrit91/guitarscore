//! The score views dialog: builds the song's named views of the score (which tracks, in
//! what order, with which staves, and whether empty staves are left out), and Guitar Pro's
//! own one, "All tracks", whose switches are the eyes of Guitar Pro's track list.
//!
//! Every change is one undoable edit and shows in the score at once: choosing a view in
//! the list also shows it.

use egui::{Margin, RichText, Sense, vec2};
use gscore_edit::Command;
use gscore_model::{InstrumentSymbol, ScoreView, Song, TrackIndex, TrackStaves, ViewTrack};

use super::GuitarScoreApp;
use super::icons::{self, Icon};
use super::instrument;
use super::score_view::ScoreTracks;
use super::style;

/// The dialog's size.
const SIZE: egui::Vec2 = vec2(700.0, 470.0);
/// The width of the list of views.
const LIST_WIDTH: f32 = 190.0;
/// The width of a track's name in its row.
const NAME_WIDTH: f32 = 210.0;

/// The open dialog.
#[derive(Debug, Default)]
pub(super) struct ScoreViewsDialog {
    /// Where the window is; `None` until it is centered on its first frame.
    pos: Option<egui::Pos2>,
    /// The entry being edited.
    selected: Entry,
    /// The name as typed, and the view it belongs to.
    name: String,
    name_of: Option<usize>,
    /// Why the last change could not be made.
    error: Option<String>,
}

/// An entry of the list: Guitar Pro's view of all tracks, or one of the song's views.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Entry {
    #[default]
    All,
    View(usize),
}

impl Entry {
    fn shown(self) -> ScoreTracks {
        match self {
            Entry::All => ScoreTracks::All,
            Entry::View(index) => ScoreTracks::View(index),
        }
    }
}

/// One track's row.
struct Row {
    track: usize,
    name: String,
    symbol: InstrumentSymbol,
    /// In the entry (for all tracks: shown in Guitar Pro's multitrack view).
    on: bool,
    /// The staves it shows there.
    staves: TrackStaves,
}

/// What the user did this frame.
enum Edit {
    Select(Entry),
    New,
    Duplicate,
    Delete,
    Rename(String),
    Toggle(usize, bool),
    Staves(usize, TrackStaves),
    Move(usize, isize),
    HideEmpty(bool),
}

impl GuitarScoreApp {
    /// Opens (or focuses) the score views dialog on the view the score shows.
    pub fn open_score_views(&mut self) {
        let selected = match self.tabs.get(self.active).map(|tab| tab.score) {
            Some(ScoreTracks::View(index)) => Entry::View(index),
            _ => Entry::All,
        };
        let dialog = self
            .score_views
            .get_or_insert_with(ScoreViewsDialog::default);
        dialog.selected = selected;
        dialog.name_of = None;
    }

    /// The score views dialog, if it is open.
    pub(super) fn show_score_views(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.score_views.take() else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let song = tab.document.song();
        let views = song.score_views();
        // A view removed meanwhile (an undo): back to all tracks.
        if let Entry::View(index) = dialog.selected
            && index >= views.len()
        {
            dialog.selected = Entry::All;
        }
        if let Entry::View(index) = dialog.selected
            && dialog.name_of != Some(index)
            && let Some(view) = views.get(index)
        {
            dialog.name.clone_from(&view.name);
            dialog.name_of = Some(index);
        }
        let rows = rows(song, dialog.selected);
        let hide_empty = match dialog.selected {
            Entry::All => self.view.hide_empty_staves,
            Entry::View(index) => views.get(index).is_some_and(|view| view.hide_empty_staves),
        };
        let pos = *dialog
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, SIZE));
        let mut edits: Vec<Edit> = Vec::new();
        let mut close = false;
        egui::Window::new("Score views")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .min_width(SIZE.x)
            .max_width(SIZE.x)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, "Score views", &mut dialog.pos) {
                    close = true;
                }
                close |= style::dialog_body(ui, Margin::symmetric(14, 10), |ui| {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(LIST_WIDTH);
                            view_list(ui, &views, dialog.selected, &mut edits);
                        });
                        ui.separator();
                        ui.vertical(|ui| {
                            entry_editor(ui, &mut dialog, &rows, hide_empty, &mut edits);
                        });
                    });
                });
            });
        if close {
            return;
        }
        for edit in edits {
            self.apply_view_edit(&mut dialog, edit);
        }
        self.score_views = Some(dialog);
    }

    /// Makes one change of the dialog.
    fn apply_view_edit(&mut self, dialog: &mut ScoreViewsDialog, edit: Edit) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let song = tab.document.song();
        let views = song.score_views();
        let current = match dialog.selected {
            Entry::View(index) => views.get(index).cloned().map(|view| (index, view)),
            Entry::All => None,
        };
        let command = match edit {
            Edit::Select(entry) => {
                dialog.selected = entry;
                dialog.error = None;
                self.choose_score_tracks(entry.shown());
                return;
            }
            Edit::New | Edit::Duplicate => {
                let view = match (&edit, &current) {
                    (Edit::Duplicate, Some((_, view))) => ScoreView {
                        name: unique_name(&views, &format!("{} copy", view.name)),
                        ..view.clone()
                    },
                    // A new view starts as what the score shows now.
                    _ => {
                        let shown = tab.shown_score(false);
                        ScoreView {
                            name: unique_name(&views, "View"),
                            tracks: shown
                                .tracks
                                .iter()
                                .map(|&track| {
                                    let at = TrackIndex::new(track);
                                    let staves = shown
                                        .staves
                                        .iter()
                                        .find(|(index, _)| *index == at)
                                        .map_or_else(
                                            || {
                                                song.track(at)
                                                    .map(|t| t.staves())
                                                    .unwrap_or_default()
                                            },
                                            |(_, staves)| *staves,
                                        );
                                    ViewTrack { track: at, staves }
                                })
                                .collect(),
                            hide_empty_staves: shown
                                .hide_empty_staves
                                .unwrap_or(self.view.hide_empty_staves),
                        }
                    }
                };
                let index = views.len();
                if self.run_view_command(dialog, set_view(index, view)) {
                    dialog.selected = Entry::View(index);
                    dialog.name_of = None;
                    self.choose_score_tracks(ScoreTracks::View(index));
                }
                return;
            }
            Edit::Delete => {
                let Some((index, _)) = current else {
                    return;
                };
                if self.run_view_command(dialog, Command::DeleteScoreView { view: index }) {
                    dialog.selected = Entry::All;
                    dialog.name_of = None;
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.score = match tab.score {
                            ScoreTracks::View(shown) if shown == index => ScoreTracks::All,
                            ScoreTracks::View(shown) if shown > index => {
                                ScoreTracks::View(shown.saturating_sub(1))
                            }
                            other => other,
                        };
                    }
                }
                return;
            }
            Edit::HideEmpty(on) => match current {
                None => {
                    self.view.hide_empty_staves = on;
                    return;
                }
                Some((index, view)) => set_view(
                    index,
                    ScoreView {
                        hide_empty_staves: on,
                        ..view
                    },
                ),
            },
            Edit::Rename(name) => match current {
                Some((index, view)) if name.trim() != view.name => {
                    set_view(index, ScoreView { name, ..view })
                }
                _ => return,
            },
            Edit::Toggle(track, on) => match current {
                None => Command::SetTrackVisible {
                    track: TrackIndex::new(track),
                    visible: on,
                },
                Some((index, view)) => set_view(index, with_track(song, view, track, on)),
            },
            Edit::Staves(track, staves) => match current {
                None => Command::SetTrackStaves {
                    track: TrackIndex::new(track),
                    staves,
                },
                Some((index, mut view)) => {
                    for entry in &mut view.tracks {
                        if entry.track.get() == track {
                            entry.staves = staves;
                        }
                    }
                    set_view(index, view)
                }
            },
            Edit::Move(track, step) => {
                let Some((index, mut view)) = current else {
                    return;
                };
                let Some(from) = view.tracks.iter().position(|e| e.track.get() == track) else {
                    return;
                };
                let Some(to) = from
                    .checked_add_signed(step)
                    .filter(|&to| to < view.tracks.len())
                else {
                    return;
                };
                view.tracks.swap(from, to);
                set_view(index, view)
            }
        };
        let _ = self.run_view_command(dialog, command);
    }

    /// Runs `command`, keeping why it failed for the dialog to show.
    fn run_view_command(&mut self, dialog: &mut ScoreViewsDialog, command: Command) -> bool {
        match self.run_command_from_window(command) {
            Ok(()) => {
                dialog.error = None;
                true
            }
            Err(error) => {
                dialog.error = Some(error);
                false
            }
        }
    }
}

impl GuitarScoreApp {
    /// The song grid's eye on `track`: shows it in the score's current view or leaves it
    /// out. With all tracks it is Guitar Pro's eye (saved in the file); in a score view it
    /// adds or removes the track; with one track it shows that track.
    pub(super) fn toggle_track_shown(&mut self, track: usize) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let song = tab.document.song();
        let at = TrackIndex::new(track);
        let command = match tab.score {
            ScoreTracks::Cursor => {
                let from = tab.document.editor().cursor();
                let to = super::score_view::cursor_in_track(song, from, track, true);
                tab.track = track;
                tab.document.editor_mut().set_cursor(to);
                return;
            }
            ScoreTracks::All => Command::SetTrackVisible {
                track: at,
                visible: !song.track(at).is_some_and(|view| view.is_visible()),
            },
            ScoreTracks::View(index) => {
                let Some(view) = song.score_views().into_iter().nth(index) else {
                    return;
                };
                let on = !view.tracks.iter().any(|entry| entry.track == at);
                set_view(index, with_track(song, view, track, on))
            }
        };
        if let Err(error) = self.run_command_from_window(command) {
            self.notice = Some(format!("The track's eye could not change: {error}"));
        }
    }
}

/// `view` with `track` in it (`on`, in its song place among the view's tracks, with its
/// own staves) or left out.
fn with_track(song: &Song, mut view: ScoreView, track: usize, on: bool) -> ScoreView {
    let at = TrackIndex::new(track);
    if on {
        if view.tracks.iter().any(|entry| entry.track == at) {
            return view;
        }
        let staves = song.track(at).map(|t| t.staves()).unwrap_or_default();
        let place = view
            .tracks
            .iter()
            .position(|entry| entry.track > at)
            .unwrap_or(view.tracks.len());
        view.tracks.insert(place, ViewTrack { track: at, staves });
    } else {
        view.tracks.retain(|entry| entry.track != at);
    }
    view
}

/// The command that sets view `index` to `view`.
fn set_view(index: usize, view: ScoreView) -> Command {
    Command::SetScoreView {
        view: index,
        name: view.name,
        tracks: view.tracks,
        hide_empty_staves: view.hide_empty_staves,
    }
}

/// `base`, or `base 2`, `base 3`, ...: the first no view has.
fn unique_name(views: &[ScoreView], base: &str) -> String {
    let taken = |name: &str| {
        views
            .iter()
            .any(|view| view.name.eq_ignore_ascii_case(name))
    };
    if !taken(base) && base != "View" {
        return base.to_owned();
    }
    (1_usize..)
        .map(|number| format!("{base} {number}"))
        .find(|name| !taken(name))
        .unwrap_or_else(|| base.to_owned())
}

/// The rows for `entry`: a view's tracks in its order, then the others in song order; for
/// all tracks, every track in song order.
fn rows(song: &Song, entry: Entry) -> Vec<Row> {
    let row = |track: usize, on: bool, staves: Option<TrackStaves>| -> Option<Row> {
        let at = TrackIndex::new(track);
        let view = song.track(at)?;
        let name = view.name().trim();
        let number = track.saturating_add(1);
        Some(Row {
            track,
            name: if name.is_empty() {
                format!("Track {number}")
            } else {
                format!("{number}. {name}")
            },
            symbol: instrument::derived(&view, song.track_sound(at)),
            on,
            staves: staves.unwrap_or_else(|| view.staves()),
        })
    };
    match entry {
        Entry::All => song
            .tracks()
            .filter_map(|track| row(track.index().get(), track.is_visible(), None))
            .collect(),
        Entry::View(index) => {
            let view = song.score_views().into_iter().nth(index);
            let tracks = view.map(|view| view.tracks).unwrap_or_default();
            let mut rows: Vec<Row> = tracks
                .iter()
                .filter_map(|entry| row(entry.track.get(), true, Some(entry.staves)))
                .collect();
            rows.extend(
                (0..song.track_count())
                    .filter(|&track| !tracks.iter().any(|entry| entry.track.get() == track))
                    .filter_map(|track| row(track, false, None)),
            );
            rows
        }
    }
}

/// The list of views and the buttons that add, copy and delete them.
fn view_list(ui: &mut egui::Ui, views: &[ScoreView], selected: Entry, edits: &mut Vec<Edit>) {
    ui.label(RichText::new("Views").strong());
    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .id_salt("score_views_list")
        .max_height(SIZE.y - 130.0)
        .show(ui, |ui| {
            if ui
                .selectable_label(selected == Entry::All, "All tracks")
                .on_hover_text("Every track Guitar Pro shows together; shared with Guitar Pro")
                .clicked()
            {
                edits.push(Edit::Select(Entry::All));
            }
            for (index, view) in views.iter().enumerate() {
                if ui
                    .selectable_label(selected == Entry::View(index), &view.name)
                    .clicked()
                {
                    edits.push(Edit::Select(Entry::View(index)));
                }
            }
        });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if icons::button(
            ui,
            Icon::Add,
            "New view",
            "New view: starts as the score shows now",
        )
        .clicked()
        {
            edits.push(Edit::New);
        }
        let is_view = matches!(selected, Entry::View(_));
        if ui
            .add_enabled_ui(is_view, |ui| {
                icons::button(ui, Icon::Copy, "Duplicate view", "Duplicate this view")
            })
            .inner
            .clicked()
        {
            edits.push(Edit::Duplicate);
        }
        if ui
            .add_enabled_ui(is_view, |ui| {
                icons::button(ui, Icon::Delete, "Delete view", "Delete this view")
            })
            .inner
            .clicked()
        {
            edits.push(Edit::Delete);
        }
    });
}

/// The selected entry: its name, its tracks and whether empty staves are left out.
fn entry_editor(
    ui: &mut egui::Ui,
    dialog: &mut ScoreViewsDialog,
    rows: &[Row],
    hide_empty: bool,
    edits: &mut Vec<Edit>,
) {
    let is_view = matches!(dialog.selected, Entry::View(_));
    if is_view {
        ui.horizontal(|ui| {
            let label = ui.label("Name");
            let response = ui
                .add(
                    egui::TextEdit::singleline(&mut dialog.name)
                        .desired_width(260.0)
                        .char_limit(gscore_model::MAX_VIEW_NAME_CHARS),
                )
                .labelled_by(label.id);
            if response.lost_focus() {
                edits.push(Edit::Rename(dialog.name.clone()));
            }
        });
    } else {
        ui.label(RichText::new("All tracks").strong());
        ui.label(
            RichText::new(
                "Shared with Guitar Pro: a track switched off here is hidden in its \
                 multitrack view too. The staves are each track's own.",
            )
            .weak(),
        );
    }
    ui.add_space(6.0);
    let shown = rows.iter().filter(|row| row.on).count();
    egui::ScrollArea::vertical()
        .id_salt("score_views_tracks")
        .max_height(SIZE.y - 170.0)
        .show(ui, |ui| {
            for (position, row) in rows.iter().enumerate() {
                track_row(ui, row, position, shown, is_view, edits);
            }
        });
    ui.add_space(6.0);
    let mut hide = hide_empty;
    if ui
        .checkbox(&mut hide, "Hide empty staves")
        .on_hover_text(
            "Leave a track out of a line where it plays nothing (the first line shows every \
             track)",
        )
        .changed()
    {
        edits.push(Edit::HideEmpty(hide));
    }
    if let Some(error) = &dialog.error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
}

/// One track: whether it is in, its instrument and name, its staves, and its place.
fn track_row(
    ui: &mut egui::Ui,
    row: &Row,
    position: usize,
    shown: usize,
    is_view: bool,
    edits: &mut Vec<Edit>,
) {
    ui.horizontal(|ui| {
        let (icon, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
        let ink = if row.on {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        instrument::paint(ui.painter(), icon.shrink(1.0), row.symbol, ink);
        let mut on = row.on;
        // The last track shown stays: a score shows at least one.
        let last = row.on && shown <= 1;
        let response = ui
            .allocate_ui_with_layout(
                vec2(NAME_WIDTH, 18.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(NAME_WIDTH);
                    ui.add_enabled(!last, egui::Checkbox::new(&mut on, row.name.as_str()))
                },
            )
            .inner
            .on_disabled_hover_text("A score shows at least one track");
        if response.changed() {
            edits.push(Edit::Toggle(row.track, on));
        }
        ui.add_enabled_ui(row.on, |ui| {
            let combo = egui::ComboBox::from_id_salt(("score_view_staves", row.track))
                .width(150.0)
                .selected_text(row.staves.label())
                .show_ui(ui, |ui| {
                    for staves in TrackStaves::ALL {
                        if ui
                            .selectable_label(staves == row.staves, staves.label())
                            .clicked()
                        {
                            edits.push(Edit::Staves(row.track, staves));
                        }
                    }
                });
            ui.ctx().accesskit_node_builder(combo.response.id, |node| {
                node.set_label(format!("Staves of {}", row.name));
            });
        });
        if is_view && row.on {
            let up = position > 0;
            let down = position.saturating_add(1) < shown;
            if ui
                .add_enabled_ui(up, |ui| {
                    icons::button(ui, Icon::MoveUp, &format!("Move {} up", row.name), "Up")
                })
                .inner
                .clicked()
            {
                edits.push(Edit::Move(row.track, -1));
            }
            if ui
                .add_enabled_ui(down, |ui| {
                    icons::button(
                        ui,
                        Icon::MoveDown,
                        &format!("Move {} down", row.name),
                        "Down",
                    )
                })
                .inner
                .clicked()
            {
                edits.push(Edit::Move(row.track, 1));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(names: &[&str]) -> Vec<ScoreView> {
        names
            .iter()
            .map(|name| ScoreView {
                name: (*name).to_owned(),
                tracks: Vec::new(),
                hide_empty_staves: false,
            })
            .collect()
    }

    #[test]
    fn new_views_get_a_name_no_view_has() {
        assert_eq!(unique_name(&[], "View"), "View 1");
        assert_eq!(unique_name(&named(&["View 1", "view 2"]), "View"), "View 3");
        assert_eq!(unique_name(&named(&["Drums"]), "Drums copy"), "Drums copy");
        assert_eq!(
            unique_name(&named(&["Drums copy"]), "Drums copy"),
            "Drums copy 1"
        );
    }
}
