//! Checked editing methods on [`Song`].
//!
//! Every method validates the request and is transactional (AGENTS.md rule 8): a failed
//! edit leaves the song exactly as it was. Note and beat edits work on a copy of the one
//! measure they touch ([`Song::edit_measure`]); measure insertion and removal check
//! everything before they change anything ([`Song::edit_structure`]). Neither copies the
//! whole song, so an edit takes as long in a long song as in a short one.
//!
//! Each successful method returns a [`Change`]. Keep it and call [`Song::revert`] to undo
//! the edit. The change is opaque: its data stays behind the model boundary, and
//! `gscore-edit` only has to keep it in its undo history.

mod beat_flags;
mod beats;
mod chord;
mod dynamics;
mod effect_edits;
mod headers;
pub(crate) mod measures;
mod mix;
mod notes;
mod page;
mod song;
mod tracks;

pub use crate::validate::MAX_STRINGS;
pub use headers::MAX_SECTION_TITLE_CHARS;
pub(crate) use page::system_break;
pub use tracks::{MAX_TRACK_NAME_CHARS, MAX_TRACKS};

use gscore_gp::{
    Beat as GpBeat, BeatStatus, Duration as GpDuration, Measure as GpMeasure,
    MeasureHeader as GpMeasureHeader, MidiChannel as GpMidiChannel, Note as GpNote, Song as GpSong,
    Track as GpTrack, Voice as GpVoice,
};

use crate::duration::NoteValue;
use crate::error::ModelError;
use crate::position::{BeatPosition, Fret, MeasureIndex, StringNumber, TrackIndex};
use crate::song::Song;
use tracks::MixFlag;

/// An applied edit, kept so it can be undone with [`Song::revert`].
#[must_use = "keep the change to undo the edit"]
#[derive(Debug)]
pub struct Change {
    kind: ChangeKind,
}

impl Change {
    /// The change of one lyric line, for [`Song::set_lyric_line`].
    pub(crate) fn lyric_line(
        line: u8,
        previous: Option<(u16, String)>,
        previous_track: u8,
        previous_beats: Vec<(BeatPosition, Vec<String>)>,
    ) -> Self {
        Change {
            kind: ChangeKind::LyricLine {
                line,
                previous,
                previous_track,
                previous_beats,
            },
        }
    }

    /// The change of a beat's chord diagram.
    pub(crate) fn chord_diagram(
        at: BeatPosition,
        previous: Option<crate::techniques::ChordDiagram>,
    ) -> Self {
        Change {
            kind: ChangeKind::ChordDiagram {
                at,
                previous: previous.map(Box::new),
            },
        }
    }
}

/// What an edit changed, with the data needed to put it back.
/// A GP7 track's RSE sound as the file had it (`<Sounds>` and `<AudioEngineState>`),
/// kept by an instrument change so undo can bring it back.
#[derive(Debug)]
struct RseSound {
    sounds: Option<String>,
    audio_engine: Option<String>,
}

impl RseSound {
    /// Takes the RSE sound off `track`: it was chosen for the instrument the track had,
    /// and a GP7 save would otherwise keep playing it in Guitar Pro.
    fn take(track: &mut gscore_gp::Track) -> Self {
        RseSound {
            sounds: track.gpif_sounds.take(),
            audio_engine: track.gpif_audio_engine.take(),
        }
    }

    /// Gives `track` this sound again.
    fn restore(&self, track: &mut gscore_gp::Track) {
        track.gpif_sounds.clone_from(&self.sounds);
        track.gpif_audio_engine.clone_from(&self.audio_engine);
    }

    /// The bytes it keeps, for the undo history's budget.
    fn size(&self) -> usize {
        self.sounds
            .as_ref()
            .map_or(0, String::len)
            .saturating_add(self.audio_engine.as_ref().map_or(0, String::len))
    }
}

