//! The clipboard: copying and cutting read the song into plain data, pasting turns it back
//! into commands (see `paste.rs`) that run as one batch.

use gscore_model::{
    BeatEffect, BeatKind, BeatView, Dynamic, Fade, Fret, Hand, MeasureIndex, NoteEffect, NoteKind,
    NoteValue, NoteView, Section, SlideKind, StaffIndex, StringNumber, StrokeKind, TimeSignature,
    TrackIndex,
};

use super::paste::{paste_measures, replace_beats};
use super::{BeatSpot, MAX_PASTE_COPIES, PasteOptions, Selection};
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
    /// The text written above the beat.
    pub(super) text: String,
}

/// One copied measure across every voice of one track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MeasureClip {
    pub(super) voices: Vec<Vec<BeatClip>>,
    pub(super) time_signature: TimeSignature,
    pub(super) tempo: Option<u32>,
    pub(super) section: Option<Section>,
}

/// One staff's copied measures, with the track and staff they belong to.
pub(crate) type StaffClip = (TrackIndex, StaffIndex, Vec<MeasureClip>);

/// What was copied, or nothing. Opaque: the app moves it between open songs
/// ([`TabEditor::clipboard`], [`TabEditor::set_clipboard`]), so what was copied in one can
/// be pasted in another.
#[derive(Debug, Clone, Default)]
pub struct Clipboard(Clip);

#[derive(Debug, Clone, Default)]
pub(crate) enum Clip {
    #[default]
    Empty,
    /// Copied beats of one voice, grouped by the measure they came from, so a paste keeps
    /// the bar lines where they were.
    Beats(Vec<Vec<BeatClip>>),
    /// Copied measures, each with the track and staff it belongs to (all tracks, or only
    /// one; every staff of a track), and the bar they were copied from: pasting them with
    /// the selection still there duplicates them after it. Cut bars have no source: they
    /// are no longer there, whatever the selection now covers.
    Measures {
        staves: Vec<StaffClip>,
        source: Option<MeasureIndex>,
    },
}

impl TabEditor {
    /// What was copied, to hand to another open song ([`TabEditor::set_clipboard`]).
    pub fn clipboard(&self) -> Clipboard {
        Clipboard(self.clipboard.clone())
    }

    /// Takes over what was copied in another song. Copied bars forget the bar they came
    /// from: here, a selection on the same bar numbers is other music, so a paste writes
    /// over it instead of duplicating it after it.
    pub fn set_clipboard(&mut self, clipboard: Clipboard) {
        let Clipboard(mut clip) = clipboard;
        if let Clip::Measures { source, .. } = &mut clip {
            *source = None;
        }
        self.clipboard = clip;
    }

    /// A plain-text line describing what was copied (`None` when nothing was), for the
    /// system clipboard. The app puts it there on Copy and Cut: a desktop sends Ctrl+V on
    /// as a paste only when the system clipboard holds text, so without it our own paste
    /// would never arrive.
    pub fn clipboard_text(&self) -> Option<String> {
        let plural = |count: usize, one: &str, many: &str| {
            if count == 1 {
                format!("1 {one}")
            } else {
                format!("{count} {many}")
            }
        };
        let what = match &self.clipboard {
            Clip::Empty => return None,
            Clip::Beats(groups) => plural(groups.iter().map(Vec::len).sum(), "beat", "beats"),
            Clip::Measures { staves, .. } => {
                let bars = staves.first().map_or(0, |(_, _, bars)| bars.len());
                let mut text = plural(bars, "bar", "bars");
                let tracks: Vec<TrackIndex> = staves.iter().map(|(track, _, _)| *track).fold(
                    Vec::new(),
                    |mut tracks, track| {
                        if !tracks.contains(&track) {
                            tracks.push(track);
                        }
                        tracks
                    },
                );
                if tracks.len() > 1 {
                    text.push_str(&format!(" of {} tracks", tracks.len()));
                }
                text
            }
        };
        Some(format!("GuitarScore: {what}"))
    }

    /// Copies the selected beats or measures; with nothing selected, the beat under the
    /// cursor (as Guitar Pro does). Copying changes nothing in the song.
    pub fn copy_selection(&mut self) -> Result<Outcome, EditError> {
        let clipboard = match self.selection {
            Some(selection) => self.clipboard_from(selection)?,
            None => self.beat_at_cursor()?,
        };
        self.clipboard = clipboard;
        Ok(Outcome::Nothing)
    }

