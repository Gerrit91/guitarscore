//! Step entry (M23 step 1): a mode of the [`TabEditor`] in which entering a note, a chord
//! or a rest writes the cursor's beat at the entry length and moves the cursor on, adding
//! beats and bars as `cursor_right` does. Each step is one undo step, the advance included.
//!
//! The inputs: typed frets on the cursor's string, notes by name (the letters, with the
//! key signature's accidentals) and pitches (the keyboard tool, a MIDI keyboard) placed by
//! the [fingering](crate::fingering), places given directly (the fretboard tool, a MIDI
//! guitar), and rests.
//!
//! A typed fret keeps its beat open for [`FRET_DIGIT_WINDOW_MS`], so a second digit can
//! still make it a two-digit fret; the beat advances when the window ends
//! ([`TabEditor::tick`]) or with the next note. A fret typed with the chord modifier
//! (`enter_chord_fret`) holds the beat open until a plain note or a move ends it, and moving
//! between strings keeps it open meanwhile: that is how chords are typed. A chord by name or
//! pitch adds its notes to the beat entered last.
//!
//! Rewriting the open beat (a second digit, another chord note, a correction) undoes the
//! step and writes it again, so it stays one undo step.

use gscore_model::{Fret, StringNumber};

use crate::action::{Action, ActionId};
use crate::cursor::Cursor;
use crate::editor::EditError;
use crate::fingering::{FingeringError, Neck, Place, hand_before};
use crate::tab_editor::{FRET_DIGIT_WINDOW_MS, MAX_TYPED_FRET, Outcome, TabEditor};

mod names;
mod write;

use write::with_place;

/// What one step enters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepInput {
    /// Pitches as they sound (MIDI note numbers), placed by the fingering. On a drum
    /// track, each is the drum itself.
    Pitches(Vec<u8>),
    /// Notes at given places (the fretboard tool, a MIDI guitar).
    Places(Vec<Place>),
    /// A rest.
    Rest,
}

/// A change to the note step entry entered last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Correction {
    /// This many semitones higher (lower when negative).
    Semitones(i8),
    /// On the next string with the same pitch, towards the lower strings when `true`.
    String(bool),
}

/// The beat step entry entered last.
#[derive(Debug, Clone)]
pub(crate) struct Step {
    /// The cursor it was entered at.
    at: Cursor,
    /// Its notes, in the order entered (none: a rest).
    notes: Vec<Place>,
    /// Where the cursor goes after it.
    next: Cursor,
    /// The editor's revision right after it was written: any other edit since makes it
    /// final.
    revision: u64,
    /// Whether the cursor follows the entered notes to their string (notes by name or
    /// pitch); a typed fret leaves it on the string it was typed on.
    follow: bool,
    /// While the cursor still waits on the beat.
    open: Option<Open>,
}

/// A beat that has not advanced yet.
#[derive(Debug, Clone, Copy)]
struct Open {
    /// When it advances on its own; `None` while it is held open for a chord.
    deadline_ms: Option<u64>,
    /// The fret typed last, which a second digit may still extend.
    digit: Option<TypedDigit>,
}

#[derive(Debug, Clone, Copy)]
struct TypedDigit {
    string: StringNumber,
    value: u8,
    time_ms: u64,
}

/// What [`TabEditor::perform_step`] did with an action.
pub(crate) enum Handled {
    /// Step entry handled it.
    Done(Outcome),
    /// The action runs as usual; `moved` when an open beat advanced first.
    Continue { moved: bool },
}

impl TabEditor {
    /// Whether step entry is on.
    pub fn step_entry(&self) -> bool {
        self.step_entry
    }

    /// Switches step entry on or off. Switching it off advances a beat still waiting.
    pub fn set_step_entry(&mut self, on: bool) -> Outcome {
        if on == self.step_entry {
            return Outcome::Nothing;
        }
        let moved = self.close_step();
        self.step = None;
        self.step_entry = on;
        // The mode shows, so the view changes even when the cursor does not move.
        let _ = moved;
        Outcome::Moved
    }

    /// The beat step entry entered last, while a chord note or a correction can still
    /// change it (the app plays it).
    pub fn stepped_beat(&self) -> Option<gscore_model::BeatPosition> {
        self.live_step()
            .filter(|step| !step.notes.is_empty())
            .map(|step| step.at.position())
    }

