//! The empty workspace (with no song open): a new-song and an open button, the recent songs
//! in a searchable table, and a preview of the chosen song's first page.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{RichText, Sense};
use gscore_layout::{Layout as ScoreLayout, Theme};

use crate::canvas::{self, Arrangement, POINTS_PER_SPACE};

/// Width of the preview of the first page, in points.
const PREVIEW_WIDTH: f32 = 230.0;

/// The empty workspace's state between frames.
#[derive(Debug, Default)]
pub(super) struct RecentFiles {
    /// What the search field holds.
    filter: String,
    /// The row chosen, whose first page the preview shows.
    selected: Option<PathBuf>,
    /// The song whose first page is being prepared (one preview runs at a time).
    running: Option<PathBuf>,
    /// The first page of the chosen song, once it is prepared.
    preview: Option<(PathBuf, Arc<ScoreLayout>)>,
    /// Why a song's preview could not be made.
    error: Option<(PathBuf, String)>,
}

/// What the empty workspace asked the app to do this frame.
#[derive(Debug, Default)]
pub(super) struct RecentActions {
    /// Open the new-song dialog.
    pub(super) new_song: bool,
    /// Show the file dialog.
    pub(super) ask_open: bool,
    /// Open this recent song.
    pub(super) open: Option<PathBuf>,
    /// Prepare the first page of this song (the row chosen, once).
    pub(super) preview: Option<PathBuf>,
}

impl RecentFiles {
    /// The preview task finished (or failed): keep the page for the row it belongs to.
    pub(super) fn previewed(&mut self, path: &Path, result: Result<Arc<ScoreLayout>, String>) {
        if self.running.as_deref() != Some(path) {
            // Another job's answer (it can only be a stale one): there is nothing to keep.
            return;
        }
        self.running = None;
        // The user may have chosen another row while this was being prepared.
        if self.selected.as_deref() != Some(path) {
            return;
        }
        match result {
            Ok(layout) => {
                self.error = None;
                self.preview = Some((path.to_path_buf(), layout));
            }
            Err(error) => {
                self.preview = None;
                self.error = Some((path.to_path_buf(), error));
            }
        }
    }

    /// A preview job could not be started.
    pub(super) fn failed(&mut self, path: PathBuf, error: String) {
        if self.running.as_deref() == Some(path.as_path()) {
            self.running = None;
        }
        self.error = Some((path, error));
    }

    /// A background job failed without saying which song it was for: the preview row stops
    /// waiting (the notice says what happened).
    pub(super) fn stop_waiting(&mut self) {
        self.running = None;
    }

    /// A song opened: the empty workspace's page and choice are not needed until it
    /// returns (a preview still being prepared is dropped when it arrives).
    pub(super) fn forget_preview(&mut self) {
        self.preview = None;
        self.error = None;
        self.selected = None;
        self.running = None;
    }
}

/// The empty workspace: the way to start, the recent songs, and the preview. The caller
/// opens what the returned actions ask for.
pub(super) fn show(
    ui: &mut egui::Ui,
    state: &mut RecentFiles,
    recent: &[PathBuf],
) -> RecentActions {
    let mut actions = RecentActions::default();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let width = ui.available_width().min(920.0);
            let margin = ((ui.available_width() - width) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(margin);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    ui.add_space((ui.available_height() * 0.06).max(12.0));
                    ui.heading("Open a song");
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Start a new score, open a Guitar Pro file, or pick one of your \
                             recent songs. You can also drop a file here.",
                        )
                        .weak(),
                    );
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new(RichText::new("New song…").strong())
                                    .min_size(egui::vec2(130.0, 32.0)),
                            )
                            .on_hover_text("Start an empty score")
                            .clicked()
                        {
                            actions.new_song = true;
                        }
                        if ui
                            .add(egui::Button::new("Open song…").min_size(egui::vec2(130.0, 32.0)))
                            .on_hover_text("Open a Guitar Pro file from disk")
                            .clicked()
                        {
                            actions.ask_open = true;
                        }
                    });
                    ui.add_space(20.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.horizontal_top(|ui| {
                        let preview_width = PREVIEW_WIDTH + 16.0;
                        ui.vertical(|ui| {
                            ui.set_width((width - preview_width).max(280.0));
                            recent_table(ui, state, recent, &mut actions);
                        });
                        ui.add_space(16.0);
                        ui.vertical(|ui| {
                            if preview(ui, state, actions.open.is_some())
                                && let Some(path) = state.selected.clone()
                            {
                                actions.open = Some(path);
                            }
                        });
                    });
                });
            });
        });
    // A chosen row whose page is not prepared asks for it once; a failed one is not asked
    // again (its error stays until another row is chosen). One preview runs at a time.
    if state.running.is_none()
        && let Some(selected) = state.selected.as_deref()
        && state
            .preview
            .as_ref()
            .is_none_or(|(path, _)| path != selected)
        && state
            .error
            .as_ref()
            .is_none_or(|(path, _)| path != selected)
    {
        state.running = Some(selected.to_path_buf());
        actions.preview = Some(selected.to_path_buf());
    }
    actions
}

