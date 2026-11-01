//! The application: menus, toolbar, the page view and the status bar. It holds state and
//! delegates: opening files to [`Tasks`], layout to [`Document`], painting to the canvas.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use egui::{Align, Layout as UiLayout, ScrollArea, Sense};
use gscore_layout::{Fonts, LayoutOptions};

use crate::canvas;
use crate::document::Document;
use crate::playback::Playback;
use crate::state::{ThemeChoice, ViewState};
use crate::tasks::{Event, Tasks};

/// Space around the page inside the view, in points.
const PAGE_MARGIN: f32 = 24.0;

/// The GuitarScore application (a read-only viewer for now).
#[derive(Debug)]
pub struct GuitarScoreApp {
    view: ViewState,
    fonts: Result<Fonts, String>,
    options: LayoutOptions,
    document: Option<Document>,
    tasks: Tasks,
    /// A message for the user (an error); shown until dismissed.
    notice: Option<String>,
    /// The theme applied to egui's own widgets, to apply a change once.
    applied_theme: Option<ThemeChoice>,
    /// The window title last sent, to send a change once.
    applied_title: String,
    playback: Playback,
}

impl GuitarScoreApp {
    /// A new app; opens `file` in the background if given.
    pub fn new(ctx: &egui::Context, file: Option<PathBuf>) -> Self {
        crate::fonts::install(ctx);
        let mut app = GuitarScoreApp {
            view: ViewState::default(),
            fonts: Fonts::bundled().map_err(|e| e.to_string()),
            options: LayoutOptions::default(),
            document: None,
            tasks: Tasks::default(),
            notice: None,
            applied_theme: None,
            applied_title: String::new(),
            playback: Playback::default(),
        };
        if let Some(path) = file {
            app.open(path, ctx);
        }
        app
    }

