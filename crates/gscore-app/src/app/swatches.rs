//! The editor's preset colors and the swatch grid that offers them: the same colors for
//! track colors (track panel and new-track dialog) and for section markers.

use egui::{Color32, Sense, Stroke, vec2};
use gscore_model::TrackColor;

/// The preset colors: names (for tooltips and screen readers) and `rrggbb`.
pub(crate) const PRESETS: [(&str, u32); 17] = [
    ("Red", 0xE5_3935),
    ("Orange", 0xFB_8C00),
    ("Yellow", 0xFD_D835),
    ("Lime", 0x7C_B342),
    ("Green", 0x43_A047),
    ("Teal", 0x00_897B),
    ("Cyan", 0x00_ACC1),
    ("Sky", 0x03_9BE5),
    ("Blue", 0x1E_88E5),
    ("Indigo", 0x39_49AB),
    ("Purple", 0x8E_24AA),
    ("Pink", 0xD8_1B60),
    ("Brown", 0x6D_4C41),
    ("Grey", 0x75_7575),
    ("Light grey", 0xB0_B0B0),
    ("Slate", 0x54_6E7A),
    ("Black", 0x21_2121),
];

/// Draws the preset swatches in a wrapping row. The chosen color is outlined; a click on
/// another one returns that color (clicking the chosen one changes nothing).
pub(crate) fn presets(ui: &mut egui::Ui, current: TrackColor) -> Option<TrackColor> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for (name, rgb) in PRESETS {
            let preset = TrackColor::new(rgb);
            let (rect, response) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_label(format!("Color {name}"));
                node.set_role(egui::Role::Button);
            });
            let (red, green, blue) = preset.parts();
            ui.painter()
                .rect_filled(rect, 3.0, Color32::from_rgb(red, green, blue));
            // A thin edge keeps dark presets visible in the dark theme (and light ones in
            // the light theme).
            let edge = Stroke::new(1.0, ui.visuals().weak_text_color());
            ui.painter()
                .rect_stroke(rect, 3.0, edge, egui::StrokeKind::Inside);
            let chosen = preset == current;
            if chosen || response.hovered() {
                let stroke = Stroke::new(2.0, ui.visuals().strong_text_color());
                ui.painter()
                    .rect_stroke(rect.expand(1.5), 4.0, stroke, egui::StrokeKind::Outside);
            }
            if response.on_hover_text(name).clicked() && !chosen {
                picked = Some(preset);
            }
        }
    });
    picked
}
