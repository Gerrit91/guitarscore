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

/// Most places tried while fingering one beat's notes onto separate strings.
const MAX_FINGERING_STEPS: usize = 10_000;

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

    /// Whether some track that is not a drum kit is transposed (by any amount; the
    /// tracks need not agree, unlike [`Self::transposition`]).
    pub fn is_transposed(&self) -> bool {
        self.tracks()
            .any(|track| !track.is_percussion() && track.transposition() != 0)
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
    /// It switches the transposition on, so the song plays as it was saved.
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
        let change = self.edit_track_settings(|inner| {
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
        })?;
        self.transpose_active = true;
        Ok(change)
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

/// `written` moved by `semitones`, kept within [`MAX_TRANSPOSE`]. A file may store more:
/// such a track only moves back towards that range, never further out.
fn clamp_step(written: i32, semitones: i32) -> i32 {
    written
        .saturating_add(semitones)
        .clamp(written.min(-MAX_TRANSPOSE), written.max(MAX_TRANSPOSE))
}

/// The transposition a track stores, as playback reads it ([`TrackView::transposition`]),
/// so undo puts back exactly what was there.
fn stored(track: &gscore_gp::Track) -> i32 {
    track.transpose_sound.clamp(
        -crate::views::MAX_TRACK_TRANSPOSE,
        crate::views::MAX_TRACK_TRANSPOSE,
    )
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
                let at = BeatPosition {
                    track,
                    staff,
                    measure: MeasureIndex::new(measure_index),
                    voice: VoiceIndex::new(voice_index),
                    beat: BeatIndex::new(beat_index),
                };
                let moved = move_beat(&beat.notes, strings, capo, fingerings, semitones)
                    .map_err(|why| ModelError::Invalid(format!("{at}: {why}")))?;
                let Some(moved) = moved else {
                    continue;
                };
                previous.push((at, std::mem::replace(&mut beat.notes, moved)));
            }
        }
    }
    Ok(())
}

/// Where a moved note goes: its string and fret, and whether a natural harmonic becomes
/// an artificial one (touched `node` frets above the new fret).
#[derive(Clone, Copy)]
struct Placement {
    string: i8,
    open: i8,
    fret: i32,
    artificial_node: Option<i8>,
}

/// The notes of one beat moved by `semitones`, or `None` when none of them moves (drum
/// sounds, dead notes, notes on a string the staff does not have stay as they are). The
/// notes are fingered together, so no two of them end up on the same string. The error
/// says why they cannot be moved.
fn move_beat(
    notes: &[GpNote],
    strings: &[(i8, i8)],
    capo: i32,
    fingerings: bool,
    semitones: i32,
) -> Result<Option<Vec<GpNote>>, String> {
    let movable = |note: &GpNote| {
        note.kind != gscore_gp::NoteType::Dead
            && !(note.value < 0 && fingerings)
            && open_pitch(strings, note.string).is_some()
    };
    if !notes.iter().any(movable) {
        return Ok(None);
    }
    let mut moved = notes.to_vec();
    if !fingerings {
        // A keyboard's or a voice's stand-in strings: the fret moves with the pitch.
        for note in moved.iter_mut().filter(|note| movable(note)) {
            let fret = i32::from(note.value).saturating_add(semitones);
            note.value =
                i16::try_from(fret).map_err(|_| format!("the fret {fret} is out of range"))?;
        }
        return Ok(Some(moved));
    }
    // The strings of the notes that stay are taken; each moving note gets the places that
    // can hold its new pitch, best first, and the first set of distinct strings wins.
    let mut taken: Vec<i8> = notes
        .iter()
        .filter(|note| !movable(note))
        .map(|note| note.string)
        .collect();
    let mut options = Vec::new();
    for (index, note) in notes.iter().enumerate().filter(|(_, note)| movable(note)) {
        let places = placements(note, strings, capo, semitones);
        if places.is_empty() {
            return Err(format!(
                "string {}, fret {} cannot go {} semitones {}: it would leave the strings",
                note.string,
                note.value,
                semitones.abs(),
                if semitones > 0 { "up" } else { "down" }
            ));
        }
        options.push((index, places));
    }
    // Notes that can stay on their own string claim it first; the others move around them.
    options.sort_by_key(|(index, places)| {
        let own = notes.get(*index).map(|note| note.string);
        places.first().map(|place| Some(place.string)) != Some(own)
    });
    let mut chosen = Vec::with_capacity(options.len());
    let mut budget = MAX_FINGERING_STEPS;
    if !assign(&options, &mut taken, &mut chosen, &mut budget) {
        return Err(format!(
            "its notes cannot all go {} semitones {} on separate strings",
            semitones.abs(),
            if semitones > 0 { "up" } else { "down" }
        ));
    }
    for (&(index, _), place) in options.iter().zip(chosen) {
        let (Some(note), Some(old_open)) = (
            moved.get_mut(index),
            notes
                .get(index)
                .and_then(|note| open_pitch(strings, note.string)),
        ) else {
            continue;
        };
        note.string = place.string;
        note.value = i16::try_from(place.fret).unwrap_or(note.value);
        if let (Some(node), Some(harmonic)) = (place.artificial_node, &mut note.effect.harmonic) {
            harmonic.kind = gscore_gp::HarmonicType::Artificial;
            harmonic.fret = Some(node);
        }
        if let Some(grace) = &mut note.effect.grace {
            // A grace note is played on its main note's string: its pitch moves by
            // `semitones`, its fret by that less the step between the two open strings.
            let step = i32::from(place.open).saturating_sub(i32::from(old_open));
            let fret = i32::from(grace.fret)
                .saturating_add(semitones)
                .saturating_sub(step);
            if !(0..=MAX_FRET).contains(&fret) {
                return Err(format!(
                    "the grace note's fret {fret} would leave the strings"
                ));
            }
            grace.fret = i8::try_from(fret)
                .map_err(|_| format!("the grace note's fret {fret} would leave the strings"))?;
        }
    }
    Ok(Some(moved))
}

