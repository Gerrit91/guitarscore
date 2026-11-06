//! The lyrics dialog: choose one of the five lines, where it starts and its syllables.

use egui::{Pos2, RichText, Ui};
use gscore_model::{LYRIC_LINES, LyricLine, MAX_LYRIC_LINE_CHARS};

use super::shape_editor::Outcome;
use super::style;

/// The open lyrics dialog.
#[derive(Debug, Clone)]
pub(super) struct LyricsEditor {
    pos: Option<Pos2>,
    line: u8,
    start: usize,
    text: String,
    /// The song's lines, to load another line when it is chosen.
    lines: Vec<LyricLine>,
    measure_count: usize,
}

impl LyricsEditor {
    /// A dialog on the first line, starting at `measure` (0-based) when it has no text.
    pub(super) fn new(lines: Vec<LyricLine>, measure: usize, measure_count: usize) -> Self {
        let measure = measure.min(measure_count.saturating_sub(1));
        let line = lines.first().map_or(0, |line| line.line);
        let found = lines.iter().find(|entry| entry.line == line);
        LyricsEditor {
            pos: None,
            line,
            start: found.map_or(measure, |entry| entry.start.get()),
            text: found.map_or_else(String::new, |entry| entry.text.clone()),
            lines,
            measure_count,
        }
    }

    /// The line, its start (0-based) and text as chosen; an empty text removes it.
    pub(super) fn line(&self) -> u8 {
        self.line
    }

    pub(super) fn start(&self) -> usize {
        self.start
    }

    pub(super) fn text(&self) -> &str {
        self.text.trim_end()
    }

    /// Shows the window.
    pub(super) fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let size = egui::vec2(380.0, 300.0);
        let pos = *self
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut outcome = Outcome::Open;
        egui::Window::new("Lyrics")
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size(size)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Lyrics").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("X").clicked() {
                            outcome = Outcome::Cancel;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Line");
                    let mut chosen = self.line;
                    let line_combo = egui::ComboBox::from_id_salt("lyric_line")
                        .selected_text(format!("{}", chosen.saturating_add(1)))
                        .show_ui(ui, |ui| {
                            for line in 0..LYRIC_LINES {
                                ui.selectable_value(
                                    &mut chosen,
                                    line,
                                    format!("{}", line.saturating_add(1)),
                                );
                            }
                        });
                    ui.ctx()
                        .accesskit_node_builder(line_combo.response.id, |node| {
                            node.set_label("Lyric line");
                        });
                    if chosen != self.line {
                        self.line = chosen;
                        let found = self.lines.iter().find(|entry| entry.line == chosen);
                        if let Some(entry) = found {
                            self.start = entry.start.get();
                            self.text = entry.text.clone();
                        } else {
                            self.text.clear();
                        }
                    }
                    ui.label("Starts at measure");
                    let mut start = self.start.saturating_add(1);
                    let response = ui
                        .add(egui::DragValue::new(&mut start).range(1..=self.measure_count.max(1)));
                    ui.ctx().accesskit_node_builder(response.id, |node| {
                        node.set_label("Lyric start measure");
                    });
                    if response.changed() {
                        self.start = start.saturating_sub(1);
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Syllables are separated by spaces; a + joins a word to the next beat. \
                         Text in [brackets] is hidden.",
                    )
                    .weak(),
                );
                ui.add_space(2.0);
                let response = ui.add(
                    egui::TextEdit::multiline(&mut self.text)
                        .hint_text("Hel lo world")
                        .desired_rows(5)
                        .char_limit(MAX_LYRIC_LINE_CHARS),
                );
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label("Lyric text");
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        outcome = Outcome::Apply;
                    }
                    if ui
                        .button("Remove line")
                        .on_hover_text("Remove this lyric line")
                        .clicked()
                    {
                        outcome = Outcome::Remove;
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = Outcome::Cancel;
                    }
                });
                self.keys(ui, &mut outcome);
            });
        outcome
    }

    fn keys(&self, ui: &mut Ui, outcome: &mut Outcome) {
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            *outcome = Outcome::Cancel;
        }
        if ui.input(|input| {
            input.key_pressed(egui::Key::Enter) && (input.modifiers.command || input.modifiers.ctrl)
        }) {
            *outcome = Outcome::Apply;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use gscore_model::MeasureIndex;

    #[test]
    fn the_dialog_loads_the_first_line_and_its_start() {
        let lines = vec![LyricLine {
            line: 0,
            start: MeasureIndex::new(2),
            text: "one two".to_owned(),
        }];
        let editor = LyricsEditor::new(lines, 0, 4);
        assert_eq!(editor.line(), 0);
        assert_eq!(editor.start(), 2);
        assert_eq!(editor.text(), "one two");
    }

    #[test]
    fn a_song_without_lyrics_starts_at_the_cursor() {
        let editor = LyricsEditor::new(Vec::new(), 2, 4);
        assert_eq!(editor.start(), 2);
        assert_eq!(editor.text(), "");
        assert_eq!(editor.line(), 0);
    }
}
