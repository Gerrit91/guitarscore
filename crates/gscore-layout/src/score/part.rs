//! One track of a score, set up for layout: its staves, its measures and marks, and
//! what a system needs to know about it (the room before its first measure, how far its
//! notes reach beyond its staves, its name in the margin).

use std::sync::Arc;

use gscore_model::{Song, TrackIndex, TrackView};

use super::NAME_SIZE;
use crate::marks::MeasureMarks;
use crate::measure::MeasureLayout;
use crate::notation::{self, Clef, Notation};
use crate::staff::Staff;
use crate::{Context, Fonts, LayoutError, LayoutOptions, MeasureSource};

/// One track of the score, set up for layout.
pub(super) struct Part<'a> {
    pub(super) track: TrackIndex,
    pub(super) view: TrackView<'a>,
    pub(super) ctx: Context<'a>,
    pub(super) show_tab: bool,
    /// Whether a slash-notation staff is shown below the other staves.
    pub(super) show_slash: bool,
    pub(super) measures: Vec<Arc<MeasureLayout>>,
    pub(super) marks: Vec<MeasureMarks>,
    /// From the top staff's top line to the tab's at the least (0 without notation): the
    /// gap between the staves grows in a system whose low notes or dynamics need it.
    tab_offset: f32,
}

/// Room between the lowest note of the notation staff and the tab's top line, for what
/// is written just above the tab (bends, the tab's own marks).
const NOTES_ABOVE_TAB: f32 = 2.0;

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
        // Slash notation is a staff of its own, with or without notation and tab. Drums
        // already have their percussion staff.
        let slash = !drums && staves.slash();
        // A track showing neither notation nor slash always keeps its tab.
        let show_tab = !drums && (staves.tablature() || (notation.is_none() && !slash));
        let ctx = Context {
            options,
            fonts,
            staff: Staff::new(view.string_count(), options.tab_line_spacing),
            notation,
            slash,
        };
        let mut measures = Vec::with_capacity(view.measures().len());
        let mut previous_signature = None;
        let mut previous_key = None;
        let mut previous_clef = None;
        for measure in view.measures() {
            let signature = measure.time_signature()?;
            let show = previous_signature != Some(signature);
            let key = song.key_signature(measure.index()).map_or(0, |key| key.key);
            let clef = match ctx.notation {
                Some(notation) if notation.clef != Clef::Percussion => {
                    measure.clef().map_or(notation.clef, notation::clef_of)
                }
                Some(notation) => notation.clef,
                None => Clef::Treble,
            };
            measures.push(measure_layout(
                track,
                measure,
                show,
                song.free_time(measure.index()).unwrap_or(false),
                key,
                previous_key,
                clef,
                previous_clef,
                &ctx,
            )?);
            previous_signature = Some(signature);
            previous_key = Some(key);
            previous_clef = Some(clef);
        }
        let notation_staff = notation::staff();
        // With notation, the tab sits below the notation staff and the gap between them.
        let tab_offset = if ctx.notation.is_some() {
            notation_staff.height() + options.notation_gap
        } else {
            0.0
        };
        Ok(Part {
            track,
            view,
            ctx,
            show_tab,
            show_slash: slash,
            measures,
            marks: Vec::new(),
            tab_offset,
        })
    }

    /// From the top staff's top line to the tab's in a system of `measures`: the gap
    /// between notation and tab is the options' gap, or more where the notes hang lower or
    /// a dynamic under them needs room above the tab.
    pub(super) fn tab_offset(&self, measures: std::ops::Range<usize>) -> f32 {
        if self.ctx.notation.is_none() || !self.show_tab {
            return self.tab_offset;
        }
        let notation_height = notation::staff().height();
        let gap = self.tab_offset - notation_height;
        let layouts = self.measures.get(measures.clone()).unwrap_or_default();
        let marks = self.marks.get(measures).unwrap_or_default();
        let needed = layouts
            .iter()
            .enumerate()
            .map(|(index, layout)| {
                let notes = layout.notation_reach().1 + NOTES_ABOVE_TAB;
                let dynamics = marks.get(index).map_or(0.0, |marks| {
                    crate::marks::dynamics_depth(layout, marks, &self.ctx)
                        + crate::marks::DYNAMIC_ABOVE_TAB
                });
                notes.max(dynamics)
            })
            .fold(gap, f32::max);
        notation_height + needed
    }

    /// From the top staff's top line to the slash staff's top line in a system of
    /// `measures`.
    pub(super) fn slash_offset(&self, measures: std::ops::Range<usize>) -> f32 {
        if !self.show_slash {
            return 0.0;
        }
        if self.show_tab {
            self.tab_offset(measures.clone())
                + self.ctx.staff.height()
                + self.ctx.options.notation_gap
        } else {
            // Below the notation staff (tab_offset includes its gap), or at the top.
            self.tab_offset(measures)
        }
    }

    /// From the top staff's top line to the lowest staff's bottom line, in a system of
    /// `measures`.
    pub(super) fn staves_height(&self, measures: std::ops::Range<usize>) -> f32 {
        if self.show_slash {
            self.slash_offset(measures) + notation::staff().height()
        } else if self.show_tab {
            self.tab_offset(measures) + self.ctx.staff.height()
        } else {
            notation::staff().height()
        }
    }

    /// The room a system starting with measure `index` needs before its first measure:
    /// the clef, and the key signature when the measure does not draw one of its own.
    pub(super) fn prefix(&self, index: usize, key: i8) -> f32 {
        let ctx = &self.ctx;
        let drawn_by_measure = self
            .measures
            .get(index)
            .is_some_and(|measure| measure.has_key_signature());
        let clef = self
            .measures
            .get(index)
            .map_or(Clef::Treble, |measure| measure.clef());
        let notation_prefix = ctx.notation.map_or(0.0, |_| {
            if drawn_by_measure {
                notation::key_start_width(clef, ctx)
            } else {
                notation::prefix_width(clef, key, false, ctx)
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
        let slash_prefix = if self.show_slash && !self.show_tab && ctx.notation.is_none() {
            let clef = ctx.notation.map_or(Clef::Treble, |notation| notation.clef);
            if drawn_by_measure {
                notation::key_start_width(clef, ctx)
            } else {
                notation::prefix_width(clef, key, false, ctx)
            }
        } else {
            0.0
        };
        notation_prefix.max(tab_prefix).max(slash_prefix)
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

    /// Whether any of `measures` holds a note (a staff that is only rests is empty).
    pub(super) fn has_notes(&self, measures: std::ops::Range<usize>) -> bool {
        self.measures
            .get(measures)
            .unwrap_or_default()
            .iter()
            .any(|measure| measure.has_notes())
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
