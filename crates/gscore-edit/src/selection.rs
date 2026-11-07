//! Selection: a beat range in one track and voice (it may cross bar lines), or a range of
//! measures across every track; and the keyboard actions that make and use it.
//!
//! Copying and cutting read the song into plain data (`clipboard.rs`). Pasting turns that
//! data back into ordinary commands and runs them as one [`Command::Batch`], so an edit
//! (and its entry in the recovery journal) is one line and one undo step. Placeholder beats
//! (Guitar Pro's unused-voice fillers, which saving drops) are never selected or copied.

use gscore_model::{BeatIndex, BeatKind, BeatPosition, MeasureIndex, Song, TrackIndex, VoiceIndex};

mod clipboard;
mod paste;

pub(crate) use clipboard::Clipboard;

use crate::command::Command;
use crate::cursor::Cursor;
use crate::editor::EditError;
use crate::tab_editor::{Outcome, TabEditor};

/// One beat of a voice, as an end of a beat selection. Spots order by measure, then beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BeatSpot {
    /// The measure.
    pub measure: MeasureIndex,
    /// The beat in the measure's voice.
    pub beat: BeatIndex,
}

impl BeatSpot {
    /// The spot of `cursor`'s beat.
    pub fn of(cursor: Cursor) -> Self {
        BeatSpot {
            measure: cursor.measure,
            beat: cursor.beat,
        }
    }
}

/// What is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// A range of whole beats in one track and voice (inclusive, either direction), possibly
    /// over several measures.
    Beats {
        /// The track.
        track: TrackIndex,
        /// The voice.
        voice: VoiceIndex,
        /// Where the selection started.
        anchor: BeatSpot,
        /// Where it ends now.
        focus: BeatSpot,
    },
    /// A range of whole measures, in one track or in every track.
    Measures {
        /// Where the selection started.
        anchor: MeasureIndex,
        /// Where it ends now.
        focus: MeasureIndex,
        /// The one selected track, or `None` for every track.
        track: Option<TrackIndex>,
    },
}

impl Selection {
    /// The first selected measure.
    pub fn first_measure(self) -> MeasureIndex {
        match self {
            Selection::Beats { anchor, focus, .. } => anchor.min(focus).measure,
            Selection::Measures { anchor, focus, .. } => anchor.min(focus),
        }
    }

    /// The last selected measure.
    pub fn last_measure(self) -> MeasureIndex {
        match self {
            Selection::Beats { anchor, focus, .. } => anchor.max(focus).measure,
            Selection::Measures { anchor, focus, .. } => anchor.max(focus),
        }
    }

    /// Every selected beat that exists in `song`, in order: for a beat selection, its beats
    /// of the one voice; for a measure selection, every beat of every track and voice in
    /// those measures. Placeholder beats are left out.
    pub fn positions(self, song: &Song) -> Vec<BeatPosition> {
        let mut positions = Vec::new();
        let mut push_measure = |track: TrackIndex,
                                measure: MeasureIndex,
                                voice: VoiceIndex,
                                from: usize,
                                to: Option<usize>| {
            let Some(view) = song
                .track(track)
                .and_then(|t| t.measure(measure))
                .and_then(|m| m.voice(voice))
            else {
                return;
            };
            for (index, beat) in view.beats().enumerate() {
                let inside = index >= from && to.is_none_or(|to| index <= to);
                if inside && beat.kind() != BeatKind::Empty && !beat.is_grace() {
                    positions.push(BeatPosition {
                        track,
                        measure,
                        voice,
                        beat: BeatIndex::new(index),
                    });
                }
            }
        };
        match self {
            Selection::Beats {
                track,
                voice,
                anchor,
                focus,
            } => {
                let first = anchor.min(focus);
                let last = anchor.max(focus);
                let count = song.measure_count();
                for index in first.measure.get()..=last.measure.get().min(count.saturating_sub(1)) {
                    let measure = MeasureIndex::new(index);
                    let from = if measure == first.measure {
                        first.beat.get()
                    } else {
                        0
                    };
                    let to = (measure == last.measure).then_some(last.beat.get());
                    push_measure(track, measure, voice, from, to);
                }
            }
            Selection::Measures {
                anchor,
                focus,
                track,
            } => {
                let first = anchor.min(focus).get();
                let last = anchor.max(focus).get();
                let tracks: Vec<usize> = match track {
                    Some(track) if track.get() < song.track_count() => vec![track.get()],
                    Some(_) => Vec::new(),
                    None => (0..song.track_count()).collect(),
                };
                for track in tracks {
                    for index in first..=last {
                        let measure = MeasureIndex::new(index);
                        let voices = song
                            .track(TrackIndex::new(track))
                            .and_then(|t| t.measure(measure))
                            .map_or(0, |m| m.voices().len());
                        for voice in 0..voices {
                            push_measure(
                                TrackIndex::new(track),
                                measure,
                                VoiceIndex::new(voice),
                                0,
                                None,
                            );
                        }
                    }
                }
            }
        }
        positions
    }
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

