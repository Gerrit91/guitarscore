//! The beat text dialog: the text Guitar Pro writes above a beat, separate from a section
//! title.

use egui::{Margin, Pos2, RichText, vec2};
use gscore_model::MAX_BEAT_TEXT_CHARS;

use super::harmonic_editor::{buttons, keys};
use super::shape_editor::Outcome;
use super::style;

/// Width of the dialog's content.
const WIDTH: f32 = 460.0;

/// The open beat text dialog.
#[derive(Debug, Clone)]
pub(super) struct BeatTextEditor {
    pos: Option<Pos2>,
    text: String,
}

impl BeatTextEditor {
    /// A dialog on the beat's current text.
    pub(super) fn new(current: &str) -> Self {
        BeatTextEditor {
            pos: None,
            text: current.to_owned(),
        }
    }

    /// The text as chosen; an empty text removes the beat's text.
    pub(super) fn text(&self) -> &str {
        self.text.trim_end()
    }

    /// Shows the window.
    pub(super) fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let title = "Beat text";
        let size = vec2(WIDTH + 32.0, 250.0);
        let pos = *self
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut outcome = Outcome::Open;
        egui::Window::new(title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, title, &mut self.pos) {
                    outcome = Outcome::Cancel;
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        ui.label(
                            RichText::new(
                                "Written above the beat, like Guitar Pro's beat text. It is \
                                 not a section marker.",
                            )
                            .weak(),
                        );
                        ui.add_space(4.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.text)
                                .hint_text("Text above the beat")
                                .char_limit(MAX_BEAT_TEXT_CHARS)
                                .desired_width(WIDTH),
                        );
                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);
                        buttons(ui, &mut outcome, "Remove the text", None);
                        keys(ui, &mut outcome);
                    });
            });
        outcome
    }
}