    /// Copies whole bars from every track, whatever track the selection is in: the
    /// selected bars, the bars a beat selection in the score touches, or with nothing
    /// selected the cursor's bar. The bars can then be placed with the paste dialog.
    pub fn copy_selection_all_tracks(&mut self) -> Result<Outcome, EditError> {
        let (anchor, focus) = match self.selection {
            Some(Selection::Measures { anchor, focus, .. }) => (anchor, focus),
            Some(Selection::Beats { anchor, focus, .. }) => (anchor.measure, focus.measure),
            None => (self.cursor().measure, self.cursor().measure),
        };
        let selection = Selection::Measures {
            anchor,
            focus,
            track: None,
        };
        self.clipboard = self.clipboard_from(selection)?;
        Ok(Outcome::Nothing)
    }

    /// How many bars the clipboard holds, or `None` when it holds notes or beats.
    pub fn clipboard_bars(&self) -> Option<usize> {
        match &self.clipboard {
            Clip::Measures { staves, .. } => staves
                .first()
                .map(|(_, _, measures)| measures.len())
                .filter(|count| *count > 0),
            Clip::Beats(_) | Clip::Empty => None,
        }
    }

    /// Swaps the bars of the two sections that start at `a` and at `b` (each lasting to
    /// the bar before the next section, or the song's end), in every track and staff, as
    /// one undo step. Whole bars move, so everything each bar holds goes with it: its
    /// notes, section name and color, tempo change, signatures, repeats, endings,
    /// directions, fermatas, clefs and line breaks.
    pub fn swap_sections(
        &mut self,
        a: MeasureIndex,
        b: MeasureIndex,
    ) -> Result<Outcome, EditError> {
        if a == b {
            return Err(EditError::Unsupported(
                "a section cannot be swapped with itself".to_owned(),
            ));
        }
        let a = self
            .section_span(a)
            .ok_or_else(|| EditError::Unsupported("that bar starts no section".to_owned()))?;
        let b = self
            .section_span(b)
            .ok_or_else(|| EditError::Unsupported("that bar starts no section".to_owned()))?;
        self.swap_spans(a, b)
    }

    /// Swaps a run of whole sections, the bars `first` to `last` (several sections
    /// selected together), with the section that starts at `target`, as one undo step:
    /// the run takes the section's place and the section the run's, the sections between
    /// them staying in order. Dragging several selected sections onto another in the Song
    /// tab's list.
    pub fn swap_section_run(
        &mut self,
        first: MeasureIndex,
        last: MeasureIndex,
        target: MeasureIndex,
    ) -> Result<Outcome, EditError> {
        let song = self.song();
        let count = song.measure_count();
        let (first, last) = (first.get().min(last.get()), first.get().max(last.get()));
        // The run must be whole sections: it starts at one and ends before the next one
        // (or at the song's end).
        let whole = last < count
            && song.section(MeasureIndex::new(first)).is_some()
            && (last.saturating_add(1) == count
                || song
                    .section(MeasureIndex::new(last.saturating_add(1)))
                    .is_some());
        if !whole {
            return Err(EditError::Unsupported(
                "only whole sections can be moved together".to_owned(),
            ));
        }
        let target = self
            .section_span(target)
            .ok_or_else(|| EditError::Unsupported("that bar starts no section".to_owned()))?;
        if (first..=last).contains(&target.0) {
            return Err(EditError::Unsupported(
                "the sections cannot be swapped with one of themselves".to_owned(),
            ));
        }
        self.swap_spans((first, last), target)
    }

    /// The bars of the section that starts at `start`: to the bar before the next section,
    /// or the song's last bar. `None` if no section starts there.
    fn section_span(&self, start: MeasureIndex) -> Option<(usize, usize)> {
        let song = self.song();
        let count = song.measure_count();
        if start.get() >= count || song.section(start).is_none() {
            return None;
        }
        let end = (start.get().saturating_add(1)..count)
            .find(|index| song.section(MeasureIndex::new(*index)).is_some())
            .map_or(count.saturating_sub(1), |next| next.saturating_sub(1));
        Some((start.get(), end))
    }

