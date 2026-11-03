//! The larger staff dialogs: tempo (with a swinging metronome and tap tempo), time
//! signature (engraved, with steppers and presets), key signature (a circle of fifths),
//! the section marker with a preview, and the alternative endings.

use egui::{RichText, vec2};
use gscore_model::{MAX_SECTION_TITLE_CHARS, MAX_TEMPO};

use super::circle_of_fifths;
use super::edit_staff::StaffState;
use super::edit_staff_dialogs::hint;
use super::metronome;
use super::staff_widgets::stepper;
use super::style;
use super::tile_faces::music_glyph;

/// The tempo: the metronome, the value as "♩ = 120", its name, presets and tap tempo.
pub(super) fn tempo(ui: &mut egui::Ui, state: &mut StaffState) {
    ui.horizontal(|ui| {
        metronome::metronome(ui, state.tempo, vec2(110.0, 150.0), &mut state.swinging);
        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(24.0, 40.0), egui::Sense::hover());
                music_glyph(
                    ui.painter(),
                    rect.center() + vec2(0.0, 10.0),
                    "\u{ECA5}",
                    34.0,
                    style::tokens(ui).text,
                );
                ui.label(RichText::new("=").size(26.0).strong());
                // The value as large as the note beside it.
                ui.style_mut().text_styles.insert(
                    egui::TextStyle::Button,
                    egui::FontId::new(24.0, egui::FontFamily::Name(crate::fonts::BOLD.into())),
                );
                let response = ui.add(
                    egui::DragValue::new(&mut state.tempo)
                        .range(1..=MAX_TEMPO)
                        .speed(0.5),
                );
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_label("Tempo value"));
            });
            ui.label(
                RichText::new(metronome::tempo_name(state.tempo))
                    .size(20.0)
                    .italics()
                    .strong(),
            );
            ui.add_space(4.0);
            ui.spacing_mut().slider_width = 170.0;
            let mut slider = f64::from(state.tempo.clamp(30, 300));
            let response = ui.add(
                egui::Slider::new(&mut slider, 30.0..=300.0)
                    .show_value(false)
                    .logarithmic(true),
            );
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_label("Tempo slider"));
            if response.changed() {
                state.tempo = slider.round() as u32;
            }
            ui.add_space(4.0);
            let tap = ui.add(egui::Button::new("Tap tempo").min_size(vec2(120.0, 26.0)));
            if tap.clicked() {
                let now = ui.input(|input| input.time);
                if let Some(tempo) = state.tap(now) {
                    state.tempo = tempo.clamp(1, MAX_TEMPO);
                }
            }
            tap.on_hover_text("Click on each beat; the tempo follows the last few clicks");
        });
    });
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for (name, preset) in metronome::TEMPO_NAMES {
            let on = metronome::tempo_name(state.tempo) == name;
            if ui
                .add(egui::Button::new(RichText::new(name).italics().small()).selected(on))
                .on_hover_text(format!("{preset} BPM"))
                .clicked()
            {
                state.tempo = preset;
            }
        }
    });
    hint(ui, "The song keeps this tempo until another change.");
}

/// Common time signatures offered as presets.
const TIME_PRESETS: [(u8, u16); 8] = [
    (2, 4),
    (3, 4),
    (4, 4),
    (5, 4),
    (6, 8),
    (7, 8),
    (9, 8),
    (12, 8),
];

/// Bravura's time-signature digits for `value`.
fn time_digits(value: u32) -> String {
    value
        .to_string()
        .chars()
        .filter_map(|ch| ch.to_digit(10).and_then(|d| char::from_u32(0xE080 + d)))
        .collect()
}

/// The time signature: engraved numerator over denominator with steppers, and presets.
pub(super) fn time(ui: &mut egui::Ui, state: &mut StaffState) {
    const DENOMINATORS: [u16; 8] = [1, 2, 4, 8, 16, 32, 64, 128];
    let ink = style::tokens(ui).text;
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        ui.vertical(|ui| {
            let (rect, step) = stepper(ui, "beats", 44.0);
            music_glyph(
                ui.painter(),
                rect.center(),
                &time_digits(u32::from(state.numerator)),
                44.0,
                ink,
            );
            state.numerator = (i32::from(state.numerator) + step).clamp(1, 64) as u8;
            let (rect, step) = stepper(ui, "beat value", 44.0);
            music_glyph(
                ui.painter(),
                rect.center(),
                &time_digits(u32::from(state.denominator)),
                44.0,
                ink,
            );
            let index = DENOMINATORS
                .iter()
                .position(|&d| d == state.denominator)
                .unwrap_or(2);
            let index = (index as i32 + step).clamp(0, 7) as usize;
            state.denominator = DENOMINATORS.get(index).copied().unwrap_or(4);
        });
        ui.add_space(16.0);
        ui.vertical(|ui| {
            ui.add_space(6.0);
            let beat = match state.denominator {
                1 => "whole notes",
                2 => "half notes",
                4 => "quarter notes",
                8 => "eighth notes",
                16 => "sixteenth notes",
                _ => "short notes",
            };
            ui.label(RichText::new(format!("{} {beat}", state.numerator)).strong());
            ui.label(RichText::new("per bar").weak());
        });
    });
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for (numerator, denominator) in TIME_PRESETS {
            let on = (state.numerator, state.denominator) == (numerator, denominator);
            let (rect, response) = ui.allocate_exact_size(vec2(34.0, 40.0), egui::Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::Role::Button,
                    true,
                    on,
                    format!("{numerator}/{denominator}"),
                )
            });
            let tokens = style::tokens(ui);
            let fill = if on {
                tokens.accent
            } else if response.hovered() {
                tokens.control_hover
            } else {
                tokens.control
            };
            ui.painter().rect_filled(rect, 4.0, fill);
            let ink = if on { tokens.on_accent } else { tokens.text };
            for (dy, value) in [(-7.5, u32::from(numerator)), (7.5, u32::from(denominator))] {
                music_glyph(
                    ui.painter(),
                    rect.center() + vec2(0.0, dy),
                    &time_digits(value),
                    28.0,
                    ink,
                );
            }
            if response.clicked() {
                state.numerator = numerator;
                state.denominator = denominator;
            }
        }
    });
    hint(
        ui,
        "Holds for the following bars, up to the next bar with its own time signature.",
    );
}

