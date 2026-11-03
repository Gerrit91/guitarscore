//! GuitarScore addition: GP7's `Content/BinaryStylesheet`, Guitar Pro's style settings
//! (page size and margins, header and footer, fonts, spacing, ...), as named values.
//!
//! The layout (as Guitar Pro 7 writes it): a big-endian `u32` count, then per entry a
//! one-byte name length, the name (UTF-8, e.g. `PageSetup/pageTopMargin`), a type byte and
//! the value: 0 bool (one byte), 1 `i32`, 2 `f32`, 3 text (big-endian `u16` length and
//! UTF-8), 7 color (four bytes), all big-endian. The page setup is read from it, and saving
//! writes every entry back with the page setup's values, so nothing else is lost.

use crate::model::page::{
    HEADER_FOOTER_COPYRIGHT, HEADER_FOOTER_PAGE_NUMBER, HEADER_FOOTER_RUNNING_HEADER, PageSetup,
    StyleValue, Stylesheet,
};

/// The file's name in the archive.
pub(crate) const STYLESHEET: &str = "Content/BinaryStylesheet";

/// More entries than this means a broken file: it is ignored.
const MAX_ENTRIES: usize = 20_000;

const WIDTH: &str = "PageSetup/pagePortraitWidth";
const HEIGHT: &str = "PageSetup/pagePortraitHeight";
const ORIENTATION: &str = "PageSetup/pageOrientation";
const TOP: &str = "PageSetup/pageTopMargin";
const LEFT: &str = "PageSetup/pageLeftMargin";
const RIGHT: &str = "PageSetup/pageRightMargin";
const BOTTOM: &str = "PageSetup/pageBottomMargin";
const SCALE: &str = "PageSetup/pageScale";
const PAGE_NUMBER: &str = "Footer/drawPageNumber";
const COPYRIGHT: &str = "Footer/drawCopyright";
const RUNNING_HEADER: &str = "Odd/drawOddHeader";

/// The style sheet in `data`, or `None` if it is not one (an unknown value type, a
/// truncated entry or bytes left over).
pub(crate) fn read(data: &[u8]) -> Option<Stylesheet> {
    let mut at = 0_usize;
    let mut take = |n: usize| -> Option<&[u8]> {
        let bytes = data.get(at..at.checked_add(n)?)?;
        at = at.checked_add(n)?;
        Some(bytes)
    };
    let count = u32::from_be_bytes(take(4)?.try_into().ok()?);
    let count = usize::try_from(count).ok()?;
    if count > MAX_ENTRIES {
        return None;
    }
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let length = usize::from(*take(1)?.first()?);
        let name = String::from_utf8(take(length)?.to_vec()).ok()?;
        let kind = *take(1)?.first()?;
        let four = |bytes: &[u8]| -> Option<[u8; 4]> { bytes.try_into().ok() };
        let value = match kind {
            0 => StyleValue::Bool(*take(1)?.first()? != 0),
            1 => StyleValue::Int(i32::from_be_bytes(four(take(4)?)?)),
            2 => StyleValue::Float(f32::from_be_bytes(four(take(4)?)?)),
            3 => {
                let length = usize::from(u16::from_be_bytes(take(2)?.try_into().ok()?));
                StyleValue::Text(String::from_utf8(take(length)?.to_vec()).ok()?)
            }
            7 => StyleValue::Color(four(take(4)?)?),
            _ => return None,
        };
        entries.push((name, value));
    }
    (at == data.len()).then_some(Stylesheet { entries })
}