#[derive(Debug)]
enum ChangeKind {
    /// A note was added or replaced; `previous` is what was on the string before.
    NoteSet {
        at: BeatPosition,
        string: StringNumber,
        previous: Option<Box<GpNote>>,
        previous_status: BeatStatus,
    },
    /// A note was removed. Clearing the last note turns the beat into a rest.
    NoteCleared {
        at: BeatPosition,
        string: StringNumber,
        note: Box<GpNote>,
        previous_status: BeatStatus,
    },
    /// A beat's length changed.
    DurationSet {
        at: BeatPosition,
        previous: GpDuration,
    },
    /// A rest beat was inserted before `at.beat`; `created_voice` (the first voice created)
    /// if the insertion had to
    /// create the voice (a measure imported without voices).
    BeatInserted {
        at: BeatPosition,
        /// The voice index created for the beat, if it had to be.
        created_voice: Option<usize>,
    },
    /// The beat at `at.beat` was removed.
    BeatRemoved { at: BeatPosition, beat: Box<GpBeat> },
    /// A measure was inserted at `at` in every track.
    MeasureInserted { at: MeasureIndex },
    /// Measures were removed at `at`, one from every staff of every track (in that order),
    /// plus the measure header.
    MeasureRemoved {
        at: MeasureIndex,
        header: Box<GpMeasureHeader>,
        measures: Vec<GpMeasure>,
    },
    /// A track was inserted at `at`; `channel` is the MIDI channel appended for it.
    TrackInserted {
        at: TrackIndex,
        channel: Option<usize>,
    },
    /// The track at `at` was removed.
    TrackRemoved { at: TrackIndex, track: Box<GpTrack> },
    /// The track at `from` moved to `to`.
    TrackMoved { from: TrackIndex, to: TrackIndex },
    /// A track's name changed.
    TrackRenamed { at: TrackIndex, previous: String },
    /// A track's color changed.
    TrackColor { at: TrackIndex, previous: i32 },
    /// The instrument symbol drawn for a track changed.
    TrackSymbol {
        at: TrackIndex,
        previous: Option<String>,
    },
    /// A track's own sound state (GuitarScore's opaque payload) changed.
    TrackSoundState {
        at: TrackIndex,
        previous: Option<String>,
    },
    /// The song-wide sound state (GuitarScore's opaque payload: the mixer's buses, ...)
    /// changed.
    SongSoundState { previous: Option<String> },
    /// A track's mute or solo flag changed.
    TrackFlag {
        at: TrackIndex,
        flag: MixFlag,
        previous: bool,
    },
    /// A track's General MIDI program changed.
    TrackProgram {
        at: TrackIndex,
        previous: Option<i32>,
        /// The GP7 sound the track had, when the instrument changed and it was dropped.
        previous_rse: Option<RseSound>,
    },
    /// A track became (or stopped being) a drum kit, which also changes its GP7
    /// instrument type and name.
    TrackDrums {
        at: TrackIndex,
        previous: bool,
        previous_type: Option<String>,
        previous_name: Option<String>,
        /// The GP7 sound the track had, when it became or stopped being a kit and it was
        /// dropped.
        previous_rse: Option<RseSound>,
    },
    /// A track's capo fret changed.
    TrackCapo { at: TrackIndex, previous: i32 },
    /// The page setup changed.
    PageSetup {
        previous: Box<gscore_gp::page::PageSetup>,
    },
    /// A measure's line or page break changed.
    SystemBreak {
        track: TrackIndex,
        measure: MeasureIndex,
        previous: gscore_gp::LineBreak,
    },
    /// The line breaks of all tracks shown together changed; `previous` is the systems
    /// layout before.
    ScoreLineBreaks { previous: Option<String> },
    /// A track's bars-per-line limit changed.
    BarsPerSystem {
        track: TrackIndex,
        previous: Option<u8>,
    },
    /// A track's string count and tuning changed.
    TrackStrings {
        at: TrackIndex,
        previous: Vec<(i8, i8)>,
        previous_has_tuning: bool,
        previous_label: Option<String>,
    },
    /// One string's open pitch changed.
    TrackStringPitch {
        at: TrackIndex,
        string: StringNumber,
        previous: i8,
        previous_has_tuning: bool,
        previous_label: Option<String>,
    },
    /// One text field of the song changed.
    SongText {
        field: crate::metadata::SongTextField,
        previous: String,
    },
    /// The initial tempo changed; `previous_first` is measure 1's tempo when it had one
    /// (GP6 and later store the initial tempo there too) and was changed with it.
    SongTempo {
        previous: i16,
        previous_first: Option<i32>,
    },
    /// A beat's text changed; `previous` is the text before.
    BeatText { at: BeatPosition, previous: String },
    /// A lyric line changed; `previous` is its starting measure and text before.
    LyricLine {
        line: u8,
        previous: Option<(u16, String)>,
        previous_track: u8,
        previous_beats: Vec<(BeatPosition, Vec<String>)>,
    },
    /// A beat's chord diagram changed.
    ChordDiagram {
        at: BeatPosition,
        previous: Option<Box<crate::techniques::ChordDiagram>>,
    },
    /// A beat was made a rest; its notes are kept to put them back.
    BeatRest {
        at: BeatPosition,
        previous_status: BeatStatus,
        notes: Vec<GpNote>,
    },
    /// A note became (or stopped being) a tie.
    NoteTie {
        at: BeatPosition,
        string: StringNumber,
        previous: gscore_gp::NoteType,
    },
    /// A beat's dynamic changed; `previous` holds its notes' velocities, in note order.
    BeatDynamic {
        at: BeatPosition,
        previous: Vec<i16>,
    },
    /// A note became (or stopped being) a dead note.
    NoteDead {
        at: BeatPosition,
        string: StringNumber,
        previous: gscore_gp::NoteType,
    },
    /// One effect of a note changed; `previous` is the note's whole effect block.
    NoteEffect {
        at: BeatPosition,
        string: StringNumber,
        previous: Box<gscore_gp::NoteEffect>,
    },
    /// One effect of a beat changed; `previous` is the beat's whole effect block.
    BeatEffect {
        at: BeatPosition,
        previous: Box<gscore_gp::BeatEffects>,
    },
    /// A measure's tempo changed; at measure 1 the song's initial tempo changed too.
    MeasureTempo {
        measure: MeasureIndex,
        previous: i32,
        previous_initial: Option<i16>,
    },
    /// A measure's time signature changed, in the header and every track.
    TimeSignature {
        measure: MeasureIndex,
        previous_header: gscore_gp::TimeSignature,
        previous_measures: Vec<gscore_gp::TimeSignature>,
    },
    /// A measure's key signature changed, in the header and every track.
    KeySignature {
        measure: MeasureIndex,
        previous_header: gscore_gp::KeySignature,
        previous_measures: Vec<gscore_gp::KeySignature>,
    },
    /// A measure's section marker changed.
    Section {
        measure: MeasureIndex,
        previous: Option<gscore_gp::Marker>,
    },
    /// A measure's repeat signs and alternative endings changed; `previous` is
    /// `(repeat_open, repeat_close, repeat_alternative)`.
    Repeat {
        measure: MeasureIndex,
        previous: (bool, i8, u8),
    },
    /// Whether a measure ends with a double barline changed.
    DoubleBar {
        measure: MeasureIndex,
        previous: bool,
    },
    /// A measure's feel changed; `previous` is its triplet feel, `previous_initial` the
    /// song's (set with the first measure's).
    Feel {
        measure: MeasureIndex,
        previous: gscore_gp::TripletFeel,
        previous_initial: Option<gscore_gp::TripletFeel>,
    },
    /// A measure's fermatas changed; `previous` is the whole list.
    Fermata {
        measure: MeasureIndex,
        previous: Vec<(String, String)>,
    },
    /// A measure's free-time (no metronome) switch changed.
    FreeTime {
        measure: MeasureIndex,
        previous: bool,
    },
    /// A measure's clef changed; `previous` is the clef and whether the file stored one.
    Clef {
        track: TrackIndex,
        measure: MeasureIndex,
        previous: (crate::style::Clef, bool),
    },
    /// A measure's bar-repetition (simile) mark changed; `previous` is its stored value.
    SimileMark {
        track: TrackIndex,
        measure: MeasureIndex,
        previous: Option<String>,
    },
    /// A track's notation, tab and slash switches changed; `previous` is
    /// `(notation, tablature, slash)`.
    TrackStaves {
        at: TrackIndex,
        previous: (bool, bool, bool),
    },
    /// The master volume and pan changed (`None`: the song had none).
    MasterMix { previous: Option<(f32, f32)> },
    /// Several changes made as one edit, undone in reverse order.
    Batch { changes: Vec<Change> },
    /// A track's volume or pan changed. `previous_channel` is the channel the track used
    /// before; `added` is the channel appended for it when it had to stop sharing one.
    TrackMix {
        at: TrackIndex,
        previous_channel: usize,
        previous: GpMidiChannel,
        added: Option<usize>,
        /// How many channels the song had before; undo trims any channel (and padding)
        /// this edit appended.
        previous_channel_count: usize,
    },
}

