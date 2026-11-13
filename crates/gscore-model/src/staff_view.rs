//! One staff of a track. A track has one staff or more (a piano's treble and bass), each
//! with its own measures, strings and capo; staff 0 is the track's own (what
//! [`TrackView::measures`] and the track's tuning give). Every staff of every track has a
//! measure for each of the song's measures.

use crate::error::ModelError;
use crate::position::{MeasureIndex, StaffIndex, TrackIndex};
use crate::views::{MeasureView, StringTuning, TrackView};

/// A staff of a track: the track seen through it ([`TrackView::on_staff`]).
#[derive(Debug, Clone, Copy)]
pub struct StaffView<'a> {
    view: TrackView<'a>,
}

impl<'a> StaffView<'a> {
    /// The track's staff `index`, if it has one.
    pub(crate) fn of(track: &TrackView<'a>, index: StaffIndex) -> Option<Self> {
        track.on_staff(index).map(|view| StaffView { view })
    }

    /// The track seen through this staff: its measures, strings, tuning and capo are the
    /// staff's.
    pub fn view(&self) -> TrackView<'a> {
        self.view
    }

    /// The track it belongs to.
    pub fn track(&self) -> TrackIndex {
        self.view.index()
    }

    /// Its position in the track.
    pub fn index(&self) -> StaffIndex {
        self.view.staff_index()
    }

    /// All its measures, in order.
    pub fn measures(&self) -> impl ExactSizeIterator<Item = MeasureView<'a>> + use<'a> {
        self.view.measures()
    }

    /// One measure, or `None` if the index is out of range.
    pub fn measure(&self, index: MeasureIndex) -> Option<MeasureView<'a>> {
        self.view.measure(index)
    }

    /// How many strings its notes are placed on.
    pub fn string_count(&self) -> usize {
        self.view.string_count()
    }

    /// The strings its notes are written on (1 = highest) and their open pitches: a
    /// string instrument's tuning, or for a keyboard's or a voice's staff the stand-in
    /// tuning Guitar Pro stores, whose strings and frets give the notes' pitches. `None`
    /// when the file stores none (and for drums).
    pub fn note_tuning(&self) -> Result<Option<Vec<StringTuning>>, ModelError> {
        self.view.note_tuning()
    }

    /// The capo fret, as stored.
    pub fn capo_fret(&self) -> i32 {
        self.view.staff_data().offset
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
