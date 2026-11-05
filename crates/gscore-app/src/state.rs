//! The view state: plain data that says how the document is shown (theme, zoom, track).
//! Kept separate from the widgets so it can be saved, restored and tested.

use serde::{Deserialize, Serialize};

/// Which color theme the app uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    /// Dark ink on white paper.
    Light,
    /// Light ink on a dark background.
    Dark,
}

impl ThemeChoice {
    /// The layout palette for this choice.
    pub fn palette(self) -> &'static gscore_layout::Theme {
        match self {
            ThemeChoice::Light => &gscore_layout::Theme::LIGHT,
            ThemeChoice::Dark => &gscore_layout::Theme::DARK,
        }
    }
}

/// How the score fills the window: on printed pages (stacked, side by side or in a grid),
/// or on one endless surface (vertically, or all on one line to scroll right).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ScoreView {
    /// Printed pages, one below the other.
    #[default]
    PageVertical,
    /// Printed pages in one row, scrolling to the right.
    PageHorizontal,
    /// As many pages side by side as fit, then the next row.
    PageGrid,
    /// No pages: the score fills the window and scrolls down.
    ScreenVertical,
    /// No pages: the whole score on one line, scrolling to the right.
    ScreenHorizontal,
}

impl ScoreView {
    /// Every choice, in menu order.
    pub const ALL: [ScoreView; 5] = [
        ScoreView::PageVertical,
        ScoreView::PageHorizontal,
        ScoreView::PageGrid,
        ScoreView::ScreenVertical,
        ScoreView::ScreenHorizontal,
    ];

    /// Whether the score is laid out on printed pages.
    pub fn on_pages(self) -> bool {
        matches!(
            self,
            ScoreView::PageVertical | ScoreView::PageHorizontal | ScoreView::PageGrid
        )
    }

    /// Whether pages sit beside each other instead of below each other.
    pub fn pages_side_by_side(self) -> bool {
        matches!(self, ScoreView::PageHorizontal | ScoreView::PageGrid)
    }

    /// Whether the whole score is one line that scrolls to the right.
    pub fn single_line(self) -> bool {
        self == ScoreView::ScreenHorizontal
    }

    /// A label for the View menu.
    pub fn label(self) -> &'static str {
        match self {
            ScoreView::PageVertical => "Page view (vertical)",
            ScoreView::PageHorizontal => "Page view (horizontal)",
            ScoreView::PageGrid => "Page grid",
            ScoreView::ScreenVertical => "Screen view (vertical)",
            ScoreView::ScreenHorizontal => "Screen view (horizontal)",
        }
    }
}

/// Smallest and largest zoom factor.
pub const ZOOM_RANGE: (f32, f32) = (0.25, 4.0);
/// Factor of one zoom step.
const ZOOM_STEP: f32 = 1.25;
/// Smallest and largest size of the whole interface (menus, panels, icons), and one step.
pub(crate) const INTERFACE_ZOOM_RANGE: (f32, f32) = (0.5, 2.0);
const INTERFACE_ZOOM_STEP: f32 = 1.1;

/// A range of measures selected in the song grid: from `anchor` to `focus`, inclusive and
/// in either direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasureSelection {
    /// Where the selection started (the measure that was pressed).
    pub anchor: usize,
    /// Where it ends now (the measure the pointer is over).
    pub focus: usize,
}

impl MeasureSelection {
    /// A selection of just `measure`.
    pub fn new(measure: usize) -> Self {
        MeasureSelection {
            anchor: measure,
            focus: measure,
        }
    }

    /// The same selection, extended to `measure`.
    pub fn to(self, measure: usize) -> Self {
        MeasureSelection {
            focus: measure,
            ..self
        }
    }

    /// The first selected measure (0-based).
    pub fn start(self) -> usize {
        self.anchor.min(self.focus)
    }

    /// The last selected measure (0-based, inclusive).
    pub fn end(self) -> usize {
        self.anchor.max(self.focus)
    }

    /// Whether `measure` is selected.
    pub fn contains(self, measure: usize) -> bool {
        (self.start()..=self.end()).contains(&measure)
    }
}

