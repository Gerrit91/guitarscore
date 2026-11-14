//! Read-only views into a [`crate::Song`]: track, measure, voice, beat, note.
//!
//! A view borrows the song and knows its own index. Accessors that convert stored values
//! return `Result`: songs are validated on creation, so they succeed in practice, but the
//! types stay honest instead of hiding a fallback. Effects (bends, slides, harmonics, ...)
//! are in [`crate::effects`].

use gscore_gp::{BeatStatus, NoteType};

use crate::duration::{Duration, NoteValue, TimeSignature};
use crate::error::ModelError;
use crate::grace::Grace;
use crate::instrument::{self, InstrumentKind, InstrumentSymbol};
use crate::position::{BeatIndex, Fret, MeasureIndex, Pitch, StringNumber, TrackIndex, VoiceIndex};

/// One string of a track and the pitch it is tuned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringTuning {
    /// The string (1 = highest).
    pub string: StringNumber,
    /// The open-string pitch.
    pub pitch: Pitch,
}

/// A track (instrument part), seen through one of its staves: its measures, strings,
/// tuning and capo are that staff's (the first, unless [`TrackView::on_staff`] chose
/// another); everything else (name, sound, instrument) is the track's.
#[derive(Debug, Clone, Copy)]
pub struct TrackView<'a> {
    index: TrackIndex,
    pub(crate) inner: &'a gscore_gp::Track,
    /// The staff seen; always one the track has.
    staff: usize,
    /// Whether the song's transposition is switched on (the toolbar's A/B switch).
    transpose_active: bool,
}

/// How far a file may transpose a track: four octaves either way. GuitarScore's own
/// transpose stays within one octave, but a file may store more, and playback follows it.
pub(crate) const MAX_TRACK_TRANSPOSE: i32 = 48;

/// What a staff has of its own.
pub(crate) struct StaffData<'a> {
    pub(crate) measures: &'a [gscore_gp::Measure],
    pub(crate) strings: &'a [(i8, i8)],
    pub(crate) has_tuning: bool,
    pub(crate) tuning_label: Option<&'a String>,
    pub(crate) offset: i32,
}

/// A track name as GP3, GP4 and GP5 read it back after a save: in their single-byte code
/// page (a character it has no code for becomes a numeric reference) and cut to their
/// 40-byte field. Newer formats keep the name as it is.
pub fn track_name_in_binary_formats(name: &str) -> String {
    gscore_gp::track_name_as_stored(name)
}

impl<'a> TrackView<'a> {
    pub(crate) fn new(
        index: TrackIndex,
        inner: &'a gscore_gp::Track,
        transpose_active: bool,
    ) -> Self {
        TrackView {
            index,
            inner,
            staff: 0,
            transpose_active,
        }
    }

    /// What the track sounds like relative to what is written, whatever the on/off switch
    /// says: positive is higher, 0 when it is not transposed. GuitarScore's own setting
    /// (the Guitar Pro format's `<Transpose>` is the instrument's notation octave and is
    /// kept as it is).
    pub fn transposition(&self) -> i32 {
        self.inner
            .transpose_sound
            .clamp(-MAX_TRACK_TRANSPOSE, MAX_TRACK_TRANSPOSE)
    }

    /// Whether the song's transposition is switched on (see [`crate::Song::transpose_active`]).
    pub fn transpose_active(&self) -> bool {
        self.transpose_active
    }