/// The recent songs, the search field and the way to open the chosen one.
fn recent_table(
    ui: &mut egui::Ui,
    state: &mut RecentFiles,
    recent: &[PathBuf],
    actions: &mut RecentActions,
) {
    ui.label(RichText::new("Recent songs").strong());
    ui.add_space(4.0);
    let search = ui.add(
        egui::TextEdit::singleline(&mut state.filter)
            .hint_text("Search recent songs")
            .desired_width(ui.available_width()),
    );
    ui.ctx()
        .accesskit_node_builder(search.id, |node| node.set_label("Search recent songs"));
    ui.add_space(6.0);
    if recent.is_empty() {
        ui.label(RichText::new("No recent songs yet.").weak());
        return;
    }
    let filter = state.filter.trim().to_lowercase();
    let matching: Vec<&PathBuf> = recent
        .iter()
        .filter(|path| filter.is_empty() || path.to_string_lossy().to_lowercase().contains(&filter))
        .collect();
    if matching.is_empty() {
        ui.label(
            RichText::new(format!(
                "No recent song matches \"{}\".",
                state.filter.trim()
            ))
            .weak(),
        );
        return;
    }
    ui.label(
        RichText::new("Most recent first; double-click a song to open it.")
            .small()
            .weak(),
    );
    ui.add_space(2.0);
    egui::Grid::new("recent_songs_table")
        .num_columns(2)
        .striped(true)
        .min_col_width(180.0)
        .spacing([18.0, 3.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Song").strong());
            ui.label(RichText::new("Folder").strong());
            ui.end_row();
            for path in &matching {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                let folder = path
                    .parent()
                    .and_then(Path::file_name)
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let chosen = state.selected.as_deref() == Some(path.as_path());
                let row = ui
                    .add(egui::Button::selectable(chosen, name))
                    .on_hover_text(path.display().to_string());
                if row.clicked() {
                    state.selected = Some((*path).clone());
                }
                if row.double_clicked() {
                    actions.open = Some((*path).clone());
                }
                ui.label(RichText::new(folder).weak());
                ui.end_row();
            }
        });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let selected = state.selected.clone();
        if ui
            .add_enabled(
                selected.is_some(),
                egui::Button::new("Open selected song").min_size(egui::vec2(0.0, 24.0)),
            )
            .clicked()
            && let Some(path) = selected
        {
            actions.open = Some(path);
        }
        ui.label(RichText::new("or double-click a row").small().weak());
    });
}

/// The chosen song's first page, as it prints, or what waits for it. True when the preview
/// was clicked (to open the song).
fn preview(ui: &mut egui::Ui, state: &RecentFiles, opening: bool) -> bool {
    ui.label(RichText::new("Preview").strong());
    ui.add_space(4.0);
    let Some(selected) = state.selected.as_deref() else {
        placeholder(ui, "Choose a recent song to see its first page.");
        return false;
    };
    if let Some((path, layout)) = &state.preview
        && path == selected
    {
        if let Some(page) = layout.pages.first().copied() {
            let zoom = PREVIEW_WIDTH / (page.width * POINTS_PER_SPACE);
            let height = page.height * POINTS_PER_SPACE * zoom;
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(PREVIEW_WIDTH, height), Sense::click());
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_role(egui::Role::Button);
                node.set_label("Recent song preview");
            });
            let response = response
                .on_hover_text("Click to open this song")
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            canvas::paint(
                &ui.painter_at(rect),
                rect.min,
                layout,
                &Arrangement::new(layout, 1),
                &Theme::LIGHT,
                zoom,
                rect,
                canvas::Surface::Pages,
            );
            ui.label(
                RichText::new(format!("Page 1 of {}", layout.pages.len()))
                    .small()
                    .weak(),
            );
            return response.clicked();
        }
        placeholder(ui, "This song has no pages to show.");
        return false;
    }
    if let Some((path, error)) = &state.error
        && path == selected
    {
        ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
        return false;
    }
    // The page is on its way; the empty workspace is still usable meanwhile.
    placeholder(
        ui,
        if opening {
            "Opening the song…"
        } else {
            "Preparing the preview…"
        },
    );
    ui.spinner();
    false
}

/// An empty page-shaped box with `text` in it, wrapped to the page's width.
fn placeholder(ui: &mut egui::Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(PREVIEW_WIDTH, PREVIEW_WIDTH * 297.0 / 210.0),
        Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, 2.0, ui.visuals().faint_bg_color);
    let galley = ui.painter().layout(
        text.to_owned(),
        egui::FontId::proportional(12.0),
        ui.visuals().weak_text_color(),
        rect.width() - 24.0,
    );
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        galley,
        ui.visuals().weak_text_color(),
    );
}
