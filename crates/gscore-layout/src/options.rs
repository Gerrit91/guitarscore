//! Settings that shape the layout: page size, staff geometry, spacing.

/// Layout settings. Lengths are in staff spaces unless the name says otherwise.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutOptions {
    /// Size of one staff space in millimetres (MuseScore's default is 1.75).
    pub staff_space_mm: f32,
    /// Page width.
    pub page_width: f32,
    /// Page height for a layout on separate pages; `None` lays the score out on one
    /// endless page.
    pub page_height: Option<f32>,
    /// The paper's height in millimetres, used by [`LayoutOptions::on_pages`]; `None` is
    /// A4.
    pub paper_height_mm: Option<f32>,
    /// Page margins.
    pub margin_left: f32,
    pub margin_right: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    /// On pages: "Page 2/5" at the bottom right of each page.
    pub page_numbers: bool,
    /// On pages: the song's copyright at the bottom of the first page.
    pub copyright: bool,
    /// On pages: "title by artist" atop every page after the first.
    pub running_header: bool,
    /// Distance between two lines of the tab staff.
    pub tab_line_spacing: f32,
    /// Font size of fret numbers.
    pub fret_size: f32,
    /// Font size of measure numbers.
    pub measure_number_size: f32,
    /// Horizontal spacing of beats.
    pub spacing: Spacing,
    /// Space between two systems (from the bottom of one staff to the top of the next).
    pub system_gap: f32,
    /// Show standard notation above the tab (for tracks with a tuning).
    pub notation: bool,
    /// Space between the notation staff's bottom line and the tab's top line.
    pub notation_gap: f32,
}

/// How much horizontal space a beat gets, depending on how long it lasts.
///
/// A quarter note gets `quarter`; every doubling of the length multiplies the space by
/// `doubling_ratio` (and every halving divides it), so long notes do not take
/// proportionally more room. No beat gets less than `min_space`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spacing {
    /// Space after a quarter note.
    pub quarter: f32,
    /// Factor between the space of a note and one twice as long.
    pub doubling_ratio: f32,
    /// Smallest space after any beat.
    pub min_space: f32,
    /// Smallest gap between the fret numbers of neighbouring beats.
    pub min_gap: f32,
    /// Smallest space between a fret number and the barline on either side of a measure.
    pub barline_padding: f32,
}

/// A4 width and height in millimetres.
const A4_WIDTH_MM: f32 = 210.0;
const A4_HEIGHT_MM: f32 = 297.0;
/// The staff space at score size 100 %, in millimetres.
const STAFF_SPACE_MM: f32 = 1.75;

impl LayoutOptions {
    /// These options with the score on separate pages of the options' paper (A4 unless
    /// [`LayoutOptions::with_page_setup`] set another).
    pub fn on_pages(self) -> Self {
        let height_mm = self.paper_height_mm.unwrap_or(A4_HEIGHT_MM);
        LayoutOptions {
            page_height: Some(height_mm / self.staff_space_mm),
            ..self
        }
    }

    /// These options with the song's page setup: paper, margins, score size (a bigger
    /// score has a bigger staff space, so fewer bars fit), header and footer. Still one
    /// endless page until [`LayoutOptions::on_pages`].
    pub fn with_page_setup(self, setup: &gscore_model::PageSetup) -> Self {
        let space = STAFF_SPACE_MM * f32::from(setup.scale_percent) / 100.0;
        let space = if space.is_finite() && space > 0.0 {
            space
        } else {
            STAFF_SPACE_MM
        };
        let spaces = |mm: u16| f32::from(mm) / space;
        LayoutOptions {
            staff_space_mm: space,
            page_width: spaces(setup.width_mm),
            paper_height_mm: Some(f32::from(setup.height_mm)),
            page_height: self.page_height.map(|_| f32::from(setup.height_mm) / space),
            margin_left: spaces(setup.margins.left),
            margin_right: spaces(setup.margins.right),
            margin_top: spaces(setup.margins.top),
            margin_bottom: spaces(setup.margins.bottom),
            page_numbers: setup.page_numbers,
            copyright: setup.copyright,
            running_header: setup.running_header,
            ..self
        }
    }

    /// These options without what only places measures on pages (paper, margins, staff
    /// size, header and footer): what one measure's layout depends on, for the cache key.
    pub(crate) fn for_measures(&self) -> Self {
        let page = LayoutOptions::default();
        LayoutOptions {
            staff_space_mm: page.staff_space_mm,
            page_width: page.page_width,
            page_height: None,
            paper_height_mm: None,
            margin_left: page.margin_left,
            margin_right: page.margin_right,
            margin_top: page.margin_top,
            margin_bottom: page.margin_bottom,
            page_numbers: page.page_numbers,
            copyright: page.copyright,
            running_header: page.running_header,
            ..self.clone()
        }
    }

    /// The width between the left and right margins.
    pub fn line_width(&self) -> f32 {
        self.page_width - self.margin_left - self.margin_right
    }
}

impl Default for LayoutOptions {
    fn default() -> Self {
        let staff_space_mm = 1.75;
        LayoutOptions {
            staff_space_mm,
            page_width: A4_WIDTH_MM / staff_space_mm,
            page_height: None,
            paper_height_mm: None,
            margin_left: 8.0,
            margin_right: 8.0,
            margin_top: 8.0,
            margin_bottom: 8.0,
            page_numbers: true,
            copyright: true,
            running_header: false,
            tab_line_spacing: 1.5,
            fret_size: 1.5,
            measure_number_size: 1.1,
            spacing: Spacing::default(),
            system_gap: 6.0,
            notation: true,
            notation_gap: 8.0,
        }
    }
}

impl Default for Spacing {
    fn default() -> Self {
        Spacing {
            quarter: 4.5,
            doubling_ratio: 1.3,
            min_space: 1.8,
            min_gap: 0.5,
            barline_padding: 1.2,
        }
    }
}

impl Spacing {
    /// The space after a beat that lasts `length` whole notes.
    pub fn after(&self, length: f64) -> f32 {
        if !(length.is_finite() && length > 0.0) {
            return self.min_space;
        }
        // `length * 4` quarters; log2 of it is the number of doublings.
        let doublings = (length * 4.0).log2() as f32;
        let space = self.quarter * self.doubling_ratio.powf(doublings);
        if space.is_finite() {
            space.max(self.min_space)
        } else {
            self.min_space
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longer_notes_get_more_space_but_not_proportionally() {
        let spacing = Spacing::default();
        let quarter = spacing.after(0.25);
        let half = spacing.after(0.5);
        assert!((quarter - spacing.quarter).abs() < 1e-4);
        assert!(half > quarter && half < 2.0 * quarter);
        assert_eq!(spacing.after(1.0 / 128.0), spacing.min_space);
        for odd in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(spacing.after(odd), spacing.min_space);
        }
    }
}
