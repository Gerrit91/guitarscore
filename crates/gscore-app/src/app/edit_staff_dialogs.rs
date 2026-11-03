//! The fields of the staff dialogs: one small form per tile. Choices show the engraved
//! sign next to what it does and are chosen with a click; the larger pickers (tempo,
//! time and key signature, section, endings) are in `staff_pickers`.

use egui::{RichText, vec2};
use gscore_model::{Feel, Fermata, MAX_BARS_PER_SYSTEM, SystemBreak};

use super::edit_staff::{Dialog, StaffState, Target};
use super::staff_pickers;
use super::staff_widgets::{choice, choice_with_sign, feel_detail, glyph_in, paint_feel};
use super::tile_faces::{Face, paint_face};

/// The window title of a dialog.
pub(super) fn title(dialog: Dialog) -> &'static str {
    match dialog {
        Dialog::Tempo => "Tempo",
        Dialog::Time => "Time signature",
        Dialog::Key => "Key signature",
        Dialog::Section => "Section",
        Dialog::RepeatStart => "Start repeat",
        Dialog::RepeatEnd => "End repeat",
        Dialog::Endings => "Alternative endings",
        Dialog::Feel => "Swing feel",
        Dialog::Fermata => "Fermata",
        Dialog::Barline => "Barline",
        Dialog::Break => "Line break",
        Dialog::Bars => "Bars per line",
    }
}

/// The width of a dialog's content.
pub(super) fn width(dialog: Dialog) -> f32 {
    match dialog {
        Dialog::Key => 340.0,
        Dialog::Feel => 430.0,
        Dialog::Tempo | Dialog::Fermata => 380.0,
        _ => 330.0,
    }
}

/// A small, weak hint line.
pub(super) fn hint(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(text).small().weak());
}

/// Paints a tile face (the palette's sign) larger, into a choice row's sign box.
fn face(face: Face<'static>) -> impl FnOnce(&egui::Painter, egui::Rect, egui::Color32) {
    move |painter, rect, ink| {
        // The tile faces are drawn for a 27 x 32 tile; scale by drawing into a centered
        // tile-sized box (they are crisp vector shapes and glyphs at any size).
        let tile = egui::Rect::from_center_size(rect.center(), vec2(27.0, 32.0));
        paint_face(painter, tile, face, ink);
    }
}

/// Paints music-font glyphs into a choice row's sign box.
fn glyphs(
    text: &'static str,
    size: f32,
    baseline: f32,
) -> impl FnOnce(&egui::Painter, egui::Rect, egui::Color32) {
    move |painter, rect, ink| glyph_in(painter, rect, text, size, baseline, ink)
}

