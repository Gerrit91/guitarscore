//! The edit cursor: a beat and a string of one track. Plain data; the moves only read the
//! song, and [`Cursor::clamp`] pulls a cursor back into a song that changed under it.

use gscore_model::{
    BeatIndex, BeatPosition, MeasureIndex, Song, StringNumber, TrackIndex, VoiceIndex,
};

/// Where the next note goes: one string of one beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cursor {
    /// The track.
    pub track: TrackIndex,
    /// The measure.
    pub measure: MeasureIndex,
    /// The voice (voice 1 for now).
    pub voice: VoiceIndex,
    /// The beat. In a measure without beats (an empty GP7 bar) it is 0, the place where
    /// the first beat goes.
    pub beat: BeatIndex,
    /// The string, 1 = highest.
    pub string: u8,
}

impl Cursor {
    /// The first beat of `track`, on its highest string.
    pub fn start(track: TrackIndex) -> Self {
        Cursor {
            track,
            measure: MeasureIndex::new(0),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(0),
            string: 1,
        }
    }

    /// The beat the cursor is on.
    pub fn position(&self) -> BeatPosition {
        BeatPosition {
            track: self.track,
            measure: self.measure,
            voice: self.voice,
            beat: self.beat,
        }
    }

    /// The cursor's string, if it is a valid string number.
    pub fn string_number(&self) -> Option<StringNumber> {
        StringNumber::new(self.string).ok()
    }

    /// Keeps the cursor inside `song`: an existing track, measure, beat (or beat 0) and
    /// string. Call after every edit, undo and redo.
    pub fn clamp(&mut self, song: &Song) {
        let tracks = song.track_count();
        if self.track.get() >= tracks {
            self.track = TrackIndex::new(tracks.saturating_sub(1));
        }
        let measures = song.measure_count();
        if self.measure.get() >= measures {
            self.measure = MeasureIndex::new(measures.saturating_sub(1));
        }
        let beats = self.beat_count(song);
        if self.beat.get() >= beats {
            self.beat = BeatIndex::new(beats.saturating_sub(1));
        }
        let strings = song
            .track(self.track)
            .map_or(1, |t| t.string_count())
            .max(1);
        let strings = u8::try_from(strings).unwrap_or(u8::MAX);
        self.string = self.string.clamp(1, strings);
    }

    /// Beats in the cursor's voice of `measure` (0 if the measure or voice is missing).
    pub fn beats_in(&self, song: &Song, measure: MeasureIndex) -> usize {
        song.track(self.track)
            .and_then(|t| t.measure(measure))
            .and_then(|m| m.voice(self.voice))
            .map_or(0, |v| v.beats().len())
    }

    fn beat_count(&self, song: &Song) -> usize {
        self.beats_in(song, self.measure)
    }

    /// Whether the cursor is on the last beat of its measure (or the measure has none).
    pub fn at_measure_end(&self, song: &Song) -> bool {
        self.beat.get().saturating_add(1) >= self.beat_count(song)
    }

    /// Whether the cursor is in the last measure.
    pub fn in_last_measure(&self, song: &Song) -> bool {
        self.measure.get().saturating_add(1) >= song.measure_count()
    }

    /// One beat back; from a measure's first beat to the previous measure's last. Returns
    /// whether it moved.
    pub fn left(&mut self, song: &Song) -> bool {
        if let Some(beat) = self.beat.get().checked_sub(1) {
            self.beat = BeatIndex::new(beat);
            return true;
        }
        let Some(measure) = self.measure.get().checked_sub(1) else {
            return false;
        };
        self.measure = MeasureIndex::new(measure);
        self.beat = BeatIndex::new(self.beat_count(song).saturating_sub(1));
        true
    }

    /// One beat on; from a measure's last beat to the next measure's first. Returns whether
    /// it moved (not at the end of the song).
    pub fn right(&mut self, song: &Song) -> bool {
        if !self.at_measure_end(song) {
            self.beat = BeatIndex::new(self.beat.get().saturating_add(1));
            return true;
        }
        if self.in_last_measure(song) {
            return false;
        }
        self.measure = MeasureIndex::new(self.measure.get().saturating_add(1));
        self.beat = BeatIndex::new(0);
        true
    }

    /// To the first beat of the next (`forward`) or previous measure; backwards from inside
    /// a measure, to its own first beat first. Returns whether it moved.
    pub fn to_measure(&mut self, song: &Song, forward: bool) -> bool {
        let target = if forward {
            self.measure.get().saturating_add(1)
        } else {
            match self.measure.get().checked_sub(1) {
                Some(m) => m,
                None if self.beat.get() > 0 => 0,
                None => return false,
            }
        };
        if target >= song.measure_count() {
            return false;
        }
        self.measure = MeasureIndex::new(target);
        self.beat = BeatIndex::new(0);
        true
    }

    /// One string up (towards string 1); stays on string 1.
    pub fn up(&mut self) -> bool {
        if self.string > 1 {
            self.string = self.string.saturating_sub(1);
            true
        } else {
            false
        }
    }

    /// One string down; stays on the track's lowest string.
    pub fn down(&mut self, song: &Song) -> bool {
        let strings = song.track(self.track).map_or(1, |t| t.string_count());
        if usize::from(self.string) < strings {
            self.string = self.string.saturating_add(1);
            true
        } else {
            false
        }
    }
}
