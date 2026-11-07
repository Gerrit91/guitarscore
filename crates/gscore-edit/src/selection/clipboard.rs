//! The clipboard: copying and cutting read the song into plain data, pasting turns it back
//! into commands (see `paste.rs`) that run as one batch.

use gscore_model::{
    BeatEffect, BeatKind, BeatView, Dynamic, Fade, Fret, Hand, MeasureIndex, NoteEffect, NoteKind,
    NoteValue, NoteView, Section, SlideKind, StringNumber, StrokeKind, TimeSignature, TrackIndex,
};

use super::paste::{paste_beats, paste_measures};
use super::{BeatSpot, Selection};
use crate::command::Command;
use crate::cursor::Cursor;
use crate::editor::EditError;
use crate::tab_editor::{Outcome, TabEditor};

/// One copied note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoteClip {
    pub(super) string: StringNumber,
    pub(super) fret: Fret,
    pub(super) tie: bool,
    /// A dead (muted, "x") note.
    pub(super) dead: bool,
    /// Every effect, set explicitly, so the pasted note matches the copied one.
    pub(super) effects: Vec<NoteEffect>,
}

/// One copied beat: its length, whether it is a rest, its notes, its beat effects and
/// its dynamic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BeatClip {
    pub(super) duration: NoteValue,
    pub(super) rest: bool,
    pub(super) notes: Vec<NoteClip>,
    pub(super) effects: Vec<BeatEffect>,
    pub(super) dynamic: Option<Dynamic>,
}

/// One copied measure across every voice of one track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MeasureClip {
    pub(super) voices: Vec<Vec<BeatClip>>,
    pub(super) time_signature: TimeSignature,
    pub(super) tempo: Option<u32>,
    pub(super) section: Option<Section>,
}

/// What was copied, or nothing.
#[derive(Debug, Clone, Default)]
pub(crate) enum Clipboard {
    #[default]
    Empty,
    /// Copied beats of one voice, grouped by the measure they came from, so a paste keeps
    /// the bar lines where they were.
    Beats(Vec<Vec<BeatClip>>),
    /// Copied measures, each with the track it belongs to (all tracks, or only one).
    Measures(Vec<(TrackIndex, Vec<MeasureClip>)>),
}