/// Gives every note of `options` one of its places on a string not yet `taken`, the
/// earliest possible for the first note, then the next; `false` when there is no way
/// (or the search takes more than `budget` steps).
fn assign(
    options: &[(usize, Vec<Placement>)],
    taken: &mut Vec<i8>,
    chosen: &mut Vec<Placement>,
    budget: &mut usize,
) -> bool {
    let Some(((_, places), rest)) = options.split_first() else {
        return true;
    };
    for place in places {
        if taken.contains(&place.string) {
            continue;
        }
        let Some(left) = budget.checked_sub(1) else {
            return false;
        };
        *budget = left;
        taken.push(place.string);
        chosen.push(*place);
        if assign(rest, taken, chosen, budget) {
            return true;
        }
        let _ = taken.pop();
        let _ = chosen.pop();
    }
    false
}

/// Where `note` moved by `semitones` can be played, best first: its own string, then the
/// closest (ties to the higher string, then the lower fret). A natural harmonic sounds
/// over its open string, so it keeps its node on a string whose open note is the moved
/// one; elsewhere it becomes an artificial harmonic with the same node above the fret.
fn placements(note: &GpNote, strings: &[(i8, i8)], capo: i32, semitones: i32) -> Vec<Placement> {
    let Some(open) = open_pitch(strings, note.string) else {
        return Vec::new();
    };
    let natural_node = note
        .effect
        .harmonic
        .as_ref()
        .filter(|harmonic| harmonic.kind == gscore_gp::HarmonicType::Natural)
        .and_then(|_| u8::try_from(note.value).ok())
        .filter(|&node| crate::playback::harmonic_interval(node).is_some());
    // The pitch to play: the open string (with capo) plus the fret, or for a natural
    // harmonic the open string alone, its node coming on top.
    let fretted = natural_node.map_or(i32::from(note.value), |_| 0);
    let target = i32::from(open)
        .saturating_add(capo)
        .saturating_add(fretted)
        .saturating_add(semitones);
    let mut places: Vec<(u8, i32, i8, Placement)> = Vec::new();
    for &(number, string_open) in strings.iter().filter(|&&(number, _)| number >= 1) {
        let fret = target
            .saturating_sub(i32::from(string_open))
            .saturating_sub(capo);
        if !(0..=MAX_FRET).contains(&fret) {
            continue;
        }
        let distance = number.abs_diff(note.string);
        let place = |fret, artificial_node| Placement {
            string: number,
            open: string_open,
            fret,
            artificial_node,
        };
        match natural_node {
            None => places.push((distance, fret, number, place(fret, None))),
            Some(node) if fret == 0 => {
                places.push((distance, -1, number, place(i32::from(node), None)));
            }
            Some(node) => {
                let Ok(offset) = i8::try_from(node) else {
                    continue;
                };
                if fret.saturating_add(i32::from(node)) <= MAX_FRET {
                    // After every natural place: an artificial harmonic is harder to play.
                    let rank = distance.saturating_add(u8::MAX / 2);
                    places.push((rank, fret, number, place(fret, Some(offset))));
                }
            }
        }
    }
    places.sort_by_key(|&(distance, fret, number, _)| (distance, fret, number));
    places.into_iter().map(|(_, _, _, place)| place).collect()
}

/// The open pitch of `string` (1-based) among `strings`, if the staff has it.
fn open_pitch(strings: &[(i8, i8)], string: i8) -> Option<i8> {
    strings
        .iter()
        .find(|(number, _)| *number == string)
        .map(|&(_, pitch)| pitch)
}
