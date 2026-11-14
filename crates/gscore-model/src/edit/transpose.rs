//! Transposing. Two things can move: what the song *sounds* like (Guitar Pro's per-track
//! transpose, which leaves every written note and the tablature as they are), and the notes
//! themselves, moved on the fretboard so the tablature shows the new pitches ("change the
//! fingerings").
//!
//! The transposition is stored in the file's `<Transpose>` block, so Guitar Pro plays a
//! transposed song the same way. The on/off switch the toolbar offers is a listening aid:
//! it is not saved and leaves the written transposition alone.

use gscore_gp::{Measure as GpMeasure, Note as GpNote, Song as GpSong};

use super::{Change, ChangeKind};
use crate::error::ModelError;
use crate::instrument::InstrumentKind;
use crate::position::{BeatIndex, BeatPosition, MeasureIndex, StaffIndex, TrackIndex, VoiceIndex};
use crate::song::Song;
use crate::views::TrackView;

/// How far GuitarScore transposes a track: one octave either way. (A file may store more;
/// playback follows what it stores.)
pub const MAX_TRANSPOSE: i32 = 12;

/// Highest fret a moved note may land on (the model's own limit).
const MAX_FRET: i32 = 127;

impl Song {
    /// What the song's tracks are transposed by (see [`TrackView::transposition`]), when
    /// they all agree; `None` when they differ or the song has no pitched track.
    pub fn transposition(&self) -> Option<i32> {
        let mut common: Option<i32> = None;
        for track in self.tracks().filter(|track| !track.is_percussion()) {
            let value = track.transposition();
            match common {
                None => common = Some(value),
                Some(previous) if previous != value => return None,
                Some(_) => {}
            }
        }
        common
    }

    /// Whether the transposition is applied when the song plays. It is switched on with
    /// [`Self::set_transpose_active`]; it is a listening aid (A/B) and not part of the file.
    pub fn transpose_active(&self) -> bool {
        self.transpose_active
    }

    /// Switches the transposition on or off without changing it: off, the song sounds as
    /// written; on again, it sounds transposed. Not part of the song file.
    pub fn set_transpose_active(&mut self, active: bool) -> Result<Change, ModelError> {
        if active == self.transpose_active {
            return Err(ModelError::Invalid(format!(
                "the transposition is already {}",
                if active { "on" } else { "off" }
            )));
        }
        let previous = std::mem::replace(&mut self.transpose_active, active);
        Ok(Change {
            kind: ChangeKind::TransposeActive { previous },
        })
    }

    /// Whether [`Self::transpose_by`] would change anything: some track that is not a drum
    /// kit still has room left in the direction of `semitones`.
    pub fn can_transpose(&self, semitones: i32) -> bool {
        self.tracks().any(|track| transposable(&track, semitones))
    }

    /// Transposes what every track sounds like by `semitones` (1 to [`MAX_TRANSPOSE`] up or
    /// down), keeping each track within an octave of its written pitch; when that leaves no
    /// room, the edit fails and changes nothing. Drum kits have no pitch and stay as they
    /// are. Its [`Change`] puts every track's transposition back on undo.
    pub fn transpose_by(&mut self, semitones: i32) -> Result<Change, ModelError> {
        check_step(semitones)?;
        if !self.can_transpose(semitones) {
            return Err(no_room(semitones));
        }
        let previous_active = self.transpose_active;
        self.transpose_active = true;
        self.edit_track_settings(|inner| {
            let mut previous = Vec::new();
            for (index, track) in inner.tracks.iter_mut().enumerate() {
                let written = stored(track);
                let total = clamp_step(written, semitones);
                if total == written || track.percussion_track {
                    continue;
                }
                previous.push((TrackIndex::new(index), written));
                track.transpose_sound = total;
            }
            Ok(Change {
                kind: ChangeKind::SongTranspose {
                    previous,
                    previous_active,
                },
            })
        })
    }

    /// Whether [`Self::transpose_track_by`] would change anything.
    pub fn can_transpose_track(&self, at: TrackIndex, semitones: i32) -> bool {
        self.track(at)
            .is_some_and(|track| transposable(&track, semitones))
    }

