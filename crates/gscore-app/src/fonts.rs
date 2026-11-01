//! Registers the bundled fonts with egui: Noto Sans for text (the UI and fret numbers),
//! Noto Sans Bold as its own family (egui has no font weights), and Bravura for music
//! glyphs. The fonts are compiled into the binary (see `THIRD_PARTY.md`).

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// Family name for bold text.
pub(crate) const BOLD: &str = "Noto Sans Bold";
/// Family name for SMuFL music glyphs.
pub(crate) const MUSIC: &str = "Bravura";

const NOTO_SANS: &[u8] = include_bytes!("../../../assets/fonts/noto-sans/NotoSans-Regular.ttf");
const NOTO_SANS_BOLD: &[u8] = include_bytes!("../../../assets/fonts/noto-sans/NotoSans-Bold.ttf");
const BRAVURA: &[u8] = include_bytes!("../../../assets/fonts/bravura/Bravura.otf");

/// The bundled fonts on top of egui's defaults: Noto Sans first for proportional text
/// (egui's own fonts stay as fallbacks, e.g. for emoji).
pub(crate) fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        ("Noto Sans", NOTO_SANS),
        (BOLD, NOTO_SANS_BOLD),
        (MUSIC, BRAVURA),
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