impl Change {
    /// About how many bytes this change keeps in memory (for an undo history capped by
    /// memory). An estimate: the sizes of the kept notes, beats and measures, not their
    /// text or allocator overhead.
    pub fn size_estimate(&self) -> usize {
        let base = size_of::<Change>();
        let extra = match &self.kind {
            ChangeKind::NoteSet { previous, .. } => {
                previous.as_ref().map_or(0, |_| size_of::<GpNote>())
            }
            ChangeKind::NoteCleared { .. } => size_of::<GpNote>(),
            ChangeKind::DurationSet { .. }
            | ChangeKind::BeatInserted { .. }
            | ChangeKind::MeasureInserted { .. } => 0,
            ChangeKind::BeatRemoved { beat, .. } => beat_size(beat),
            ChangeKind::MeasureRemoved { measures, .. } => {
                size_of::<GpMeasureHeader>().saturating_add(measures.iter().map(measure_size).sum())
            }
            ChangeKind::TrackRenamed { previous, .. } => previous.len(),
            ChangeKind::TrackSymbol { previous, .. } => previous.as_ref().map_or(0, String::len),
            ChangeKind::TrackSoundState { previous, .. }
            | ChangeKind::SongSoundState { previous } => previous.as_ref().map_or(0, String::len),
            ChangeKind::TrackRemoved { track, .. } => track_size(track),
            ChangeKind::TrackProgram {
                previous,
                previous_rse,
                ..
            } => previous
                .map_or(0, |_| size_of::<i32>())
                .saturating_add(previous_rse.as_ref().map_or(0, RseSound::size)),
            ChangeKind::TrackDrums {
                previous_type,
                previous_name,
                previous_rse,
                ..
            } => previous_type
                .as_ref()
                .map_or(0, String::len)
                .saturating_add(previous_name.as_ref().map_or(0, String::len))
                .saturating_add(previous_rse.as_ref().map_or(0, RseSound::size)),
            ChangeKind::TrackStrings {
                previous,
                previous_label,
                ..
            } => previous
                .len()
                .saturating_mul(size_of::<(i8, i8)>())
                .saturating_add(previous_label.as_ref().map_or(0, String::len)),
            ChangeKind::TrackStringPitch { previous_label, .. } => {
                previous_label.as_ref().map_or(0, String::len)
            }
            ChangeKind::SongText { previous, .. } | ChangeKind::BeatText { previous, .. } => {
                previous.len()
            }
            ChangeKind::LyricLine {
                previous,
                previous_beats,
                ..
            } => previous
                .as_ref()
                .map_or(0, |(_, text)| text.len())
                .saturating_add(
                    previous_beats
                        .iter()
                        .map(|(_, texts)| {
                            texts
                                .iter()
                                .map(|text| text.len().saturating_add(size_of::<String>()))
                                .sum::<usize>()
                        })
                        .sum::<usize>(),
                ),
            ChangeKind::ChordDiagram { previous, .. } => previous.as_ref().map_or(0, |diagram| {
                diagram
                    .name
                    .len()
                    .saturating_add(diagram.frets.len().saturating_mul(size_of::<(u8, i8)>()))
            }),
            ChangeKind::SongTempo { .. } => 0,
            ChangeKind::BeatRest { notes, .. } => notes.len().saturating_mul(size_of::<GpNote>()),
            ChangeKind::NoteTie { .. } | ChangeKind::NoteDead { .. } => 0,
            ChangeKind::BeatDynamic { previous, .. } => {
                previous.len().saturating_mul(size_of::<i16>())
            }
            ChangeKind::NoteEffect { .. } => size_of::<gscore_gp::NoteEffect>(),
            ChangeKind::BeatEffect { .. } => size_of::<gscore_gp::BeatEffects>(),
            ChangeKind::MeasureTempo { .. } => 0,
            ChangeKind::TimeSignature {
                previous_measures, ..
            } => previous_measures
                .len()
                .saturating_mul(size_of::<gscore_gp::TimeSignature>()),
            ChangeKind::KeySignature {
                previous_measures, ..
            } => previous_measures
                .len()
                .saturating_mul(size_of::<gscore_gp::KeySignature>()),
            ChangeKind::Section { previous, .. } => {
                previous.as_ref().map_or(0, |marker| marker.title.len())
            }
            ChangeKind::TrackStaves { .. } | ChangeKind::MasterMix { .. } => 0,
            ChangeKind::Repeat { .. }
            | ChangeKind::DoubleBar { .. }
            | ChangeKind::Feel { .. }
            | ChangeKind::FreeTime { .. }
            | ChangeKind::Clef { .. }
            | ChangeKind::SimileMark { .. } => 0,
            ChangeKind::Fermata { previous, .. } => previous
                .iter()
                .map(|(kind, offset)| kind.len().saturating_add(offset.len()))
                .sum(),
            ChangeKind::Batch { changes } => changes
                .iter()
                .map(Change::size_estimate)
                .fold(0_usize, usize::saturating_add),
            ChangeKind::TrackInserted { .. }
            | ChangeKind::TrackMoved { .. }
            | ChangeKind::TrackColor { .. }
            | ChangeKind::TrackFlag { .. }
            | ChangeKind::TrackCapo { .. }
            | ChangeKind::SystemBreak { .. }
            | ChangeKind::BarsPerSystem { .. }
            | ChangeKind::TrackMix { .. } => 0,
            ChangeKind::ScoreLineBreaks { previous } => previous.as_ref().map_or(0, String::len),
            ChangeKind::PageSetup { .. } => size_of::<gscore_gp::page::PageSetup>(),
        };
        base.saturating_add(extra)
    }