/// The key signature: the circle of fifths, and the other spelling of keys far round it.
pub(super) fn key(ui: &mut egui::Ui, state: &mut StaffState) {
    ui.vertical_centered(|ui| {
        circle_of_fifths::circle(ui, &mut state.key, &mut state.minor, 320.0);
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(circle_of_fifths::display_name(state.key, state.minor))
                .strong()
                .size(15.0),
        );
        if let Some(other) = circle_of_fifths::enharmonic(state.key) {
            let text = format!(
                "Write as {}",
                circle_of_fifths::display_name(other, state.minor)
            );
            if ui.button(text).clicked() {
                state.key = other;
            }
        }
    });
    hint(
        ui,
        "Outer ring: major keys, inner ring: relative minors. Holds up to the next key change.",
    );
}

/// The section marker: its title, color and a preview of the framed mark.
pub(super) fn section(ui: &mut egui::Ui, state: &mut StaffState) {
    ui.horizontal(|ui| {
        let response = ui.color_edit_button_srgb(&mut state.section_color);
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_label("Section color");
        });
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.section_title)
                .hint_text("Section title")
                .desired_width(220.0),
        );
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_label("Section title");
        });
    });
    if state.section_title.chars().count() > MAX_SECTION_TITLE_CHARS {
        state.error = Some(format!(
            "The title is longer than {MAX_SECTION_TITLE_CHARS} characters."
        ));
    }
    let title = state.section_title.trim();
    if !title.is_empty() {
        ui.add_space(8.0);
        let [red, green, blue] = state.section_color;
        let color = egui::Color32::from_rgb(red, green, blue);
        egui::Frame::new()
            .stroke(egui::Stroke::new(1.5, color))
            .inner_margin(egui::Margin::symmetric(8, 3))
            .corner_radius(2)
            .show(ui, |ui| {
                ui.label(RichText::new(title).strong().size(16.0).color(color));
            });
    }
    hint(ui, "Leave the title empty to remove the marker.");
}

/// The alternative endings: one bracket tile per pass.
pub(super) fn endings(ui: &mut egui::Ui, state: &mut StaffState) {
    ui.label("This bar is played on these passes:");
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for number in 1..=4_u8 {
            let bit = 1_u8 << number.saturating_sub(1);
            let on = state.endings & bit != 0;
            let (rect, response) = ui.allocate_exact_size(vec2(56.0, 44.0), egui::Sense::click());
            let label = format!("Ending {number}");
            response
                .widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, &label));
            let tokens = style::tokens(ui);
            let fill = if on {
                tokens.accent
            } else if response.hovered() {
                tokens.control_hover
            } else {
                tokens.control
            };
            let ink = if on { tokens.on_accent } else { tokens.text };
            let painter = ui.painter();
            painter.rect_filled(rect, 5.0, fill);
            let stroke = egui::Stroke::new(1.8, ink);
            let top = rect.top() + 11.0;
            painter.line_segment(
                [
                    egui::pos2(rect.left() + 9.0, top),
                    egui::pos2(rect.right() - 9.0, top),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(rect.left() + 9.0, top),
                    egui::pos2(rect.left() + 9.0, top + 18.0),
                ],
                stroke,
            );
            painter.text(
                egui::pos2(rect.center().x + 3.0, top + 12.0),
                egui::Align2::CENTER_CENTER,
                format!("{number}."),
                egui::FontId::new(15.0, egui::FontFamily::Name(crate::fonts::BOLD.into())),
                ink,
            );
            if response.clicked() {
                state.endings ^= bit;
            }
        }
    });
    hint(ui, "A bar can belong to more than one ending.");
}
