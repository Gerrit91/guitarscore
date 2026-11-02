//! Selection and the clipboard: a beat range in one track and voice, or a range of
//! measures across every track.
//!
//! Copying and cutting read the song into plain data. Pasting turns that data back into
//! ordinary commands and runs them as one [`Command::Batch`], so an edit (and its entry in
//! the recovery journal) is one line and one undo step. Placeholder beats (Guitar Pro's
//! unused-voice fillers, which saving drops) are not copied.

use gscore_model::{
    BeatEffect, BeatIndex, BeatKind, BeatPosition, BeatView, Fret, MeasureIndex, ModelError,
    NoteEffect, NoteKind, NoteValue, NoteView, Section, SlideKind, StringNumber, TimeSignature,
    TrackIndex, VoiceIndex,
};

mod paste;

use paste::{paste_beats, paste_measures};

use crate::command::Command;
use crate::cursor::Cursor;
use crate::editor::EditError;
use crate::tab_editor::{Outcome, TabEditor};

/// What is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// A range of beats in one measure, voice and track (inclusive, either direction).
    Beats {
        /// The track.
        track: TrackIndex,
        /// The measure.
        measure: MeasureIndex,
        /// The voice.
        voice: VoiceIndex,
        /// Where the selection started.
        anchor: BeatIndex,
        /// Where it ends now.
        focus: BeatIndex,
    },
    /// A range of whole measures, in every track.
    Measures {
        /// Where the selection started.
        anchor: MeasureIndex,
        /// Where it ends now.
        focus: MeasureIndex,
    },
}

impl Selection {
    /// The first selected beat of a beat selection.
    pub fn first_beat(self) -> Option<BeatIndex> {
        match self {
            Selection::Beats { anchor, focus, .. } => Some(anchor.min(focus)),
            Selection::Measures { .. } => None,
        }
    }

    /// The last selected beat of a beat selection.
    pub fn last_beat(self) -> Option<BeatIndex> {
        match self {
            Selection::Beats { anchor, focus, .. } => Some(anchor.max(focus)),
            Selection::Measures { .. } => None,
        }
    }

    /// The first selected measure.
    pub fn first_measure(self) -> MeasureIndex {
        match self {
            Selection::Beats { measure, .. } => measure,
            Selection::Measures { anchor, focus } => anchor.min(focus),
        }
    }

    /// The last selected measure.
    pub fn last_measure(self) -> MeasureIndex {
        match self {
            Selection::Beats { measure, .. } => measure,
            Selection::Measures { anchor, focus } => anchor.max(focus),
        }
    }

    /// Whether the start and end are the same.
    pub fn is_empty(self) -> bool {
        match self {
            Selection::Beats { anchor, focus, .. } => anchor == focus,
            Selection::Measures { anchor, focus } => anchor == focus,
        }
    }
}

/// One copied note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoteClip {
    string: StringNumber,
    fret: Fret,
    tie: bool,
    /// A dead (muted, "x") note.
    dead: bool,
    /// Every effect, set explicitly, so the pasted note matches the copied one.
    effects: Vec<NoteEffect>,
}

/// One copied beat: its length, whether it is a rest, its notes and its beat effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BeatClip {
    duration: NoteValue,
    rest: bool,
    notes: Vec<NoteClip>,
    effects: Vec<BeatEffect>,
}

/// One copied measure across every voice of one track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MeasureClip {
    voices: Vec<Vec<BeatClip>>,
    time_signature: TimeSignature,
    tempo: Option<u32>,
    section: Option<Section>,
}

/// What was copied, or nothing.
#[derive(Debug, Clone, Default)]
pub(crate) enum Clipboard {
    #[default]
    Empty,
    Beats(Vec<BeatClip>),
    Measures(Vec<Vec<MeasureClip>>),
}

impl TabEditor {
    /// The current selection.
    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    /// Replaces the selection; `None` clears it.
    pub fn set_selection(&mut self, selection: Option<Selection>) {
        self.selection = selection;
    }

    /// Copies the selected beats or measures. Copying changes nothing in the song.
    pub fn copy_selection(&mut self) -> Result<Outcome, EditError> {
        let selection = self.selection.ok_or(EditError::NothingSelected)?;
        let clipboard = self.clipboard_from(selection)?;
        self.clipboard = clipboard;
        Ok(Outcome::Nothing)
    }