    /// Opens `path` in the background.
    pub fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        if let Err(error) = self.tasks.open(path, ctx) {
            self.notice = Some(error);
        }
    }

    /// Whether a file is being opened (or the file dialog is showing).
    pub fn is_busy(&self) -> bool {
        self.tasks.busy()
    }

    /// The open document.
    pub fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }

    /// The current message for the user, if any.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Playback (MIDI output).
    pub fn playback(&self) -> &Playback {
        &self.playback
    }

    /// Playback, to change it.
    pub fn playback_mut(&mut self) -> &mut Playback {
        &mut self.playback
    }

    /// Starts or pauses playback of the open song.
    pub fn play_pause(&mut self) {
        let Some(document) = &self.document else {
            return;
        };
        if let Err(error) = self.playback.play_pause(document.song()) {
            self.notice = Some(error);
        }
    }

    /// The playback controls: play/pause, stop, the current measure and the output port.
    fn playback_controls(&mut self, ui: &mut egui::Ui) {
        let playing = self.playback.state() == gscore_play::PlayState::Playing;
        let has_song = self.document.is_some();
        let label = if playing { "Pause" } else { "Play" };
        if ui.add_enabled(has_song, egui::Button::new(label)).clicked() {
            self.play_pause();
        }
        if ui
            .add_enabled(has_song, egui::Button::new("Stop"))
            .clicked()
            && let Err(error) = self.playback.stop()
        {
            self.notice = Some(error);
        }
        if let Some(measure) = self.playback.current_measure() {
            ui.label(format!("Measure {measure}"));
        }
        let shown = self
            .playback
            .connected_port()
            .or(self.playback.port())
            .unwrap_or("Automatic")
            .to_owned();
        let mut chosen: Option<Option<String>> = None;
        let combo = egui::ComboBox::from_label("MIDI out")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(self.playback.port().is_none(), "Automatic")
                    .clicked()
                {
                    chosen = Some(None);
                }
                for port in self.playback.ports() {
                    let selected = self.playback.port() == Some(port.as_str());
                    if ui.selectable_label(selected, port).clicked() {
                        chosen = Some(Some(port.clone()));
                    }
                }
            });
        // List the ports each time the menu opens, so newly started synths show up.
        if combo.response.clicked()
            && let Err(error) = self.playback.refresh_ports()
        {
            self.notice = Some(error);
        }
        if let Some(port) = chosen {
            self.playback.choose_port(port);
        }
        if let Some(error) = self.playback.new_error() {
            self.notice = Some(error);
        }
        if playing {
            // Keep the measure display moving.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    /// The view state (theme, zoom, track).
    pub fn view(&self) -> &ViewState {
        &self.view
    }

    /// The view state, to change it.
    pub fn view_mut(&mut self) -> &mut ViewState {
        &mut self.view
    }

    /// Handles finished background jobs.
    fn handle_events(&mut self, ctx: &egui::Context) {
        for event in self.tasks.poll() {
            match event {
                Event::Picked(Some(path)) => self.open(path, ctx),
                Event::Picked(None) => {}
                Event::Opened { path, result } => match result {
                    Ok(song) => {
                        self.playback.song_changed();
                        self.document = Some(Document::new(path, *song));
                        self.view.track = 0;
                        self.notice = None;
                    }
                    Err(error) => self.notice = Some(error),
                },
                Event::Failed(error) => self.notice = Some(error),
            }
        }
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme != Some(self.view.theme) {
            // `set_theme` (not `set_visuals`) switches egui's own widgets too; the change
            // shows on the next frame, so ask for one.
            ctx.set_theme(match self.view.theme {
                ThemeChoice::Light => egui::Theme::Light,
                ThemeChoice::Dark => egui::Theme::Dark,
            });
            ctx.request_repaint();
            self.applied_theme = Some(self.view.theme);
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open…").clicked()
                    && let Err(error) = self.tasks.pick_file(ui.ctx())
                {
                    self.notice = Some(error);
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("View", |ui| {
                ui.radio_value(&mut self.view.theme, ThemeChoice::Light, "Light theme");
                ui.radio_value(&mut self.view.theme, ThemeChoice::Dark, "Dark theme");
                ui.separator();
                if ui.button("Zoom in").clicked() {
                    self.view.zoom_in();
                }
                if ui.button("Zoom out").clicked() {
                    self.view.zoom_out();
                }
                if ui.button("Actual size").clicked() {
                    self.view.set_zoom(1.0);
                }
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(document) = &self.document {
                let song = document.song();
                let name = |index: usize| {
                    song.track(gscore_model::TrackIndex::new(index))
                        .map_or_else(String::new, |t| format!("{}: {}", index + 1, t.name()))
                };
                egui::ComboBox::from_label("Track")
                    .selected_text(name(self.view.track))
                    .show_ui(ui, |ui| {
                        for index in 0..song.track_count() {
                            ui.selectable_value(&mut self.view.track, index, name(index));
                        }
                    });
                ui.separator();
            }
            if ui.button("−").on_hover_text("Zoom out").clicked() {
                self.view.zoom_out();
            }
            ui.label(format!("{:.0} %", self.view.zoom() * 100.0));
            if ui.button("+").on_hover_text("Zoom in").clicked() {
                self.view.zoom_in();
            }
            ui.separator();
            self.playback_controls(ui);
            if self.tasks.busy() {
                ui.spinner();
            }
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| match &self.document {
            Some(document) => {
                // The file name only (screenshots stay the same wherever the file is); the
                // full path on hover.
                let path = document.path();
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                ui.label(name).on_hover_text(path.display().to_string());
                ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                    let song = document.song();
                    ui.label(format!(
                        "{}, {}",
                        count(song.measure_count(), "measure"),
                        count(song.track_count(), "track")
                    ));
                });
            }
            None => {
                ui.label("No file open");
            }
        });
    }

    fn notice_bar(&mut self, ui: &mut egui::Ui) {
        let Some(notice) = self.notice.clone() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.colored_label(ui.visuals().error_fg_color, notice);
            if ui.small_button("Dismiss").clicked() {
                self.notice = None;
            }
        });
    }

    /// The page, scrollable, centered horizontally.
    fn page_view(&mut self, ui: &mut egui::Ui) {
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
        let layout = match document.layout(self.view.track, &self.options, fonts) {
            Ok(layout) => layout,
            Err(error) => {
                ui.label(error);
                return;
            }
        };
        let zoom = self.view.zoom();
        let theme = self.view.theme.palette();
        ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            let page = canvas::page_size(&layout, zoom);
            let available = ui.available_width();
            let size = egui::vec2(
                (page.x + 2.0 * PAGE_MARGIN).max(available),
                page.y + 2.0 * PAGE_MARGIN,
            );
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let origin = egui::pos2(rect.center().x - page.x / 2.0, rect.top() + PAGE_MARGIN);
            let painter = ui.painter_at(rect);
            canvas::paint(&painter, origin, &layout, theme, zoom, ui.clip_rect());
        });
    }
}

/// "1 track", "3 tracks".
fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

impl eframe::App for GuitarScoreApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_events(&ctx);
        self.apply_theme(&ctx);
        let title = self.document.as_ref().map_or_else(
            || "GuitarScore".to_owned(),
            |d| format!("{} — GuitarScore", d.display_title()),
        );
        if title != self.applied_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.applied_title = title;
        }

        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        if self.notice.is_some() {
            egui::Panel::top("notice").show(ui, |ui| self.notice_bar(ui));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            // A bug in drawing must not take the app down: contain it and tell the user.
            let drawn = catch_unwind(AssertUnwindSafe(|| self.page_view(ui)));
            if drawn.is_err() {
                self.notice = Some(
                    "Drawing the page failed unexpectedly; a crash report was written".to_owned(),
                );
            }
        });
    }
}
