//! Read-only views into a [`crate::Song`]: track, measure, voice, beat, note.
//!
//! A view borrows the song and knows its own index. Accessors that convert stored values
//! return `Result`: songs are validated on creation, so they succeed in practice, but the
//! types stay honest instead of hiding a fallback.

use guitarpro::{BeatStatus, NoteType};

use crate::duration::{Duration, TimeSignature};
use crate::error::ModelError;
use crate::instrument::{self, InstrumentKind};
use crate::position::{BeatIndex, Fret, MeasureIndex, Pitch, StringNumber, TrackIndex, VoiceIndex};

/// One string of a track and the pitch it is tuned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringTuning {
    /// The string (1 = highest).
    pub string: StringNumber,
    /// The open-string pitch.
    pub pitch: Pitch,
}

/// A track (instrument part).
#[derive(Debug, Clone, Copy)]
pub struct TrackView<'a> {
    index: TrackIndex,
    inner: &'a guitarpro::Track,
}

impl<'a> TrackView<'a> {
    pub(crate) fn new(index: TrackIndex, inner: &'a guitarpro::Track) -> Self {
        TrackView { index, inner }
    }

    /// This track's index in the song.
    pub fn index(&self) -> TrackIndex {
        self.index
    }

    /// The track name (any Unicode text, may be empty).
    pub fn name(&self) -> &'a str {
        &self.inner.name
    }

    /// Number of strings (4 for a bass, 6 for a guitar, ...).
    pub fn string_count(&self) -> usize {
        self.inner.strings.len()
    }

    /// The broad kind of instrument: string instrument, drums, or other.
    pub fn instrument_kind(&self) -> InstrumentKind {
        instrument::classify(
            self.inner.instrument_type.as_deref(),
            self.inner.percussion_track,
        )
    }

    /// The instrument's display name ("Electric Guitar", "Voice"), if the file stores one
    /// (GP7 and later).
    pub fn instrument_name(&self) -> Option<&'a str> {
        self.inner.instrument_name.as_deref()
    }

    /// The tuning, from string 1 (highest) down, or `None` if the track has none: it is
    /// not a string instrument (Guitar Pro still stores E standard for vocal and piano
    /// tracks), the file stores no tuning, or every string is at pitch 0 (drum kits).
    /// The track still has [`Self::string_count`] strings for placing notes.
    pub fn tuning(&self) -> Result<Option<Vec<StringTuning>>, ModelError> {
        if !self.inner.has_tuning || self.instrument_kind() != InstrumentKind::Strings {
            return Ok(None);
        }
        let strings = self.strings()?;
        if strings.iter().all(|s| s.pitch.get() == 0) {
            return Ok(None);
        }
        Ok(Some(strings))
    }

    /// The tuning's name: the one given in the file, or a recognized common tuning
    /// ("E standard", "Drop D", "Open G", "DADGAD", ...). `None` if the track has no tuning
    /// or it has no common name.
    pub fn tuning_name(&self) -> Result<Option<String>, ModelError> {
        let Some(strings) = self.tuning()? else {
            return Ok(None);
        };
        if let Some(label) = &self.inner.tuning_label {
            return Ok(Some(label.clone()));
        }
        let low_to_high: Vec<Pitch> = strings.iter().rev().map(|s| s.pitch).collect();
        Ok(crate::tuning::recognize(&low_to_high))
    }

    /// The strings as stored, including placeholder pitches of tracks without a tuning.
    pub(crate) fn strings(&self) -> Result<Vec<StringTuning>, ModelError> {
        self.inner
            .strings
            .iter()
            .map(|&(string, pitch)| {
                Ok(StringTuning {
                    string: StringNumber::from_gp(string)?,
                    pitch: Pitch::from_gp(pitch)?,
                })
            })
            .collect()
    }

    /// True for drum tracks, whose "frets" are MIDI drum notes.
    pub fn is_percussion(&self) -> bool {
        self.inner.percussion_track
    }

    /// All measures, in order.
    pub fn measures(&self) -> impl ExactSizeIterator<Item = MeasureView<'a>> {
        self.inner
            .measures
            .iter()
            .enumerate()
            .map(|(i, m)| MeasureView::new(MeasureIndex::new(i), m))
    }

    /// One measure, or `None` if the index is out of range.
    pub fn measure(&self, index: MeasureIndex) -> Option<MeasureView<'a>> {
        self.inner
            .measures
            .get(index.get())
            .map(|m| MeasureView::new(index, m))
    }
}

/// One measure (bar) of one track.
#[derive(Debug, Clone, Copy)]
pub struct MeasureView<'a> {
    index: MeasureIndex,
    inner: &'a guitarpro::Measure,
}

impl<'a> MeasureView<'a> {
    fn new(index: MeasureIndex, inner: &'a guitarpro::Measure) -> Self {
        MeasureView { index, inner }
    }

    /// This measure's index in the track.
    pub fn index(&self) -> MeasureIndex {
        self.index
    }