    /// Transposes what one track sounds like, like [`Self::transpose_by`].
    pub fn transpose_track_by(
        &mut self,
        at: TrackIndex,
        semitones: i32,
    ) -> Result<Change, ModelError> {
        check_step(semitones)?;
        if !self.can_transpose_track(at, semitones) {
            return Err(no_room(semitones));
        }
        let previous_active = self.transpose_active;
        self.transpose_active = true;
        self.edit_track_settings(|inner| {
            let track = inner
                .tracks
                .get_mut(at.get())
                .ok_or_else(|| ModelError::Invalid(format!("{at}: the song has no such track")))?;
            let before = stored(track);
            let total = clamp_step(before, semitones);
            track.transpose_sound = total;
            Ok(Change {
                kind: ChangeKind::SongTranspose {
                    previous: vec![(at, before)],
                    previous_active,
                },
            })
        })
    }

    /// Sets what `at` sounds like relative to what is written (for loading a song whose
    /// app keeps the setting); a value outside [`MAX_TRANSPOSE`] or a drum kit is refused.
    pub fn transpose_track_to(
        &mut self,
        at: TrackIndex,
        semitones: i32,
    ) -> Result<Change, ModelError> {
        if semitones == 0 {
            return Err(ModelError::Invalid("no transposition".to_owned()));
        }
        if !(-MAX_TRANSPOSE..=MAX_TRANSPOSE).contains(&semitones) {
            return Err(ModelError::Invalid(format!(
                "transposing by {semitones} semitones (1 to {MAX_TRANSPOSE} up or down)"
            )));
        }
        let previous_active = self.transpose_active;
        self.edit_track_settings(|inner| {
            let track = inner
                .tracks
                .get_mut(at.get())
                .ok_or_else(|| ModelError::Invalid(format!("{at}: the song has no such track")))?;
            if track.percussion_track {
                return Err(ModelError::Invalid(format!("{at}: a drum kit")));
            }
            let before = stored(track);
            track.transpose_sound = semitones;
            Ok(Change {
                kind: ChangeKind::SongTranspose {
                    previous: vec![(at, before)],
                    previous_active,
                },
            })
        })
    }

    /// Whether [`Self::transpose_to`] would change anything.
    pub fn can_transpose_to(&self, semitones: i32) -> bool {
        (-MAX_TRANSPOSE..=MAX_TRANSPOSE).contains(&semitones)
            && self
                .tracks()
                .any(|track| !track.is_percussion() && track.transposition() != semitones)
    }

    /// Sets what every track sounds like relative to what is written (the number typed in
    /// the toolbar): a whole-song transposition like [`Self::transpose_by`], but absolute.
    pub fn transpose_to(&mut self, semitones: i32) -> Result<Change, ModelError> {
        if !(-MAX_TRANSPOSE..=MAX_TRANSPOSE).contains(&semitones) {
            return Err(ModelError::Invalid(format!(
                "transposing by {semitones} semitones (1 to {MAX_TRANSPOSE} up or down)"
            )));
        }
        if !self.can_transpose_to(semitones) {
            return Err(ModelError::Invalid(
                "the song is already transposed that far".to_owned(),
            ));
        }
        let previous_active = self.transpose_active;
        self.transpose_active = semitones != 0;
        self.edit_track_settings(|inner| {
            let mut previous = Vec::new();
            for (index, track) in inner.tracks.iter_mut().enumerate() {
                if track.percussion_track {
                    continue;
                }
                let before = stored(track);
                if before == semitones {
                    continue;
                }
                previous.push((TrackIndex::new(index), before));
                track.transpose_sound = semitones;
            }
            Ok(Change {
                kind: ChangeKind::SongTranspose {
                    previous,
                    previous_active,
                },
            })
        })
    }

    /// Whether [`Self::transpose_notes`] would move any note.
    pub fn can_transpose_notes(&self, scope: Option<TrackIndex>, semitones: i32) -> bool {
        if semitones == 0 || !(-MAX_TRANSPOSE..=MAX_TRANSPOSE).contains(&semitones) {
            return false;
        }
        match scope {
            Some(at) => self
                .track(at)
                .is_some_and(|track| track.has_movable_notes()),
            None => self.tracks().any(|track| track.has_movable_notes()),
        }
    }

    /// Moves the notes of `scope` (the whole song, or one track) `semitones` higher or
    /// lower and re-fingers them onto the track's strings, so the tablature shows the new
    /// pitches. Drum kits stay as they are; a note that would leave the strings (below the
    /// lowest open string, or above the highest fret) makes the edit fail unchanged.
    pub fn transpose_notes(
        &mut self,
        scope: Option<TrackIndex>,
        semitones: i32,
    ) -> Result<Change, ModelError> {
        check_step(semitones)?;
        if let Some(at) = scope
            && at.get() >= self.track_count()
        {
            return Err(ModelError::Invalid(format!(
                "{at}: the song has no such track"
            )));
        }
        if !self.can_transpose_notes(scope, semitones) {
            return Err(ModelError::Invalid("there are no notes to move".to_owned()));
        }
        // The notes are moved on a copy of the song data, so a note that cannot be
        // re-fingered leaves the song untouched; the copy is validated as it goes in.
        let mut candidate = self.inner().clone();
        let previous = move_notes(&mut candidate, scope, semitones)?;
        let sound_state = self.sound_state().map(str::to_owned);
        let transpose_active = self.transpose_active;
        let mut wrapped = Song::from_inner(candidate)?;
        wrapped.replace_sound_state(sound_state);
        wrapped.transpose_active = transpose_active;
        *self = wrapped;
        Ok(Change {
            kind: ChangeKind::NotesTransposed { previous },
        })
    }
}

