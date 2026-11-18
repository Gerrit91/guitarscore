//! The "paste bars" dialog: where the copied bars go — before or after the selection, or
//! at a bar typed in — either adding bars or overwriting the ones there.

use egui::{Margin, RichText, vec2};
use gscore_edit::{MAX_PASTE_COPIES, PasteOptions};
use gscore_model::MeasureIndex;

use super::GuitarScoreApp;
use super::style;

/// Where the copied bars go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Place {
    /// Before the selection.
    Before,
    /// After the selection.
    #[default]
    After,
    /// At a bar typed in, before the bar there.
    AtBar,
}

/// The open dialog: the selection the copied bars go around, and the choices made so far.
#[derive(Debug)]
pub(super) struct PasteAround {
    /// The selection's first and last bar (0-based).
    start: usize,
    end: usize,
    /// The song's bars, to bound the bar field.
    measure_count: usize,
    /// How many bars were copied.
    count: usize,
    /// Where the bars go.
    place: Place,
    /// The bar typed in for [`Place::AtBar`] (1-based).
    bar: usize,
    /// Write the bars over the ones at the place instead of adding them.
    overwrite: bool,
    /// How many times the copied bars are pasted.
    copies: usize,
    /// Take the copied bars' time signatures.
    time_signatures: bool,
    /// Take their tempo changes.
    tempo: bool,
    /// Take their section markers.
    sections: bool,
    /// Take their beat texts.
    texts: bool,
    /// Where the window is, remembered while it is open.
    pos: Option<egui::Pos2>,
}

impl GuitarScoreApp {
    /// Opens the dialog for the active tab. Only bars can be placed: with notes or beats
    /// on the clipboard (copied from a selection in the score) the status bar says how to
    /// copy bars instead, rather than pasting them where they came from unnoticed.
    pub(super) fn open_paste_around(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let editor = tab.document.editor();
        let Some(count) = editor.clipboard_bars() else {
            self.status = Some(
                "Paste bars places copied bars, and the clipboard holds notes or beats: \
                 copy bars with Ctrl+Shift+C (every track) or from the song grid first"
                    .to_owned(),
            );
            return;
        };
        let measure_count = editor.song().measure_count();
        let (start, end) = match editor.selection() {
            Some(gscore_edit::Selection::Measures { anchor, focus, .. }) => {
                (anchor.get(), focus.get())
            }
            // Notes or beats copied: the bar under the cursor is the one they go around.
            _ => {
                let measure = editor.cursor().measure.get();
                (measure, measure)
            }
        };
        let (start, end) = (start.min(end), start.max(end));
        self.paste_around = Some(PasteAround {
            start,
            end,
            measure_count,
            count,
            place: Place::After,
            bar: end.saturating_add(2).min(measure_count.max(1)),
            overwrite: false,
            copies: 1,
            time_signatures: true,
            tempo: true,
            sections: true,
            texts: true,
            pos: None,
        });
    }

    /// The dialog, while it is open.
    pub(super) fn show_paste_around(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.paste_around.take() else {
            return;
        };
        let title = "Paste bars";
        let width = 400.0;
        let size = vec2(width + 32.0, 390.0);
        let pos = *dialog
            .pos
            .get_or_insert_with(|| style::centered_window_pos(ctx, size));
        let mut paste = false;
        let mut close = false;
        egui::Window::new(title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, title, &mut dialog.pos) {
                    close = true;
                }
                let escape = style::dialog_body(ui, Margin::symmetric(16, 12), |ui| {
                    ui.set_width(width);
                    let (noun, verb) = if dialog.count == 1 {
                        ("bar", "goes")
                    } else {
                        ("bars", "go")
                    };
                    ui.label(
                        RichText::new(format!(
                            "The copied {noun} {verb} before, after or at a bar; the selection \
                             is bars {} to {}.",
                            dialog.start.saturating_add(1),
                            dialog.end.saturating_add(1),
                        ))
                        .weak(),
                    );
                    ui.add_space(6.0);
                    ui.radio_value(&mut dialog.place, Place::Before, "Before the selection");
                    ui.radio_value(&mut dialog.place, Place::After, "After the selection");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut dialog.place, Place::AtBar, "At bar");
                        let response = ui.add(
                            egui::DragValue::new(&mut dialog.bar)
                                .range(1..=dialog.measure_count.max(1))
                                .speed(0.2),
                        );
                        ui.ctx().accesskit_node_builder(response.id, |node| {
                            node.set_label("Paste bar");
                        });
                    });
                    ui.add_space(6.0);
                    ui.checkbox(
                        &mut dialog.overwrite,
                        "Overwrite the bars there (otherwise the song gets longer)",
                    )
                    .on_hover_text(
                        "Write the copied bars over the bars at that place, in every track; \
                         off, bars are added and the rest of the song moves right",
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Copies");
                        let response = ui.add(
                            egui::DragValue::new(&mut dialog.copies)
                                .range(1..=MAX_PASTE_COPIES)
                                .speed(0.2),
                        );
                        ui.ctx().accesskit_node_builder(response.id, |node| {
                            node.set_label("Paste copies");
                        });
                        ui.label(
                            RichText::new(format!(
                                "({} bars)",
                                dialog.count.saturating_mul(dialog.copies)
                            ))
                            .weak(),
                        );
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new("Include").weak());
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut dialog.time_signatures, "Time signatures");
                        ui.checkbox(&mut dialog.tempo, "Tempo");
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut dialog.sections, "Sections");
                        ui.checkbox(&mut dialog.texts, "Texts");
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let tokens = style::tokens(ui);
                        let ok = egui::Button::new(RichText::new("Paste").color(tokens.on_accent))
                            .fill(tokens.accent)
                            .min_size(vec2(84.0, 28.0));
                        if ui.add(ok).clicked() {
                            paste = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
                close |= escape;
            });
        if paste {
            let at = match dialog.place {
                Place::Before => dialog.start,
                Place::After => dialog.end.saturating_add(1),
                Place::AtBar => dialog.bar.saturating_sub(1),
            };
            let at = MeasureIndex::new(at.min(dialog.measure_count));
            let options = PasteOptions {
                copies: dialog.copies,
                time_signatures: dialog.time_signatures,
                tempo: dialog.tempo,
                sections: dialog.sections,
                texts: dialog.texts,
            };
            let pasted = self.document_mut().map(|document| {
                let editor = document.editor_mut();
                if dialog.overwrite {
                    editor.paste_measures_overwriting_with(at, options)
                } else {
                    editor.paste_measures_inserting_with(at, options)
                }
            });
            match pasted {
                Some(Ok(_)) => self.after_bars_edit(),
                Some(Err(error)) => self.notice = Some(error.to_string()),
                None => {}
            }
        } else if !close {
            self.paste_around = Some(dialog);
        }
    }
}