    /// Combines several changes into one undoable edit (a paste, a palette action that
    /// touches many beats). Undoing it reverts the parts in reverse order.
    pub fn batch(changes: Vec<Change>) -> Change {
        Change {
            kind: ChangeKind::Batch { changes },
        }
    }
}

/// About how many bytes a beat and its notes take.
fn beat_size(beat: &GpBeat) -> usize {
    size_of::<GpBeat>().saturating_add(beat.notes.len().saturating_mul(size_of::<GpNote>()))
}

/// About how many bytes a track, its name and every staff's measures take.
fn track_size(track: &GpTrack) -> usize {
    size_of::<GpTrack>()
        .saturating_add(track.name.len())
        .saturating_add(track.all_measures().map(measure_size).sum())
}

/// About how many bytes a measure and its beats take.
fn measure_size(measure: &GpMeasure) -> usize {
    let beats: usize = measure
        .voices
        .iter()
        .flat_map(|voice| &voice.beats)
        .map(beat_size)
        .sum();
    size_of::<GpMeasure>()
        .saturating_add(measure.voices.len().saturating_mul(size_of::<GpVoice>()))
        .saturating_add(beats)
}

impl Song {
    /// Sets a normal note at `fret` on `string` of the beat at `at`, replacing any note
    /// already on that string but keeping its effects. The beat becomes a normal beat.
    pub fn set_note(
        &mut self,
        at: BeatPosition,
        string: StringNumber,
        fret: Fret,
    ) -> Result<Change, ModelError> {
        notes::set(self, at, string, fret)
    }

    /// Removes the note on `string` of the beat at `at`. If it was the last note, the beat
    /// becomes a rest, which is what Guitar Pro stores for a beat without notes.
    pub fn clear_note(
        &mut self,
        at: BeatPosition,
        string: StringNumber,
    ) -> Result<Change, ModelError> {
        notes::clear(self, at, string)
    }

    /// Sets the length of the beat at `at`.
    pub fn set_duration(
        &mut self,
        at: BeatPosition,
        length: NoteValue,
    ) -> Result<Change, ModelError> {
        beats::set_duration(self, at, length)
    }

