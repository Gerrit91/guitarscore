//! One track of a score, set up for layout: its staves, its measures and marks, and
//! what a system needs to know about it (the room before its first measure, how far its
//! notes reach beyond its staves, its name in the margin).

use std::sync::Arc;

use gscore_model::{LyricSyllable, Song, TrackIndex, TrackView};

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
    /// Whether a slash-notation staff is shown above the other staves.
    pub(super) show_slash: bool,
    pub(super) measures: Vec<Arc<MeasureLayout>>,
    pub(super) marks: Vec<MeasureMarks>,
    /// The lyric syllables of each measure, in measure order.
    pub(super) lyrics: Vec<Vec<LyricSyllable>>,
    /// From the top staff's top line to the tab's at the least (0 without notation): the
    /// gap between the staves grows in a system whose low notes or dynamics need it.
    tab_offset: f32,
}

/// Room between the lowest note of the notation staff and the tab's top line, for what
/// is written just above the tab (bends, the tab's own marks).
const NOTES_ABOVE_TAB: f32 = 2.0;

/// The slash staff's height: one line in the middle of a five-line staff's room, so its
/// slashes, stems and rests sit as on a notation staff.
const SLASH_HEIGHT: f32 = 4.0;
/// Room between the slash staff's room and what the staff below it reaches up to.
const SLASH_GAP: f32 = 3.0;
/// How far the slash staff's stems and beams reach above its room.
const SLASH_REACH: f32 = 1.5;

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
        let mut lyrics: Vec<Vec<LyricSyllable>> = vec![Vec::new(); view.measures().len()];
        for syllable in song.lyric_syllables(track) {
            if let Some(slot) = lyrics.get_mut(syllable.measure.get()) {
                slot.push(syllable);
            }
        }
        let mut previous_signature = None;
        let mut previous_key = None;
        let mut previous_clef = None;
        let mut previous_free = false;
        for measure in view.measures() {
            let signature = measure.time_signature()?;
            let free_time = song.free_time(measure.index()).unwrap_or(false);
            // Where free time starts, its signature is written in parentheses even when it
            // did not change. Bars inside a free-time run do not repeat it, and the bar after
            // the run does not either (as Guitar Pro prints it).
            let show = previous_signature != Some(signature) || (free_time && !previous_free);
            previous_free = free_time;
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
                free_time,
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
            lyrics,
            tab_offset,
        })
    }

    /// From the top staff's top line (the slash staff's, when shown) to the notation
    /// staff's (or without notation, the tab's) in a system of `measures`: 0 without a
    /// slash staff; else the slash staff, the gap and what the notes below reach above
    /// their staff.
    pub(super) fn below_slash(&self, measures: std::ops::Range<usize>) -> f32 {
        if !self.show_slash || (self.ctx.notation.is_none() && !self.show_tab) {
            return 0.0;
        }
        let notes_above = if self.ctx.notation.is_some() {
            self.notation_reach(measures).0
        } else {
            0.0
        };
        SLASH_HEIGHT + SLASH_GAP + notes_above
    }

    /// From the top staff's top line to the tab's in a system of `measures`: below the
    /// slash staff, the notation staff and the gap between notation and tab, which is the
    /// options' gap or more where the notes hang lower or a dynamic under them needs room
    /// above the tab.
    pub(super) fn tab_offset(&self, measures: std::ops::Range<usize>) -> f32 {
        let slash = self.below_slash(measures.clone());
        if self.ctx.notation.is_none() || !self.show_tab {
            return slash + self.tab_offset;
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
        slash + notation_height + needed
    }

    /// From the top staff's top line to the lowest staff's bottom line, in a system of
    /// `measures`.
    pub(super) fn staves_height(&self, measures: std::ops::Range<usize>) -> f32 {
        let staves = if self.show_tab {
            self.tab_offset(measures.clone()) + self.ctx.staff.height()
        } else if self.ctx.notation.is_some() {
            self.below_slash(measures.clone()) + notation::staff().height()
        } else {
            SLASH_HEIGHT
        };
        staves + self.lyric_depth(measures)
    }

    /// The room the lyrics under the lowest staff take in a system of `measures`.
    fn lyric_depth(&self, measures: std::ops::Range<usize>) -> f32 {
        const GAP: f32 = 1.1;
        const LINE: f32 = 1.35;
        let lines = self
            .lyrics
            .get(measures)
            .unwrap_or_default()
            .iter()
            .flatten()
            .map(|syllable| usize::from(syllable.line).saturating_add(1))
            .max()
            .unwrap_or(0);
        if lines == 0 {
            0.0
        } else {
            GAP + LINE * lines as f32
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
        // A slash staff has no clef or key signature (as Guitar Pro writes it); alone, it
        // only needs a little room before the first measure.
        let slash_prefix = if self.show_slash {
            notation::SLASH_PREFIX
        } else {
            0.0
        };
        notation_prefix.max(tab_prefix).max(slash_prefix)
    }

    /// How far `measures` reach above the top staff (the slash staff's stems, or the
    /// notation's notes) and below the notation staff (0 without notation).
    pub(super) fn reach(&self, measures: std::ops::Range<usize>) -> (f32, f32) {
        let (above, below) = self.notation_reach(measures);
        if self.show_slash {
            (SLASH_REACH, below)
        } else {
            (above, below)
        }
    }

    /// How far the notation of `measures` reaches above and below the notation staff (0
    /// without notation).
    fn notation_reach(&self, measures: std::ops::Range<usize>) -> (f32, f32) {
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