    /// The same track seen through staff `staff` (its measures, strings, tuning and capo),
    /// or `None` if it has no such staff.
    pub fn on_staff(&self, staff: crate::StaffIndex) -> Option<TrackView<'a>> {
        let index = staff.get();
        (index <= self.inner.staves.len()).then_some(TrackView {
            staff: index,
            ..*self
        })
    }

    /// The staff it is seen through.
    pub fn staff_index(&self) -> crate::StaffIndex {
        crate::StaffIndex::new(self.staff)
    }

    /// The seen staff's own data.
    pub(crate) fn staff_data(&self) -> StaffData<'a> {
        let track = self.inner;
        match self
            .staff
            .checked_sub(1)
            .and_then(|index| track.staves.get(index))
        {
            Some(staff) => StaffData {
                measures: &staff.measures,
                strings: &staff.strings,
                has_tuning: staff.has_tuning,
                tuning_label: staff.tuning_label.as_ref(),
                offset: staff.offset,
            },
            None => StaffData {
                measures: &track.measures,
                strings: &track.strings,
                has_tuning: track.has_tuning,
                tuning_label: track.tuning_label.as_ref(),
                offset: track.offset,
            },
        }
    }

    /// This track's index in the song.
    pub fn index(&self) -> TrackIndex {
        self.index
    }

    /// The track name (any Unicode text, may be empty).
    pub fn name(&self) -> &'a str {
        &self.inner.name
    }

    /// The short name Guitar Pro writes before the systems after the first ("el.guit.");
    /// may be empty (GP3-5 have none).
    pub fn short_name(&self) -> &'a str {
        &self.inner.short_name
    }

    /// At most this many bars on a line of the page; `None` for as many as fit.
    pub fn bars_per_system(&self) -> Option<u8> {
        self.inner
            .bars_per_system
            .filter(|bars| (1..=crate::page::MAX_BARS_PER_SYSTEM).contains(bars))
    }

    /// Number of strings (4 for a bass, 6 for a guitar, ...).
    pub fn string_count(&self) -> usize {
        self.staff_data().strings.len()
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

    /// The symbol the user chose for this track, if one was chosen (see
    /// [`InstrumentSymbol`]); otherwise the app derives one.
    pub fn symbol(&self) -> Option<InstrumentSymbol> {
        self.inner
            .symbol
            .as_deref()
            .and_then(InstrumentSymbol::from_id)
    }

    /// GuitarScore's own sound state for this track, if it has any: the opaque payload the
    /// app serializes (what plays the track, its tone and chain, its sends, ...). It has
    /// no field in the Guitar Pro format and is saved in the song's notes; see
    /// [`crate::Song::set_track_sound_state`].
    pub fn sound_state(&self) -> Option<&'a str> {
        self.inner.sound_state.as_deref()
    }

    /// Guitar Pro 7's `<Sounds>` element for this track exactly as the file had it: the RSE
    /// sound the author chose (its path, bank patch, pickups and effect chain), or `None`
    /// when the file had none. GuitarScore keeps it on a load-save cycle and reads it to
    /// start the track on a fitting modelled tone.
    pub fn rse_sound(&self) -> Option<&'a str> {
        self.inner.gpif_sounds.as_deref()
    }

    /// The tuning, from string 1 (highest) down, or `None` if the track has none: it is
    /// not a string instrument (Guitar Pro still stores E standard for vocal and piano
    /// tracks), the file stores no tuning, or every string is at pitch 0 (drum kits).
    /// The track still has [`Self::string_count`] strings for placing notes.
    pub fn tuning(&self) -> Result<Option<Vec<StringTuning>>, ModelError> {
        if !self.staff_data().has_tuning || self.instrument_kind() != InstrumentKind::Strings {
            return Ok(None);
        }
        let strings = self.strings()?;
        if strings.iter().all(|s| s.pitch.get() == 0) {
            return Ok(None);
        }
        Ok(Some(strings))
    }

    /// The open strings the notes are written on: [`Self::tuning`] for a string
    /// instrument, and for a voice, keyboard or other instrument the stand-in tuning Guitar
    /// Pro stores (its notes are a string and a fret on it, the fret possibly negative).
    /// `None` for drums and for tracks that store no pitches at all.
    pub fn note_tuning(&self) -> Result<Option<Vec<StringTuning>>, ModelError> {
        if self.is_percussion() || !self.staff_data().has_tuning {
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
        if let Some(label) = self.staff_data().tuning_label {
            return Ok(Some(label.clone()));
        }
        let low_to_high: Vec<Pitch> = strings.iter().rev().map(|s| s.pitch).collect();
        Ok(crate::tuning::recognize(&low_to_high))
    }

    /// The strings as stored, including placeholder pitches of tracks without a tuning.
    pub(crate) fn strings(&self) -> Result<Vec<StringTuning>, ModelError> {
        self.staff_data()
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

    /// Which staves the track's page shows.
    pub fn staves(&self) -> crate::style::TrackStaves {
        crate::style::TrackStaves::from_flags(
            self.inner.settings.notation,
            self.inner.settings.tablature,
            self.inner.settings.slash,
        )
    }

    /// True for drum tracks, whose "frets" are MIDI drum notes.
    pub fn is_percussion(&self) -> bool {
        self.inner.percussion_track
    }

    /// The track's color, as shown in the song grid and other track lists.
    pub fn color(&self) -> crate::style::TrackColor {
        crate::style::TrackColor::from_gp(self.inner.color)
    }

    /// Whether the player should skip this track (Guitar Pro's mute).
    pub fn is_muted(&self) -> bool {
        self.inner.mute
    }

    /// Whether this track is soloed: only soloed tracks play.
    pub fn is_soloed(&self) -> bool {
        self.inner.solo
    }

    /// All measures, in order.
    pub fn measures(&self) -> impl ExactSizeIterator<Item = MeasureView<'a>> + use<'a> {
        self.staff_data()
            .measures
            .iter()
            .enumerate()
            .map(|(i, m)| MeasureView::new(MeasureIndex::new(i), m))
    }

    /// One measure, or `None` if the index is out of range.
    pub fn measure(&self, index: MeasureIndex) -> Option<MeasureView<'a>> {
        self.staff_data()
            .measures
            .get(index.get())
            .map(|m| MeasureView::new(index, m))
    }

    /// The measure whose music sounds at `index`: `index` itself, or for a bar-repetition
    /// mark the bar it repeats (the bar before a one-bar repeat, two bars before either
    /// half of a two-bar repeat), followed through repeats of repeats. A mark with no bar
    /// before it to repeat plays its own (usually empty) bar.
    pub fn sounding_measure(&self, index: MeasureIndex) -> MeasureIndex {
        let mut at = index.get();
        // Every step goes back at least one bar, so this many steps always end.
        for _ in 0..=at {
            let back = match self
                .measure(MeasureIndex::new(at))
                .and_then(|m| m.simile_mark())
            {
                Some(crate::style::SimileMark::Simple) => 1,
                Some(_) => 2,
                None => break,
            };
            match at.checked_sub(back) {
                Some(source) => at = source,
                None => break,
            }
        }
        MeasureIndex::new(at)
    }
}

