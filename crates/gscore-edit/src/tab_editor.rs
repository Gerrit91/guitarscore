//! [`TabEditor`]: interactive editing at a cursor. It turns [`Action`]s (from keys or menus)
//! into model commands on its [`Editor`], keeps the cursor inside the song, and remembers
//! where each edit happened so undo and redo put the cursor back there.
//!
//! Time comes in as milliseconds from the caller (`now_ms`), never from the system clock,
//! so this stays usable on every target and testable without waiting.

use gscore_model::{
    BeatIndex, BeatKind, Fret, MeasureIndex, ModelError, NoteValue, Song, TrackIndex,
};

use crate::action::{Action, ActionId};
use crate::command::{Command, Recorded};
use crate::cursor::Cursor;
use crate::editor::{EditError, Editor};
use crate::selection::{Clipboard, Selection};

/// How long after a fret digit a second digit still joins it (`1` then `2` is fret 12).
pub const FRET_DIGIT_WINDOW_MS: u64 = 500;
/// Highest fret that can be typed (two digits).
const MAX_TYPED_FRET: u8 = 99;
/// Note value of a new beat when there is no beat to copy it from: a quarter.
const DEFAULT_ENTRY_VALUE: u16 = 4;

/// What an action did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing (for example, moving left at the start of the song).
    Nothing,
    /// The cursor moved; the song is unchanged.
    Moved,
    /// The song changed (and possibly the cursor).
    Edited,
    /// The action is for the application: playback or saving.
    Request(Request),
}

/// Actions the editor passes on to the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Start or pause playback.
    PlayPause,
    /// Stop playback.
    Stop,
    /// Save the file.
    Save,
}

/// A fret digit that a second digit may still extend.
#[derive(Debug, Clone, Copy)]
struct PendingDigit {
    cursor: Cursor,
    value: u8,
    time_ms: u64,
}

/// A song being edited at a cursor, with undo and redo.
#[derive(Debug)]
pub struct TabEditor {
    pub(crate) editor: Editor,
    pub(crate) cursor: Cursor,
    pending: Option<PendingDigit>,
    /// Where the cursor was before each undoable edit, parallel to the editor's history.
    undo_cursors: Vec<Cursor>,
    /// Where the cursor goes when each undone edit is redone.
    redo_cursors: Vec<Cursor>,
    /// Counts changes to the song; a new value means layouts and sequences are stale.
    revision: u64,
    /// The note value new beats get when there is no beat to take it from.
    entry_value: NoteValue,
    /// The command the last edit ran (or undid or redid), for the app to tell a live mixer
    /// change from an edit that invalidates playback.
    last_command: Option<Command>,
    /// The selected beats or measures, if any.
    pub(crate) selection: Option<Selection>,
    /// What was last copied or cut.
    pub(crate) clipboard: Clipboard,
    /// What changed the song since [`TabEditor::take_applied`] was last called, in order:
    /// commands (marked when merged into the undo step before), `undo` and `redo`. The
    /// recovery journal records these; replaying them with [`TabEditor::replay`] gives the
    /// same song and the same undo history.
    applied: Vec<Recorded>,
}

impl TabEditor {
    /// Edits `song` with the cursor at the start of `track`.
    pub fn new(song: Song, track: TrackIndex) -> Self {
        let mut cursor = Cursor::start(track);
        cursor.clamp(&song);
        TabEditor {
            editor: Editor::new(song),
            cursor,
            pending: None,
            undo_cursors: Vec::new(),
            redo_cursors: Vec::new(),
            revision: 0,
            entry_value: default_entry_value(),
            last_command: None,
            selection: None,
            clipboard: Clipboard::default(),
            applied: Vec::new(),
        }
    }

    /// Takes the commands applied since the last call (see [`TabEditor::replay`]).
    pub fn take_applied(&mut self) -> Vec<Recorded> {
        std::mem::take(&mut self.applied)
    }

    /// Applies one recorded command again (from the recovery journal): `undo`, `redo`, or
    /// an edit, with the cursor at the edit, merged into the undo step before it if it was
    /// then. It is recorded again, like any edit.
    pub fn replay(&mut self, entry: impl Into<Recorded>) -> Result<Outcome, EditError> {
        let Recorded { command, merged } = entry.into();
        self.pending = None;
        match command {
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            command => {
                let mut cursor = self.cursor;
                if let Some(at) = command.position() {
                    cursor.track = at.track;
                    cursor.measure = at.measure;
                    cursor.voice = at.voice;
                    cursor.beat = at.beat;
                } else if let Some(measure) = command.measure() {
                    cursor.measure = measure;
                    cursor.beat = BeatIndex::new(0);
                }
                cursor.clamp(self.editor.song());
                self.cursor = cursor;
                self.execute(command, cursor, merged)
            }
        }
    }

