//! Note and beat ornaments drawn on the tab staff.
//!
//! Guitar Pro writes most ornaments in standard notation above the tab. A tab-only layout
//! keeps their meaning with the usual tab conventions: curved slurs for hammer-ons and
//! legato slides, a slanted line for a shift slide, slashes at the fret for slides in and
//! out, wiggles above the staff for vibrato, angle brackets for harmonics, and `P.M.` /
//! `let ring` rows below the staff. Bend arrows and the whammy-bar line rise above the staff.
//!
//! This module holds what a note remembers about its ornaments; [`draw`] paints it.

mod lines;
mod notes;

pub(crate) use lines::{
    draw_legato_run, draw_link, draw_tie, draw_tie_fragment, draw_under_staff_run, draw_whammy,
};
pub(crate) use notes::draw_vibrato;

use gscore_model::{AccentKind, Bend, Harmonic, HarmonicKind, SlideKind, Vibrato, WhammyShape};

/// Thickness of slurs, slides and arrows.
const LINE_THICKNESS: f32 = 0.15;

/// The effects attached to one fret number.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NoteMarks {
    /// Hammer-on or pull-off origin.
    pub(crate) hammer: bool,
    /// The note's bend, if any.
    pub(crate) bend: Option<Bend>,
    /// All slides, in file order.
    pub(crate) slides: Vec<SlideKind>,
    /// The harmonic and its node.
    pub(crate) harmonic: Option<Harmonic>,
    /// A normal or heavy accent.
    pub(crate) accent: Option<AccentKind>,
    /// A staccato dot.
    pub(crate) staccato: bool,
    /// A trill.
    pub(crate) trill: bool,
    /// Tremolo picking.
    pub(crate) tremolo_picking: bool,
    /// Palm mute (an under-staff row).
    pub(crate) palm_mute: bool,
    /// Let ring (an under-staff row).
    pub(crate) let_ring: bool,
}

impl NoteMarks {
    /// Marks with nothing set; the note only carries a fret.
    pub(crate) fn new() -> Self {
        NoteMarks {
            hammer: false,
            bend: None,
            slides: Vec::new(),
            harmonic: None,
            accent: None,
            staccato: false,
            trill: false,
            tremolo_picking: false,
            palm_mute: false,
            let_ring: false,
        }
    }

    /// The slide that continues on the next note, if there is one.
    pub(crate) fn direct_slide(&self) -> Option<SlideKind> {
        self.slides
            .iter()
            .copied()
            .find(|slide| matches!(slide, SlideKind::ShiftSlideTo | SlideKind::LegatoSlideTo))
    }

    fn slides_in_from_above(&self) -> bool {
        self.slides.contains(&SlideKind::IntoFromAbove)
    }

    fn slides_in_from_below(&self) -> bool {
        self.slides.contains(&SlideKind::IntoFromBelow)
    }

    fn slides_out_downwards(&self) -> bool {
        self.slides.contains(&SlideKind::OutDownwards)
    }

    fn slides_out_upwards(&self) -> bool {
        self.slides.contains(&SlideKind::OutUpwards)
    }

    /// Extra room the slashes before and after the number need.
    fn extra_width(&self) -> f32 {
        let left = self.slides_in_from_above() || self.slides_in_from_below();
        let right = self.slides_out_downwards() || self.slides_out_upwards();
        let left = if left { SLASH_REACH + SLASH_GAP } else { 0.0 };
        let right = if right { SLASH_REACH + SLASH_GAP } else { 0.0 };
        left + right
    }
}

/// Width of one slash beside a fret number.
const SLASH_REACH: f32 = 0.45;
/// Gap between a fret number and its slide slash.
const SLASH_GAP: f32 = 0.2;
/// Extra room around a fret number's mask, horizontally and vertically.
pub(crate) const MASK_PAD: (f32, f32) = (0.15, 0.2);

/// One fret number and its ornaments.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fret {
    /// The staff line (0 = top).
    pub(crate) line: usize,
    /// The text shown (ghost notes in parentheses, harmonics in brackets, ...).
    pub(crate) text: String,
    /// Text width in staff spaces.
    pub(crate) width: f32,
    /// Effects attached to the note.
    pub(crate) marks: NoteMarks,
    /// The fret number, when the model can read one; used to tell a hammer-on from a
    /// pull-off.
    pub(crate) fret: Option<u8>,
    /// A tie continuation: it reserves its column for the tie curve but shows no number.
    pub(crate) tie_destination: bool,
}

impl Fret {
    /// Half the space the note needs, including its mask and slide slashes.
    pub(crate) fn half_width(&self) -> f32 {
        if self.tie_destination {
            return 0.0;
        }
        self.width / 2.0 + MASK_PAD.0 + self.marks.extra_width() / 2.0
    }
}

/// Beat-level marks on a column.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ColumnMarks {
    /// This column starts a beat-level legato run.
    pub(crate) legato_origin: bool,
    /// This column continues a beat-level legato run.
    pub(crate) legato_destination: bool,
    /// The whammy-bar shape on this column.
    pub(crate) whammy: Option<WhammyShape>,
    /// The widest vibrato of the beat or its notes; drawn once per column.
    pub(crate) vibrato: Option<Vibrato>,
}

impl ColumnMarks {
    /// Adds a vibrato from a beat or a note; wide wins over slight.
    pub(crate) fn add_vibrato(&mut self, vibrato: Option<Vibrato>) {
        let Some(vibrato) = vibrato else {
            return;
        };
        self.vibrato = match (self.vibrato, vibrato) {
            (Some(Vibrato::Wide), _) | (_, Vibrato::Wide) => Some(Vibrato::Wide),
            _ => Some(Vibrato::Slight),
        };
    }
}

/// The text of a fret number, including ghost-note parentheses and harmonic brackets.
pub(crate) fn fret_text(base: &str, marks: &NoteMarks) -> String {
    let mut text = base.to_owned();
    if let Some(harmonic) = marks.harmonic {
        text = match harmonic.kind {
            HarmonicKind::Natural => format!("<{text}>"),
            HarmonicKind::Tapped => format!("{text} ({})", node_text(harmonic.node)),
            HarmonicKind::Artificial | HarmonicKind::Pinch | HarmonicKind::Semi => {
                format!("{text} <{}>", node_text(harmonic.node))
            }
        };
    }
    text
}

fn node_text(node: Option<u8>) -> String {
    node.map_or_else(|| "?".to_owned(), |node| node.to_string())
}