    /// The anchor of a beat selection in the cursor's track and voice, if there is one.
    fn beat_anchor(&self) -> Option<BeatSpot> {
        match self.selection {
            Some(Selection::Beats {
                track,
                voice,
                anchor,
                ..
            }) if track == self.cursor.track && voice == self.cursor.voice => Some(anchor),
            _ => None,
        }
    }

    /// Moves the cursor one beat (across bar lines, never adding beats) and extends the beat
    /// selection to it; without one, the selection starts at the beat the cursor was on.
    pub(crate) fn extend_selection(&mut self, forward: bool) -> Outcome {
        let anchor = self
            .beat_anchor()
            .unwrap_or_else(|| BeatSpot::of(self.cursor));
        let song = self.editor.song();
        if forward {
            self.cursor.right(song);
        } else {
            self.cursor.left(song);
        }
        self.selection = Some(Selection::Beats {
            track: self.cursor.track,
            voice: self.cursor.voice,
            anchor,
            focus: BeatSpot::of(self.cursor),
        });
        Outcome::Moved
    }

    /// Moves the cursor to the track's first (`to_start`) or last beat and extends the
    /// beat selection to it, as Shift+Home and Shift+End do.
    pub(crate) fn extend_selection_to(&mut self, to_start: bool) -> Outcome {
        let anchor = self
            .beat_anchor()
            .unwrap_or_else(|| BeatSpot::of(self.cursor));
        let song = self.editor.song();
        if to_start {
            self.cursor.start_of_track(song);
        } else {
            self.cursor.end_of_track(song);
        }
        self.selection = Some(Selection::Beats {
            track: self.cursor.track,
            voice: self.cursor.voice,
            anchor,
            focus: BeatSpot::of(self.cursor),
        });
        Outcome::Moved
    }

    /// Selects the cursor's whole beat (every string), unless a beat selection in its voice
    /// already holds it.
    pub(crate) fn select_beat(&mut self) -> Outcome {
        let spot = BeatSpot::of(self.cursor);
        if let Some(Selection::Beats { anchor, focus, .. }) = self.selection
            && self.beat_anchor().is_some()
            && (anchor.min(focus)..=anchor.max(focus)).contains(&spot)
        {
            return Outcome::Nothing;
        }
        self.selection = Some(Selection::Beats {
            track: self.cursor.track,
            voice: self.cursor.voice,
            anchor: spot,
            focus: spot,
        });
        Outcome::Moved
    }

    /// Removes every note of the selected beats (they become rests), as one undo step. The
    /// selection stays. Beats without notes are left alone.
    pub(crate) fn clear_selection(&mut self, selection: Selection) -> Result<Outcome, EditError> {
        let song = self.song();
        let commands: Vec<Command> = selection
            .positions(song)
            .into_iter()
            .filter(|at| {
                song.beat(*at)
                    .is_some_and(|beat| beat.notes().next().is_some())
            })
            .map(|at| Command::SetBeatRest { at })
            .collect();
        if commands.is_empty() {
            return Ok(Outcome::Nothing);
        }
        self.execute(Command::Batch { commands }, self.cursor, false)
    }
}
