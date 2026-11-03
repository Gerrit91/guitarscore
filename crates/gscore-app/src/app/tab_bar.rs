//! The tab strip: one tab per open song, with a dot marking unsaved changes and a close
//! icon inside the tab. Clicking a tab switches to it; clicking its close icon asks before
//! closing a tab with unsaved changes.

use egui::{FontId, Rect, Sense, pos2, vec2};

use super::GuitarScoreApp;
use super::icons::{self, Icon};
use super::style;

/// Height of a tab (the strip adds a little padding around them).
const TAB_HEIGHT: f32 = 26.0;
/// Horizontal padding inside a tab, on each side.
const TAB_PADDING: f32 = 10.0;
/// Space between the title and the close icon.
const TITLE_GAP: f32 = 8.0;
/// The close icon's clickable square, inside the tab.
const CLOSE_SIZE: f32 = 18.0;
/// The unsaved-changes dot, and the gap after it.
const DOT: f32 = 6.0;
const DOT_GAP: f32 = 6.0;
/// Tab corner rounding.
const RADIUS: f32 = 4.0;

impl GuitarScoreApp {
    /// The strip of open tabs, shown below the toolbar between the side panels.
    pub(super) fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let mut switch_to = None;
        let mut close = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for (index, tab) in self.tabs.iter().enumerate() {
                let title = tab.document.display_title();
                let path = tab.document.path().display().to_string();
                let (switch, shut) = tab_widget(
                    ui,
                    index,
                    &title,
                    tab.document.is_dirty(),
                    &path,
                    index == self.active,
                );
                if shut {
                    close = Some(index);
                } else if switch {
                    switch_to = Some(index);
                }
            }
        });
        if let Some(index) = switch_to {
            self.switch_tab(index);
        }
        if let Some(index) = close {
            self.close_tab(index);
        }
    }

    /// Makes `index` the active tab. Playback follows the active tab: the previous song
    /// stops, and the new one's synthesizer starts loading.
    pub(super) fn switch_tab(&mut self, index: usize) {
        if index == self.active || index >= self.tabs.len() {
            return;
        }
        self.active = index;
        self.playback.song_changed();
        self.playback.preload_synth();
        self.status = None;
    }
}

/// One tab: its title, an unsaved-changes dot before it, and a close icon inside on the
/// right. Returns `(switched to it, close clicked)`.
fn tab_widget(
    ui: &mut egui::Ui,
    index: usize,
    title: &str,
    dirty: bool,
    path: &str,
    selected: bool,
) -> (bool, bool) {
    let tokens = style::tokens(ui);
    let galley =
        ui.painter()
            .layout_no_wrap(title.to_owned(), FontId::proportional(12.5), tokens.text);
    let dot_room = if dirty { DOT + DOT_GAP } else { 0.0 };
    let width = TAB_PADDING * 2.0 + dot_room + galley.size().x + TITLE_GAP + CLOSE_SIZE;
    let (rect, response) = ui.allocate_exact_size(vec2(width, TAB_HEIGHT), Sense::click());
    let response = response.on_hover_text(path);

    // The tab's background: raised when selected or hovered, flat otherwise.
    let background = if selected {
        tokens.control
    } else if response.hovered() {
        tokens.control_hover
    } else {
        tokens.panel
    };
    ui.painter().rect_filled(rect, RADIUS, background);

    // The unsaved-changes dot, then the title.
    let mut x = rect.left() + TAB_PADDING;
    if dirty {
        let center = pos2(x + DOT / 2.0, rect.center().y);
        ui.painter().circle_filled(center, DOT / 2.0, tokens.accent);
        x += DOT + DOT_GAP;
    }
    let ink = if selected {
        tokens.text
    } else {
        tokens.text_weak
    };
    ui.painter().galley(
        pos2(x, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );

    // The close icon, inside the tab on the right: its own clickable square.
    let close_center = pos2(
        rect.right() - TAB_PADDING - CLOSE_SIZE / 2.0,
        rect.center().y,
    );
    let close_rect = Rect::from_center_size(close_center, vec2(CLOSE_SIZE, CLOSE_SIZE));
    let close_response = ui
        .interact(
            close_rect,
            ui.scope_id().with(("tab_close", index)),
            Sense::click(),
        )
        .on_hover_text("Close tab");
    let close_hover = close_response.hovered();
    if close_hover {
        // The same red background as the window's close button, not a circle.
        ui.painter().rect_filled(close_rect, 3.0, style::CLOSE_RED);
    }
    let close_ink = if close_hover {
        egui::Color32::WHITE
    } else {
        tokens.text_weak
    };
    icons::paint(
        ui.painter(),
        close_rect.shrink(3.5),
        close_ink,
        Icon::WindowClose,
    );

    // What assistive technology announces, and what tests can find.
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::Button,
            true,
            selected,
            format!("Tab {}: {title}", index.saturating_add(1)),
        )
    });
    close_response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            format!("Close tab {}", index.saturating_add(1)),
        )
    });

    let shut = close_response.clicked();
    let switch = response.clicked() && !shut;
    (switch, shut)
}