impl TrackView<'_> {
    /// Whether this track has notes that [`Song::transpose_notes`] would move: it is not a
    /// drum kit and some note sits on one of its strings.
    pub(crate) fn has_movable_notes(&self) -> bool {
        if self.is_percussion() {
            return false;
        }
        let strings = self.string_count();
        self.measures().any(|measure| {
            measure.voices().any(|voice| {
                voice.beats().any(|beat| {
                    beat.notes().any(|note| {
                        let on_string = note
                            .string()
                            .is_ok_and(|string| usize::from(string.get()) <= strings);
                        on_string && note.kind() != crate::views::NoteKind::Dead
                    })
                })
            })
        })
    }
}

/// A step that can be excluded or included in a transposition: 1 to 12 semitones either way.
fn check_step(semitones: i32) -> Result<(), ModelError> {
    if semitones == 0 || !(-MAX_TRANSPOSE..=MAX_TRANSPOSE).contains(&semitones) {
        return Err(ModelError::Invalid(format!(
            "transposing by {semitones} semitones (1 to {MAX_TRANSPOSE} up or down)"
        )));
    }
    Ok(())
}

/// Why nothing could be transposed any further in the direction of `semitones`.
fn no_room(semitones: i32) -> ModelError {
    let direction = if semitones > 0 { "up" } else { "down" };
    ModelError::Invalid(format!(
        "the song is already transposed {direction} as far as it goes ({MAX_TRANSPOSE} \
         semitones)"
    ))
}

/// Whether `track` can be transposed by `semitones` (a drum kit never can).
fn transposable(track: &TrackView<'_>, semitones: i32) -> bool {
    if track.is_percussion() || semitones == 0 {
        return false;
    }
    clamp_step(track.transposition(), semitones) != track.transposition()
}

/// `written` moved by `semitones`, kept within [`MAX_TRANSPOSE`].
fn clamp_step(written: i32, semitones: i32) -> i32 {
    written
        .saturating_add(semitones)
        .clamp(-MAX_TRANSPOSE, MAX_TRANSPOSE)
}

/// The transposition a track stores, kept within [`MAX_TRANSPOSE`].
fn stored(track: &gscore_gp::Track) -> i32 {
    track.transpose_sound.clamp(-MAX_TRANSPOSE, MAX_TRANSPOSE)
}

/// The notes of one beat before they were moved.
type PreviousBeat = (BeatPosition, Vec<GpNote>);

/// Moves every note that `scope` selects on a copy of the song data, returning each changed
/// beat's notes as they were (for undo).
fn move_notes(
    song: &mut GpSong,
    scope: Option<TrackIndex>,
    semitones: i32,
) -> Result<Vec<PreviousBeat>, ModelError> {
    let mut previous = Vec::new();
    for (index, track) in song.tracks.iter_mut().enumerate() {
        let track_at = TrackIndex::new(index);
        if scope.is_some_and(|wanted| wanted != track_at) || track.percussion_track {
            continue;
        }
        // A keyboard's or a voice's notes are written on stand-in strings: their fret moves
        // with the pitch, but they are not re-fingered like a guitar's.
        let fingerings =
            crate::instrument::classify(track.instrument_type.as_deref(), track.percussion_track)
                == InstrumentKind::Strings;
        move_measures(
            &mut previous,
            track_at,
            StaffIndex::new(0),
            &mut track.measures,
            &track.strings,
            track.offset,
            fingerings,
            semitones,
        )?;
        for (staff_index, staff) in track.staves.iter_mut().enumerate() {
            move_measures(
                &mut previous,
                track_at,
                StaffIndex::new(staff_index.saturating_add(1)),
                &mut staff.measures,
                &staff.strings,
                staff.offset,
                fingerings,
                semitones,
            )?;
        }
    }
    Ok(previous)
}

