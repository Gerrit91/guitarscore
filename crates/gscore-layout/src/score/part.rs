//! One track of a score, set up for layout: its staves, its measures and marks, and
//! what a system needs to know about it (the room before its first measure, how far its
//! notes reach beyond its staves, its name in the margin).

use std::sync::Arc;

use gscore_model::{Song, TrackIndex, TrackView};

use super::NAME_SIZE;
use crate::marks::MeasureMarks;
use crate::measure::MeasureLayout;
use crate::notation::{self, Notation};
use crate::staff::Staff;
use crate::{Context, Fonts, LayoutError, LayoutOptions, MeasureSource};

/// One track of the score, set up for layout.
pub(super) struct Part<'a> {
    pub(super) track: TrackIndex,
    pub(super) view: TrackView<'a>,
    pub(super) ctx: Context<'a>,
    pub(super) show_tab: bool,
    pub(super) measures: Vec<Arc<MeasureLayout>>,
    pub(super) marks: Vec<MeasureMarks>,
    /// From the top staff's top line to the tab's (0 without notation).
    pub(super) tab_offset: f32,
    /// From the top staff's top line to the lowest staff's bottom line.
    pub(super) staves_height: f32,
}

impl<'a> Part<'a> {
    pub(super) fn new(
        song: &'a Song,
        track: TrackIndex,
        options: &'a LayoutOptions,
        fonts: &'a Fonts,
        measure_layout: &mut MeasureSource<'_>,
    ) -> Result<Self, LayoutError> {
        let view = song.track(track).ok_or(LayoutError::NoSuchTrack(track))?;
        let staves = view.staves();
        // Drums are always written on a percussion staff: their "frets" are drum sounds,
        // which a tab staff cannot show.
        let drums = view.is_percussion();
        let notation = if drums || (options.notation && staves.notation()) {
            Notation::for_track(view)?
        } else {
            None
        };
        // A track without notation (none possible, or switched off) always shows its tab.
        let show_tab = !drums && (staves.tablature() || notation.is_none());
        let ctx = Context {
            options,
            fonts,
            staff: Staff::new(view.string_count(), options.tab_line_spacing),
            notation,
        };
        let mut measures = Vec::with_capacity(view.measures().len());
        let mut previous_signature = None;
        let mut previous_key = None;
        for measure in view.measures() {
            let signature = measure.time_signature()?;
            let show = previous_signature != Some(signature);
            let key = song.key_signature(measure.index()).map_or(0, |key| key.key);
            measures.push(measure_layout(
                track,
                measure,
                show,
                key,
                previous_key,
                &ctx,
            )?);
            previous_signature = Some(signature);
            previous_key = Some(key);
        }
        let notation_staff = notation::staff();
        // With notation, the tab sits below the notation staff and the gap between them.
        let tab_offset = if ctx.notation.is_some() {
            notation_staff.height() + options.notation_gap
        } else {
            0.0
        };
        let staves_height = if show_tab {
            tab_offset + ctx.staff.height()
        } else {
            notation_staff.height()
        };
        Ok(Part {
            track,
            view,
            ctx,
            show_tab,
            measures,
            marks: Vec::new(),
            tab_offset,
            staves_height,
        })
    }

    /// The room a system starting with measure `index` needs before its first measure:
    /// the clef, and the key signature when the measure does not draw one of its own.
    pub(super) fn prefix(&self, index: usize, key: i8) -> f32 {
        let ctx = &self.ctx;
        let drawn_by_measure = self
            .measures
            .get(index)
            .is_some_and(|measure| measure.has_key_signature());
        let notation_prefix = ctx.notation.map_or(0.0, |notation| {
            if drawn_by_measure {
                notation::key_start_width(notation.clef, ctx)
            } else {
                notation::prefix_width(notation.clef, key, false, ctx)
            }
        });
        let tab_prefix = if self.show_tab {
            if ctx.notation.is_some() {
                // With a notation staff, the key signature is written there only.
                ctx.staff.prefix_width(&ctx.fonts.music)
            } else {
                notation::tab_prefix_width(key, drawn_by_measure, ctx)
            }
        } else {
            0.0
        };
        notation_prefix.max(tab_prefix)
    }

    /// How far the notation of `measures` reaches above the top staff and below the
    /// notation staff (0 without notation).
    pub(super) fn reach(&self, measures: std::ops::Range<usize>) -> (f32, f32) {
        if self.ctx.notation.is_none() {
            return (0.0, 0.0);
        }
        self.measures
            .get(measures)
            .unwrap_or_default()
            .iter()
            .map(|measure| measure.notation_reach())
            .fold((0.0_f32, 0.0_f32), |(a, b), (above, below)| {
                (a.max(above), b.max(below))
            })
    }

    /// The name written in the margin before each system: the short name (else the
    /// name), cut with "…" to fit `room`; empty when not even one letter fits.
    pub(super) fn margin_name(&self, room: f32) -> String {
        let short = self.view.short_name().trim();
        let name = if short.is_empty() {
            self.view.name().trim()
        } else {
            short
        };
        let width = |text: &str| self.ctx.fonts.text.width(text, NAME_SIZE);
        if width(name) <= room {
            return name.to_owned();
        }
        let chars: Vec<char> = name.chars().collect();
        (1..chars.len())
            .rev()
            .map(|keep| {
                let mut cut: String = chars.iter().take(keep).collect();
                cut.push('…');
                cut
            })
            .find(|cut| width(cut) <= room)
            .unwrap_or_default()
    }
}
