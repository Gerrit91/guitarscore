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

/// Smallest and largest zoom factor.
pub const ZOOM_RANGE: (f32, f32) = (0.25, 4.0);
/// Factor of one zoom step.
const ZOOM_STEP: f32 = 1.25;

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

/// How the open document is shown.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    /// Color theme.
    pub theme: ThemeChoice,
    /// Zoom factor; 1.0 is the normal size.
    zoom: f32,
    /// The shown track (0-based).
    pub track: usize,
    /// Whether the track and song grid (the bottom panel) is shown.
    pub show_track_grid: bool,
    /// Whether the track panel (the right panel) is shown.
    pub show_track_panel: bool,
    /// The selected range of measures in the song grid, if any.
    pub selection: Option<MeasureSelection>,
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState {
            theme: ThemeChoice::Light,
            zoom: 1.0,
            track: 0,
            show_track_grid: true,
            show_track_panel: true,
            selection: None,
        }
    }
}

impl ViewState {
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

    /// Selects `measure`, or extends the selection to it when `extend` is set.
    pub fn select_measure(&mut self, measure: usize, extend: bool) {
        self.selection = Some(match (self.selection, extend) {
            (Some(selection), true) => selection.to(measure),
            _ => MeasureSelection::new(measure),
        });
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
        let mut view = ViewState::default();
        view.select_measure(4, false);
        view.select_measure(1, true);
        assert_eq!(view.selection.map(MeasureSelection::start), Some(1));
        assert_eq!(view.selection.map(MeasureSelection::end), Some(4));
        view.select_measure(7, false);
        assert_eq!(view.selection.map(MeasureSelection::start), Some(7));
    }

    #[test]
    fn themes_map_to_layout_palettes() {
        assert_eq!(ThemeChoice::Dark.palette().name, "dark");
        assert_eq!(ThemeChoice::Light.palette().name, "light");
    }
}