/// Moves the notes of one staff's measures; see [`move_notes`].
#[allow(clippy::too_many_arguments)]
fn move_measures(
    previous: &mut Vec<PreviousBeat>,
    track: TrackIndex,
    staff: StaffIndex,
    measures: &mut [GpMeasure],
    strings: &[(i8, i8)],
    capo: i32,
    fingerings: bool,
    semitones: i32,
) -> Result<(), ModelError> {
    for (measure_index, measure) in measures.iter_mut().enumerate() {
        for (voice_index, voice) in measure.voices.iter_mut().enumerate() {
            for (beat_index, beat) in voice.beats.iter_mut().enumerate() {
                let before = beat.notes.clone();
                let mut changed = false;
                for note in &mut beat.notes {
                    let Some(moved) = move_note(note, strings, capo, fingerings, semitones)
                        .map_err(|why| {
                            let at = BeatPosition {
                                track,
                                staff,
                                measure: MeasureIndex::new(measure_index),
                                voice: VoiceIndex::new(voice_index),
                                beat: BeatIndex::new(beat_index),
                            };
                            ModelError::Invalid(format!("{at}: {why}"))
                        })?
                    else {
                        continue;
                    };
                    *note = moved;
                    changed = true;
                }
                if changed {
                    let at = BeatPosition {
                        track,
                        staff,
                        measure: MeasureIndex::new(measure_index),
                        voice: VoiceIndex::new(voice_index),
                        beat: BeatIndex::new(beat_index),
                    };
                    previous.push((at, before));
                }
            }
        }
    }
    Ok(())
}

/// `note` moved by `semitones`, or `None` when it is one to leave alone (a drum sound, a
/// dead note, a note on a string its staff does not have).
fn move_note(
    note: &GpNote,
    strings: &[(i8, i8)],
    capo: i32,
    fingerings: bool,
    semitones: i32,
) -> Result<Option<GpNote>, ModelError> {
    if note.kind == gscore_gp::NoteType::Dead || (note.value < 0 && fingerings) {
        return Ok(None);
    }
    let Some(open) = open_pitch(strings, note.string) else {
        return Ok(None);
    };
    let mut moved = note.clone();
    if fingerings {
        // The pitch it should sound: its open string plus fret and capo, moved.
        let target = i32::from(open)
            .saturating_add(capo)
            .saturating_add(i32::from(note.value))
            .saturating_add(semitones);
        let Some((string, fret)) = refinger(strings, capo, note.string, target) else {
            return Err(ModelError::Invalid(format!(
                "string {}, fret {} cannot go {} semitones {}: it would leave the strings",
                note.string,
                note.value,
                semitones.abs(),
                if semitones > 0 { "up" } else { "down" }
            )));
        };
        moved.string = string;
        moved.value = fret;
        if let Some(grace) = &mut moved.effect.grace {
            // A grace note is played on its main note's string, so its fret moves equally.
            let fret = i32::from(grace.fret).saturating_add(semitones);
            if !(0..=MAX_FRET).contains(&fret) {
                return Err(ModelError::Invalid(format!(
                    "the grace note's fret {fret} would leave the strings"
                )));
            }
            grace.fret = i8::try_from(fret).unwrap_or(grace.fret);
        }
    } else {
        let fret = i32::from(note.value).saturating_add(semitones);
        moved.value = i16::try_from(fret)
            .map_err(|_| ModelError::Invalid(format!("the fret {fret} is out of range")))?;
    }
    Ok(Some(moved))
}

/// The open pitch of `string` (1-based) among `strings`, if the staff has it.
fn open_pitch(strings: &[(i8, i8)], string: i8) -> Option<i8> {
    strings
        .iter()
        .find(|(number, _)| *number == string)
        .map(|&(_, pitch)| pitch)
}

/// Where a note at `target` can be played: its own string when the fret fits, else the
/// nearest string that can hold the pitch. `None` when no string can.
fn refinger(strings: &[(i8, i8)], capo: i32, string: i8, target: i32) -> Option<(i8, i16)> {
    let candidates = || {
        strings
            .iter()
            .filter(|&&(number, _)| number >= 1)
            .filter_map(move |&(number, open)| {
                let fret = target.saturating_sub(i32::from(open)).saturating_sub(capo);
                (0..=MAX_FRET)
                    .contains(&fret)
                    .then(|| (number, fret, number.abs_diff(string)))
            })
    };
    // The note's own string first, then the closest one; ties go to the higher string.
    candidates()
        .min_by_key(|&(number, fret, distance)| (distance, fret, number))
        .map(|(number, fret, _)| (number, i16::try_from(fret).unwrap_or(0)))
}
