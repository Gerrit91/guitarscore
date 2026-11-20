//! Registers the bundled fonts with egui: Noto Sans for text (the UI and fret numbers),
//! Noto Sans Bold as its own family (egui has no font weights), and Bravura for music
//! glyphs. The fonts are compiled into the binary (see `THIRD_PARTY.md`).

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// Family name for bold text.
pub(crate) const BOLD: &str = "Noto Sans Bold";
/// Family name for SMuFL music glyphs.
pub(crate) const MUSIC: &str = "Bravura";

/// The bundled fonts on top of egui's defaults: Noto Sans first for proportional text
/// (egui's own fonts stay as fallbacks, e.g. for emoji).
pub(crate) fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    // The same files the PDF export embeds, taken from there: compiled in twice, they made
    // the browser download 1.2 MiB (gzip) larger.
    let bundled = gscore_layout::PdfFonts::bundled();
    for (name, bytes) in [
        ("Noto Sans", bundled.text),
        (BOLD, bundled.bold),
        (MUSIC, bundled.music),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    if let Some(proportional) = fonts.families.get_mut(&FontFamily::Proportional) {
        proportional.insert(0, "Noto Sans".to_owned());
    }
    let fallbacks = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut bold = vec![BOLD.to_owned()];
    bold.extend(fallbacks);
    fonts.families.insert(FontFamily::Name(BOLD.into()), bold);
    fonts
        .families
        .insert(FontFamily::Name(MUSIC.into()), vec![MUSIC.to_owned()]);
    fonts
}

/// Installs the bundled fonts in `ctx`.
pub(crate) fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}