    /// Swaps two runs of bars that do not overlap, `(a_start, a_end)` and
    /// `(b_start, b_end)`, in every track, as one undo step; the bars of `a` are selected
    /// where they went, and the cursor goes to their first bar.
    fn swap_spans(
        &mut self,
        (a_start, a_end): (usize, usize),
        (b_start, b_end): (usize, usize),
    ) -> Result<Outcome, EditError> {
        let (first, last) = (a_start.min(b_start), a_end.max(b_end));
        // The later run moves to where the earlier one starts, then the earlier one
        // (pushed right by it) moves past the bars between them, if any. Whole bars move,
        // so everything a bar holds goes with it: its tempo change, signatures, section,
        // repeats, endings, directions and the rest.
        let (x0, x1, y0, y1) = if a_start < b_start {
            (a_start, a_end, b_start, b_end)
        } else {
            (b_start, b_end, a_start, a_end)
        };
        let len_x = x1.saturating_sub(x0).saturating_add(1);
        let len_y = y1.saturating_sub(y0).saturating_add(1);
        let mut commands = vec![Command::MoveMeasures {
            from: MeasureIndex::new(y0),
            count: len_y,
            to: MeasureIndex::new(x0),
        }];
        let x_now = x0.saturating_add(len_y);
        let x_final = last.saturating_add(1).saturating_sub(len_x);
        if x_now != x_final {
            commands.push(Command::MoveMeasures {
                from: MeasureIndex::new(x_now),
                count: len_x,
                to: MeasureIndex::new(x_final),
            });
        }
        let len_a = a_end.saturating_sub(a_start).saturating_add(1);
        let (start, end) = if a_start < b_start {
            (last.saturating_add(1).saturating_sub(len_a), last)
        } else {
            (first, first.saturating_add(len_a).saturating_sub(1))
        };
        let cursor = Cursor {
            measure: MeasureIndex::new(start),
            beat: gscore_model::BeatIndex::new(0),
            ..self.cursor()
        };
        let outcome = self.execute(Command::Batch { commands }, cursor, false)?;
        self.selection = Some(Selection::Measures {
            anchor: MeasureIndex::new(start),
            focus: MeasureIndex::new(end),
            track: None,
        });
        Ok(outcome)
    }

    /// Moves the `count` bars starting at `from` so they start at `to`, in every track and
    /// staff, with everything each bar holds, as one undo step (dragging a bar selection in
    /// the song grid). The moved bars are selected where they went, in the track the
    /// selection was in (or every track), and the cursor goes to the first of them.
    pub fn move_bars(
        &mut self,
        from: MeasureIndex,
        count: usize,
        to: MeasureIndex,
    ) -> Result<Outcome, EditError> {
        let track = match self.selection {
            Some(Selection::Measures { track, .. }) => track,
            _ => None,
        };
        let mut cursor = Cursor {
            measure: to,
            beat: gscore_model::BeatIndex::new(0),
            ..self.cursor()
        };
        if let Some(track) = track {
            cursor.track = track;
        }
        let outcome = self.execute(Command::MoveMeasures { from, count, to }, cursor, false)?;
        self.selection = Some(Selection::Measures {
            anchor: to,
            focus: MeasureIndex::new(to.get().saturating_add(count.saturating_sub(1))),
            track,
        });
        Ok(outcome)
    }

    /// Inserts the copied bars at `at` (before the bar there; the song's end adds them at
    /// the end), in every track, as one undo step. The pasted bars are selected.
    pub fn paste_measures_inserting(&mut self, at: MeasureIndex) -> Result<Outcome, EditError> {
        self.paste_measures_inserting_with(at, PasteOptions::default())
    }

    /// [`TabEditor::paste_measures_inserting`] with `options`: how many times the bars are
    /// pasted, and which of their attributes come along.
    pub fn paste_measures_inserting_with(
        &mut self,
        at: MeasureIndex,
        options: PasteOptions,
    ) -> Result<Outcome, EditError> {
        self.paste_measures_at(at, true, options)
    }

    /// Puts the copied bars at `at` over the bars already there (the song grows only when
    /// the paste runs past its end), in every track, as one undo step. The pasted bars are
    /// selected.
    pub fn paste_measures_overwriting(&mut self, at: MeasureIndex) -> Result<Outcome, EditError> {
        self.paste_measures_overwriting_with(at, PasteOptions::default())
    }

    /// [`TabEditor::paste_measures_overwriting`] with `options`.
    pub fn paste_measures_overwriting_with(
        &mut self,
        at: MeasureIndex,
        options: PasteOptions,
    ) -> Result<Outcome, EditError> {
        self.paste_measures_at(at, false, options)
    }

