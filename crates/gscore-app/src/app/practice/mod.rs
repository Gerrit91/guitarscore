//! The practice view for phones and tablets (M25): the score fitted to the width and
//! moved by swiping, the shown track in a chip floating over it, a touch-sized player,
//! and a drawer of tiles for the rest: the loop, the speed and the speed trainer, the
//! count-in, the metronome and more. No side panels, no edit palette, no menus meant for
//! a mouse.
//!
//! A tap on a bar puts where play starts there, without starting it (in a rehearsal, play
//! is pressed when the band counts in); while playing, playback jumps there. Pressing on
//! a bar and dragging (a finger held still on it first: a quick swipe scrolls) selects
//! the bars it passes and loops them. The view is chosen by screen width in the browser
//! (Settings: `auto`), or by hand from the drawer and from View in the full interface.

mod controls;
mod drawer;
mod header;
mod tiles;
mod widgets;

use egui::{FontFamily, FontId, Rect, RichText, vec2};

use super::GuitarScoreApp;
use super::style;
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

/// How long a finger is held still before its drag selects bars instead of scrolling,
/// in seconds.
pub(super) const LONG_PRESS: f64 = 0.5;
/// How long a swipe's speed carries the score on once the finger is lifted, in seconds.
pub(super) const GLIDE: f32 = 0.25;

/// Whether a drag scrolls the score: a finger's (`touch`) that moved before it was held
/// `held` seconds; a mouse's, and a finger's held still first, select bars instead.
pub(super) fn drag_scrolls(touch: bool, held: f64) -> bool {
    touch && held < LONG_PRESS
}