/// The dialog's fields.
pub(super) fn body(ui: &mut egui::Ui, state: &mut StaffState, target: Target) {
    state.error = None;
    ui.spacing_mut().item_spacing.y = 4.0;
    match target.dialog {
        Dialog::Tempo => staff_pickers::tempo(ui, state),
        Dialog::Time => staff_pickers::time(ui, state),
        Dialog::Key => staff_pickers::key(ui, state),
        Dialog::Section => staff_pickers::section(ui, state),
        Dialog::RepeatStart => {
            for (on, name, detail, sign) in [
                (false, "No repeat start", "A plain barline.", "\u{E030}"),
                (
                    true,
                    "Start repeat at this bar",
                    "Playing returns here from the next end repeat.",
                    "\u{E040}",
                ),
            ] {
                if choice(
                    ui,
                    state.repeat_start == on,
                    name,
                    detail,
                    glyphs(sign, 30.0, 15.0),
                )
                .clicked()
                {
                    state.repeat_start = on;
                }
            }
        }
        Dialog::RepeatEnd => {
            for (on, name, detail, sign) in [
                (false, "No repeat end", "A plain barline.", "\u{E030}"),
                (
                    true,
                    "End the repeat at this bar",
                    "Playing jumps back to the start repeat.",
                    "\u{E041}",
                ),
            ] {
                if choice(
                    ui,
                    state.repeat_end == on,
                    name,
                    detail,
                    glyphs(sign, 30.0, 15.0),
                )
                .clicked()
                {
                    state.repeat_end = on;
                }
            }
            ui.add_space(4.0);
            ui.add_enabled_ui(state.repeat_end, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Played again");
                    let response = ui.add(
                        egui::DragValue::new(&mut state.repeat_count)
                            .range(1..=127)
                            .suffix(" ×"),
                    );
                    ui.ctx().accesskit_node_builder(response.id, |node| {
                        node.set_label("Repeat count");
                    });
                });
            });
        }
        Dialog::Endings => staff_pickers::endings(ui, state),
        Dialog::Feel => {
            for feel in Feel::ALL {
                let paint = move |painter: &egui::Painter, rect: egui::Rect, ink| {
                    paint_feel(painter, rect.shrink2(vec2(6.0, 0.0)), feel, 20.0, ink);
                };
                if choice_with_sign(
                    ui,
                    state.feel == feel,
                    feel.label(),
                    feel_detail(feel),
                    120.0,
                    paint,
                )
                .clicked()
                {
                    state.feel = feel;
                }
            }
            hint(
                ui,
                "Holds for the following bars, up to the next bar with its own feel. GP3-5 \
                 files keep only the triplet feels.",
            );
        }
        Dialog::Fermata => {
            if choice(
                ui,
                state.fermata.is_none(),
                "No fermata",
                "The bar is played in time.",
                |_, _, _| {},
            )
            .clicked()
            {
                state.fermata = None;
            }
            for fermata in Fermata::ALL {
                let (sign, detail) = fermata_sign(fermata);
                if choice(
                    ui,
                    state.fermata == Some(fermata),
                    fermata.label(),
                    detail,
                    glyphs(sign, 36.0, 6.0),
                )
                .clicked()
                {
                    state.fermata = Some(fermata);
                }
            }
            hint(
                ui,
                "Shown over the bar's first note; playback does not hold it yet.",
            );
        }
        Dialog::Barline => {
            for (on, name, detail, sign) in [
                (false, "Single barline", "The usual barline.", "\u{E030}"),
                (
                    true,
                    "End this bar with a double barline",
                    "Divides sections of the score.",
                    "\u{E031}",
                ),
            ] {
                if choice(
                    ui,
                    state.double_bar == on,
                    name,
                    detail,
                    glyphs(sign, 30.0, 15.0),
                )
                .clicked()
                {
                    state.double_bar = on;
                }
            }
        }
        Dialog::Break => {
            let first = target.measure.get() == 0;
            if first {
                hint(ui, "The first bar always starts the score.");
            }
            ui.add_enabled_ui(!first, |ui| {
                for (kind, name, detail) in [
                    (SystemBreak::None, "No break", "Bars flow on as they fit."),
                    (
                        SystemBreak::Line,
                        "Start a new line here",
                        "This bar begins the next line.",
                    ),
                    (
                        SystemBreak::Page,
                        "Start a new page here",
                        "This bar begins the next page.",
                    ),
                ] {
                    let paint =
                        move |painter: &egui::Painter, rect: egui::Rect, ink: egui::Color32| {
                            match kind {
                                SystemBreak::None => {}
                                SystemBreak::Line => face(Face::Break)(painter, rect, ink),
                                SystemBreak::Page => paint_page(painter, rect, ink),
                            }
                        };
                    if choice(ui, state.system_break == kind, name, detail, paint).clicked() {
                        state.system_break = kind;
                    }
                }
            });
        }
        Dialog::Bars => {
            if choice(
                ui,
                !state.bars_fixed,
                "As many as fit",
                "Lines fill up to the page width.",
                |_, _, _| {},
            )
            .clicked()
            {
                state.bars_fixed = false;
            }
            if choice(
                ui,
                state.bars_fixed,
                "Fixed number",
                "Every line holds the same number of bars.",
                face(Face::Bars),
            )
            .clicked()
            {
                state.bars_fixed = true;
            }
            ui.add_enabled_ui(state.bars_fixed, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Bars per line:");
                    let response = ui
                        .add(egui::DragValue::new(&mut state.bars).range(1..=MAX_BARS_PER_SYSTEM));
                    ui.ctx().accesskit_node_builder(response.id, |node| {
                        node.set_label("Bars per line value");
                    });
                });
            });
            hint(ui, "This applies to the whole shown track.");
        }
    }
}

/// A fermata's glyph and what it does.
fn fermata_sign(fermata: Fermata) -> (&'static str, &'static str) {
    match fermata {
        Fermata::VeryShort => ("\u{E4C2}", "A brief hold."),
        Fermata::Short => ("\u{E4C4}", "A short hold."),
        Fermata::Medium => ("\u{E4C0}", "The usual fermata: hold about twice as long."),
        Fermata::Long => ("\u{E4C6}", "A long hold."),
        Fermata::VeryLong => ("\u{E4C8}", "A very long hold."),
    }
}

/// A page with a folded corner, for the page break.
fn paint_page(painter: &egui::Painter, rect: egui::Rect, ink: egui::Color32) {
    let page = egui::Rect::from_center_size(rect.center(), vec2(22.0, 28.0));
    let fold = 7.0;
    let stroke = egui::Stroke::new(1.6, ink);
    let points = vec![
        page.left_top(),
        page.right_top() - vec2(fold, 0.0),
        page.right_top() + vec2(0.0, fold),
        page.right_bottom(),
        page.left_bottom(),
    ];
    painter.add(egui::Shape::closed_line(points, stroke));
    painter.line_segment(
        [
            page.right_top() - vec2(fold, 0.0),
            page.right_top() + vec2(-fold, fold),
        ],
        stroke,
    );
    painter.line_segment(
        [
            page.right_top() + vec2(-fold, fold),
            page.right_top() + vec2(0.0, fold),
        ],
        stroke,
    );
}
