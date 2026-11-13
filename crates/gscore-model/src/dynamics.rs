//! Dynamics: how loud notes are played, as the eight marks from ppp to fff.
//!
//! Guitar Pro stores a velocity on every note; GP6 and later write it per beat as one of
//! these eight marks, GP3-5 as a step from 1 to 8. The marks map to the MIDI velocities
//! Guitar Pro uses (15, 31, ... 127). Guitar Pro 6 and later start new notes at
//! mezzo-forte and do not print it; GP3-5 files (and their reader) default to forte.

use gscore_gp::{FORTE, MIN_VELOCITY, VELOCITY_INCREMENT};

use crate::position::{BeatIndex, BeatPosition, MeasureIndex};
use crate::song::Song;
use crate::views::{BeatKind, BeatView, NoteKind, NoteView};

/// A dynamic mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Dynamic {
    /// Pianississimo.
    Ppp,
    /// Pianissimo.
    Pp,
    /// Piano.
    P,
    /// Mezzo-piano.
    Mp,
    /// Mezzo-forte, the default of new notes (as in Guitar Pro 6 and later).
    Mf,
    /// Forte.
    F,
    /// Fortissimo.
    Ff,
    /// Fortississimo.
    Fff,
}

impl Dynamic {
    /// Every mark, from the softest to the loudest.
    pub const ALL: [Dynamic; 8] = [
        Dynamic::Ppp,
        Dynamic::Pp,
        Dynamic::P,
        Dynamic::Mp,
        Dynamic::Mf,
        Dynamic::F,
        Dynamic::Ff,
        Dynamic::Fff,
    ];

    /// The dynamic of notes nothing else was said about: new notes, and the start of a
    /// track (a score writes no mark until the dynamic differs from it).
    pub const DEFAULT: Dynamic = Dynamic::Mf;

    /// Position from the softest (0) to the loudest (7).
    fn step(self) -> i16 {
        match self {
            Dynamic::Ppp => 0,
            Dynamic::Pp => 1,
            Dynamic::P => 2,
            Dynamic::Mp => 3,
            Dynamic::Mf => 4,
            Dynamic::F => 5,
            Dynamic::Ff => 6,
            Dynamic::Fff => 7,
        }
    }

    /// The MIDI velocity Guitar Pro plays the mark with (15 for ppp up to 127 for fff).
    pub fn velocity(self) -> u8 {
        u8::try_from(self.raw_velocity()).unwrap_or(127)
    }

    /// The velocity as Guitar Pro's note stores it.
    pub(crate) fn raw_velocity(self) -> i16 {
        MIN_VELOCITY.saturating_add(VELOCITY_INCREMENT.saturating_mul(self.step()))
    }

    /// The mark nearest to a stored velocity (any value: files may hold odd ones).
    pub(crate) fn from_raw_velocity(velocity: i16) -> Dynamic {
        Dynamic::ALL
            .into_iter()
            .min_by_key(|d| d.raw_velocity().abs_diff(velocity))
            .unwrap_or(Dynamic::DEFAULT)
    }

    /// The mark as written in scores and command lines: `ppp`, `pp`, `p`, `mp`, `mf`,
    /// `f`, `ff`, `fff`.
    pub fn id(self) -> &'static str {
        match self {
            Dynamic::Ppp => "ppp",
            Dynamic::Pp => "pp",
            Dynamic::P => "p",
            Dynamic::Mp => "mp",
            Dynamic::Mf => "mf",
            Dynamic::F => "f",
            Dynamic::Ff => "ff",
            Dynamic::Fff => "fff",
        }
    }

    /// The mark written as `id` (case-insensitive), if there is one.
    pub fn from_id(id: &str) -> Option<Dynamic> {
        Dynamic::ALL
            .into_iter()
            .find(|d| d.id().eq_ignore_ascii_case(id))
    }

    /// The Italian name: "pianissimo", "forte", ...
    pub fn name(self) -> &'static str {
        match self {
            Dynamic::Ppp => "pianississimo",
            Dynamic::Pp => "pianissimo",
            Dynamic::P => "piano",
            Dynamic::Mp => "mezzo-piano",
            Dynamic::Mf => "mezzo-forte",
            Dynamic::F => "forte",
            Dynamic::Ff => "fortissimo",
            Dynamic::Fff => "fortississimo",
        }
    }
}

