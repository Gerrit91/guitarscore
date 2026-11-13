//! One staff of a track. A track has one staff or more (a piano's treble and bass), each
//! with its own measures, strings and capo; staff 0 is the track's own (what
//! [`TrackView::measures`] and the track's tuning give). Every staff of every track has a
//! measure for each of the song's measures.

use crate::error::ModelError;
use crate::position::{MeasureIndex, Pitch, StaffIndex, StringNumber, TrackIndex};
use crate::views::{MeasureView, StringTuning, TrackView};

/// A staff of a track.
#[derive(Debug, Clone, Copy)]
pub struct StaffView<'a> {
    track: TrackIndex,
    index: StaffIndex,
    measures: &'a [gscore_gp::Measure],
    strings: &'a [(i8, i8)],
    has_tuning: bool,
    capo: i32,
}

impl<'a> StaffView<'a> {
    /// The track's staff `index`, if it has one.
    pub(crate) fn of(track: &TrackView<'a>, index: StaffIndex) -> Option<Self> {
        let inner = track.inner;
        let (measures, strings, has_tuning, capo) = match index.get() {
            0 => (
                inner.measures.as_slice(),
                inner.strings.as_slice(),
                inner.has_tuning,
                inner.offset,
            ),
            n => {
                let staff = inner.staves.get(n.checked_sub(1)?)?;
                (
                    staff.measures.as_slice(),
                    staff.strings.as_slice(),
                    staff.has_tuning,
                    staff.offset,
                )
            }
        };
        Some(StaffView {
            track: track.index(),
            index,
            measures,
            strings,
            has_tuning,
            capo,
        })
    }

    /// The track it belongs to.
    pub fn track(&self) -> TrackIndex {
        self.track
    }

    /// Its position in the track.
    pub fn index(&self) -> StaffIndex {
        self.index
    }

    /// All its measures, in order.
    pub fn measures(&self) -> impl ExactSizeIterator<Item = MeasureView<'a>> + use<'a> {
        self.measures
            .iter()
            .enumerate()
            .map(|(i, m)| MeasureView::new(MeasureIndex::new(i), m))
    }

    /// One measure, or `None` if the index is out of range.
    pub fn measure(&self, index: MeasureIndex) -> Option<MeasureView<'a>> {
        self.measures
            .get(index.get())
            .map(|m| MeasureView::new(index, m))
    }

    /// How many strings its notes are placed on.
    pub fn string_count(&self) -> usize {
        self.strings.len()
    }

    /// The strings its notes are written on (1 = highest) and their open pitches: a
    /// string instrument's tuning, or for a keyboard's or a voice's staff the stand-in
    /// tuning Guitar Pro stores, whose strings and frets give the notes' pitches. `None`
    /// when the file stores none.
    pub fn note_tuning(&self) -> Result<Option<Vec<StringTuning>>, ModelError> {
        if !self.has_tuning {
            return Ok(None);
        }
        self.strings
            .iter()
            .map(|&(string, pitch)| {
                Ok(StringTuning {
                    string: StringNumber::from_gp(string)?,
                    pitch: Pitch::from_gp(pitch)?,
                })
            })
            .collect::<Result<Vec<_>, ModelError>>()
            .map(Some)
    }

    /// The capo fret, as stored.
    pub fn capo_fret(&self) -> i32 {
        self.capo
    }
}

impl<'a> TrackView<'a> {
    /// How many staves the track has (at least one).
    pub fn staff_count(&self) -> usize {
        self.inner.staves.len().saturating_add(1)
    }

    /// Staff `index`, or `None` if the track has no such staff.
    pub fn staff(&self, index: StaffIndex) -> Option<StaffView<'a>> {
        StaffView::of(self, index)
    }

    /// Every staff, the first first. (Not [`TrackView::staves`], which says which kinds of
    /// staff the track's page shows.)
    pub fn all_staves(&self) -> impl Iterator<Item = StaffView<'a>> + use<'a> {
        let track = *self;
        (0..self.staff_count()).filter_map(move |index| track.staff(StaffIndex::new(index)))
    }
}
