//! Settings that shape the layout: page size, staff geometry, spacing.

/// Layout settings. Lengths are in staff spaces unless the name says otherwise.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutOptions {
    /// Size of one staff space in millimetres (MuseScore's default is 1.75).
    pub staff_space_mm: f32,
    /// Page width.
    pub page_width: f32,
    /// Left and right page margin.
    pub margin_x: f32,
    /// Top and bottom page margin.
    pub margin_y: f32,
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
}

/// A4 width in millimetres.
const A4_WIDTH_MM: f32 = 210.0;

impl Default for LayoutOptions {
    fn default() -> Self {
        let staff_space_mm = 1.75;
        LayoutOptions {
            staff_space_mm,
            page_width: A4_WIDTH_MM / staff_space_mm,
            margin_x: 8.0,
            margin_y: 8.0,
            tab_line_spacing: 1.5,
            fret_size: 1.5,
            measure_number_size: 1.1,
            spacing: Spacing::default(),
            system_gap: 6.0,
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
