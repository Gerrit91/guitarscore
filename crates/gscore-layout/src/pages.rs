//! Pages: putting systems one below the other, onto pages when the layout has them, and
//! what the pages carry besides the music (page numbers, copyright, running header).

use gscore_model::Song;

use crate::draw::{ColorToken, DrawOp, Point, Rect, TextAnchor, TextFont};
use crate::{HEADER_GAP, LayoutOptions, PAGE_GAP};

/// Room kept free at the bottom of a page (above the bottom margin), so the last system
/// does not touch the footer.
const FOOTER_ROOM: f32 = 3.0;
/// Height of one copyright line at the bottom of the first page.
const FOOTER_LINE: f32 = 1.4;
/// Room for the running header below the top margin of pages after the first.
const RUNNING_HEADER_ROOM: f32 = 2.5;
/// The most space added between two systems when a page's systems are spread down to its
/// bottom.
const MAX_SPREAD: f32 = 6.0;
/// Room below a system's lowest staff line for what hangs under it (P.M. and let ring rows,
/// dynamics), counted when deciding whether a system still fits on a page.
const BELOW_SYSTEM: f32 = 3.0;

/// Where the systems go: the top of each system's block (the rows above its staves come
/// first), and the pages (none for the endless page).
pub(crate) struct Placement {
    pub(crate) tops: Vec<f32>,
    pub(crate) pages: Vec<Rect>,
}

/// Puts systems of `heights` (rows above plus staves) one below the other from `first`.
/// With pages, a system that does not fit (or that starts with a page break) goes to the
/// next page, and the systems of a page that ended full are spread down to its bottom.
pub(crate) fn place_systems(
    heights: &[f32],
    page_starts: &[bool],
    first: f32,
    options: &LayoutOptions,
    decor: &PageDecor,
) -> Placement {
    let step = |height: f32| height + options.system_gap + HEADER_GAP;
    let page_height = options.page_height.filter(|height| {
        height.is_finite() && *height > options.margin_top + options.margin_bottom
    });
    let Some(page_height) = page_height else {
        let mut tops = Vec::with_capacity(heights.len());
        let mut cursor = first;
        for height in heights {
            tops.push(cursor);
            cursor += step(*height);
        }
        return Placement {
            tops,
            pages: Vec::new(),
        };
    };

    let mut tops = Vec::with_capacity(heights.len());
    let mut pages: Vec<Rect> = Vec::new();
    // Per page: its systems, and whether it ended because the next one did not fit.
    let mut page_systems: Vec<(std::ops::Range<usize>, bool)> = Vec::new();
    let mut page_top = 0.0_f32;
    let mut page_first = 0_usize;
    let mut cursor = first;
    let limit = |page: usize, page_top: f32| {
        page_top + page_height - options.margin_bottom - decor.footer_room(page)
    };
    for (index, height) in heights.iter().enumerate() {
        let page = pages.len();
        if index > page_first {
            let forced = page_starts.get(index).copied().unwrap_or(false);
            let overflow = cursor + height + BELOW_SYSTEM > limit(page, page_top);
            if forced || overflow {
                pages.push(Rect {
                    origin: Point::new(0.0, page_top),
                    width: options.page_width,
                    height: page_height,
                });
                page_systems.push((page_first..index, overflow && !forced));
                page_top += page_height + PAGE_GAP;
                page_first = index;
                cursor = page_top
                    + options.margin_top
                    + decor.header_room(page.saturating_add(1))
                    + HEADER_GAP;
            }
        }
        tops.push(cursor);
        cursor += step(*height);
    }
    pages.push(Rect {
        origin: Point::new(0.0, page_top),
        width: options.page_width,
        height: page_height,
    });
    page_systems.push((page_first..heights.len(), false));

    // Spread the systems of every full page down to its bottom, but never more than
    // MAX_SPREAD between two systems (a page of two systems stays readable).
    for (page, (systems, full)) in page_systems.iter().enumerate() {
        let count = systems.len();
        if !*full || count < 2 {
            continue;
        }
        let last = systems.end.saturating_sub(1);
        let (Some(top), Some(height), Some(rect)) =
            (tops.get(last), heights.get(last), pages.get(page))
        else {
            continue;
        };
        let free = limit(page, rect.origin.y) - (top + height + BELOW_SYSTEM);
        if !(free.is_finite() && free > 0.0) {
            continue;
        }
        let extra = (free / count.saturating_sub(1) as f32).min(MAX_SPREAD);
        for (offset, index) in systems.clone().enumerate() {
            if let Some(top) = tops.get_mut(index) {
                *top += extra * offset as f32;
            }
        }
    }
    Placement { tops, pages }
}

/// What the pages carry besides the music: page numbers, the copyright on the first page,
/// and a running header on the others.
pub(crate) struct PageDecor {
    page_numbers: bool,
    copyright: Vec<String>,
    header: Option<String>,
}

impl PageDecor {
    pub(crate) fn new(song: &Song, options: &LayoutOptions) -> Self {
        let copyright = if options.copyright {
            song.copyright()
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect()
        } else {
            Vec::new()
        };
        let header = options
            .running_header
            .then(|| match (song.title().trim(), song.artist().trim()) {
                ("", _) => None,
                (title, "") => Some(title.to_owned()),
                (title, artist) => Some(format!("{title} by {artist}")),
            })
            .flatten();
        PageDecor {
            page_numbers: options.page_numbers,
            copyright,
            header,
        }
    }

    /// Room kept above page `page`'s bottom margin.
    fn footer_room(&self, page: usize) -> f32 {
        let copyright = if page == 0 {
            self.copyright.len() as f32 * FOOTER_LINE
        } else {
            0.0
        };
        FOOTER_ROOM + copyright
    }

    /// Room kept below page `page`'s top margin (the first page has the title block).
    fn header_room(&self, page: usize) -> f32 {
        if page > 0 && self.header.is_some() {
            RUNNING_HEADER_ROOM
        } else {
            0.0
        }
    }

    /// Page numbers at the bottom right ("Page 2/5", as Guitar Pro), the copyright at the
    /// bottom of the first page, the running header atop the others.
    pub(crate) fn draw(&self, pages: &[Rect], options: &LayoutOptions, ops: &mut Vec<DrawOp>) {
        let count = pages.len();
        let text = |text: String, at: Point, anchor: TextAnchor| DrawOp::Text {
            text,
            at,
            size: options.measure_number_size,
            font: TextFont::Regular,
            anchor,
            color: ColorToken::Annotation,
        };
        let center = (options.margin_left + options.page_width - options.margin_right) / 2.0;
        for (index, page) in pages.iter().enumerate() {
            let bottom = page.origin.y + page.height;
            if self.page_numbers {
                ops.push(text(
                    format!("Page {}/{count}", index.saturating_add(1)),
                    Point::new(
                        page.origin.x + page.width - options.margin_right,
                        bottom - options.margin_bottom / 2.0,
                    ),
                    TextAnchor::End,
                ));
            }
            if index == 0 {
                let lines = self.copyright.len();
                for (line, copyright) in self.copyright.iter().enumerate() {
                    let up = lines.saturating_sub(line).saturating_sub(1) as f32;
                    ops.push(text(
                        copyright.clone(),
                        Point::new(center, bottom - options.margin_bottom - up * FOOTER_LINE),
                        TextAnchor::Middle,
                    ));
                }
            } else if let Some(header) = &self.header {
                ops.push(text(
                    header.clone(),
                    Point::new(center, page.origin.y + options.margin_top + 1.0),
                    TextAnchor::Middle,
                ));
            }
        }
    }
}