/// The practice view's state between frames.
#[derive(Debug, Default)]
pub(super) struct PracticeState {
    /// Whether the practice view is shown this frame.
    pub(super) shown: bool,
    /// The bar a drag across bars started on, while it lasts.
    drag_from: Option<usize>,
    /// Whether the drag going on scrolls the score (a finger's swipe) instead of
    /// selecting bars.
    pub(super) drag_scrolls: bool,
    /// How fast the swipe went last, in points per second: the score glides on with it.
    pub(super) fling: egui::Vec2,
    /// Whether a finger held still is being followed across bars (after egui's long
    /// press, which ends the page's own drag).
    pub(super) held: bool,
    /// The tile whose page the drawer shows in place of its tiles, if one.
    page: Option<drawer::Item>,
    /// Whether the drawer is pulled up (the loop, the speed, the count-in, the click, mute
    /// and solo, the transposition, the score's size, the tuner, settings, saving).
    pub(super) drawer: bool,
    /// Whether the mixer shows between the score and the player (the drawer's Mixer tile,
    /// F3); the full interface's own choice of grid or mixer stays as it was.
    pub(super) mixer: bool,
    /// What a window drawn as a sheet may cover this frame (everything but the drawer),
    /// and how much of its bottom the floating drawer hides; `None` without a song.
    pub(super) cover: Option<(Rect, f32)>,
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
        self.practice.drag_from = None;
    }

    /// The rect a window drawn as a sheet covers in the practice view (everything but the
    /// drawer), and how much of its bottom the floating drawer hides; `None` outside the
    /// practice view or without a song.
    pub(super) fn practice_cover(&self) -> Option<(Rect, f32)> {
        self.practice.cover.filter(|_| self.practice.shown)
    }

    /// Zooms the score that is shown (Ctrl+plus, Ctrl+minus, Ctrl+0): the practice view
    /// keeps a size of its own, apart from the full interface's.
    pub(super) fn zoom_score(&mut self, change: impl FnOnce(&mut crate::state::ViewState)) {
        if !self.practice.shown {
            change(&mut self.view);
            return;
        }
        let kept = self.view.zoom();
        self.view.set_zoom(self.settings.practice_zoom);
        change(&mut self.view);
        self.settings.practice_zoom = self.view.zoom();
        self.view.set_zoom(kept);
    }

    /// The rect the Sound chain window covers as a sheet (a phone, or a phone on its side),
    /// and how much of its bottom the floating drawer hides; `None` where it is a window.
    pub(super) fn practice_sheet(&self) -> Option<(Rect, f32)> {
        self.practice_cover()
            .filter(|(cover, _)| super::sound_chain::is_sheet(*cover))
    }

    /// Whether a sheet covers the practice view (the Sound chain or the settings window,
    /// on a phone).
    fn practice_sheet_shown(&self, ctx: &egui::Context) -> bool {
        (self.sound_chain.is_some() && self.practice_sheet().is_some())
            || (self.settings_window.is_some() && self.settings_sheet(ctx).is_some())
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

    /// A tap on bar `measure` (0-based) of the practice view: playback will start there
    /// when play is pressed (in a rehearsal, when the band counts in), or, while playing,
    /// jumps there. A new start ends the loop there was.
    pub(super) fn practice_tap(&mut self, measure: usize) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        tab.selection = None;
        tab.document.editor_mut().set_selection(None);
        self.playback.set_loop_selection(false);
        self.goto_measure(measure, false);
    }

    /// A drag across the practice view's bars, now over bar `at` (0-based; `None` between
    /// bars): from where it `started` it selects the bars it passes, and when it is
    /// `stopped` they loop.
    pub(super) fn practice_drag(&mut self, at: Option<usize>, started: bool, stopped: bool) {
        if started {
            self.practice.drag_from = at;
        }
        if let (Some(from), Some(at)) = (self.practice.drag_from, at) {
            self.select_measures(MeasureSelection::new(from.min(at)).to(from.max(at)));
        }
        if stopped && self.practice.drag_from.take().is_some() {
            self.set_loop_selection(true);
        }
    }

    /// The practice view: the header, the controls and the score between them.
    pub(super) fn practice_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let has_song = self.document().is_some();
        // On the desktop the window has no system bar: its own, with only the title and
        // the window buttons, moves it, maximizes, minimizes and closes it.
        let mut top = ctx.content_rect().top();
        if super::title_bar::WINDOW_CONTROLS {
            top = egui::Panel::top("practice_title_bar")
                .frame(style::chrome_frame(&ctx, 0))
                .exact_size(super::title_bar::TITLE_BAR_HEIGHT)
                .resizable(false)
                .show(ui, |ui| self.title_bar(ui, false))
                .response
                .rect
                .bottom();
        }
        // Without a song there is nothing to play: a header says what the view is for. With
        // one the score reaches the top, the track's chip floating over it.
        if !has_song {
            egui::Panel::top("practice_header")
                .resizable(false)
                .frame(style::chrome_frame(&ctx, 6))
                .show(ui, |ui| self.practice_header(ui));
        }
        if self.notice.is_some() {
            egui::Panel::top("notice").show(ui, |ui| self.notice_bar(ui));
        }
        // From a tablet's width on, the drawer floats over the score's corner, with the
        // player in it; on a phone both are in the bar at the bottom.
        let floating = ctx.content_rect().width() >= drawer::FLOATING;
        let mut bottom = ctx.content_rect().bottom();
        if has_song && !floating {
            bottom = egui::Panel::bottom("practice_controls")
                .resizable(false)
                .frame(style::chrome_frame(&ctx, 8))
                .show(ui, |ui| self.practice_controls(ui))
                .response
                .rect
                .top();
        }
        // A sheet covers everything down to the bar at the bottom, or (under a floating
        // drawer) down to the screen's bottom.
        let mut cover = Rect::from_min_max(
            egui::pos2(ctx.content_rect().left(), top),
            egui::pos2(ctx.content_rect().right(), bottom),
        );
        if has_song && self.practice.mixer {
            bottom = self.practice_mixer(ui, bottom);
        }
        let mut hidden = 0.0;
        if has_song && floating {
            let card = self.practice_drawer_floating(&ctx, bottom);
            hidden = cover.bottom() - card.top();
        }
        cover.max.y = cover.max.y.max(cover.min.y);
        self.practice.cover = has_song.then_some((cover, hidden));
        // The chip would cover a sheet's close button: it waits while one is shown.
        if has_song && !self.practice_sheet_shown(&ctx) {
            self.practice_chip_floating(&ctx, top);
        }
        egui::CentralPanel::default()
            .frame(style::canvas_frame(&ctx))
            .show(ui, |ui| self.practice_page(ui));
    }

    /// The mixer strips in a panel above `bottom` (the bottom bar's top, or the screen's
    /// bottom), as the full interface's mixer view draws them; a strip's sound button opens
    /// the Sound chain window. Returns the panel's top.
    fn practice_mixer(&mut self, ui: &mut egui::Ui, bottom: f32) -> f32 {
        let ctx = ui.ctx().clone();
        // The score keeps at least a third of what is left above the panel.
        let room = (bottom - ctx.content_rect().top()) * 2.0 / 3.0;
        let max = room.clamp(super::grid::MIXER_MIN_HEIGHT, super::grid::MIXER_MAX_HEIGHT);
        let kept = self.view.grid_mixer;
        self.view.grid_mixer = true;
        let panel = egui::Panel::bottom("practice_mixer")
            .resizable(true)
            .min_size(super::grid::MIXER_MIN_HEIGHT)
            .max_size(max)
            .default_size(super::grid::MIXER_HEIGHT.min(max))
            .frame(style::panel_frame(&ctx))
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| self.track_grid(ui));
            });
        self.view.grid_mixer = kept;
        panel.response.rect.top()
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
                        // Without a song there is no drawer: the way back to the full
                        // interface stays here.
                        let full = ui.add_sized(
                            vec2(ui.available_width(), 44.0),
                            egui::Button::new(RichText::new("Full interface (editing)").size(15.0))
                                .frame(false),
                        );
                        if full.clicked() {
                            self.set_practice_view(PracticeChoice::Off);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The practice view on a song of `measures` empty bars.
    fn practising(measures: usize) -> GuitarScoreApp {
        let ctx = egui::Context::default();
        let mut app = GuitarScoreApp::with_settings(
            &ctx,
            None,
            crate::Keymap::default(),
            None,
            crate::Settings::default(),
            None,
        );
        let song = gscore_model::testing::song(&[gscore_model::testing::GUITAR_6], measures)
            .expect("song");
        app.push_tab(crate::document::Document::new(
            std::path::PathBuf::from("practice.gp"),
            song,
            gscore_session::FileFormat::Gp,
        ));
        app.set_practice_view(PracticeChoice::On);
        app.practice.shown = true;
        app
    }

    fn cursor_bar(app: &GuitarScoreApp) -> usize {
        app.document().map_or(usize::MAX, |document| {
            document.editor().cursor().measure.get()
        })
    }

    #[test]
    fn a_swipe_scrolls_and_a_long_press_or_a_mouse_selects() {
        assert!(drag_scrolls(true, 0.1), "a quick swipe");
        assert!(!drag_scrolls(true, LONG_PRESS), "a finger held still first");
        assert!(!drag_scrolls(true, 0.8), "held until egui calls it a drag");
        assert!(!drag_scrolls(false, 0.0), "a mouse");
    }

    #[test]
    fn a_tap_only_chooses_where_play_starts() {
        let mut app = practising(6);
        app.practice_tap(3);
        assert_eq!(cursor_bar(&app), 3);
        assert_eq!(app.playback.state(), gscore_play::PlayState::Stopped);
        assert!(!app.playback.count_in_pending());
    }

    #[test]
    fn a_second_tap_moves_on_without_a_selection_or_a_loop() {
        let mut app = practising(6);
        app.practice_tap(1);
        app.practice_tap(4);
        assert_eq!(cursor_bar(&app), 4);
        assert_eq!(app.selection(), None);
        assert!(!app.playback.loop_selection());
    }

    #[test]
    fn a_drag_across_bars_selects_them_and_loops_them_once_let_go() {
        let mut app = practising(6);
        // Dragged back from bar 4 to bar 2, passing a gap between bars on the way.
        app.practice_drag(Some(4), true, false);
        app.practice_drag(Some(3), false, false);
        app.practice_drag(None, false, false);
        app.practice_drag(Some(2), false, false);
        let bars = app.selection().expect("the bars dragged across");
        assert_eq!((bars.start(), bars.end()), (2, 4));
        app.practice_drag(None, false, true);
        assert!(app.playback.loop_selection(), "looped once let go");
        // A tap ends the loop and puts the start at its bar.
        app.practice_tap(0);
        assert_eq!(app.selection(), None);
        assert!(!app.playback.loop_selection());
    }

    #[test]
    fn a_drag_that_starts_between_bars_selects_nothing() {
        let mut app = practising(6);
        app.practice_drag(None, true, false);
        app.practice_drag(Some(3), false, false);
        app.practice_drag(Some(3), false, true);
        assert_eq!(app.selection(), None);
        assert!(!app.playback.loop_selection());
    }

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
}