    /// [`TabEditor::paste_measures_inserting_with`] or
    /// [`TabEditor::paste_measures_overwriting_with`]: bars are added at `at` (shifting the
    /// rest) when `insert`, else written over the bars there.
    fn paste_measures_at(
        &mut self,
        at: MeasureIndex,
        insert: bool,
        options: PasteOptions,
    ) -> Result<Outcome, EditError> {
        let Clip::Measures { staves, .. } = self.clipboard.clone() else {
            return Err(EditError::NothingToPaste);
        };
        let (staves, track) = self.to_paste_track(staves);
        let count = staves.first().map_or(0, |(_, _, measures)| measures.len());
        if count == 0 {
            return Err(EditError::NothingToPaste);
        }
        let options = options.with_copies(options.copies.clamp(1, MAX_PASTE_COPIES));
        let total = count.saturating_mul(options.copies);
        let at = MeasureIndex::new(at.get().min(self.song().measure_count()));
        let mut commands = Vec::new();
        let mut target = self.song().clone();
        if insert {
            for offset in 0..total {
                let insert = Command::InsertMeasure {
                    measure: MeasureIndex::new(at.get().saturating_add(offset)),
                };
                let _ = insert.apply(&mut target).map_err(EditError::Model)?;
                commands.push(insert);
            }
        } else {
            // Overwriting still adds the bars a paste past the end needs.
            while target.measure_count() < at.get().saturating_add(total) {
                let insert = Command::InsertMeasure {
                    measure: MeasureIndex::new(target.measure_count()),
                };
                let _ = insert.apply(&mut target).map_err(EditError::Model)?;
                commands.push(insert);
            }
        }
        commands.extend(paste_measures(&target, at, &staves, options)?);
        let mut cursor = Cursor {
            measure: at,
            beat: gscore_model::BeatIndex::new(0),
            ..self.cursor()
        };
        if let Some(track) = track {
            cursor.track = track;
        }
        self.selection = Some(Selection::Measures {
            anchor: at,
            focus: MeasureIndex::new(at.get().saturating_add(total.saturating_sub(1))),
            track,
        });
        self.execute(Command::Batch { commands }, cursor, false)
    }