/// One measure (bar) of one track.
#[derive(Debug, Clone, Copy)]
pub struct MeasureView<'a> {
    index: MeasureIndex,
    inner: &'a gscore_gp::Measure,
}

impl<'a> MeasureView<'a> {
    pub(crate) fn new(index: MeasureIndex, inner: &'a gscore_gp::Measure) -> Self {
        MeasureView { index, inner }
    }

    /// This measure's index in the track.
    pub fn index(&self) -> MeasureIndex {
        self.index
    }

    /// Whether this measure starts a new line or page of its track.
    pub fn system_break(&self) -> crate::page::SystemBreak {
        if self.index.get() == 0 {
            return crate::page::SystemBreak::None;
        }
        crate::edit::system_break(&self.inner.line_break)
    }

    /// The time signature in effect in this measure.
    pub fn time_signature(&self) -> Result<TimeSignature, ModelError> {
        TimeSignature::from_gp(&self.inner.time_signature)
    }

    /// Whether this measure holds notes (normal beats with at least one note). Rests and
    /// placeholders do not count: the song grid fills a square only where there is music.
    pub fn has_notes(&self) -> bool {
        self.voices().any(|voice| {
            voice
                .beats()
                .any(|beat| beat.kind() == BeatKind::Normal && beat.notes().next().is_some())
        })
    }

    /// Guitar Pro's bar-repetition (simile) mark on this measure, if any.
    pub fn simile_mark(&self) -> Option<crate::style::SimileMark> {
        self.inner
            .simile_mark
            .as_deref()
            .and_then(crate::style::SimileMark::from_gp)
    }