    /// When the beat step entry waits on advances by itself (milliseconds on the clock
    /// [`TabEditor::perform`] gets), if one does: the app calls [`TabEditor::tick`] then.
    pub fn step_deadline_ms(&self) -> Option<u64> {
        self.step.as_ref()?.open?.deadline_ms
    }

    /// Advances the beat step entry waits on once its fret's digit window has ended.
    pub fn tick(&mut self, now_ms: u64) -> Outcome {
        if self.close_due(now_ms) {
            Outcome::Moved
        } else {
            Outcome::Nothing
        }
    }

    /// Enters one step at the cursor: a note, a chord or a rest at the entry length, and
    /// moves the cursor on. With `chord`, the notes join the beat entered last instead (a
    /// new beat when there is none). Works whether step entry is on or not, so a MIDI
    /// keyboard or a note tool can enter steps directly. An error leaves the song as it was.
    pub fn enter_step(
        &mut self,
        input: StepInput,
        chord: bool,
        now_ms: u64,
    ) -> Result<Outcome, EditError> {
        self.pending = None;
        self.last_command = None;
        self.close_due(now_ms);
        match input {
            StepInput::Rest => {
                self.close_step();
                self.write_new(Vec::new(), true, None)
            }
            StepInput::Places(places) => self.enter_places(places, chord),
            StepInput::Pitches(pitches) => {
                let places = self.place_pitches(&pitches, chord)?;
                self.enter_places(places, chord)
            }
        }
    }

    /// Step entry's part of [`TabEditor::perform`].
    pub(crate) fn perform_step(
        &mut self,
        action: Action,
        now_ms: u64,
    ) -> Result<Handled, EditError> {
        let mut moved = self.close_due(now_ms);
        let outcome = match action {
            Action::Digit(digit) => self.step_digit(digit, now_ms, false)?,
            Action::ChordDigit(digit) => self.step_digit(digit, now_ms, true)?,
            Action::Note { letter, chord } => self.step_letter(letter, chord)?,
            Action::Plain(id) => match id {
                ActionId::StepRest => self.enter_step(StepInput::Rest, false, now_ms)?,
                ActionId::StepSemitoneUp => self.correct(Correction::Semitones(1))?,
                ActionId::StepSemitoneDown => self.correct(Correction::Semitones(-1))?,
                ActionId::StepOctaveUp => self.correct(Correction::Semitones(12))?,
                ActionId::StepOctaveDown => self.correct(Correction::Semitones(-12))?,
                ActionId::StepStringUp => self.correct(Correction::String(false))?,
                ActionId::StepStringDown => self.correct(Correction::String(true))?,
                ActionId::LeaveStepEntry => self.set_step_entry(false),
                // Moving between strings keeps a held beat open for its next chord note.
                ActionId::CursorUpString | ActionId::CursorDownString if self.held() => {
                    return Ok(Handled::Continue { moved });
                }
                id => {
                    moved |= self.close_step();
                    let cursor_move = matches!(
                        id,
                        ActionId::CursorLeft
                            | ActionId::CursorRight
                            | ActionId::CursorUpString
                            | ActionId::CursorDownString
                            | ActionId::PreviousMeasure
                            | ActionId::NextMeasure
                            | ActionId::CursorHome
                            | ActionId::CursorEnd
                            | ActionId::CursorMeasureStart
                            | ActionId::CursorMeasureEnd
                    );
                    // A move elsewhere ends what a chord note or a correction could change.
                    if cursor_move {
                        self.step = None;
                    }
                    return Ok(Handled::Continue { moved });
                }
            },
        };
        Ok(Handled::Done(outcome))
    }

    /// The step entered last, if nothing else changed the song since.
    fn live_step(&self) -> Option<&Step> {
        self.step
            .as_ref()
            .filter(|step| step.revision == self.revision)
    }

    /// Whether the cursor waits on a beat held open for a chord.
    fn held(&self) -> bool {
        self.live_step()
            .and_then(|step| step.open)
            .is_some_and(|open| open.deadline_ms.is_none())
    }