    /// Copies the selection and clears it (notes become rests; measures are removed). With
    /// nothing selected, cuts the note under the cursor.
    pub fn cut_selection(&mut self) -> Result<Outcome, EditError> {
        let Some(selection) = self.selection else {
            // The cursor's beat is copied and becomes a rest, as a cut beat selection does.
            self.copy_selection()?;
            let at = self.cursor.position();
            return self.execute(Command::SetBeatRest { at }, self.cursor, false);
        };
        self.copy_selection()?;
        // The cut bars leave (or lose their notes): a paste over the same selection puts
        // them there, it does not duplicate them after it.
        if let Clip::Measures { source, .. } = &mut self.clipboard {
            *source = None;
        }
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
            Clip::Empty => Err(EditError::NothingToPaste),
            Clip::Beats(groups) => {
                // As Guitar Pro: copied beats replace the selected ones whatever their
                // lengths (as many as fit stay, more are added, fewer leave the rest of the
                // selection out); with only the cursor they go in after its beat.
                let cursor = self.cursor();
                let (voice, start, remove) = match self.selection {
                    Some(Selection::Beats {
                        track,
                        staff,
                        voice,
                        anchor,
                        focus,
                    }) => {
                        let (first, last) = (anchor.min(focus), anchor.max(focus));
                        let at = gscore_model::BeatPosition {
                            track,
                            staff,
                            voice,
                            measure: first.measure,
                            beat: first.beat,
                        };
                        (at, first, Some((first, last)))
                    }
                    _ => {
                        let at = cursor.position();
                        // After the cursor's beat; in a bar without beats, at its start.
                        let beat = if self.song().beat(at).is_some() {
                            at.beat.get().saturating_add(1)
                        } else {
                            0
                        };
                        let start = BeatSpot {
                            measure: at.measure,
                            beat: gscore_model::BeatIndex::new(beat),
                        };
                        (at, start, None)
                    }
                };
                let (commands, first, last) =
                    replace_beats(self.song(), voice, start, remove, &groups)?;
                let after = Cursor {
                    track: voice.track,
                    staff: voice.staff,
                    voice: voice.voice,
                    measure: first.measure,
                    beat: first.beat,
                    ..cursor
                };
                let outcome = self.execute(Command::Batch { commands }, after, false)?;
                self.selection = Some(Selection::Beats {
                    staff: voice.staff,
                    track: voice.track,
                    voice: voice.voice,
                    anchor: first,
                    focus: last,
                });
                Ok(outcome)
            }
            Clip::Measures { staves, source } => {
                let count = staves.first().map_or(0, |(_, _, measures)| measures.len());
                let (staves, track) = self.to_paste_track(staves);
                // Copied bars pasted with a selection of every track still on them go right
                // after it: copying a section and pasting it again duplicates it. A bar
                // selection of one track (made in the score, or in one row of the grid)
                // pastes over its bars, as a moved selection or cursor and cut bars do.
                let duplicate_at = match (self.selection, source) {
                    (
                        Some(Selection::Measures {
                            anchor,
                            focus,
                            track: None,
                        }),
                        Some(source),
                    ) if count > 0
                        && anchor.min(focus) == source
                        && anchor.max(focus).get()
                            == source.get().saturating_add(count.saturating_sub(1)) =>
                    {
                        Some(MeasureIndex::new(source.get().saturating_add(count)))
                    }
                    _ => None,
                };
                let on_source = duplicate_at.is_some();
                let first = duplicate_at.unwrap_or_else(|| {
                    self.selection
                        .map(Selection::first_measure)
                        .unwrap_or_else(|| self.cursor().measure)
                });
                // Bars go in before the paste when it duplicates them after the selection
                // (the song grows), and when a paste at the end would run past the song.
                let insert_at = on_source.then_some(first);
                let needed = first.get().saturating_add(count);
                let mut commands = Vec::new();
                let mut target = self.song().clone();
                let inserts = match insert_at {
                    Some(at) => (0..count)
                        .map(|offset| at.get().saturating_add(offset))
                        .collect::<Vec<_>>(),
                    None => (target.measure_count()..needed).collect(),
                };
                for index in inserts {
                    let measure = MeasureIndex::new(index);
                    let insert = Command::InsertMeasure { measure };
                    let _ = insert.apply(&mut target).map_err(EditError::Model)?;
                    commands.push(insert);
                }
                commands.extend(paste_measures(
                    &target,
                    first,
                    &staves,
                    PasteOptions::default(),
                )?);
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

    /// Copied bars of one track go into the track pasted into (the selection's, else the
    /// cursor's), not back into the track they came from; bars of every track stay in
    /// theirs. Returns the staves to write and that one track, if it is one.
    fn to_paste_track(&self, staves: Vec<StaffClip>) -> (Vec<StaffClip>, Option<TrackIndex>) {
        let mut tracks = staves.iter().map(|(track, _, _)| *track);
        let first = tracks.next();
        let Some(first) = first.filter(|first| tracks.all(|track| track == *first)) else {
            return (staves, None);
        };
        let target = match self.selection {
            Some(Selection::Measures {
                track: Some(track), ..
            })
            | Some(Selection::Beats { track, .. }) => track,
            _ => self.cursor().track,
        };
        if target == first {
            return (staves, Some(first));
        }
        let staves = staves
            .into_iter()
            .map(|(_, staff, measures)| (target, staff, measures))
            .collect();
        (staves, Some(target))
    }

    /// The cursor after cutting `selection`: the start of the removed range.
    fn cursor_after_cut(&self, selection: Selection) -> Cursor {
        let mut cursor = self.cursor();
        match selection {
            Selection::Beats {
                track,
                staff,
                voice,
                anchor,
                focus,
            } => {
                let first = anchor.min(focus);
                cursor.track = track;
                cursor.staff = staff;
                cursor.voice = voice;
                cursor.measure = first.measure;
                cursor.beat = first.beat;
            }
            Selection::Measures {
                anchor,
                focus,
                track,
                ..
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
    fn clipboard_from(&self, selection: Selection) -> Result<Clip, EditError> {
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
                Ok(Clip::Beats(groups))
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
                let mut staves = Vec::new();
                for track_index in indices {
                    let track_at = TrackIndex::new(track_index);
                    let Some(track) = song.track(track_at) else {
                        continue;
                    };
                    // Every staff of the track: a measure of a piano takes its treble and
                    // its bass.
                    for staff in track.all_staves() {
                        let mut measures = Vec::new();
                        for index in first..=last {
                            let measure_index = MeasureIndex::new(index);
                            let Some(measure) = staff.measure(measure_index) else {
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
                            let time_signature =
                                measure.time_signature().map_err(EditError::Model)?;
                            measures.push(MeasureClip {
                                voices,
                                time_signature,
                                tempo: song.tempo_change(measure_index),
                                section: song.section(measure_index),
                            });
                        }
                        staves.push((track_at, staff.index(), measures));
                    }
                }
                Ok(Clip::Measures {
                    staves,
                    source: Some(MeasureIndex::new(first)),
                })
            }
        }
    }

    /// The note under the cursor, for Copy without a selection.
    /// The beat under the cursor, as Guitar Pro copies it without a selection.
    fn beat_at_cursor(&self) -> Result<Clip, EditError> {
        let Some(beat) = self.song().beat(self.cursor.position()) else {
            return Err(EditError::NothingSelected);
        };
        let Some(clip) = beat_clip(&beat)? else {
            return Err(EditError::NothingSelected);
        };
        Ok(Clip::Beats(vec![vec![clip]]))
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
        text: beat.text().to_owned(),
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
