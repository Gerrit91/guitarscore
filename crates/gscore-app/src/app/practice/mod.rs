//! The practice view for phones and tablets (M25): the score fitted to the width and
//! moved by swiping, a slim header with the song and the shown track, and touch-sized
//! controls at the bottom for playing along: play, the loop, the speed and the speed
//! trainer, the count-in and the metronome. No side panels, no edit palette, no menus
//! meant for a mouse.
//!
//! A tap on a bar starts playback there; a second tap on a bar loops the bars between
//! the two. The view is chosen by screen width in the browser (Settings: `auto`), or by
//! hand from its own menu and from View in the full interface.

mod controls;
mod drawer;
mod header;
mod widgets;

use egui::{FontFamily, FontId, RichText, vec2};

use super::GuitarScoreApp;
use super::style;
use super::tab::TabId;
use crate::ScoreView;
use crate::state::MeasureSelection;

/// Below this screen width (in points, the browser's CSS pixels) the browser build shows
/// the practice view on its own: phones in both orientations and tablets held upright.
pub(crate) const NARROW: f32 = 900.0;
/// The smallest touch target, in points (Apple's and Google's guidelines agree on about
/// this size).
pub(crate) const TARGET: f32 = 44.0;
/// The play button: as large as the others, marked by its colour and round shape.
pub(super) const PLAY: f32 = TARGET;
/// One step of the speed buttons.
pub(super) const SPEED_STEP: f32 = 0.05;

/// Whether the practice view is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PracticeChoice {
    /// In the browser, on a screen narrower than [`NARROW`].
    Auto,
    /// Always.
    On,
    /// Never: the full interface.
    Off,
}

impl PracticeChoice {
    /// The choice a settings value names; anything unknown is [`PracticeChoice::Auto`].
    pub fn parse(text: &str) -> Self {
        match text {
            "on" => PracticeChoice::On,
            "off" => PracticeChoice::Off,
            _ => PracticeChoice::Auto,
        }
    }

    /// The settings value.
    pub fn id(self) -> &'static str {
        match self {
            PracticeChoice::Auto => "auto",
            PracticeChoice::On => "on",
            PracticeChoice::Off => "off",
        }
    }

    /// Whether the practice view shows on a screen `width` points wide; `browser` is
    /// whether this is the browser build (the desktop never switches by itself).
    pub fn shows(self, width: f32, browser: bool) -> bool {
        match self {
            PracticeChoice::On => true,
            PracticeChoice::Off => false,
            PracticeChoice::Auto => browser && width.is_finite() && width < NARROW,
        }
    }
}

/// What a tap on a bar does, given the bar tapped before it (if it waits for a second
/// tap).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tap {
    /// Play from this bar (and wait for a second tap).
    StartAt(usize),
    /// Loop these bars (first and last, inclusive).
    Loop(usize, usize),
}

/// What a tap on `measure` does after `pending`, the bar tapped first.
pub(crate) fn tap(pending: Option<usize>, measure: usize) -> Tap {
    match pending {
        Some(first) => Tap::Loop(first.min(measure), first.max(measure)),
        None => Tap::StartAt(measure),
    }
}

/// The practice view's state between frames.
#[derive(Debug, Default)]
pub(super) struct PracticeState {
    /// Whether the practice view is shown this frame.
    pub(super) shown: bool,
    /// The bar tapped first, waiting for a second tap to make a loop: its tab and measure.
    pending: Option<(TabId, usize)>,
    /// Whether a bar was tapped yet: until then a hint says what a tap does.
    tapped: bool,
    /// Whether the bottom bar is pulled up to show more (transposition, mute and solo,
    /// count-in length, score size, tuner, settings, saving).
    pub(super) drawer: bool,
}

impl GuitarScoreApp {
    /// Whether this frame shows the practice view.
    pub(super) fn practice_view_shown(&self, ctx: &egui::Context) -> bool {
        PracticeChoice::parse(&self.settings.practice_view)
            .shows(ctx.content_rect().width(), cfg!(target_arch = "wasm32"))
    }