    /// Inserts a rest beat with the given length before `at.beat`. `at.beat` may be one
    /// past the voice's last beat to append.
    pub fn insert_beat(
        &mut self,
        at: BeatPosition,
        length: NoteValue,
    ) -> Result<Change, ModelError> {
        beats::insert(self, at, length)
    }

    /// Removes the beat at `at.beat`.
    pub fn delete_beat(&mut self, at: BeatPosition) -> Result<Change, ModelError> {
        beats::delete(self, at)
    }

    /// Inserts an empty measure (filled with rests) before `at` in every track, plus its
    /// measure header. `at` may be one past the last measure to append. The new measure
    /// inherits the time and key signature in effect there.
    pub fn insert_measure(&mut self, at: MeasureIndex) -> Result<Change, ModelError> {
        measures::insert(self, at)
    }

    /// Removes the measure at `at` from every track, plus its measure header. The last
    /// remaining measure cannot be removed: a song without measures is not useful, and
    /// Guitar Pro stores no such score.
    pub fn delete_measure(&mut self, at: MeasureIndex) -> Result<Change, ModelError> {
        measures::delete(self, at)
    }

    /// Undoes a change returned by one of the editing methods. The editor must revert
    /// changes in the reverse order they were applied; anything else is reported as an
    /// error and leaves the song untouched.
    pub fn revert(&mut self, change: &Change) -> Result<(), ModelError> {
        match &change.kind {
            ChangeKind::NoteSet {
                at,
                string,
                previous,
                previous_status,
            } => self.edit_measure(at.track, at.measure, |measure| {
                notes::restore(measure, *at, *string, previous.as_deref())?;
                beat_in(measure, *at)?.status = previous_status.clone();
                Ok(())
            }),
            ChangeKind::NoteCleared {
                at,
                string,
                note,
                previous_status,
            } => self.edit_measure(at.track, at.measure, |measure| {
                notes::restore(measure, *at, *string, Some(note.as_ref()))?;
                beat_in(measure, *at)?.status = previous_status.clone();
                Ok(())
            }),
            ChangeKind::DurationSet { at, previous } => {
                self.edit_measure(at.track, at.measure, |measure| {
                    beat_in(measure, *at)?.duration = previous.clone();
                    Ok(())
                })
            }
            ChangeKind::BeatInserted { at, created_voice } => {
                self.edit_measure(at.track, at.measure, |measure| {
                    let voice = voice_in(measure, *at)?;
                    let index = at.beat.get();
                    if index >= voice.beats.len() {
                        return Err(ModelError::Invalid(format!("{at}: no such beat")));
                    }
                    voice.beats.remove(index);
                    if let Some(first) = *created_voice
                        && voice.beats.is_empty()
                    {
                        measure.voices.truncate(first);
                    }
                    Ok(())
                })
            }
            ChangeKind::BeatRemoved { at, beat } => {
                self.edit_measure(at.track, at.measure, |measure| {
                    let voice = voice_in(measure, *at)?;
                    let index = at.beat.get();
                    if index > voice.beats.len() {
                        return Err(ModelError::Invalid(format!("{at}: no such beat")));
                    }
                    voice.beats.insert(index, beat.as_ref().clone());
                    Ok(())
                })
            }
            ChangeKind::MeasureInserted { at } => {
                self.edit_structure(|inner| measures::remove(inner, *at))
            }
            ChangeKind::MeasureRemoved {
                at,
                header,
                measures,
            } => self
                .edit_structure(|inner| measures::restore(inner, *at, header.as_ref(), measures)),
            ChangeKind::TrackInserted { at, channel } => self.edit_tracks(|inner| {
                if at.get() >= inner.tracks.len() {
                    return Err(ModelError::Invalid(format!(
                        "{at}: no such track to remove"
                    )));
                }
                inner.tracks.remove(at.get());
                if let Some(channel) = channel
                    && channel.saturating_add(1) == inner.channels.len()
                {
                    inner.channels.pop();
                }
                tracks::renumber(inner)
            }),
            ChangeKind::TrackRemoved { at, track } => self.edit_tracks(|inner| {
                if at.get() > inner.tracks.len() {
                    return Err(ModelError::Invalid(format!(
                        "{at}: the song has {} track(s)",
                        inner.tracks.len()
                    )));
                }
                inner.tracks.insert(at.get(), track.as_ref().clone());
                tracks::renumber(inner)
            }),
            ChangeKind::TrackMoved { from, to } => self.edit_tracks(|inner| {
                let count = inner.tracks.len();
                if from.get() >= count || to.get() >= count {
                    return Err(ModelError::Invalid(
                        "the undo data does not match the song's tracks".to_owned(),
                    ));
                }
                let track = inner.tracks.remove(to.get());
                inner.tracks.insert(from.get(), track);
                tracks::renumber(inner)
            }),
            ChangeKind::TrackRenamed { at, previous } => self.edit_track_settings(|inner| {
                track_mut(inner, *at)?.name = previous.clone();
                Ok(())
            }),
            ChangeKind::TrackColor { at, previous } => self.edit_track_settings(|inner| {
                track_mut(inner, *at)?.color = *previous;
                Ok(())
            }),
            ChangeKind::TrackSymbol { at, previous } => self.edit_track_settings(|inner| {
                track_mut(inner, *at)?.symbol.clone_from(previous);
                Ok(())
            }),
            ChangeKind::TrackSoundState { at, previous } => self.edit_track_settings(|inner| {
                track_mut(inner, *at)?.sound_state.clone_from(previous);
                Ok(())
            }),
            ChangeKind::SongSoundState { previous } => {
                self.replace_sound_state(previous.clone());
                Ok(())
            }
            ChangeKind::TrackFlag { at, flag, previous } => self.edit_track_settings(|inner| {
                let track = track_mut(inner, *at)?;
                match flag {
                    MixFlag::Mute => track.mute = *previous,
                    MixFlag::Solo => track.solo = *previous,
                }
                Ok(())
            }),
            ChangeKind::TrackProgram {
                at,
                previous,
                previous_rse,
            } => self.edit_track_settings(|inner| {
                let track = track_mut(inner, *at)?;
                track.midi_program_gpif = *previous;
                if let Some(rse) = previous_rse {
                    rse.restore(track);
                }
                Ok(())
            }),
            ChangeKind::TrackDrums {
                at,
                previous,
                previous_type,
                previous_name,
                previous_rse,
            } => self.edit_track_settings(|inner| {
                let track = track_mut(inner, *at)?;
                track.percussion_track = *previous;
                track.instrument_type.clone_from(previous_type);
                track.instrument_name.clone_from(previous_name);
                if let Some(rse) = previous_rse {
                    rse.restore(track);
                }
                Ok(())
            }),
            ChangeKind::TrackCapo { at, previous } => self.edit_track_settings(|inner| {
                track_mut(inner, *at)?.offset = *previous;
                Ok(())
            }),
            ChangeKind::PageSetup { previous } => self.edit_metadata(|inner| {
                inner.page_setup.clone_from(previous);
                Ok(())
            }),
            ChangeKind::ScoreLineBreaks { previous } => self.edit_metadata(|inner| {
                inner.score_systems_layout = previous.clone();
                Ok(())
            }),
            ChangeKind::SystemBreak {
                track,
                measure,
                previous,
            } => self.edit_track_settings(|inner| {
                track_mut(inner, *track)?
                    .measures
                    .get_mut(measure.get())
                    .ok_or_else(|| ModelError::Invalid(format!("{measure}: no such measure")))?
                    .line_break = previous.clone();
                Ok(())
            }),
            ChangeKind::BarsPerSystem { track, previous } => self.edit_track_settings(|inner| {
                track_mut(inner, *track)?.bars_per_system = *previous;
                Ok(())
            }),
            ChangeKind::TrackStrings {
                at,
                previous,
                previous_has_tuning,
                previous_label,
            } => self.edit_track_settings(|inner| {
                let track = track_mut(inner, *at)?;
                track.strings.clone_from(previous);
                track.has_tuning = *previous_has_tuning;
                track.tuning_label.clone_from(previous_label);
                Ok(())
            }),
            ChangeKind::TrackStringPitch {
                at,
                string,
                previous,
                previous_has_tuning,
                previous_label,
            } => self.edit_track_settings(|inner| {
                let track = track_mut(inner, *at)?;
                let entry = track
                    .strings
                    .iter_mut()
                    .find(|(number, _)| u8::try_from(*number).ok() == Some(string.get()))
                    .ok_or_else(|| {
                        ModelError::Invalid(format!("{at} has no {string} to restore"))
                    })?;
                entry.1 = *previous;
                track.has_tuning = *previous_has_tuning;
                track.tuning_label.clone_from(previous_label);
                Ok(())
            }),
            ChangeKind::SongText { field, previous } => self.edit_metadata(|inner| {
                let replace = |slot: &mut String| slot.clone_from(previous);
                match field {
                    crate::metadata::SongTextField::Title => replace(&mut inner.name),
                    crate::metadata::SongTextField::Subtitle => replace(&mut inner.subtitle),
                    crate::metadata::SongTextField::Artist => replace(&mut inner.artist),
                    crate::metadata::SongTextField::Album => replace(&mut inner.album),
                    crate::metadata::SongTextField::Lyricist => replace(&mut inner.words),
                    crate::metadata::SongTextField::Composer => replace(&mut inner.author),
                    crate::metadata::SongTextField::Copyright => replace(&mut inner.copyright),
                    crate::metadata::SongTextField::TabAuthor => replace(&mut inner.writer),
                    crate::metadata::SongTextField::Instructions => {
                        replace(&mut inner.instructions);
                    }
                    crate::metadata::SongTextField::Notes => {
                        inner.notice = if previous.is_empty() {
                            Vec::new()
                        } else {
                            previous.split('\n').map(str::to_owned).collect()
                        };
                    }
                }
                Ok(())
            }),
            ChangeKind::SongTempo {
                previous,
                previous_first,
            } => self.edit_metadata(|inner| {
                inner.tempo = *previous;
                if let Some(first) = previous_first
                    && let Some(header) = inner.measure_headers.first_mut()
                {
                    header.tempo = *first;
                }
                Ok(())
            }),
            ChangeKind::BeatText { at, previous } => {
                self.edit_measure(at.track, at.measure, |measure| {
                    beat_in(measure, *at)?.text = previous.clone();
                    Ok(())
                })
            }
            ChangeKind::LyricLine {
                line,
                previous,
                previous_track,
                previous_beats,
            } => {
                crate::lyrics::restore_line(self, *line, previous, *previous_track, previous_beats);
                Ok(())
            }
            ChangeKind::ChordDiagram { at, previous } => {
                self.edit_measure(at.track, at.measure, |measure| {
                    beat_in(measure, *at)?.chord_diagram =
                        previous.as_ref().map(|diagram| diagram.to_gp());
                    Ok(())
                })
            }
            ChangeKind::BeatRest {
                at,
                previous_status,
                notes,
            } => self.edit_measure(at.track, at.measure, |measure| {
                let beat = beat_in(measure, *at)?;
                beat.notes.clone_from(notes);
                beat.status = previous_status.clone();
                Ok(())
            }),
            ChangeKind::BeatDynamic { at, previous } => {
                self.edit_measure(at.track, at.measure, |measure| {
                    dynamics::restore(measure, *at, previous)
                })
            }
            ChangeKind::NoteTie {
                at,
                string,
                previous,
            }
            | ChangeKind::NoteDead {
                at,
                string,
                previous,
            } => self.edit_measure(at.track, at.measure, |measure| {
                let beat = beat_in(measure, *at)?;
                let note = beat
                    .notes
                    .iter_mut()
                    .find(|note| u8::try_from(note.string).ok() == Some(string.get()))
                    .ok_or_else(|| {
                        ModelError::Invalid(format!("{at} has no {string} to restore"))
                    })?;
                note.kind = previous.clone();
                Ok(())
            }),
            ChangeKind::NoteEffect {
                at,
                string,
                previous,
            } => self.edit_measure(at.track, at.measure, |measure| {
                let beat = beat_in(measure, *at)?;
                let note = beat
                    .notes
                    .iter_mut()
                    .find(|note| u8::try_from(note.string).ok() == Some(string.get()))
                    .ok_or_else(|| {
                        ModelError::Invalid(format!("{at} has no {string} to restore"))
                    })?;
                note.effect = previous.as_ref().clone();
                Ok(())
            }),
            ChangeKind::BeatEffect { at, previous } => {
                self.edit_measure(at.track, at.measure, |measure| {
                    beat_in(measure, *at)?.effect = previous.as_ref().clone();
                    Ok(())
                })
            }
            ChangeKind::MeasureTempo {
                measure,
                previous,
                previous_initial,
            } => self.edit_headers(|inner| {
                let header = inner
                    .measure_headers
                    .get_mut(measure.get())
                    .ok_or_else(|| ModelError::Invalid(format!("{measure}: no such measure")))?;
                header.tempo = *previous;
                if let Some(initial) = previous_initial {
                    inner.tempo = *initial;
                }
                Ok(())
            }),
            ChangeKind::TimeSignature {
                measure,
                previous_header,
                previous_measures,
            } => self
                .edit_headers(|inner| {
                    let header = inner
                        .measure_headers
                        .get_mut(measure.get())
                        .ok_or_else(|| {
                            ModelError::Invalid(format!("{measure}: no such measure"))
                        })?;
                    header.time_signature = previous_header.clone();
                    let lists = inner
                        .tracks
                        .iter_mut()
                        .flat_map(gscore_gp::Track::measure_lists_mut);
                    for (list, previous) in lists.zip(previous_measures) {
                        if let Some(slot) = list.get_mut(measure.get()) {
                            slot.time_signature = previous.clone();
                        }
                    }
                    Ok(())
                })
                .map(|()| {
                    // Like the edit itself: every measure's layout may depend on the signature.
                    self.touch_all_measures();
                }),
            ChangeKind::KeySignature {
                measure,
                previous_header,
                previous_measures,
            } => self.edit_headers(|inner| {
                let header = inner
                    .measure_headers
                    .get_mut(measure.get())
                    .ok_or_else(|| ModelError::Invalid(format!("{measure}: no such measure")))?;
                header.key_signature = previous_header.clone();
                let lists = inner
                    .tracks
                    .iter_mut()
                    .flat_map(gscore_gp::Track::measure_lists_mut);
                for (list, previous) in lists.zip(previous_measures) {
                    if let Some(slot) = list.get_mut(measure.get()) {
                        slot.key_signature = previous.clone();
                    }
                }
                Ok(())
            }),
            ChangeKind::Section { measure, previous } => self.edit_headers(|inner| {
                let header = inner
                    .measure_headers
                    .get_mut(measure.get())
                    .ok_or_else(|| ModelError::Invalid(format!("{measure}: no such measure")))?;
                header.marker.clone_from(previous);
                Ok(())
            }),
            ChangeKind::Repeat { measure, previous } => self.edit_headers(|inner| {
                let header = inner
                    .measure_headers
                    .get_mut(measure.get())
                    .ok_or_else(|| ModelError::Invalid(format!("{measure}: no such measure")))?;
                (
                    header.repeat_open,
                    header.repeat_close,
                    header.repeat_alternative,
                ) = *previous;
                Ok(())
            }),
            ChangeKind::DoubleBar { measure, previous } => self.edit_headers(|inner| {
                let header = inner
                    .measure_headers
                    .get_mut(measure.get())
                    .ok_or_else(|| ModelError::Invalid(format!("{measure}: no such measure")))?;
                header.double_bar = *previous;
                Ok(())
            }),
            ChangeKind::Feel {
                measure,
                previous,
                previous_initial,
            } => self.edit_headers(|inner| {
                let header = inner
                    .measure_headers
                    .get_mut(measure.get())
                    .ok_or_else(|| ModelError::Invalid(format!("{measure}: no such measure")))?;
                header.triplet_feel = previous.clone();
                if let Some(initial) = previous_initial {
                    inner.triplet_feel = initial.clone();
                }
                Ok(())
            }),
            ChangeKind::Fermata { measure, previous } => self.edit_headers(|inner| {
                let header = inner
                    .measure_headers
                    .get_mut(measure.get())
                    .ok_or_else(|| ModelError::Invalid(format!("{measure}: no such measure")))?;
                header.fermatas.clone_from(previous);
                Ok(())
            }),
            ChangeKind::FreeTime { measure, previous } => {
                self.edit_headers(|inner| {
                    let header = inner
                        .measure_headers
                        .get_mut(measure.get())
                        .ok_or_else(|| {
                            ModelError::Invalid(format!("{measure}: no such measure"))
                        })?;
                    header.free_time = *previous;
                    Ok(())
                })?;
                self.touch_all_measures();
                Ok(())
            }
            ChangeKind::Clef {
                track,
                measure,
                previous,
            } => self.edit_measure(*track, *measure, |m| {
                m.clef = previous.0.to_gp();
                m.has_clef = previous.1;
                Ok(())
            }),
            ChangeKind::SimileMark {
                track,
                measure,
                previous,
            } => self.edit_measure(*track, *measure, |m| {
                m.simile_mark.clone_from(previous);
                Ok(())
            }),
            ChangeKind::TrackStaves { at, previous } => {
                self.edit_track_settings(|inner| {
                    let track = track_mut(inner, *at)?;
                    (
                        track.settings.notation,
                        track.settings.tablature,
                        track.settings.slash,
                    ) = *previous;
                    Ok(())
                })?;
                self.touch_all_measures();
                Ok(())
            }
            ChangeKind::MasterMix { previous } => self.edit_metadata(|inner| {
                inner.master_volume_pan = *previous;
                Ok(())
            }),
            ChangeKind::Batch { changes } => {
                for change in changes.iter().rev() {
                    self.revert(change)?;
                }
                Ok(())
            }
            ChangeKind::TrackMix {
                at,
                previous_channel,
                previous,
                added,
                previous_channel_count,
            } => self.edit_track_settings(|inner| {
                track_mut(inner, *at)?.channel_index = *previous_channel;
                match added {
                    Some(added) if added.saturating_add(1) == inner.channels.len() => {
                        // The appended channel (and any padding) is the tail: trim it.
                        inner
                            .channels
                            .truncate((*previous_channel_count).min(inner.channels.len()));
                    }
                    Some(added) => {
                        // A later edit appended behind this channel; leave it in place but
                        // restore its contents so the channel table is unchanged.
                        if let Some(channel) = inner.channels.get_mut(*added) {
                            *channel = *previous;
                        }
                    }
                    None => {
                        if let Some(channel) = inner.channels.get_mut(*previous_channel) {
                            *channel = *previous;
                        }
                    }
                }
                Ok(())
            }),
        }
    }
}