    /// Copies the selection and clears it (notes become rests; measures are removed).
    pub fn cut_selection(&mut self) -> Result<Outcome, EditError> {
        self.copy_selection()?;
        let selection = self.selection.ok_or(EditError::NothingSelected)?;
        let commands = match selection {
            Selection::Beats {
                track,
                measure,
                voice,
                anchor,
                focus,
            } => {
                let first = anchor.min(focus).get();
                let last = anchor.max(focus).get();
                (first..=last)
                    .map(|beat| Command::SetBeatRest {
                        at: BeatPosition {
                            track,
                            measure,
                            voice,
                            beat: BeatIndex::new(beat),
                        },
                    })
                    .collect()
            }
            Selection::Measures { anchor, focus } => {
                let first = anchor.min(focus).get();
                let last = anchor.max(focus).get();
                let count = last.saturating_sub(first).saturating_add(1);
                if count >= self.song().measure_count() {
                    return Err(EditError::Unsupported(
                        "cannot cut every measure of a song".to_owned(),
                    ));
                }
                // Delete from the end so the earlier indices stay valid.
                (first..=last)
                    .rev()
                    .map(|index| Command::DeleteMeasure {
                        measure: MeasureIndex::new(index),
                    })
                    .collect()
            }
        };
        let cursor = self.cursor_after_cut(selection);
        self.execute(Command::Batch { commands }, cursor, false)
    }

    /// Pastes at the cursor (beats) or at the selected/first measure (measures).
    pub fn paste_clipboard(&mut self) -> Result<Outcome, EditError> {
        let clipboard = self.clipboard.clone();
        match clipboard {
            Clipboard::Empty => Err(EditError::NothingToPaste),
            Clipboard::Beats(beats) => {
                let cursor = self.cursor();
                let at = cursor.position();
                let commands = paste_beats(self.song(), at, &beats)?;
                let last = cursor
                    .beat
                    .get()
                    .saturating_add(beats.len().saturating_sub(1));
                self.selection = Some(Selection::Beats {
                    track: at.track,
                    measure: at.measure,
                    voice: at.voice,
                    anchor: cursor.beat,
                    focus: BeatIndex::new(last),
                });
                self.execute(Command::Batch { commands }, cursor, false)
            }
            Clipboard::Measures(tracks) => {
                let first = self
                    .selection
                    .map(Selection::first_measure)
                    .unwrap_or_else(|| self.cursor().measure);
                let count = tracks.first().map_or(0, Vec::len);
                let commands = paste_measures(self.song(), first, &tracks)?;
                let cursor = Cursor {
                    measure: first,
                    beat: BeatIndex::new(0),
                    ..self.cursor()
                };
                if count > 0 {
                    self.selection = Some(Selection::Measures {
                        anchor: first,
                        focus: MeasureIndex::new(
                            first.get().saturating_add(count.saturating_sub(1)),
                        ),
                    });
                }
                self.execute(Command::Batch { commands }, cursor, false)
            }
        }
    }

    /// The cursor after cutting `selection`: the start of the removed range.
    fn cursor_after_cut(&self, selection: Selection) -> Cursor {
        let mut cursor = self.cursor();
        match selection {
            Selection::Beats {
                track,
                measure,
                voice,
                anchor,
                focus,
            } => {
                cursor.track = track;
                cursor.measure = measure;
                cursor.voice = voice;
                cursor.beat = anchor.min(focus);
            }
            Selection::Measures { anchor, focus } => {
                cursor.measure = anchor.min(focus);
                cursor.beat = BeatIndex::new(0);
            }
        }
        cursor
    }

