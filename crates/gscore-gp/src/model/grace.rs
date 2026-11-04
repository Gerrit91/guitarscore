//! GuitarScore addition: the two ways Guitar Pro stores grace notes, and conversions
//! between them.
//!
//! GP3-5 attach a grace note to the note it ornaments ([`NoteEffect::grace`]); GP6/GP7 give
//! the grace notes a beat of their own before the main beat ([`Beat::grace`]). The editor
//! works with attached grace notes, so reading a GP6/GP7 file folds a grace beat into the
//! next beat where nothing is lost ([`fold_grace_beats`]); writing a GP6/GP7 file turns
//! attached grace notes back into grace beats ([`unfold_grace_notes`]). GP3-5 files cannot
//! hold a grace beat at all: writing them folds what it can and leaves out the rest
//! ([`Song::with_attached_graces`]; the save check reports what went missing).

use crate::model::{
    Beat, BeatStatus, Duration, GraceEffect, GraceEffectTransition, Note, NoteEffect, NoteType,
    SlideType, Song,
};

/// The written values a grace note can have (64th, 32nd, 16th), as GP3-5 store them.
pub const GRACE_VALUES: [u16; 3] = [64, 32, 16];

/// Folds every grace beat of `beats` that can be folded into the beat after it; returns how
/// many were folded. A grace beat folds when each of its notes is a plain (or dead) note,
/// at most with a hammer-on or slide into the main note, on a string where the next beat
/// has a note without a grace note yet, and its value is a 64th, 32nd or 16th.
pub fn fold_grace_beats(beats: &mut Vec<Beat>) -> usize {
    let mut folded = 0_usize;
    let mut index = 0;
    while index < beats.len() {
        let foldable = match (beats.get(index), beats.get(index.saturating_add(1))) {
            (Some(grace), Some(main)) => can_fold(grace, main),
            _ => false,
        };
        if !foldable {
            index = index.saturating_add(1);
            continue;
        }
        let grace = beats.remove(index);
        if let Some(main) = beats.get_mut(index) {
            let value = grace.duration.value;
            for note in &grace.notes {
                let Some(target) = main.notes.iter_mut().find(|n| n.string == note.string) else {
                    continue;
                };
                target.effect.grace = Some(attached(note, value));
            }
        }
        folded = folded.saturating_add(1);
    }
    folded
}

/// Whether `grace` can fold into `main` without losing anything.
fn can_fold(grace: &Beat, main: &Beat) -> bool {
    let plain_beat = grace.grace
        && grace.status == BeatStatus::Normal
        && !grace.notes.is_empty()
        && grace.text.is_empty()
        && grace.effect.is_default()
        && grace.effect.chord.is_none()
        && plain_value(&grace.duration);
    let main_ok = !main.grace && main.status == BeatStatus::Normal;
    if !plain_beat || !main_ok {
        return false;
    }
    let on_beat = |note: &Note| note.effect.grace.as_ref().is_some_and(|g| g.is_on_beat);
    let first_on_beat = grace.notes.first().is_some_and(on_beat);
    grace.notes.iter().enumerate().all(|(index, note)| {
        let unique = !grace
            .notes
            .iter()
            .skip(index.saturating_add(1))
            .any(|other| other.string == note.string);
        let target = main
            .notes
            .iter()
            .find(|n| n.string == note.string)
            .is_some_and(|n| n.effect.grace.is_none() && n.kind != NoteType::Rest);
        unique
            && target
            && on_beat(note) == first_on_beat
            && matches!(note.kind, NoteType::Normal | NoteType::Dead)
            && (0..=i16::from(i8::MAX)).contains(&note.value)
            && plain_effects(&note.effect)
    })
}

/// A 64th, 32nd or 16th: the values an attached grace note can have.
fn plain_value(duration: &Duration) -> bool {
    GRACE_VALUES.contains(&duration.value)
        && !duration.dotted
        && !duration.double_dotted
        && duration.tuplet_enters == 1
        && duration.tuplet_times == 1
}

/// Whether a grace beat's note has nothing an attached grace note cannot say: at most a
/// hammer-on or a shift slide into the main note.
fn plain_effects(effect: &NoteEffect) -> bool {
    let mut rest = effect.clone();
    rest.hammer = false;
    rest.grace = None;
    if rest.slides == [SlideType::ShiftSlideTo] {
        rest.slides.clear();
    }
    let slide_and_hammer = effect.hammer && !effect.slides.is_empty();
    !slide_and_hammer && rest == NoteEffect::default()
}

