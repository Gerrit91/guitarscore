//! The "New song" dialog: asks for a title and a starting tempo before the song exists,
//! then creates a one-track score in a new tab. The fields start as "New Song" and 120 BPM,
//! so the song can be created without typing.

use egui::Margin;
use gscore_model::MAX_TEMPO;

use super::GuitarScoreApp;
use super::style;

/// The dialog's size; it opens centered.
const DIALOG_SIZE: egui::Vec2 = egui::vec2(380.0, 210.0);

/// The open new-song dialog.
#[derive(Debug)]
pub(super) struct NewSongDialog {
    title: String,
    tempo: u32,
    /// Where the window is; `None` until it is centered on its first frame.
    pos: Option<egui::Pos2>,
    /// Whether the title field has been focused yet.
    focused: bool,
}

impl Default for NewSongDialog {
    fn default() -> Self {
        NewSongDialog {
            title: "New Song".to_owned(),
            tempo: 120,
            pos: None,
            focused: false,
        }
    }
}

impl GuitarScoreApp {
    /// The new-song dialog, if it is open.
    pub(super) fn show_new_song_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.new_song_dialog.take() else {
            return;
        };
        let pos = *dialog
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, DIALOG_SIZE));
        let mut close = false;
        let mut create = false;
        egui::Window::new("New song")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .default_size(DIALOG_SIZE)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, "New song", &mut dialog.pos) {
                    close = true;
                }
                let escape = style::dialog_body(ui, Margin::symmetric(16, 12), |ui| {
                    ui.label(egui::RichText::new("Title").weak());
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut dialog.title)
                            .hint_text("Title")
                            .desired_width(f32::INFINITY),
                    );
                    ui.ctx().accesskit_node_builder(response.id, |node| {
                        node.set_label("New song title");
                    });
                    if !dialog.focused {
                        response.request_focus();
                        dialog.focused = true;
                    }
                    if response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        create = true;
                    }

                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Tempo").weak());
                    let response = ui.add(
                        egui::DragValue::new(&mut dialog.tempo)
                            .range(1..=MAX_TEMPO)
                            .suffix(" BPM"),
                    );
                    ui.ctx()
                        .accesskit_node_builder(response.id, |node| node.set_label("Tempo"));
                    if response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        create = true;
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            create = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
                close |= escape;
            });
        if create {
            let title = dialog.title.trim().to_owned();
            let tempo = dialog.tempo;
            self.create_new_song(title, tempo);
            return;
        }
        if close {
            self.new_song_dialog = None;
            return;
        }
        self.new_song_dialog = Some(dialog);
    }
}
