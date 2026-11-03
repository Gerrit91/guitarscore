//! The About window: the version, the build it came from and the license.

use egui::{Margin, RichText};

use super::GuitarScoreApp;
use super::style;

/// The About window's size; it opens centered and is not resizable.
const ABOUT_SIZE: egui::Vec2 = egui::vec2(440.0, 340.0);

/// The open About window; nothing to remember but where it sits.
#[derive(Debug, Default)]
pub(super) struct AboutWindow {
    /// Where the window is; `None` until it is centered on its first frame.
    pos: Option<egui::Pos2>,
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
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(18, 14))
                    .show(ui, |ui| {
                        ui.heading("GuitarScore");
                        ui.label(
                            RichText::new(format!("Version {}", gscore_version::VERSION)).weak(),
                        );
                        ui.label(RichText::new(format!("Build {}", gscore_version::SHA)).weak());
                        ui.add_space(8.0);
                        ui.label("A fast, keyboard-first guitar tablature editor.");
                        ui.add_space(8.0);
                        ui.hyperlink_to(
                            "github.com/Gerrit91/guitarscore",
                            "https://github.com/Gerrit91/guitarscore",
                        );
                        ui.label(RichText::new("MIT license").weak());
                        ui.add_space(10.0);
                        ui.hyperlink_to(
                            "Third-party licenses",
                            "https://github.com/Gerrit91/guitarscore/blob/main/THIRD_PARTY.md",
                        );
                    });
            });
        if !close {
            self.about = Some(about);
        }
    }
}