    /// This bar's own width scale (Guitar Pro's design mode; 1 is its natural width), or
    /// `None` when the track has no scale of its own and the song's applies. Use
    /// [`Song::measure_scale`] for the scale that applies.
    pub fn display_scale(&self) -> Option<f32> {
        self.inner.display_scale()
    }

    /// The clef this measure's standard-notation staff is written in, if the file stored
    /// one (GP6/GP7 do; GP3-5 do not, and the layout derives it from the instrument).
    pub fn clef(&self) -> Option<crate::style::Clef> {
        self.inner
            .has_clef
            .then(|| crate::style::Clef::from_gp(&self.inner.clef))
    }

    /// All voices, in order.
    pub fn voices(&self) -> impl ExactSizeIterator<Item = VoiceView<'a>> + use<'a> {
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
    inner: &'a gscore_gp::Voice,
}

impl<'a> VoiceView<'a> {
    fn new(index: VoiceIndex, inner: &'a gscore_gp::Voice) -> Self {
        VoiceView { index, inner }
    }

    /// This voice's index in the measure.
    pub fn index(&self) -> VoiceIndex {
        self.index
    }

    /// All beats, in order.
    pub fn beats(&self) -> impl ExactSizeIterator<Item = BeatView<'a>> + use<'a> {
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

    /// The summed length of all beats; placeholders and grace-note beats count as zero.
    pub fn total_duration(&self) -> Result<Duration, ModelError> {
        self.beats().try_fold(Duration::ZERO, |sum, beat| {
            if beat.kind() == BeatKind::Empty || beat.is_grace() {
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
    pub(crate) inner: &'a gscore_gp::Beat,
}

impl<'a> BeatView<'a> {
    fn new(index: BeatIndex, inner: &'a gscore_gp::Beat) -> Self {
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

    /// How the beat's length is written: note value, dots and tuplet.
    pub fn note_value(&self) -> Result<NoteValue, ModelError> {
        NoteValue::from_gp(&self.inner.duration)
    }

    /// Whether this is a normal beat, a rest, or a placeholder.
    pub fn kind(&self) -> BeatKind {
        match self.inner.status {
            BeatStatus::Normal => BeatKind::Normal,
            BeatStatus::Rest => BeatKind::Rest,
            BeatStatus::Empty => BeatKind::Empty,
        }
    }

    /// Whether this is a GP6/GP7 grace-note beat: its notes are quick ornaments before the
    /// next beat and take no time of their own.
    pub fn is_grace(&self) -> bool {
        self.inner.grace
    }

    /// The chord diagram shown at this beat, if any.
    pub fn chord_diagram(&self) -> Option<crate::techniques::ChordDiagram> {
        self.inner
            .chord_diagram
            .as_ref()
            .map(crate::techniques::ChordDiagram::from_gp)
    }

    /// Guitar Pro's design mode: how far this beat's column is moved from its natural place
    /// (in staff spaces, negative for left), or `None` when it is not moved. The layout
    /// reads the first moved beat of a column.
    pub fn spacing(&self) -> Option<f32> {
        self.inner.spacing
    }

    /// The octave sign on this beat, if any.
    pub fn octave(&self) -> Option<crate::effects::OctaveSign> {
        crate::effects::OctaveSign::from_gp(&self.inner.octave)
    }

    /// The notes of this beat (at most one per string).
    pub fn notes(&self) -> impl ExactSizeIterator<Item = NoteView<'a>> + use<'a> {
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
    pub(crate) inner: &'a gscore_gp::Note,
}

impl<'a> NoteView<'a> {
    fn new(inner: &'a gscore_gp::Note) -> Self {
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

    /// The grace note attached to this note (GP6/GP7 grace beats are attached when the
    /// file is read where possible; see [`BeatView::is_grace`] for the rest).
    pub fn grace(&self) -> Option<Grace> {
        self.inner.effect.grace.as_ref().map(Grace::from_gp)
    }
}