    /// The song as edited so far.
    pub fn song(&self) -> &Song {
        self.editor.song()
    }

    /// The cursor.
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    /// The note value a new beat gets when there is none at the cursor (the palette's
    /// chosen length).
    pub fn set_entry_value(&mut self, value: NoteValue) {
        self.entry_value = value;
    }

    /// How many undo steps the history holds.
    pub fn undo_depth(&self) -> usize {
        self.editor.undo_depth()
    }

    /// Moves the cursor (for example to a clicked note); it is kept inside the song.
    pub fn set_cursor(&mut self, cursor: Cursor) {
        self.pending = None;
        self.cursor = cursor;
        self.cursor.clamp(self.editor.song());
    }

    /// Shows another track: the cursor keeps its measure and moves to that track.
    pub fn set_track(&mut self, track: TrackIndex) {
        if track != self.cursor.track {
            let measure = self.cursor.measure;
            self.set_cursor(Cursor {
                measure,
                ..Cursor::start(track)
            });
        }
    }

    /// Counts changes to the song: it differs whenever the song may have changed.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The command the last edit ran (or the command undone or redone), if any.
    pub fn last_command(&self) -> Option<&Command> {
        self.last_command.as_ref()
    }

    /// Whether there is an edit to undo.
    pub fn can_undo(&self) -> bool {
        self.editor.can_undo()
    }

    /// Whether there is an edit to redo.
    pub fn can_redo(&self) -> bool {
        self.editor.can_redo()
    }

    /// The history rows that can be undone, oldest first.
    pub fn undo_history(&self) -> Vec<crate::HistoryEntry> {
        self.editor.undo_history()
    }

    /// The history rows that can be redone, in the order they would be redone.
    pub fn redo_history(&self) -> Vec<crate::HistoryEntry> {
        self.editor.redo_history()
    }

