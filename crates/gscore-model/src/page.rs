//! The printed page: paper size and margins, the score's size on it, what the header and
//! footer show; and where lines (systems) and pages break, per track.

use crate::error::ModelError;

/// Smallest and largest paper side, in millimetres.
pub const PAGE_SIZE_MM: (u16, u16) = (70, 1000);
/// Largest margin, in millimetres.
pub const MAX_MARGIN_MM: u16 = 200;
/// Room the margins must leave for the music: across, and from top to bottom (mm). With
/// the smallest page and the default margins there is always this much.
const MIN_CONTENT_MM: (u16, u16) = (40, 40);
/// Smallest and largest score size in percent (100 is normal).
pub const SCALE_RANGE: (u16, u16) = (50, 200);
/// The most bars a line may be limited to.
pub const MAX_BARS_PER_SYSTEM: u8 = 32;

/// Page margins in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Margins {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

/// How the song is printed. Lengths in millimetres; a page wider than tall is landscape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSetup {
    /// Paper width.
    pub width_mm: u16,
    /// Paper height.
    pub height_mm: u16,
    /// The margins.
    pub margins: Margins,
    /// The score's size on the page in percent: 100 is normal, 80 smaller (more bars
    /// per page).
    pub scale_percent: u16,
    /// "Page 2/5" at the bottom of each page.
    pub page_numbers: bool,
    /// The song's copyright at the bottom of the first page.
    pub copyright: bool,
    /// "Title by artist" at the top of every page after the first.
    pub running_header: bool,
}

/// Paper sizes offered by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperSize {
    /// 210 × 297 mm.
    A4,
    /// 216 × 279 mm (8.5 × 11 in).
    Letter,
    /// 148 × 210 mm.
    A5,
    /// 297 × 420 mm.
    A3,
}

impl PaperSize {
    /// Every named size, for menus.
    pub const ALL: [PaperSize; 4] = [
        PaperSize::A4,
        PaperSize::Letter,
        PaperSize::A5,
        PaperSize::A3,
    ];

    /// Width and height in portrait, in millimetres.
    pub fn portrait_mm(self) -> (u16, u16) {
        match self {
            PaperSize::A4 => (210, 297),
            PaperSize::Letter => (216, 279),
            PaperSize::A5 => (148, 210),
            PaperSize::A3 => (297, 420),
        }
    }

    /// The name shown in menus.
    pub fn name(self) -> &'static str {
        match self {
            PaperSize::A4 => "A4",
            PaperSize::Letter => "Letter",
            PaperSize::A5 => "A5",
            PaperSize::A3 => "A3",
        }
    }

    /// The named size of a page `width` × `height` mm, in either orientation.
    pub fn of(width: u16, height: u16) -> Option<PaperSize> {
        let (short, long) = (width.min(height), width.max(height));
        PaperSize::ALL
            .into_iter()
            .find(|size| size.portrait_mm() == (short, long))
    }
}

impl Default for PageSetup {
    /// A4 portrait with Guitar Pro's margins, page numbers and copyright.
    fn default() -> Self {
        PageSetup {
            width_mm: 210,
            height_mm: 297,
            margins: Margins {
                top: 15,
                right: 10,
                bottom: 10,
                left: 10,
            },
            scale_percent: 100,
            page_numbers: true,
            copyright: true,
            running_header: false,
        }
    }
}

impl PageSetup {
    /// Whether the page is wider than tall.
    pub fn is_landscape(&self) -> bool {
        self.width_mm > self.height_mm
    }

    /// The same page turned: width and height swapped.
    pub fn turned(self) -> Self {
        PageSetup {
            width_mm: self.height_mm,
            height_mm: self.width_mm,
            ..self
        }
    }

    /// Checks that the paper is 70 to 1000 mm on each side, the margins at most 200 mm and
    /// leave at least 40 mm each way for the music, and the scale is 50 to 200 percent.
    pub fn check(&self) -> Result<(), ModelError> {
        let (low, high) = PAGE_SIZE_MM;
        for (what, size) in [("width", self.width_mm), ("height", self.height_mm)] {
            if !(low..=high).contains(&size) {
                return Err(ModelError::Invalid(format!(
                    "page {what} {size} mm is outside {low} to {high} mm"
                )));
            }
        }
        let m = self.margins;
        if [m.top, m.right, m.bottom, m.left]
            .iter()
            .any(|margin| *margin > MAX_MARGIN_MM)
        {
            return Err(ModelError::Invalid(format!(
                "a page margin is above {MAX_MARGIN_MM} mm"
            )));
        }
        let across = self.width_mm.saturating_sub(m.left.saturating_add(m.right));
        let down = self
            .height_mm
            .saturating_sub(m.top.saturating_add(m.bottom));
        if across < MIN_CONTENT_MM.0 || down < MIN_CONTENT_MM.1 {
            return Err(ModelError::Invalid(format!(
                "the margins leave {across} × {down} mm for the music (at least {} × {} mm)",
                MIN_CONTENT_MM.0, MIN_CONTENT_MM.1
            )));
        }
        let (low, high) = SCALE_RANGE;
        if !(low..=high).contains(&self.scale_percent) {
            return Err(ModelError::Invalid(format!(
                "score size {} % is outside {low} to {high} %",
                self.scale_percent
            )));
        }
        Ok(())
    }

    /// This setup with every value moved into its allowed range (for files with odd page
    /// setups: the page must still be drawn). Margins that leave too little room are
    /// dropped to Guitar Pro's.
    pub fn sanitized(self) -> Self {
        let (low, high) = PAGE_SIZE_MM;
        let mut setup = PageSetup {
            width_mm: self.width_mm.clamp(low, high),
            height_mm: self.height_mm.clamp(low, high),
            scale_percent: self.scale_percent.clamp(SCALE_RANGE.0, SCALE_RANGE.1),
            ..self
        };
        if setup.check().is_err() {
            setup.margins = PageSetup::default().margins;
        }
        setup
    }
}

/// Where a track's line of music ends before a bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemBreak {
    /// Lines break where the page width (or the bar limit) says.
    None,
    /// The bar starts a new line.
    Line,
    /// The bar starts a new page.
    Page,
}