/// The attached grace note for a grace beat's `note` of written `value`.
fn attached(note: &Note, value: u16) -> GraceEffect {
    let previous = note.effect.grace.as_ref();
    GraceEffect {
        fret: i8::try_from(note.value).unwrap_or(0),
        velocity: note.velocity,
        duration: u8::try_from(value).unwrap_or(32),
        is_dead: note.kind == NoteType::Dead,
        is_on_beat: previous.is_some_and(|grace| grace.is_on_beat),
        transition: if note.effect.hammer {
            GraceEffectTransition::Hammer
        } else if !note.effect.slides.is_empty() {
            GraceEffectTransition::Slide
        } else {
            GraceEffectTransition::None
        },
    }
}

/// `beats` with a grace beat before every beat whose notes carry attached grace notes; the
/// attached grace notes are taken off. Grace notes played on the beat and before it get a
/// beat each (on the beat last, as it sounds).
pub fn unfold_grace_notes(beats: &[Beat]) -> Vec<Beat> {
    let mut out = Vec::with_capacity(beats.len());
    for beat in beats {
        if beat.grace || !beat.notes.iter().any(|note| note.effect.grace.is_some()) {
            out.push(beat.clone());
            continue;
        }
        for on_beat in [false, true] {
            let notes: Vec<Note> = beat
                .notes
                .iter()
                .filter_map(|note| {
                    let grace = note.effect.grace.as_ref()?;
                    (grace.is_on_beat == on_beat).then(|| grace_note(note, grace))
                })
                .collect();
            if let Some(first) = notes.first() {
                let value = first
                    .effect
                    .grace
                    .as_ref()
                    .map_or(32, |grace| grace_value(grace.duration));
                out.push(Beat {
                    notes,
                    duration: Duration {
                        value,
                        ..Duration::default()
                    },
                    status: BeatStatus::Normal,
                    grace: true,
                    ..Beat::default()
                });
            }
        }
        let mut main = beat.clone();
        for note in &mut main.notes {
            note.effect.grace = None;
        }
        out.push(main);
    }
    out
}

/// The written value of an attached grace note: a 64th, 32nd or 16th (anything else, as a
/// damaged file may hold, counts as a 32nd).
pub fn grace_value(duration: u8) -> u16 {
    let value = u16::from(duration);
    if GRACE_VALUES.contains(&value) {
        value
    } else {
        32
    }
}

/// The note of a grace beat for the attached `grace` of `main`.
fn grace_note(main: &Note, grace: &GraceEffect) -> Note {
    let mut effect = NoteEffect {
        grace: Some(grace.clone()),
        ..NoteEffect::default()
    };
    match grace.transition {
        GraceEffectTransition::Hammer => effect.hammer = true,
        GraceEffectTransition::Slide => effect.slides = vec![SlideType::ShiftSlideTo],
        GraceEffectTransition::None | GraceEffectTransition::Bend => {}
    }
    Note {
        value: i16::from(grace.fret.max(0)),
        velocity: grace.velocity,
        string: main.string,
        effect,
        kind: if grace.is_dead || grace.fret < 0 {
            NoteType::Dead
        } else {
            NoteType::Normal
        },
        ..Note::default()
    }
}

impl Song {
    /// Folds the grace beats of every voice into the beats after them where nothing is
    /// lost (see [`fold_grace_beats`]).
    pub fn fold_grace_beats(&mut self) {
        for voice in self
            .tracks
            .iter_mut()
            .flat_map(|track| track.measures.iter_mut())
            .flat_map(|measure| measure.voices.iter_mut())
        {
            let _ = fold_grace_beats(&mut voice.beats);
        }
    }

    /// Whether a note anywhere carries an attached grace note.
    fn has_attached_graces(&self) -> bool {
        self.beats()
            .any(|beat| beat.notes.iter().any(|note| note.effect.grace.is_some()))
    }

    /// Whether a voice anywhere has a grace beat.
    fn has_grace_beats(&self) -> bool {
        self.beats().any(|beat| beat.grace)
    }

    fn beats(&self) -> impl Iterator<Item = &Beat> {
        self.tracks
            .iter()
            .flat_map(|track| track.measures.iter())
            .flat_map(|measure| measure.voices.iter())
            .flat_map(|voice| voice.beats.iter())
    }

    /// For writing GP6/GP7: a copy with every attached grace note as a grace beat, or
    /// `None` when there is none.
    pub fn with_grace_beats(&self) -> Option<Song> {
        if !self.has_attached_graces() {
            return None;
        }
        let mut song = self.clone();
        for voice in song
            .tracks
            .iter_mut()
            .flat_map(|track| track.measures.iter_mut())
            .flat_map(|measure| measure.voices.iter_mut())
        {
            voice.beats = unfold_grace_notes(&voice.beats);
        }
        Some(song)
    }