    /// Advances the beat the cursor waits on, if it does; returns whether the cursor
    /// moved. The step stays in reach for chord notes and corrections.
    pub(crate) fn close_step(&mut self) -> bool {
        let live = self.live_step().is_some();
        let Some(step) = self.step.as_mut() else {
            return false;
        };
        let moved = if step.open.take().is_some() {
            // A typed fret's cursor stays on the string it is on now.
            let string = if step.follow {
                step.next.string
            } else {
                self.cursor.string
            };
            self.cursor = Cursor {
                string,
                ..step.next
            };
            self.cursor.clamp(self.editor.song());
            true
        } else {
            false
        };
        if !live {
            self.step = None;
        }
        moved
    }

    /// Advances the waiting beat if its time has come.
    fn close_due(&mut self, now_ms: u64) -> bool {
        let due = self
            .step
            .as_ref()
            .and_then(|step| step.open)
            .and_then(|open| open.deadline_ms)
            .is_some_and(|deadline| now_ms >= deadline);
        due && self.close_step()
    }

    /// A fret digit typed in step entry; `hold` for the chord modifier.
    fn step_digit(&mut self, digit: u8, now_ms: u64, hold: bool) -> Result<Outcome, EditError> {
        let string = self.string()?;
        let deadline = (!hold).then(|| now_ms.saturating_add(FRET_DIGIT_WINDOW_MS));
        let typed = |value| TypedDigit {
            string,
            value,
            time_ms: now_ms,
        };
        if let Some(step) = self.live_step().cloned()
            && let Some(open) = step.open
        {
            let max = self.max_typed_fret();
            // A second digit soon after the first on the same string: a two-digit fret,
            // if the neck has it (else the digit is the next note).
            if let Some(first) = open.digit
                && first.string == string
                && now_ms.saturating_sub(first.time_ms) <= FRET_DIGIT_WINDOW_MS
                && let Some(value) = first
                    .value
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(digit))
                    .filter(|v| *v <= max)
            {
                let notes = with_place(&step.notes, string, Fret::new(value)?);
                let open = Open {
                    deadline_ms: deadline,
                    digit: Some(typed(value)),
                };
                return self.rewrite(&step, notes, Some(open));
            }
            // The beat is held open (or this digit holds it): the note joins it.
            if open.deadline_ms.is_none() || hold {
                let notes = with_place(&step.notes, string, Fret::new(digit)?);
                let open = Open {
                    deadline_ms: deadline,
                    digit: Some(typed(digit)),
                };
                return self.rewrite(&step, notes, Some(open));
            }
        }
        self.close_step();
        let place = Place {
            string,
            fret: Fret::new(digit)?,
        };
        let open = Open {
            deadline_ms: deadline,
            digit: Some(typed(digit)),
        };
        self.write_new(vec![place], false, Some(open))
    }

    /// When the fret typed last is complete, for playing it once instead of every digit:
    /// `Some(deadline)` while a second digit may still make it a two-digit fret (only a
    /// first digit whose tens the neck has: 1 and 2 on a 24-fret neck), `Some(u64::MAX)`
    /// while step entry holds its beat open for more notes of a chord, and `None` once it
    /// is final (or nothing was typed).
    pub fn fret_complete_at(&self, now_ms: u64) -> Option<u64> {
        let max = self.max_typed_fret();
        let may_grow = |value: u8, time_ms: u64| {
            let deadline = time_ms.saturating_add(FRET_DIGIT_WINDOW_MS);
            (value > 0 && value.saturating_mul(10) <= max && now_ms < deadline).then_some(deadline)
        };
        if self.step_entry {
            let open = self.live_step()?.open?;
            if open.deadline_ms.is_none() {
                return Some(u64::MAX);
            }
            let digit = open.digit?;
            return may_grow(digit.value, digit.time_ms);
        }
        let pending = self.pending?;
        (pending.cursor == self.cursor)
            .then(|| may_grow(pending.value, pending.time_ms))
            .flatten()
    }

    /// The highest fret two typed digits make on the cursor's staff: its fret count, and
    /// 99 on a drum staff (the number is the sound).
    pub(crate) fn max_typed_fret(&self) -> u8 {
        // On a drum staff the "fret" is the drum sound's number, not a place on a neck.
        self.editor
            .song()
            .staff(self.cursor.track, self.cursor.staff)
            .filter(|staff| !staff.is_percussion())
            .map_or(MAX_TYPED_FRET, |staff| staff.fret_count())
            .min(MAX_TYPED_FRET)
    }

    /// Where `pitches` go: on a drum track on free strings of the kit (the pitch is the
    /// drum), else where the fingering puts them.
    fn place_pitches(&self, pitches: &[u8], chord: bool) -> Result<Vec<Place>, EditError> {
        let song = self.editor.song();
        let beside: Vec<Place> = if chord {
            self.live_step()
                .map(|step| step.notes.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let staff = song
            .staff(self.cursor.track, self.cursor.staff)
            .ok_or(FingeringError::NoStrings)?;
        if staff.is_percussion() {
            let count = u8::try_from(staff.string_count()).unwrap_or(u8::MAX);
            let mut free = (1..=count)
                .filter_map(|n| StringNumber::new(n).ok())
                .filter(|string| !beside.iter().any(|p| p.string == *string));
            return pitches
                .iter()
                .map(|&pitch| {
                    let string = free.next().ok_or_else(|| {
                        EditError::Unsupported("the drum kit has no free string left".to_owned())
                    })?;
                    Ok(Place {
                        string,
                        fret: Fret::new(pitch)?,
                    })
                })
                .collect();
        }
        let neck = Neck::of_staff(song, &self.cursor)?;
        let hand = hand_before(song, &self.cursor);
        if !beside.is_empty() {
            let mut placed = beside.clone();
            let mut added = Vec::with_capacity(pitches.len());
            for &pitch in pitches {
                let place = neck.place_note(pitch, hand, &placed)?;
                placed.push(place);
                added.push(place);
            }
            return Ok(added);
        }
        let mut distinct = pitches.to_vec();
        distinct.sort_unstable();
        distinct.dedup();
        Ok(match distinct.as_slice() {
            [] => Vec::new(),
            [pitch] => vec![neck.place_note(*pitch, hand, &[])?],
            chord => neck.place_chord(chord, hand)?,
        })
    }

    /// Enters notes at places: a new beat, or with `chord` added to the beat entered last.
    fn enter_places(&mut self, places: Vec<Place>, chord: bool) -> Result<Outcome, EditError> {
        if places.is_empty() {
            return Ok(Outcome::Nothing);
        }
        if chord && let Some(step) = self.live_step().cloned() {
            let notes = places.iter().fold(step.notes.clone(), |notes, p| {
                with_place(&notes, p.string, p.fret)
            });
            return self.rewrite(&step, notes, step.open);
        }
        self.close_step();
        // One note per string: a later one replaces an earlier one.
        let notes = places
            .iter()
            .fold(Vec::new(), |notes, p| with_place(&notes, p.string, p.fret));
        self.write_new(notes, true, None)
    }

    /// Changes the note entered last: another pitch (placed near where it was) or another
    /// string with the same pitch.
    fn correct(&mut self, correction: Correction) -> Result<Outcome, EditError> {
        let Some(step) = self.live_step().cloned() else {
            return Ok(Outcome::Nothing);
        };
        let Some((&last, beside)) = step.notes.split_last() else {
            return Ok(Outcome::Nothing);
        };
        let song = self.editor.song();
        if song
            .staff(step.at.track, step.at.staff)
            .is_none_or(|staff| staff.is_percussion())
        {
            return Ok(Outcome::Nothing);
        }
        let neck = Neck::of_staff(song, &step.at)?;
        let moved = match correction {
            Correction::Semitones(semitones) => {
                let pitch = neck
                    .pitch(last)
                    .and_then(|p| p.checked_add_signed(semitones))
                    .ok_or(FingeringError::NoStrings)?;
                // Near where the note was, so a semitone stays on its string.
                let hand = (last.fret.get() > 0)
                    .then(|| f32::from(last.fret.get()))
                    .or_else(|| hand_before(song, &step.at));
                neck.place_note(pitch, hand, beside)?
            }
            Correction::String(down) => match neck.other_string(last, down, beside) {
                Some(place) => place,
                None => return Ok(Outcome::Nothing),
            },
        };
        let mut notes = beside.to_vec();
        notes.push(moved);
        self.rewrite(&step, notes, step.open)
    }
}