impl TabEditor {
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
            Selection::Beats { .. } => selection
                .positions(self.song())
                .into_iter()
                .map(|at| Command::SetBeatRest { at })
                .collect(),
            // Cutting one track's bars clears their notes; deleting the bars themselves
            // would remove them from every track.
            Selection::Measures { track: Some(_), .. } => selection
                .positions(self.song())
                .into_iter()
                .map(|at| Command::SetBeatRest { at })
                .collect(),
            Selection::Measures { track: None, .. } => {
                let first = selection.first_measure().get();
                let last = selection.last_measure().get();
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
            Clipboard::Beats(groups) => {
                let cursor = self.cursor();
                let at = cursor.position();
                let (commands, last) = paste_beats(self.song(), at, &groups)?;
                self.selection = Some(Selection::Beats {
                    track: at.track,
                    voice: at.voice,
                    anchor: BeatSpot::of(cursor),
                    focus: last,
                });
                self.execute(Command::Batch { commands }, cursor, false)
            }
            Clipboard::Measures(tracks) => {
                let first = self
                    .selection
                    .map(Selection::first_measure)
                    .unwrap_or_else(|| self.cursor().measure);
                let count = tracks.first().map_or(0, |(_, measures)| measures.len());
                let track = match tracks.as_slice() {
                    [(track, _)] => Some(*track),
                    _ => None,
                };
                let commands = paste_measures(self.song(), first, &tracks)?;
                let mut cursor = Cursor {
                    measure: first,
                    beat: gscore_model::BeatIndex::new(0),
                    ..self.cursor()
                };
                if let Some(track) = track {
                    cursor.track = track;
                }
                if count > 0 {
                    self.selection = Some(Selection::Measures {
                        anchor: first,
                        focus: MeasureIndex::new(
                            first.get().saturating_add(count.saturating_sub(1)),
                        ),
                        track,
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
                voice,
                anchor,
                focus,
            } => {
                let first = anchor.min(focus);
                cursor.track = track;
                cursor.voice = voice;
                cursor.measure = first.measure;
                cursor.beat = first.beat;
            }
            Selection::Measures {
                anchor, focus, track, ..
            } => {
                cursor.measure = anchor.min(focus);
                cursor.beat = gscore_model::BeatIndex::new(0);
                if let Some(track) = track {
                    cursor.track = track;
                }
            }
        }
        cursor
    }

    /// Reads `selection` from the song.
    fn clipboard_from(&self, selection: Selection) -> Result<Clipboard, EditError> {
        let song = self.song();
        match selection {
            Selection::Beats { anchor, focus, .. } => {
                let first = anchor.min(focus).measure.get();
                let last = anchor.max(focus).measure.get();
                let mut groups: Vec<Vec<BeatClip>> = (first..=last).map(|_| Vec::new()).collect();
                for at in selection.positions(song) {
                    let Some(beat) = song.beat(at) else { continue };
                    let group = at.measure.get().saturating_sub(first);
                    if let (Some(clip), Some(group)) = (beat_clip(&beat)?, groups.get_mut(group)) {
                        group.push(clip);
                    }
                }
                if groups.iter().all(Vec::is_empty) {
                    return Err(EditError::NothingSelected);
                }
                Ok(Clipboard::Beats(groups))
            }
            Selection::Measures {
                anchor,
                focus,
                track,
            } => {
                let first = anchor.min(focus).get();
                let last = anchor.max(focus).get();
                if first >= song.measure_count() {
                    return Err(EditError::NothingSelected);
                }
                let last = last.min(song.measure_count().saturating_sub(1));
                let indices: Vec<usize> = match track {
                    Some(track) if track.get() < song.track_count() => vec![track.get()],
                    Some(_) => return Err(EditError::NothingSelected),
                    None => (0..song.track_count()).collect(),
                };
                let mut tracks = Vec::new();
                for track_index in indices {
                    let track_at = TrackIndex::new(track_index);
                    let Some(track) = song.track(track_at) else {
                        continue;
                    };
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
                    tracks.push((track_at, measures));
                }
                Ok(Clipboard::Measures(tracks))
            }
        }
    }

}

/// The copyable content of one beat; `None` for a placeholder beat (no length, dropped
/// when saving), which a paste must not turn into a rest.
fn beat_clip(beat: &BeatView<'_>) -> Result<Option<BeatClip>, EditError> {
    if beat.kind() == BeatKind::Empty || beat.is_grace() {
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
        dynamic: beat.dynamic(),
    }))
}

/// Every slide the model knows, so a paste can clear the ones the source lacks.
const SLIDES: [SlideKind; 8] = [
    SlideKind::IntoFromAbove,
    SlideKind::IntoFromBelow,
    SlideKind::ShiftSlideTo,
    SlideKind::LegatoSlideTo,
    SlideKind::OutDownwards,
    SlideKind::OutUpwards,
    SlideKind::PickSlideDown,
    SlideKind::PickSlideUp,
];

/// Every note effect, set to its copied state.
fn note_effects(note: &NoteView<'_>) -> Vec<NoteEffect> {
    let mut effects = vec![
        NoteEffect::Hammer(note.is_hammer()),
        NoteEffect::Ghost(note.is_ghost()),
        NoteEffect::PalmMute(note.is_palm_muted()),
        NoteEffect::LetRing(note.is_let_ring()),
        NoteEffect::Staccato(note.is_staccato()),
        NoteEffect::Trill(note.trill()),
        NoteEffect::TremoloPicking(note.tremolo_speed()),
        NoteEffect::LeftHandTap(note.is_left_hand_tapped()),
        NoteEffect::StringNumber(note.shows_string_number()),
        NoteEffect::Finger(Hand::Left, note.finger(Hand::Left)),
        NoteEffect::Finger(Hand::Right, note.finger(Hand::Right)),
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
        BeatEffect::BrushStroke(beat.stroke(StrokeKind::Brush)),
        BeatEffect::Slap(beat.slap()),
        BeatEffect::FadeIn(beat.fade() == Some(Fade::In)),
        BeatEffect::FadeOut(beat.fade() == Some(Fade::Out)),
        BeatEffect::VolumeSwell(beat.fade() == Some(Fade::Swell)),
        BeatEffect::Octave(beat.octave()),
        BeatEffect::Arpeggio(beat.stroke(StrokeKind::Arpeggio)),
        BeatEffect::DeadSlapped(beat.is_dead_slapped()),
        BeatEffect::Rasgueado(beat.has_rasgueado()),
        BeatEffect::Whammy(beat.whammy_shape()),
        BeatEffect::Legato(beat.legato()),
        BeatEffect::Vibrato(beat.vibrato()),
    ]
}
