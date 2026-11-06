//! The lyrics dialog: choose one of the five lines, where it starts and its syllables.
//! What is typed shows in the score at once (one undo step per line); Cancel takes it back.

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
    /// The line, start and text the score shows, to see what changed since.
    shown: (u8, usize, String),
    /// The undo depth before the first change shown, to take them all back on Cancel.
    undo_start: Option<usize>,
}

impl LyricsEditor {
    /// A dialog on the first line, starting at `measure` (0-based) when it has no text.
    pub(super) fn new(lines: Vec<LyricLine>, measure: usize, measure_count: usize) -> Self {
        let measure = measure.min(measure_count.saturating_sub(1));
        let line = lines.first().map_or(0, |line| line.line);
        let found = lines.iter().find(|entry| entry.line == line);
        let start = found.map_or(measure, |entry| entry.start.get());
        let text = found.map_or_else(String::new, |entry| entry.text.clone());
        LyricsEditor {
            pos: None,
            line,
            shown: (line, start, text.clone()),
            start,
            text,
            lines,
            measure_count,
            undo_start: None,
        }
    }

    /// Whether the line, its start or its text changed since the score last showed them;
    /// they count as shown from now on.
    pub(super) fn take_change(&mut self) -> bool {
        let now = (self.line, self.start, self.text().to_owned());
        if now == self.shown {
            return false;
        }
        // Choosing another line only loads it: nothing to show.
        let loaded = now.0 != self.shown.0;
        self.shown = now;
        !loaded
    }

    /// The undo depth before the first change was shown, if one was.
    pub(super) fn undo_start(&self) -> Option<usize> {
        self.undo_start
    }

    /// Remembers the undo depth before the first shown change.
    pub(super) fn note_undo_start(&mut self, depth: usize) {
        self.undo_start.get_or_insert(depth);
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
        let size = egui::vec2(400.0, 250.0);
        // Low in the window, over the track grid, so the score above shows what is typed;
        // the title bar moves it.
        let pos = *self.pos.get_or_insert_with(|| {
            let screen = ctx.content_rect();
            egui::pos2(
                screen.center().x - size.x / 2.0,
                (screen.bottom() - size.y - 48.0).max(screen.top()),
            )
        });
        let mut outcome = Outcome::Open;
        egui::Window::new("Lyrics")
            .title_bar(false)
            .collapsible(false)
            .resizable(true)
            .default_size(size)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, "Lyrics", &mut self.pos) {
                    outcome = Outcome::Cancel;
                }
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
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
                                // The line typed so far is in the score already: keep it to come
                                // back to.
                                self.lines.retain(|entry| entry.line != self.line);
                                if !self.text().is_empty() {
                                    self.lines.push(LyricLine {
                                        line: self.line,
                                        start: gscore_model::MeasureIndex::new(self.start),
                                        text: self.text().to_owned(),
                                    });
                                }
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
                            let response = ui.add(
                                egui::DragValue::new(&mut start)
                                    .range(1..=self.measure_count.max(1)),
                            );
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
                                "Syllables are separated by spaces; Tab skips a beat; a + puts two \
                         words on one beat. Text in [brackets] is hidden.",
                            )
                            .weak(),
                        );
                        ui.add_space(2.0);
                        let response = ui.add(
                            egui::TextEdit::multiline(&mut self.text)
                                .hint_text("Hel lo world")
                                // Tab is typed into the text (it skips a beat), not a focus move.
                                .lock_focus(true)
                                .desired_rows(3)
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
