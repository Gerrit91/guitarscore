//! Registers the bundled fonts with egui: Noto Sans for text (the UI and fret numbers),
//! Noto Sans Bold as its own family (egui has no font weights), and the music fonts for
//! score glyphs. Every music font this build bundles gets its own family (named after it),
//! so the score can switch between them and the settings can show them side by side. The
//! fonts are compiled into the binary (see `THIRD_PARTY.md`).

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};
use gscore_layout::MusicFontChoice;

/// Family name for bold text.
pub(crate) const BOLD: &str = "Noto Sans Bold";
/// Family name of the music font the interface's own glyphs (palette tiles, clef pickers)
/// use; the score follows the chosen font.
pub(crate) const MUSIC: &str = "Bravura";

/// The egui family of `music`.
pub(crate) fn music_family(music: MusicFontChoice) -> FontFamily {
    FontFamily::Name(music.name().into())
}

/// The bundled fonts on top of egui's defaults: Noto Sans first for proportional text
/// (egui's own fonts stay as fallbacks, e.g. for emoji), and a family per music font.
pub(crate) fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
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
    // Every music font the build bundles, under its own name. Bravura stays the interface's
    // family (`MUSIC`) as well.
    for &choice in MusicFontChoice::available() {
        let name = choice.name();
        fonts.font_data.insert(
            name.to_owned(),
            Arc::new(FontData::from_static(choice.file())),
        );
        fonts
            .families
            .insert(FontFamily::Name(name.into()), vec![name.to_owned()]);
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
}

/// Installs the bundled fonts in `ctx`.
pub(crate) fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}