    /// The stable ids of the undo entries, oldest first.
    pub fn undo_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.editor.undo_entries().map(|(id, _)| id)
    }

    /// The stable ids of the redo entries, in the order they would be redone.
    pub fn redo_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.editor.redo_entries().map(|(id, _)| id)
    }

    /// Number of edits that can be redone.
    pub fn redo_depth(&self) -> usize {
        self.editor.redo_depth()
    }

    /// Marks the end of a slider drag: the next command starts a new undo entry.
    pub fn break_merge(&mut self) {
        self.editor.break_merge();
    }

    /// Runs `command` at the cursor (for example from a panel or menu), keeping undo and
    /// the cursor. An error leaves the song unchanged.
    pub fn execute_command(&mut self, command: Command) -> Result<Outcome, EditError> {
        self.execute_command_merged(command, false)
    }

    /// Like [`TabEditor::execute_command`]; with `merge`, a volume or pan change continues
    /// the previous undo entry (callers pass `true` only within one slider drag).
    pub fn execute_command_merged(
        &mut self,
        command: Command,
        merge: bool,
    ) -> Result<Outcome, EditError> {
        self.execute(command, self.cursor, merge)
    }

    /// Runs `action` at the cursor. `now_ms` is any monotonic clock in milliseconds; it
    /// decides whether a fret digit joins the one before. An error leaves the song unchanged.
    pub fn perform(&mut self, action: Action, now_ms: u64) -> Result<Outcome, EditError> {
        // Anything but another digit ends a fret number.
        let pending = self.pending.take();
        self.last_command = None;
        let id = match action {
            Action::Digit(digit) => return self.enter_digit(digit, now_ms, pending),
            Action::Plain(id) => id,
        };
        let song = self.editor.song();
        // A plain cursor move ends the selection.
        let cursor_move = matches!(
            id,
            ActionId::CursorLeft
                | ActionId::CursorRight
                | ActionId::CursorUpString
                | ActionId::CursorDownString
                | ActionId::PreviousMeasure
                | ActionId::NextMeasure
        );
        let deselected = cursor_move && self.selection.take().is_some();
        let moved = |moved: bool| {
            if moved || deselected {
                Outcome::Moved
            } else {
                Outcome::Nothing
            }
        };
        match id {
            ActionId::CursorLeft => Ok(moved(self.cursor.left(song))),
            ActionId::CursorRight => self.cursor_right(),
            ActionId::CursorUpString => Ok(moved(self.cursor.up())),
            ActionId::CursorDownString => Ok(moved(self.cursor.down(song))),
            ActionId::PreviousMeasure => Ok(moved(self.cursor.to_measure(song, false))),
            ActionId::NextMeasure => Ok(moved(self.cursor.to_measure(song, true))),
            ActionId::SelectLeft => Ok(self.extend_selection(false)),
            ActionId::SelectRight => Ok(self.extend_selection(true)),
            ActionId::SelectBeat => Ok(self.select_beat()),
            ActionId::EnterFret => Ok(Outcome::Nothing),
            // With a selection, Delete and clear clear every selected note at once.
            ActionId::ClearNote | ActionId::DeleteBeat if let Some(selection) = self.selection => {
                self.clear_selection(selection)
            }
            ActionId::ClearNote => {
                let string = self.string()?;
                let at = self.cursor.position();
                self.execute(Command::ClearNote { at, string }, self.cursor, false)
            }
            ActionId::DurationShorter => self.change_duration(true),
            ActionId::DurationLonger => self.change_duration(false),
            ActionId::InsertBeat => {
                let duration = self.current_value();
                let at = self.cursor.position();
                self.execute(Command::InsertBeat { at, duration }, self.cursor, false)
            }
            ActionId::DeleteBeat => {
                let at = self.cursor.position();
                self.execute(Command::DeleteBeat { at }, self.cursor, false)
            }
            ActionId::InsertMeasure => {
                let measure = self.cursor.measure;
                let after = Cursor {
                    beat: BeatIndex::new(0),
                    ..self.cursor
                };
                self.execute(Command::InsertMeasure { measure }, after, false)
            }
            ActionId::DeleteMeasure => {
                let measure = self.cursor.measure;
                self.execute(Command::DeleteMeasure { measure }, self.cursor, false)
            }
            ActionId::Copy => self.copy_selection(),
            ActionId::Cut => self.cut_selection(),
            ActionId::Paste => self.paste_clipboard(),
            ActionId::Undo => self.undo(),
            ActionId::Redo => self.redo(),
            ActionId::PlayPause => Ok(Outcome::Request(Request::PlayPause)),
            ActionId::Stop => Ok(Outcome::Request(Request::Stop)),
            ActionId::Save => Ok(Outcome::Request(Request::Save)),
            // View actions: the app handles them before the editor, which has no view.
            ActionId::ZoomIn
            | ActionId::ZoomOut
            | ActionId::ZoomReset
            | ActionId::ToggleFullscreen
            | ActionId::ToggleMultitrack
            | ActionId::ShowHistory
            | ActionId::NewSong
            | ActionId::OpenFile
            | ActionId::SaveAs
            | ActionId::OpenSettings => Ok(Outcome::Nothing),
            // Palette actions: the app runs them like a click on the palette tile.
            ActionId::NoteWhole
            | ActionId::NoteHalf
            | ActionId::NoteQuarter
            | ActionId::NoteEighth
            | ActionId::NoteSixteenth
            | ActionId::NoteThirtySecond
            | ActionId::NoteSixtyFourth
            | ActionId::ToggleDot
            | ActionId::ToggleDoubleDot
            | ActionId::TupletThree
            | ActionId::TupletFive
            | ActionId::TupletSeven
            | ActionId::TupletSix
            | ActionId::TupletOther
            | ActionId::MakeRest
            | ActionId::ToggleTie
            | ActionId::TieBeat
            | ActionId::EditBeatText
            | ActionId::EditLyrics
            | ActionId::EditChordDiagram
            | ActionId::EditLeftFinger
            | ActionId::EditRightFinger
            | ActionId::ToggleDeadNote
            | ActionId::ToggleGhostNote
            | ActionId::ToggleHammer
            | ActionId::ToggleShiftSlide
            | ActionId::ToggleLegatoSlide
            | ActionId::ToggleSlideInBelow
            | ActionId::ToggleSlideInAbove
            | ActionId::ToggleSlideOutDown
            | ActionId::ToggleSlideOutUp
            | ActionId::ToggleBend
            | ActionId::ToggleVibrato
            | ActionId::ToggleWideVibrato
            | ActionId::ToggleAccent
            | ActionId::ToggleHeavyAccent
            | ActionId::ToggleStaccato
            | ActionId::TogglePalmMute
            | ActionId::ToggleLetRing
            | ActionId::ToggleTrill
            | ActionId::ToggleTremoloPicking
            | ActionId::GraceNote
            | ActionId::ToggleNaturalHarmonic
            | ActionId::ToggleArtificialHarmonic
            | ActionId::ToggleTappedHarmonic
            | ActionId::TogglePinchHarmonic
            | ActionId::ToggleSemiHarmonic
            | ActionId::TogglePickDown
            | ActionId::TogglePickUp
            | ActionId::ToggleBrushDown
            | ActionId::ToggleBrushUp
            | ActionId::ToggleTapping
            | ActionId::ToggleSlapping
            | ActionId::TogglePopping
            | ActionId::ToggleFadeIn
            | ActionId::ToggleFadeOut
            | ActionId::ToggleVolumeSwell
            | ActionId::ToggleOttava
            | ActionId::ToggleOttavaBassa
            | ActionId::ToggleQuindicesima
            | ActionId::ToggleQuindicesimaBassa
            | ActionId::ToggleWhammy
            | ActionId::ToggleLegato
            | ActionId::ToggleBeatVibrato
            | ActionId::ToggleArpeggioDown
            | ActionId::ToggleArpeggioUp
            | ActionId::ToggleDeadSlapped
            | ActionId::ToggleRasgueado
            | ActionId::ToggleLeftHandTap
            | ActionId::ToggleStringNumber
            | ActionId::TogglePickSlideDown
            | ActionId::TogglePickSlideUp
            | ActionId::TrillOther
            | ActionId::TremoloOther => Ok(Outcome::Nothing),
        }
    }

    /// Undoes the last edit and puts the cursor where it happened.
    pub fn undo(&mut self) -> Result<Outcome, EditError> {
        self.last_command = self.editor.undo()?;
        self.applied.push(Command::Undo.into());
        self.revision = self.revision.wrapping_add(1);
        if let Some(cursor) = self.undo_cursors.pop() {
            self.redo_cursors.push(cursor);
            self.cursor = cursor;
        }
        self.cursor.clamp(self.editor.song());
        Ok(Outcome::Edited)
    }

    /// Redoes the last undone edit and puts the cursor where it happened.
    pub fn redo(&mut self) -> Result<Outcome, EditError> {
        self.last_command = self.editor.redo()?;
        self.applied.push(Command::Redo.into());
        self.revision = self.revision.wrapping_add(1);
        if let Some(cursor) = self.redo_cursors.pop() {
            self.undo_cursors.push(cursor);
            self.cursor = cursor;
        }
        self.drop_trimmed_cursors();
        self.cursor.clamp(self.editor.song());
        Ok(Outcome::Edited)
    }

    /// The editor drops its oldest entries beyond its limits; drop their cursors too.
    fn drop_trimmed_cursors(&mut self) {
        let excess = self
            .undo_cursors
            .len()
            .saturating_sub(self.editor.undo_depth());
        self.undo_cursors.drain(..excess);
    }

    /// Applies `command`; afterwards the cursor goes to `after` (kept inside the song).
    pub(crate) fn execute(
        &mut self,
        command: Command,
        after: Cursor,
        merge: bool,
    ) -> Result<Outcome, EditError> {
        let before = self.cursor;
        let merged = self.editor.execute_with_merge(command.clone(), merge)?;
        self.applied.push(Recorded {
            command: command.clone(),
            merged,
        });
        self.last_command = Some(command);
        self.revision = self.revision.wrapping_add(1);
        // A coalesced mix change (one slider drag) keeps the undo entry it continues, and
        // with it the cursor from before the drag started.
        if !merged {
            self.undo_cursors.push(before);
        }
        self.redo_cursors.clear();
        self.drop_trimmed_cursors();
        self.cursor = after;
        self.cursor.clamp(self.editor.song());
        Ok(Outcome::Edited)
    }

    fn string(&self) -> Result<gscore_model::StringNumber, EditError> {
        Ok(gscore_model::StringNumber::new(self.cursor.string)?)
    }

    /// Types a fret digit. A digit right after another one at the same place makes a
    /// two-digit fret, replacing the first digit as one undo step.
    fn enter_digit(
        &mut self,
        digit: u8,
        now_ms: u64,
        pending: Option<PendingDigit>,
    ) -> Result<Outcome, EditError> {
        let string = self.string()?;
        let at = self.cursor.position();
        if let Some(previous) = pending
            && previous.cursor == self.cursor
            && now_ms.saturating_sub(previous.time_ms) <= FRET_DIGIT_WINDOW_MS
            && let Some(value) = previous
                .value
                .checked_mul(10)
                .and_then(|v| v.checked_add(digit))
                .filter(|v| *v <= MAX_TYPED_FRET)
        {
            let fret = Fret::new(value)?;
            self.undo()?;
            return self.execute(Command::SetFret { at, string, fret }, self.cursor, false);
        }
        let fret = Fret::new(digit)?;
        if self.editor.song().beat(at).is_none() && self.cursor.beat.get() == 0 {
            // A measure without beats (an empty GP7 bar): the note's beat comes first.
            let duration = self.entry_value;
            self.execute(Command::InsertBeat { at, duration }, self.cursor, false)?;
        }
        let outcome = self.execute(Command::SetFret { at, string, fret }, self.cursor, false)?;
        self.pending = Some(PendingDigit {
            cursor: self.cursor,
            value: digit,
            time_ms: now_ms,
        });
        Ok(outcome)
    }

    /// Next beat. At the end of a measure that is not full yet, a rest beat is added (as
    /// long as the current one, or what still fits); at the end of the last full measure,
    /// a measure is added.
    fn cursor_right(&mut self) -> Result<Outcome, EditError> {
        let song = self.editor.song();
        if !self.cursor.at_measure_end(song) {
            self.cursor.right(song);
            return Ok(Outcome::Moved);
        }
        let beats = self.cursor.beats_in(song, self.cursor.measure);
        if beats > 0
            && let Some(remaining) = self.remaining_in_measure()?
        {
            let current = self.current_value();
            let duration = if current.duration()? <= remaining {
                current
            } else {
                NoteValue::filling(remaining)?
                    .first()
                    .copied()
                    .unwrap_or(current)
            };
            let at = gscore_model::BeatPosition {
                beat: BeatIndex::new(beats),
                ..self.cursor.position()
            };
            let after = Cursor {
                beat: BeatIndex::new(beats),
                ..self.cursor
            };
            return self.execute(Command::InsertBeat { at, duration }, after, false);
        }
        if self.cursor.in_last_measure(song) {
            let measure = MeasureIndex::new(song.measure_count());
            let after = Cursor {
                measure,
                beat: BeatIndex::new(0),
                ..self.cursor
            };
            return self.execute(Command::InsertMeasure { measure }, after, false);
        }
        self.cursor.right(song);
        Ok(Outcome::Moved)
    }

    /// What is missing from the cursor's voice to fill its measure, if anything.
    fn remaining_in_measure(&self) -> Result<Option<gscore_model::Duration>, ModelError> {
        let song = self.editor.song();
        let Some(measure) = song
            .track(self.cursor.track)
            .and_then(|t| t.measure(self.cursor.measure))
        else {
            return Ok(None);
        };
        let Some(voice) = measure.voice(self.cursor.voice) else {
            return Ok(None);
        };
        let length = measure.time_signature()?.measure_length();
        let filled = voice.total_duration()?;
        if filled >= length {
            return Ok(None);
        }
        Ok(Some(length.checked_sub(filled)?))
    }

    /// The note value of the beat at the cursor, or the entry value.
    fn current_value(&self) -> NoteValue {
        self.editor
            .song()
            .beat(self.cursor.position())
            .filter(|beat| beat.kind() != BeatKind::Empty && !beat.is_grace())
            .and_then(|beat| beat.note_value().ok())
            .unwrap_or(self.entry_value)
    }

    /// Halves (`shorter`) or doubles the beat's note value, keeping dots and tuplet.
    fn change_duration(&mut self, shorter: bool) -> Result<Outcome, EditError> {
        let at = self.cursor.position();
        let Some(beat) = self.editor.song().beat(at) else {
            return Ok(Outcome::Nothing);
        };
        if beat.kind() == BeatKind::Empty || beat.is_grace() {
            return Ok(Outcome::Nothing);
        }
        let current = beat.note_value()?;
        let value = if shorter {
            current.value().checked_mul(2)
        } else {
            (current.value() > 1).then(|| current.value() / 2)
        };
        let Some(duration) =
            value.and_then(|v| NoteValue::new(v, current.dots(), current.tuplet()).ok())
        else {
            return Ok(Outcome::Nothing);
        };
        let outcome = self.execute(Command::SetDuration { at, duration }, self.cursor, false)?;
        self.entry_value = duration;
        Ok(outcome)
    }
}

/// A quarter note.
#[allow(clippy::expect_used)]
fn default_entry_value() -> NoteValue {
    NoteValue::new(DEFAULT_ENTRY_VALUE, 0, None).expect("a plain quarter note is valid")
}
