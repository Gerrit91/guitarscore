//! Editing the page setup, line and page breaks, and the bars per line.

use gscore_gp::LineBreak;
use gscore_gp::page::{
    HEADER_FOOTER_COPYRIGHT, HEADER_FOOTER_PAGE_NUMBER, HEADER_FOOTER_RUNNING_HEADER,
};

use super::{Change, ChangeKind, track_mut};
use crate::error::ModelError;
use crate::page::{MAX_BARS_PER_SYSTEM, Margins, PageSetup, SystemBreak};
use crate::position::{MeasureIndex, TrackIndex};
use crate::song::Song;

/// The page setup as Guitar Pro stores it: `setup` written into `gp` (its texts stay).
pub(crate) fn store(setup: &PageSetup, gp: &mut gscore_gp::page::PageSetup) {
    gp.page_size.x = setup.width_mm;
    gp.page_size.y = setup.height_mm;
    gp.page_margin.top = setup.margins.top;
    gp.page_margin.right = setup.margins.right;
    gp.page_margin.bottom = setup.margins.bottom;
    gp.page_margin.left = setup.margins.left;
    gp.score_size_proportion = f32::from(setup.scale_percent) / 100.0;
    for (on, bit) in [
        (setup.page_numbers, HEADER_FOOTER_PAGE_NUMBER),
        (setup.copyright, HEADER_FOOTER_COPYRIGHT),
        (setup.running_header, HEADER_FOOTER_RUNNING_HEADER),
    ] {
        if on {
            gp.header_and_footer |= bit;
        } else {
            gp.header_and_footer &= !bit;
        }
    }
}

/// The page setup in `gp`, as stored (not checked).
pub(crate) fn load(gp: &gscore_gp::page::PageSetup) -> PageSetup {
    PageSetup {
        width_mm: gp.page_size.x,
        height_mm: gp.page_size.y,
        margins: Margins {
            top: gp.page_margin.top,
            right: gp.page_margin.right,
            bottom: gp.page_margin.bottom,
            left: gp.page_margin.left,
        },
        scale_percent: percent(gp.score_size_proportion),
        page_numbers: gp.header_and_footer & HEADER_FOOTER_PAGE_NUMBER != 0,
        copyright: gp.header_and_footer & HEADER_FOOTER_COPYRIGHT != 0,
        running_header: gp.header_and_footer & HEADER_FOOTER_RUNNING_HEADER != 0,
    }
}

/// A score size factor in whole percent (odd values give 100, to be checked later).
fn percent(proportion: f32) -> u16 {
    let value = (proportion * 100.0).round();
    if value.is_finite() && (0.0..=f32::from(u16::MAX)).contains(&value) {
        // In range of u16 (checked just above), so the cast cannot truncate.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let value = value as u16;
        value
    } else {
        100
    }
}

pub(crate) fn line_break(kind: SystemBreak) -> LineBreak {
    match kind {
        SystemBreak::None => LineBreak::None,
        SystemBreak::Line => LineBreak::Break,
        SystemBreak::Page => LineBreak::Page,
    }
}

pub(crate) fn system_break(value: &LineBreak) -> SystemBreak {
    match value {
        LineBreak::Break => SystemBreak::Line,
        LineBreak::Page => SystemBreak::Page,
        LineBreak::None | LineBreak::Protect => SystemBreak::None,
    }
}

impl Song {
    /// Where lines start when every track is shown together (Guitar Pro's multitrack
    /// layout, kept apart from each track's own breaks): one entry per measure, `true` where
    /// a new line starts. Without such a layout (or past its end) nothing is forced.
    pub fn score_line_breaks(&self) -> Vec<bool> {
        let count = self.measure_count();
        let mut breaks = vec![false; count];
        let mut at = 0_usize;
        let layout = self
            .inner()
            .score_systems_layout
            .as_deref()
            .unwrap_or_default();
        for bars in layout.split_whitespace() {
            let Some(bars) = bars.parse::<usize>().ok().filter(|bars| *bars > 0) else {
                break;
            };
            at = at.saturating_add(bars);
            match breaks.get_mut(at) {
                Some(slot) => *slot = true,
                None => break,
            }
        }
        breaks
    }

    /// How the song is printed, with odd values from a file moved into range.
    pub fn page_setup(&self) -> PageSetup {
        load(&self.inner().page_setup).sanitized()
    }

    /// Sets how the song is printed (checked with [`PageSetup::check`]).
    pub fn set_page_setup(&mut self, setup: PageSetup) -> Result<Change, ModelError> {
        setup.check()?;
        self.edit_metadata(|inner| {
            let previous = inner.page_setup.clone();
            store(&setup, &mut inner.page_setup);
            Ok(Change {
                kind: ChangeKind::PageSetup {
                    previous: Box::new(previous),
                },
            })
        })
    }

    /// Makes measure `measure` of `track` start a new line or page (or neither). The first
    /// measure cannot: it starts the score.
    pub fn set_system_break(
        &mut self,
        track: TrackIndex,
        measure: MeasureIndex,
        kind: SystemBreak,
    ) -> Result<Change, ModelError> {
        if measure.get() == 0 && kind != SystemBreak::None {
            return Err(ModelError::Invalid(
                "the first measure always starts the score".to_owned(),
            ));
        }
        self.edit_track_settings(|inner| {
            let slot = track_mut(inner, track)?
                .measures
                .get_mut(measure.get())
                .ok_or_else(|| ModelError::Invalid(format!("{measure}: no such measure")))?;
            let previous = std::mem::replace(&mut slot.line_break, line_break(kind));
            Ok(Change {
                kind: ChangeKind::SystemBreak {
                    track,
                    measure,
                    previous,
                },
            })
        })
    }

    /// Puts at most `bars` measures on a line of `track` (1 to 32), or `None` for as many
    /// as fit.
    pub fn set_bars_per_system(
        &mut self,
        track: TrackIndex,
        bars: Option<u8>,
    ) -> Result<Change, ModelError> {
        if let Some(bars) = bars
            && !(1..=MAX_BARS_PER_SYSTEM).contains(&bars)
        {
            return Err(ModelError::Invalid(format!(
                "{bars} bars per line is outside 1 to {MAX_BARS_PER_SYSTEM}"
            )));
        }
        self.edit_track_settings(|inner| {
            let slot = &mut track_mut(inner, track)?.bars_per_system;
            let previous = std::mem::replace(slot, bars);
            Ok(Change {
                kind: ChangeKind::BarsPerSystem { track, previous },
            })
        })
    }
}
