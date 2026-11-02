//! The app's look: spacing, rounding and text sizes, applied to egui's light and dark
//! styles once. Colors still come from egui's theme (and `gscore-layout`'s palette on the
//! page), so both themes stay consistent.

use egui::{FontFamily, FontId, TextStyle, vec2};

/// Applies GuitarScore's spacing and typography to both of egui's styles. Colors are not
/// touched: `set_theme` switches between the light and dark ones.
pub(crate) fn install(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.spacing.button_padding = vec2(8.0, 4.0);
        style.spacing.menu_margin = 6.0.into();
        style.spacing.indent = 16.0;
        style.visuals.widgets.inactive.corner_radius = 4.into();
        style.visuals.widgets.hovered.corner_radius = 4.into();
        style.visuals.widgets.active.corner_radius = 4.into();
        style.visuals.widgets.open.corner_radius = 4.into();
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(18.0, FontFamily::Proportional),
        );
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        );
    });
}