/// The track at `index`, or an error naming what is missing.
fn track_mut(song: &mut GpSong, index: TrackIndex) -> Result<&mut GpTrack, ModelError> {
    let count = song.tracks.len();
    song.tracks
        .get_mut(index.get())
        .ok_or_else(|| ModelError::Invalid(format!("{index}: the song has {count} track(s)")))
}

/// The voice addressed by `at` in its measure, or an error naming what is missing.
fn voice_in(measure: &mut GpMeasure, at: BeatPosition) -> Result<&mut GpVoice, ModelError> {
    let voices = measure.voices.len();
    measure
        .voices
        .get_mut(at.voice.get())
        .ok_or_else(|| ModelError::Invalid(format!("{at}: the measure has {voices} voice(s)")))
}

/// The beat addressed by `at` in its measure, or an error naming what is missing.
fn beat_in(measure: &mut GpMeasure, at: BeatPosition) -> Result<&mut GpBeat, ModelError> {
    let voice = voice_in(measure, at)?;
    let beats = voice.beats.len();
    voice
        .beats
        .get_mut(at.beat.get())
        .ok_or_else(|| ModelError::Invalid(format!("{at}: the voice has {beats} beat(s)")))
}

#[cfg(test)]
mod dynamic_tests;
#[cfg(test)]
mod effect_tests;
#[cfg(test)]
mod header_tests;
#[cfg(test)]
mod mix_tests;
#[cfg(test)]
mod page_tests;
#[cfg(test)]
mod reach_tests;
#[cfg(test)]
mod song_tests;
#[cfg(test)]
mod staff_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod track_tests;