    /// For writing GP3-5, which have no grace beats: a copy with the grace beats folded
    /// into their main notes, and those that cannot be folded left out (they would make
    /// their measure too long), or `None` when there is no grace beat.
    pub fn with_attached_graces(&self) -> Option<Song> {
        if !self.has_grace_beats() {
            return None;
        }
        let mut song = self.clone();
        song.fold_grace_beats();
        for voice in song
            .tracks
            .iter_mut()
            .flat_map(|track| track.measures.iter_mut())
            .flat_map(|measure| measure.voices.iter_mut())
        {
            voice.beats.retain(|beat| !beat.grace);
        }
        Some(song)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(string: i8, value: i16) -> Note {
        Note {
            value,
            string,
            kind: NoteType::Normal,
            velocity: 95,
            ..Note::default()
        }
    }

    fn grace_beat(notes: Vec<Note>, value: u16) -> Beat {
        Beat {
            notes,
            duration: Duration {
                value,
                ..Duration::default()
            },
            grace: true,
            ..Beat::default()
        }
    }

    fn main_beat(notes: Vec<Note>) -> Beat {
        Beat {
            notes,
            duration: Duration {
                value: 4,
                ..Duration::default()
            },
            ..Beat::default()
        }
    }

    #[test]
    fn a_grace_beat_folds_into_its_note_and_back() {
        let mut hammered = note(1, 5);
        hammered.effect.hammer = true;
        let beats = vec![
            grace_beat(vec![hammered], 16),
            main_beat(vec![note(1, 7), note(2, 5)]),
        ];
        let mut folded = beats.clone();
        assert_eq!(fold_grace_beats(&mut folded), 1);
        assert_eq!(folded.len(), 1);
        let grace = folded[0].notes[0].effect.grace.clone().unwrap();
        assert_eq!((grace.fret, grace.duration, grace.velocity), (5, 16, 95));
        assert_eq!(grace.transition, GraceEffectTransition::Hammer);
        assert!(folded[0].notes[1].effect.grace.is_none());

        let unfolded = unfold_grace_notes(&folded);
        assert_eq!(unfolded.len(), 2);
        assert!(unfolded[0].grace);
        assert_eq!(unfolded[0].duration.value, 16);
        assert_eq!(unfolded[0].notes[0].value, 5);
        assert!(unfolded[0].notes[0].effect.hammer);
        assert!(unfolded[1].notes.iter().all(|n| n.effect.grace.is_none()));
        // And it folds again the same way.
        let mut again = unfolded;
        assert_eq!(fold_grace_beats(&mut again), 1);
        assert_eq!(again, folded);
    }

    #[test]
    fn grace_beats_that_would_lose_something_stay_beats() {
        // No main note on the grace note's string.
        let mut beats = vec![
            grace_beat(vec![note(2, 3)], 32),
            main_beat(vec![note(1, 5)]),
        ];
        assert_eq!(fold_grace_beats(&mut beats), 0);
        // An eighth-note grace beat.
        let mut beats = vec![grace_beat(vec![note(1, 3)], 8), main_beat(vec![note(1, 5)])];
        assert_eq!(fold_grace_beats(&mut beats), 0);
        // A grace note with its own vibrato.
        let mut shaky = note(1, 3);
        shaky.effect.vibrato = true;
        let mut beats = vec![grace_beat(vec![shaky], 32), main_beat(vec![note(1, 5)])];
        assert_eq!(fold_grace_beats(&mut beats), 0);
        // A grace beat at the end of the measure.
        let mut beats = vec![
            main_beat(vec![note(1, 5)]),
            grace_beat(vec![note(1, 3)], 32),
        ];
        assert_eq!(fold_grace_beats(&mut beats), 0);
    }

    #[test]
    fn writing_gp5_drops_grace_beats_that_do_not_fold() {
        let mut song = Song::default();
        let mut track = crate::model::Track::default();
        let measure = crate::model::Measure {
            voices: vec![crate::model::Voice {
                beats: vec![
                    grace_beat(vec![note(2, 3)], 32),
                    grace_beat(vec![note(1, 3)], 32),
                    main_beat(vec![note(1, 5)]),
                ],
                ..crate::model::Voice::default()
            }],
            ..crate::model::Measure::default()
        };
        track.measures.push(measure);
        song.tracks.push(track);
        let legacy = song.with_attached_graces().unwrap();
        let beats = &legacy.tracks[0].measures[0].voices[0].beats;
        assert_eq!(beats.len(), 1, "only the main beat is left");
        assert_eq!(beats[0].notes[0].effect.grace.as_ref().unwrap().fret, 3);
        assert!(legacy.with_attached_graces().is_none());
    }
}
