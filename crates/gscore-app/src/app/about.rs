//! The About window: the icon, the version, the build it came from and the license.

use egui::{Margin, RichText};

use super::GuitarScoreApp;
use super::style;

/// The About window's size; it opens centered and is not resizable.
const ABOUT_SIZE: egui::Vec2 = egui::vec2(440.0, 340.0);

/// How large the icon is drawn beside the text, in points.
const ICON_SIZE: f32 = 96.0;

/// The open About window.
#[derive(Default)]
pub(super) struct AboutWindow {
    /// Where the window is; `None` until it is centered on its first frame.
    pos: Option<egui::Pos2>,
    /// The icon's texture, made on the first frame the window shows; `None` if the
    /// embedded PNG could not be read (the window then shows the text alone).
    icon: Option<Option<egui::TextureHandle>>,
}

impl std::fmt::Debug for AboutWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AboutWindow")
            .field("pos", &self.pos)
            .finish_non_exhaustive()
    }
}

/// The app icon as a texture, or `None` if the embedded PNG cannot be read.
fn icon_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    #[cfg(not(target_arch = "wasm32"))]
    let icon = Some(eframe::icon_data::from_png_bytes(crate::APP_ICON).ok()?);
    // eframe's PNG icon reader is native only; the browser needs no window icon.
    #[cfg(target_arch = "wasm32")]
    let icon: Option<egui::IconData> = None;
    let icon = icon?;
    let size = [
        usize::try_from(icon.width).ok()?,
        usize::try_from(icon.height).ok()?,
    ];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, &icon.rgba);
    Some(ctx.load_texture("app-icon", image, egui::TextureOptions::LINEAR))
}

impl GuitarScoreApp {
    /// Opens (or focuses) the About window.
    pub(super) fn open_about(&mut self) {
        self.about.get_or_insert_with(AboutWindow::default);
    }

    /// The About window, if it is open.
    pub(super) fn show_about(&mut self, ctx: &egui::Context) {
        let Some(mut about) = self.about.take() else {
            return;
        };
        if about.pos.is_none() {
            about.pos = Some(style::centered_window_pos(ctx, ABOUT_SIZE));
        }
        let pos = about
            .pos
            .unwrap_or_else(|| style::centered_window_pos(ctx, ABOUT_SIZE));
        let icon = about.icon.get_or_insert_with(|| icon_texture(ctx)).clone();
        let soundfont = self.playback.soundfont_name();
        let mut close = false;
        egui::Window::new("About GuitarScore")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .default_size(ABOUT_SIZE)
            .current_pos(pos)
            .frame(style::window_frame(ctx))
            .show(ctx, |ui| {
                if style::window_title_bar(ui, "About GuitarScore", &mut about.pos) {
                    close = true;
                }
                close |= style::dialog_body(ui, Margin::symmetric(18, 14), |ui| {
                    ui.horizontal_top(|ui| {
                        if let Some(icon) = &icon {
                            ui.add(
                                egui::Image::new((icon.id(), egui::vec2(ICON_SIZE, ICON_SIZE)))
                                    .alt_text("GuitarScore icon"),
                            );
                            ui.add_space(14.0);
                        }
                        ui.vertical(|ui| about_text(ui, soundfont.as_deref()));
                    });
                });
            });
        if !close {
            self.about = Some(about);
        }
    }
}

/// The name, version, build, links, license and (in the browser) the bundled SoundFont.
fn about_text(ui: &mut egui::Ui, soundfont: Option<&str>) {
    ui.heading("GuitarScore");
    ui.label(RichText::new(format!("Version {}", gscore_version::VERSION)).weak());
    ui.label(RichText::new(format!("Build {}", gscore_version::SHA)).weak());
    ui.add_space(8.0);
    ui.label("A fast, keyboard-first guitar tablature editor.");
    ui.add_space(8.0);
    ui.hyperlink_to(
        "github.com/Gerrit91/guitarscore",
        "https://github.com/Gerrit91/guitarscore",
    );
    ui.label(RichText::new("MIT license").weak());
    // The bundled font's license; a font a site brings itself is only named.
    #[cfg(target_arch = "wasm32")]
    if let Some(name) = soundfont {
        let text = if name.starts_with("GeneralUser GS") {
            format!("{name} — GeneralUser GS License v2.0")
        } else {
            format!("SoundFont: {name}")
        };
        ui.label(RichText::new(text).weak());
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = soundfont;
    ui.add_space(10.0);
    ui.hyperlink_to(
        "Third-party licenses",
        "https://github.com/Gerrit91/guitarscore/blob/main/THIRD_PARTY.md",
    );
}