/// The bytes of `sheet`. Names longer than 255 bytes and texts longer than 65535 bytes
/// cannot be written and are left out (Guitar Pro never writes such entries).
pub(crate) fn write(sheet: &Stylesheet) -> Vec<u8> {
    let entries: Vec<&(String, StyleValue)> = sheet
        .entries
        .iter()
        .filter(|(name, value)| {
            u8::try_from(name.len()).is_ok()
                && !matches!(value, StyleValue::Text(text) if u16::try_from(text.len()).is_err())
        })
        .collect();
    let mut out = Vec::new();
    out.extend_from_slice(
        &u32::try_from(entries.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    for (name, value) in entries {
        out.push(u8::try_from(name.len()).unwrap_or(u8::MAX));
        out.extend_from_slice(name.as_bytes());
        match value {
            StyleValue::Bool(value) => {
                out.push(0);
                out.push(u8::from(*value));
            }
            StyleValue::Int(value) => {
                out.push(1);
                out.extend_from_slice(&value.to_be_bytes());
            }
            StyleValue::Float(value) => {
                out.push(2);
                out.extend_from_slice(&value.to_be_bytes());
            }
            StyleValue::Text(text) => {
                out.push(3);
                out.extend_from_slice(&u16::try_from(text.len()).unwrap_or(0).to_be_bytes());
                out.extend_from_slice(text.as_bytes());
            }
            StyleValue::Color(rgba) => {
                out.push(7);
                out.extend_from_slice(rgba);
            }
        }
    }
    out
}

/// A length in millimetres from the style sheet, kept in a sane range.
fn millimetres(sheet: &Stylesheet, name: &str, range: (u16, u16)) -> Option<u16> {
    let value = match sheet.get(name)? {
        StyleValue::Float(value) => *value,
        StyleValue::Int(value) => *value as f32,
        _ => return None,
    };
    if !value.is_finite() {
        return None;
    }
    // Clamped into the u16 range first, so the cast cannot truncate.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some((value.round().clamp(f32::from(range.0), f32::from(range.1))) as u16)
}

fn flag(sheet: &Stylesheet, name: &str) -> Option<bool> {
    match sheet.get(name)? {
        StyleValue::Bool(value) => Some(*value),
        _ => None,
    }
}

/// Takes the page size, margins, score size and what the header and footer show from
/// `sheet` into `setup`; values the sheet lacks stay as they are. A landscape page has its
/// width and height swapped.
pub(crate) fn apply_page_setup(sheet: &Stylesheet, setup: &mut PageSetup) {
    let size = (10, 2000);
    let margin = (0, 500);
    if let (Some(width), Some(height)) = (
        millimetres(sheet, WIDTH, size),
        millimetres(sheet, HEIGHT, size),
    ) {
        let landscape = matches!(sheet.get(ORIENTATION), Some(StyleValue::Int(1)));
        (setup.page_size.x, setup.page_size.y) = if landscape {
            (height, width)
        } else {
            (width, height)
        };
    }
    for (name, value) in [
        (TOP, &mut setup.page_margin.top),
        (LEFT, &mut setup.page_margin.left),
        (RIGHT, &mut setup.page_margin.right),
        (BOTTOM, &mut setup.page_margin.bottom),
    ] {
        if let Some(millimetres) = millimetres(sheet, name, margin) {
            *value = millimetres;
        }
    }
    if let Some(StyleValue::Float(scale)) = sheet.get(SCALE)
        && scale.is_finite()
        && *scale > 0.0
    {
        setup.score_size_proportion = scale.clamp(0.25, 4.0);
    }
    for (name, bit) in [
        (PAGE_NUMBER, HEADER_FOOTER_PAGE_NUMBER),
        (COPYRIGHT, HEADER_FOOTER_COPYRIGHT),
        (RUNNING_HEADER, HEADER_FOOTER_RUNNING_HEADER),
    ] {
        match flag(sheet, name) {
            Some(true) => setup.header_and_footer |= bit,
            Some(false) => setup.header_and_footer &= !bit,
            None => {}
        }
    }
}

/// Writes `setup`'s page size, margins, score size and header and footer switches into
/// `sheet` (the inverse of [`apply_page_setup`]).
pub(crate) fn store_page_setup(setup: &PageSetup, sheet: &mut Stylesheet) {
    let (x, y) = (setup.page_size.x, setup.page_size.y);
    let landscape = x > y;
    let (width, height) = if landscape { (y, x) } else { (x, y) };
    sheet.set(WIDTH, StyleValue::Float(f32::from(width)));
    sheet.set(HEIGHT, StyleValue::Float(f32::from(height)));
    sheet.set(ORIENTATION, StyleValue::Int(i32::from(landscape)));
    sheet.set(TOP, StyleValue::Float(f32::from(setup.page_margin.top)));
    sheet.set(LEFT, StyleValue::Float(f32::from(setup.page_margin.left)));
    sheet.set(RIGHT, StyleValue::Float(f32::from(setup.page_margin.right)));
    sheet.set(
        BOTTOM,
        StyleValue::Float(f32::from(setup.page_margin.bottom)),
    );
    sheet.set(SCALE, StyleValue::Float(setup.score_size_proportion));
    for (name, bit) in [
        (PAGE_NUMBER, HEADER_FOOTER_PAGE_NUMBER),
        (COPYRIGHT, HEADER_FOOTER_COPYRIGHT),
        (RUNNING_HEADER, HEADER_FOOTER_RUNNING_HEADER),
    ] {
        sheet.set(name, StyleValue::Bool(setup.header_and_footer & bit != 0));
    }
}

/// The style sheet to save for `setup`: the file's own one updated, or (for a file that
/// had none) only the page keys when the page setup is not the default.
pub(crate) fn for_saving(original: Option<&Stylesheet>, setup: &PageSetup) -> Option<Stylesheet> {
    let mut sheet = match original {
        Some(sheet) => sheet.clone(),
        None if setup.same_layout(&PageSetup::default()) => return None,
        None => Stylesheet::default(),
    };
    store_page_setup(setup, &mut sheet);
    Some(sheet)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> Stylesheet {
        Stylesheet {
            entries: vec![
                ("Staff/repeatSpace".to_owned(), StyleValue::Float(0.5)),
                (
                    "Header/Tabber".to_owned(),
                    StyleValue::Text("Tabbed by %TABBER%".to_owned()),
                ),
                ("Global/TimerPriority".to_owned(), StyleValue::Int(12)),
                (
                    "Global/shadowColorEnd".to_owned(),
                    StyleValue::Color([0x5a, 0x5a, 0x5a, 0x0a]),
                ),
                (WIDTH.to_owned(), StyleValue::Float(210.0)),
                (HEIGHT.to_owned(), StyleValue::Float(297.0)),
                (ORIENTATION.to_owned(), StyleValue::Int(0)),
                (TOP.to_owned(), StyleValue::Float(15.0)),
                (PAGE_NUMBER.to_owned(), StyleValue::Bool(true)),
                (RUNNING_HEADER.to_owned(), StyleValue::Bool(false)),
            ],
        }
    }

    #[test]
    fn a_style_sheet_is_written_back_byte_for_byte() {
        let bytes = write(&sheet());
        assert_eq!(read(&bytes), Some(sheet()));
        assert_eq!(write(&read(&bytes).unwrap()), bytes);
    }

    #[test]
    fn broken_style_sheets_are_ignored() {
        let bytes = write(&sheet());
        for cut in 0..bytes.len() {
            assert_eq!(read(&bytes[..cut]), None, "cut at {cut}");
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(read(&longer), None, "bytes left over");
        let mut unknown = bytes;
        // The first entry's type byte: after the count and the name.
        let at = 4 + 1 + "Staff/repeatSpace".len();
        unknown[at] = 9;
        assert_eq!(read(&unknown), None, "unknown value type");
        assert_eq!(read(&[0xff, 0xff, 0xff, 0xff]), None, "too many entries");
    }

    #[test]
    fn the_page_setup_goes_both_ways_and_landscape_swaps_the_size() {
        let mut setup = PageSetup::default();
        apply_page_setup(&sheet(), &mut setup);
        assert_eq!((setup.page_size.x, setup.page_size.y), (210, 297));
        assert_eq!(setup.page_margin.top, 15);
        assert_ne!(setup.header_and_footer & HEADER_FOOTER_PAGE_NUMBER, 0);

        setup.page_size.x = 297;
        setup.page_size.y = 210;
        setup.page_margin.left = 20;
        setup.score_size_proportion = 0.8;
        setup.header_and_footer |= HEADER_FOOTER_RUNNING_HEADER;
        setup.header_and_footer &= !HEADER_FOOTER_PAGE_NUMBER;
        let mut stored = sheet();
        store_page_setup(&setup, &mut stored);
        assert_eq!(stored.get(ORIENTATION), Some(&StyleValue::Int(1)));
        assert_eq!(
            stored.get(WIDTH),
            Some(&StyleValue::Float(210.0)),
            "portrait width"
        );
        // Entries the page setup does not touch stay where they were.
        assert_eq!(stored.entries[..4], sheet().entries[..4]);
        let mut back = PageSetup::default();
        apply_page_setup(&read(&write(&stored)).unwrap(), &mut back);
        assert!(back.same_layout(&setup), "{back:?}");
    }

    #[test]
    fn odd_values_are_clamped_or_ignored() {
        let odd = Stylesheet {
            entries: vec![
                (WIDTH.to_owned(), StyleValue::Float(f32::NAN)),
                (HEIGHT.to_owned(), StyleValue::Float(297.0)),
                (TOP.to_owned(), StyleValue::Float(1e9)),
                (SCALE.to_owned(), StyleValue::Float(-1.0)),
                (PAGE_NUMBER.to_owned(), StyleValue::Int(1)),
            ],
        };
        let mut setup = PageSetup::default();
        apply_page_setup(&odd, &mut setup);
        assert!(setup.same_layout(&PageSetup {
            page_margin: crate::model::page::Padding {
                top: 500,
                ..PageSetup::default().page_margin
            },
            ..PageSetup::default()
        }));
    }

    #[test]
    fn a_default_page_needs_no_style_sheet() {
        assert_eq!(for_saving(None, &PageSetup::default()), None);
        let mut landscape = PageSetup::default();
        landscape.page_size.x = 297;
        landscape.page_size.y = 210;
        let written = for_saving(None, &landscape).unwrap();
        assert_eq!(written.get(ORIENTATION), Some(&StyleValue::Int(1)));
    }
}