// The reader's default (GP3-5's forte) must be one of the marks.
const _: () = assert!(FORTE == MIN_VELOCITY + VELOCITY_INCREMENT * 5);

impl NoteView<'_> {
    /// The note's dynamic: the mark nearest to its velocity.
    pub fn dynamic(&self) -> Dynamic {
        Dynamic::from_raw_velocity(self.inner.velocity)
    }
}

impl BeatView<'_> {
    /// The beat's dynamic, from its first sounding note (Guitar Pro 6 and later keep one
    /// dynamic per beat). `None` for rests and beats without notes.
    pub fn dynamic(&self) -> Option<Dynamic> {
        if self.kind() != BeatKind::Normal {
            return None;
        }
        self.notes()
            .find(|note| {
                matches!(
                    note.kind(),
                    NoteKind::Normal | NoteKind::Tie | NoteKind::Dead
                )
            })
            .map(|note| note.dynamic())
    }
}

impl Song {
    /// The dynamic in effect at `at`: the beat's own, else that of the nearest earlier beat
    /// with notes in the same track and voice, else the default (mezzo-forte). New notes are
    /// entered with it, as in Guitar Pro, where a dynamic lasts until the next one.
    pub fn dynamic_at(&self, at: BeatPosition) -> Dynamic {
        let Some(track) = self.staff(at.track, at.staff) else {
            return Dynamic::DEFAULT;
        };
        // Only real positions are looked up: walking back from a hostile measure index
        // would take forever.
        if at.measure.get() >= self.measure_count() {
            return Dynamic::DEFAULT;
        }
        // From `at` back to the start of the track: this measure up to the beat, then the
        // earlier measures whole. Bounded by the song's size.
        let mut measure = at.measure.get();
        let mut until = at.beat.get().saturating_add(1);
        loop {
            let found = track
                .measure(MeasureIndex::new(measure))
                .and_then(|m| m.voice(at.voice))
                .and_then(|voice| {
                    let count = voice.beats().len().min(until);
                    (0..count)
                        .rev()
                        .find_map(|beat| voice.beat(BeatIndex::new(beat))?.dynamic())
                });
            if let Some(dynamic) = found {
                return dynamic;
            }
            let Some(previous) = measure.checked_sub(1) else {
                return Dynamic::DEFAULT;
            };
            measure = previous;
            until = usize::MAX;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_map_to_guitar_pros_velocities() {
        let velocities: Vec<u8> = Dynamic::ALL.iter().map(|d| d.velocity()).collect();
        assert_eq!(velocities, [15, 31, 47, 63, 79, 95, 111, 127]);
        assert_eq!(Dynamic::F.raw_velocity(), FORTE);
        assert_eq!(Dynamic::DEFAULT, Dynamic::Mf);
        for dynamic in Dynamic::ALL {
            assert_eq!(Dynamic::from_raw_velocity(dynamic.raw_velocity()), dynamic);
            assert_eq!(Dynamic::from_id(dynamic.id()), Some(dynamic));
        }
        // Odd stored values go to the nearest mark.
        assert_eq!(Dynamic::from_raw_velocity(0), Dynamic::Ppp);
        assert_eq!(Dynamic::from_raw_velocity(100), Dynamic::F);
        assert_eq!(Dynamic::from_raw_velocity(i16::MAX), Dynamic::Fff);
        assert_eq!(Dynamic::from_raw_velocity(i16::MIN), Dynamic::Ppp);
        assert_eq!(Dynamic::from_id("MF"), Some(Dynamic::Mf));
        assert_eq!(Dynamic::from_id("sfz"), None);
    }
}