    /// The time signature in effect in this measure.
    pub fn time_signature(&self) -> Result<TimeSignature, ModelError> {
        TimeSignature::from_gp(&self.inner.time_signature)
    }

    /// All voices, in order.
    pub fn voices(&self) -> impl ExactSizeIterator<Item = VoiceView<'a>> {
        self.inner
            .voices
            .iter()
            .enumerate()
            .map(|(i, v)| VoiceView::new(VoiceIndex::new(i), v))
    }

    /// One voice, or `None` if the index is out of range.
    pub fn voice(&self, index: VoiceIndex) -> Option<VoiceView<'a>> {
        self.inner
            .voices
            .get(index.get())
            .map(|v| VoiceView::new(index, v))
    }
}

/// One voice of a measure: a sequence of beats.
#[derive(Debug, Clone, Copy)]
pub struct VoiceView<'a> {
    index: VoiceIndex,
    inner: &'a guitarpro::Voice,
}

impl<'a> VoiceView<'a> {
    fn new(index: VoiceIndex, inner: &'a guitarpro::Voice) -> Self {
        VoiceView { index, inner }
    }

    /// This voice's index in the measure.
    pub fn index(&self) -> VoiceIndex {
        self.index
    }

    /// All beats, in order.
    pub fn beats(&self) -> impl ExactSizeIterator<Item = BeatView<'a>> {
        self.inner
            .beats
            .iter()
            .enumerate()
            .map(|(i, b)| BeatView::new(BeatIndex::new(i), b))
    }

    /// One beat, or `None` if the index is out of range.
    pub fn beat(&self, index: BeatIndex) -> Option<BeatView<'a>> {
        self.inner
            .beats
            .get(index.get())
            .map(|b| BeatView::new(index, b))
    }

    /// True if the voice holds no beats except placeholders ([`BeatKind::Empty`]).
    pub fn is_empty(&self) -> bool {
        self.beats().all(|b| b.kind() == BeatKind::Empty)
    }

    /// The summed length of all beats; placeholders count as zero.
    pub fn total_duration(&self) -> Result<Duration, ModelError> {
        self.beats().try_fold(Duration::ZERO, |sum, beat| {
            if beat.kind() == BeatKind::Empty {
                Ok(sum)
            } else {
                sum.checked_add(beat.duration()?)
            }
        })
    }
}

/// What a beat contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeatKind {
    /// Notes (possibly none, while being entered).
    Normal,
    /// A rest.
    Rest,
    /// A placeholder without length (Guitar Pro stores these in unused voices).
    Empty,
}

/// One beat: a duration and the notes played together.
#[derive(Debug, Clone, Copy)]
pub struct BeatView<'a> {
    index: BeatIndex,
    inner: &'a guitarpro::Beat,
}

impl<'a> BeatView<'a> {
    fn new(index: BeatIndex, inner: &'a guitarpro::Beat) -> Self {
        BeatView { index, inner }
    }

    /// This beat's index in the voice.
    pub fn index(&self) -> BeatIndex {
        self.index
    }

    /// The beat's length, including dots and tuplets.
    pub fn duration(&self) -> Result<Duration, ModelError> {
        Duration::from_gp(&self.inner.duration)
    }

    /// Whether this is a normal beat, a rest, or a placeholder.
    pub fn kind(&self) -> BeatKind {
        match self.inner.status {
            BeatStatus::Normal => BeatKind::Normal,
            BeatStatus::Rest => BeatKind::Rest,
            BeatStatus::Empty => BeatKind::Empty,
        }
    }

    /// The notes of this beat (at most one per string).
    pub fn notes(&self) -> impl ExactSizeIterator<Item = NoteView<'a>> {
        self.inner.notes.iter().map(NoteView::new)
    }
}

/// How a note is played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    /// A normal, sounding note.
    Normal,
    /// Continues (ties) the previous note on the same string.
    Tie,
    /// A dead (muted, "x") note.
    Dead,
    /// A rest stored as a note.
    Rest,
    /// A note type this version does not know (kept unchanged in the file).
    Unknown,
}

/// One note: a fret on a string.
#[derive(Debug, Clone, Copy)]
pub struct NoteView<'a> {
    inner: &'a guitarpro::Note,
}

impl<'a> NoteView<'a> {
    fn new(inner: &'a guitarpro::Note) -> Self {
        NoteView { inner }
    }

    /// The string the note is played on.
    pub fn string(&self) -> Result<StringNumber, ModelError> {
        StringNumber::from_gp(self.inner.string)
    }

    /// The fret (or MIDI drum note on percussion tracks).
    pub fn fret(&self) -> Result<Fret, ModelError> {
        Fret::from_gp(self.inner.value)
    }

    /// Normal, tied, dead, ...
    pub fn kind(&self) -> NoteKind {
        match self.inner.kind {
            NoteType::Normal => NoteKind::Normal,
            NoteType::Tie => NoteKind::Tie,
            NoteType::Dead => NoteKind::Dead,
            NoteType::Rest => NoteKind::Rest,
            NoteType::Unknown(_) => NoteKind::Unknown,
        }
    }
}