    /// Reads `selection` from the song.
    fn clipboard_from(&self, selection: Selection) -> Result<Clipboard, EditError> {
        let song = self.song();
        match selection {
            Selection::Beats {
                track,
                measure,
                voice,
                anchor,
                focus,
            } => {
                let view = song
                    .track(track)
                    .and_then(|track| track.measure(measure))
                    .and_then(|measure| measure.voice(voice))
                    .ok_or_else(|| {
                        EditError::Model(ModelError::Invalid(
                            "the selection is not in the song".to_owned(),
                        ))
                    })?;
                let first = anchor.min(focus).get();
                let last = anchor.max(focus).get();
                let mut beats = Vec::new();
                for index in first..=last {
                    let beat = view.beats().nth(index).ok_or(EditError::NothingSelected)?;
                    if let Some(clip) = beat_clip(&beat)? {
                        beats.push(clip);
                    }
                }
                if beats.is_empty() {
                    return Err(EditError::NothingSelected);
                }
                Ok(Clipboard::Beats(beats))
            }
            Selection::Measures { anchor, focus } => {
                let first = anchor.min(focus).get();
                let last = anchor.max(focus).get();
                if first >= song.measure_count() {
                    return Err(EditError::NothingSelected);
                }
                let last = last.min(song.measure_count().saturating_sub(1));
                let mut tracks = Vec::new();
                for track in song.tracks() {
                    let mut measures = Vec::new();
                    for index in first..=last {
                        let measure_index = MeasureIndex::new(index);
                        let Some(measure) = track.measure(measure_index) else {
                            continue;
                        };
                        let voices = measure
                            .voices()
                            .map(|voice| {
                                voice
                                    .beats()
                                    .filter_map(|beat| beat_clip(&beat).transpose())
                                    .collect::<Result<Vec<_>, _>>()
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        let time_signature = measure.time_signature().map_err(EditError::Model)?;
                        measures.push(MeasureClip {
                            voices,
                            time_signature,
                            tempo: song.tempo_change(measure_index),
                            section: song.section(measure_index),
                        });
                    }
                    tracks.push(measures);
                }
                Ok(Clipboard::Measures(tracks))
            }
        }
    }
}

/// The copyable content of one beat; `None` for a placeholder beat (no length, dropped
/// when saving), which a paste must not turn into a rest.
fn beat_clip(beat: &BeatView<'_>) -> Result<Option<BeatClip>, EditError> {
    if beat.kind() == BeatKind::Empty {
        return Ok(None);
    }
    let duration = beat.note_value().map_err(EditError::Model)?;
    let mut notes = Vec::new();
    for note in beat.notes() {
        notes.push(NoteClip {
            string: note.string().map_err(EditError::Model)?,
            fret: note.fret().map_err(EditError::Model)?,
            tie: note.kind() == NoteKind::Tie,
            dead: note.kind() == NoteKind::Dead,
            effects: note_effects(&note),
        });
    }
    Ok(Some(BeatClip {
        duration,
        rest: beat.kind() == BeatKind::Rest,
        notes,
        effects: beat_effects(beat),
    }))
}

/// Every slide the model knows, so a paste can clear the ones the source lacks.
const SLIDES: [SlideKind; 6] = [
    SlideKind::IntoFromAbove,
    SlideKind::IntoFromBelow,
    SlideKind::ShiftSlideTo,
    SlideKind::LegatoSlideTo,
    SlideKind::OutDownwards,
    SlideKind::OutUpwards,
];

/// Every note effect, set to its copied state.
fn note_effects(note: &NoteView<'_>) -> Vec<NoteEffect> {
    let mut effects = vec![
        NoteEffect::Hammer(note.is_hammer()),
        NoteEffect::Ghost(note.is_ghost()),
        NoteEffect::PalmMute(note.is_palm_muted()),
        NoteEffect::LetRing(note.is_let_ring()),
        NoteEffect::Staccato(note.is_staccato()),
        NoteEffect::Trill(note.has_trill()),
        NoteEffect::TremoloPicking(note.has_tremolo_picking()),
        NoteEffect::Accent(note.accent()),
        NoteEffect::Vibrato(note.vibrato()),
        NoteEffect::Harmonic(note.harmonic()),
        NoteEffect::Bend(note.bend_shape()),
    ];
    let present: Vec<SlideKind> = note.slides().collect();
    for kind in SLIDES {
        effects.push(NoteEffect::Slide(kind, present.contains(&kind)));
    }
    effects
}

/// Every beat effect, set to its copied state.
fn beat_effects(beat: &BeatView<'_>) -> Vec<BeatEffect> {
    vec![
        BeatEffect::PickStroke(beat.pick_stroke()),
        BeatEffect::BrushStroke(beat.brush_stroke()),
        BeatEffect::Slap(beat.slap()),
        BeatEffect::FadeIn(beat.has_fade_in()),
        BeatEffect::Whammy(beat.whammy_shape()),
        BeatEffect::Legato(beat.legato()),
        BeatEffect::Vibrato(beat.vibrato()),
    ]
}