/// How the open document is shown, shared by every tab: the theme, the zoom and which
/// panels are visible. What differs per tab (the shown track and the measure selection)
/// lives on each [`super::app::tab::Tab`].
#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    /// Color theme.
    pub theme: ThemeChoice,
    /// Zoom factor; 1.0 is the normal size.
    zoom: f32,
    /// The size of the whole interface (menus, panels, icons); 1.0 is the normal size.
    interface_zoom: f32,
    /// Whether the track and song grid (the bottom panel) is shown.
    pub show_track_grid: bool,
    /// Whether the bottom panel shows the mixer instead of the measure grid.
    pub grid_mixer: bool,
    /// Whether the track panel (the right panel) is shown.
    pub show_track_panel: bool,
    /// Whether the edit palette (the left panel) is shown.
    pub show_edit_panel: bool,
    /// How the score is shown: pages or one endless surface, vertical or horizontal.
    pub score_view: ScoreView,
    /// With several tracks shown, whether a track is left out of a line where it has no
    /// note (the cursor's track always stays).
    pub hide_empty_staves: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState {
            theme: ThemeChoice::Light,
            zoom: 1.0,
            interface_zoom: 1.0,
            show_track_grid: true,
            grid_mixer: false,
            show_track_panel: true,
            show_edit_panel: true,
            score_view: ScoreView::PageVertical,
            hide_empty_staves: false,
        }
    }
}

impl ViewState {
    /// The size of the whole interface.
    pub fn interface_zoom(&self) -> f32 {
        self.interface_zoom
    }

    /// Sets the size of the whole interface, clamped to [`INTERFACE_ZOOM_RANGE`];
    /// nonsense values reset it to 1.
    pub fn set_interface_zoom(&mut self, zoom: f32) {
        self.interface_zoom = if zoom.is_finite() {
            zoom.clamp(INTERFACE_ZOOM_RANGE.0, INTERFACE_ZOOM_RANGE.1)
        } else {
            1.0
        };
    }

    /// The whole interface one step larger.
    pub fn interface_larger(&mut self) {
        self.set_interface_zoom(self.interface_zoom * INTERFACE_ZOOM_STEP);
    }

    /// The whole interface one step smaller.
    pub fn interface_smaller(&mut self) {
        self.set_interface_zoom(self.interface_zoom / INTERFACE_ZOOM_STEP);
    }

    /// The zoom factor.
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Sets the zoom, clamped to [`ZOOM_RANGE`]; nonsense values reset it to 1.
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = if zoom.is_finite() {
            zoom.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1)
        } else {
            1.0
        };
    }

    /// One step larger.
    pub fn zoom_in(&mut self) {
        self.set_zoom(self.zoom * ZOOM_STEP);
    }

    /// One step smaller.
    pub fn zoom_out(&mut self) {
        self.set_zoom(self.zoom / ZOOM_STEP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_stays_in_range_and_ignores_nonsense() {
        let mut view = ViewState::default();
        for _ in 0..100 {
            view.zoom_in();
        }
        assert_eq!(view.zoom(), ZOOM_RANGE.1);
        for _ in 0..100 {
            view.zoom_out();
        }
        assert_eq!(view.zoom(), ZOOM_RANGE.0);
        view.set_zoom(f32::NAN);
        assert_eq!(view.zoom(), 1.0);
        view.set_zoom(f32::INFINITY);
        assert_eq!(view.zoom(), 1.0);
    }

    #[test]
    fn measure_selections_normalize_and_extend() {
        let selection = MeasureSelection::new(5);
        assert_eq!((selection.start(), selection.end()), (5, 5));
        let extended = selection.to(2);
        assert_eq!((extended.start(), extended.end()), (2, 5));
        assert!(extended.contains(3));
        assert!(!extended.contains(6));
    }

    #[test]
    fn themes_map_to_layout_palettes() {
        assert_eq!(ThemeChoice::Dark.palette().name, "dark");
        assert_eq!(ThemeChoice::Light.palette().name, "light");
    }

    #[test]
    fn the_five_views_split_into_pages_and_screens() {
        assert_eq!(ScoreView::PageVertical.label(), "Page view (vertical)");
        let pages: Vec<ScoreView> = ScoreView::ALL
            .into_iter()
            .filter(|view| view.on_pages())
            .collect();
        assert_eq!(
            pages,
            [
                ScoreView::PageVertical,
                ScoreView::PageHorizontal,
                ScoreView::PageGrid
            ]
        );
        assert!(ScoreView::PageHorizontal.pages_side_by_side());
        assert!(ScoreView::PageGrid.pages_side_by_side());
        assert!(!ScoreView::PageVertical.pages_side_by_side());
        assert_eq!(
            ScoreView::ALL
                .into_iter()
                .filter(|view| view.single_line())
                .collect::<Vec<_>>(),
            [ScoreView::ScreenHorizontal]
        );
    }
}