    /// Chooses the practice view (`On`), the full interface (`Off`) or the screen width
    /// (`Auto`), and remembers it.
    pub fn set_practice_view(&mut self, choice: PracticeChoice) {
        self.settings.practice_view = choice.id().to_owned();
        self.practice.pending = None;
    }

    /// Forgets the bar waiting for a second tap (the loop was switched off).
    pub(super) fn practice_forget_tap(&mut self) {
        self.practice.pending = None;
    }

    /// Switches between the practice view and the full interface (F2).
    pub fn toggle_practice_view(&mut self) {
        let choice = if self.practice.shown {
            PracticeChoice::Off
        } else {
            PracticeChoice::On
        };
        self.set_practice_view(choice);
    }

    /// The bar waiting for a second tap, in the active tab.
    fn practice_pending(&self) -> Option<usize> {
        let id = self.tabs.get(self.active)?.id;
        self.practice
            .pending
            .and_then(|(tab, measure)| (tab == id).then_some(measure))
    }

    /// A tap on bar `measure` (0-based) of the practice view: the first plays from there,
    /// the second loops the bars from the first to this one.
    pub(super) fn practice_tap(&mut self, measure: usize) {
        self.practice.tapped = true;
        let pending = self.practice_pending();
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let id = tab.id;
        let stopped = self.playback.state() != gscore_play::PlayState::Playing
            && !self.playback.count_in_pending();
        match tap(pending, measure) {
            Tap::StartAt(measure) => {
                // A new start ends the loop there was.
                tab.selection = None;
                tab.document.editor_mut().set_selection(None);
                self.playback.set_loop_selection(false);
                self.goto_measure(measure, false);
                self.practice.pending = Some((id, measure));
                if stopped {
                    self.play_pause();
                }
            }
            Tap::Loop(first, last) => {
                self.practice.pending = None;
                self.select_measures(MeasureSelection::new(first).to(last));
                self.set_loop_selection(true);
                if stopped {
                    self.play_pause();
                }
            }
        }
    }

    /// The practice view: the header, the controls and the score between them.
    pub(super) fn practice_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let has_song = self.document().is_some();
        // Without a song there is nothing to play: the header only, with the menu.
        let shape = if has_song {
            controls::Shape::of(ctx.content_rect().size())
        } else {
            controls::Shape::Narrow
        };
        if shape.has_header() {
            egui::Panel::top("practice_header")
                .resizable(false)
                .frame(style::chrome_frame(&ctx, 6))
                .show(ui, |ui| self.practice_header(ui, shape, has_song));
        }
        if self.notice.is_some() {
            egui::Panel::top("notice").show(ui, |ui| self.notice_bar(ui));
        }
        if has_song && shape.has_bottom() {
            egui::Panel::bottom("practice_controls")
                .resizable(false)
                .frame(style::chrome_frame(&ctx, 8))
                .show(ui, |ui| self.practice_controls(ui, shape));
        }
        egui::CentralPanel::default()
            .frame(style::canvas_frame(&ctx))
            .show(ui, |ui| self.practice_page(ui));
    }

    /// The score as the practice view shows it: one endless surface as wide as the
    /// screen, at the practice view's own zoom (a pinch changes only that), with the
    /// design mode's handles left out.
    fn practice_page(&mut self, ui: &mut egui::Ui) {
        if self.tabs.get(self.active).is_none() && self.tasks.open_progress().is_none() {
            self.practice_start(ui);
            return;
        }
        let kept = (
            self.view.score_view,
            self.view.zoom(),
            self.view.design_mode,
        );
        self.view.score_view = ScoreView::ScreenVertical;
        self.view.set_zoom(self.settings.practice_zoom);
        self.view.design_mode = false;
        let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.page_view(ui)));
        self.settings.practice_zoom = self.view.zoom();
        self.view.score_view = kept.0;
        self.view.set_zoom(kept.1);
        self.view.design_mode = kept.2;
        // Until the first tap, a hint says what a tap does.
        let hint = !self.practice.tapped
            && self.practice_pending().is_none()
            && !self.playback.loop_selection();
        if hint {
            practice_hint(ui);
        }
        if drawn.is_err() {
            self.notice =
                Some("Drawing the page failed unexpectedly; a crash report was written".to_owned());
        }
    }

    /// No song open: the recent songs as large rows (the browser keeps copies of them, so
    /// they open without the file picker, also offline), and the way to open another.
    fn practice_start(&mut self, ui: &mut egui::Ui) {
        let tokens = style::tokens(ui);
        let ctx = ui.ctx().clone();
        let recent = self.settings.recent_files.clone();
        let mut open = None;
        let mut pick = false;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(16, 16))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 10.0;
                        ui.label(
                            RichText::new("Recent songs")
                                .font(FontId::new(
                                    18.0,
                                    FontFamily::Name(crate::fonts::BOLD.into()),
                                ))
                                .color(tokens.text),
                        );
                        if recent.is_empty() {
                            ui.label(
                                RichText::new(
                                    "Songs you open are kept on this device, so they open again \
                                 here without looking for the file.",
                                )
                                .color(tokens.text_weak),
                            );
                        }
                        for path in &recent {
                            let name = path.file_name().map_or_else(
                                || path.display().to_string(),
                                |name| name.to_string_lossy().into_owned(),
                            );
                            let row = ui.add_sized(
                                vec2(ui.available_width(), 52.0),
                                egui::Button::new(RichText::new(&name).size(15.0))
                                    .fill(tokens.panel)
                                    .corner_radius(8.0)
                                    .truncate(),
                            );
                            if row.clicked() {
                                open = Some(path.clone());
                            }
                        }
                        let button = ui.add_sized(
                            vec2(ui.available_width(), 48.0),
                            egui::Button::new(RichText::new("Open a song…").size(15.0))
                                .corner_radius(8.0),
                        );
                        if button.clicked() {
                            pick = true;
                        }
                    });
            });
        if let Some(path) = open {
            self.open(path, &ctx);
        }
        if pick {
            self.ask_open(&ctx);
        }
    }
}

