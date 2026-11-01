//! The view state: plain data that says how the document is shown (theme, zoom, track).
//! Kept separate from the widgets so it can be saved, restored and tested.

/// Which color theme the app uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// How the open document is shown.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    /// Color theme.
    pub theme: ThemeChoice,
    /// Zoom factor; 1.0 is the normal size.
    zoom: f32,
    /// The shown track (0-based).
    pub track: usize,
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState {
            theme: ThemeChoice::Light,
            zoom: 1.0,
            track: 0,
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
    fn themes_map_to_layout_palettes() {
        assert_eq!(ThemeChoice::Dark.palette().name, "dark");
        assert_eq!(ThemeChoice::Light.palette().name, "light");
    }
}