/// "Tap a bar …" in a dark pill near the bottom of the score.
fn practice_hint(ui: &egui::Ui) {
    let area = ui.max_rect();
    let text = "Tap a bar to start there · tap another to loop";
    let font = FontId::proportional(13.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, egui::Color32::WHITE);
    let size = galley.size() + vec2(28.0, 16.0);
    if size.x > area.width() {
        return;
    }
    let rect = egui::Rect::from_center_size(
        egui::pos2(area.center().x, area.bottom() - 14.0 - size.y / 2.0),
        size,
    );
    let painter = ui.painter();
    painter.rect_filled(rect, size.y / 2.0, egui::Color32::from_black_alpha(220));
    painter.galley(
        rect.center() - galley.size() / 2.0,
        galley,
        egui::Color32::WHITE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_shows_by_width_only_in_the_browser_unless_chosen() {
        assert!(PracticeChoice::Auto.shows(390.0, true));
        assert!(PracticeChoice::Auto.shows(844.0, true));
        assert!(!PracticeChoice::Auto.shows(1180.0, true));
        assert!(!PracticeChoice::Auto.shows(390.0, false));
        assert!(!PracticeChoice::Auto.shows(f32::NAN, true));
        assert!(PracticeChoice::On.shows(1920.0, false));
        assert!(!PracticeChoice::Off.shows(390.0, true));
        for choice in [
            PracticeChoice::Auto,
            PracticeChoice::On,
            PracticeChoice::Off,
        ] {
            assert_eq!(PracticeChoice::parse(choice.id()), choice);
        }
        assert_eq!(PracticeChoice::parse("sideways"), PracticeChoice::Auto);
    }

    #[test]
    fn a_first_tap_starts_and_a_second_loops_in_either_order() {
        assert_eq!(tap(None, 4), Tap::StartAt(4));
        assert_eq!(tap(Some(2), 5), Tap::Loop(2, 5));
        assert_eq!(tap(Some(5), 2), Tap::Loop(2, 5));
        assert_eq!(tap(Some(3), 3), Tap::Loop(3, 3));
    }
}
