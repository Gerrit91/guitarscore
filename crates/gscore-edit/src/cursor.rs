//! The edit cursor: a beat and a string of one track. Plain data; the moves only read the
//! song, and [`Cursor::clamp`] pulls a cursor back into a song that changed under it.

use gscore_model::{
    BeatIndex, BeatPosition, MeasureIndex, Song, StaffIndex, StringNumber, TrackIndex, VoiceIndex,
};

/// Where the next note goes: one string of one beat, on one staff of a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cursor {
    /// The track.
    pub track: TrackIndex,
    /// Which staff of the track (0 for most tracks; a keyboard's bass is 1).
    pub staff: StaffIndex,
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
    /// The first beat of `track`, on its first staff's highest string.
    pub fn start(track: TrackIndex) -> Self {
        Cursor {
            track,
            staff: StaffIndex::new(0),
            measure: MeasureIndex::new(0),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(0),
            string: 1,
        }
    }

    /// The beat the cursor is on.
    pub fn position(&self) -> BeatPosition {
        BeatPosition {
            staff: self.staff,
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
        self.staff = self.clamped_staff(song);
        let beats = self.beat_count(song);
        if self.beat.get() >= beats {
            self.beat = BeatIndex::new(beats.saturating_sub(1));
        }
        // Skip grace-note beats: they are ornaments, not editable beats, and have no
        // cursor of their own on the page.
        if self.is_grace(song, self.measure, self.beat.get()) {
            let next = (self.beat.get().saturating_add(1)..beats)
                .find(|&beat| !self.is_grace(song, self.measure, beat));
            let previous = (0..self.beat.get())
                .rev()
                .find(|&beat| !self.is_grace(song, self.measure, beat));
            if let Some(beat) = next.or(previous) {
                self.beat = BeatIndex::new(beat);
            }
        }
        let strings = self.string_count(song);
        self.string = self.string.clamp(1, strings);
    }

    /// The track's staff the cursor is on, pulled back into the track when it lost staves.
    fn clamped_staff(&self, song: &Song) -> StaffIndex {
        let staves = song
            .track(self.track)
            .map_or(1, |track| track.staff_count())
            .max(1);
        let index = self.staff.get().min(staves.saturating_sub(1));
        StaffIndex::new(index)
    }

    /// How many strings the cursor's staff has (at least one).
    fn string_count(&self, song: &Song) -> u8 {
        let strings = song
            .staff(self.track, self.staff)
            .map_or(1, |staff| staff.string_count())
            .max(1);
        u8::try_from(strings).unwrap_or(u8::MAX)
    }

    /// Beats in the cursor's voice of `measure` (0 if the measure or voice is missing).
    pub fn beats_in(&self, song: &Song, measure: MeasureIndex) -> usize {
        song.staff(self.track, self.staff)
            .and_then(|t| t.measure(measure))
            .and_then(|m| m.voice(self.voice))
            .map_or(0, |v| v.beats().len())
    }

    fn beat_count(&self, song: &Song) -> usize {
        self.beats_in(song, self.measure)
    }

    /// Whether the beat at `beat` of `measure` in the cursor's voice is a grace-note beat.
    fn is_grace(&self, song: &Song, measure: MeasureIndex, beat: usize) -> bool {
        song.staff(self.track, self.staff)
            .and_then(|track| track.measure(measure))
            .and_then(|measure| measure.voice(self.voice))
            .and_then(|voice| voice.beat(BeatIndex::new(beat)))
            .is_some_and(|beat| beat.is_grace())
    }

    /// Whether the cursor is on the last beat of its measure (or the measure has none).
    pub fn at_measure_end(&self, song: &Song) -> bool {
        self.beat.get().saturating_add(1) >= self.beat_count(song)
    }

    /// Whether the cursor is in the last measure.
    pub fn in_last_measure(&self, song: &Song) -> bool {
        self.measure.get().saturating_add(1) >= song.measure_count()
    }

    /// One beat back, skipping grace-note beats; from a measure's first beat to the previous
    /// measure's last. Returns whether it moved.
    pub fn left(&mut self, song: &Song) -> bool {
        let mut measure = self.measure;
        let mut beat = self.beat.get();
        loop {
            if let Some(previous) = beat.checked_sub(1) {
                beat = previous;
            } else {
                let Some(previous_measure) = measure.get().checked_sub(1) else {
                    return false;
                };
                measure = MeasureIndex::new(previous_measure);
                // An empty measure is a stop: its beat 0 is where a note would go.
                beat = self.beats_in(song, measure).saturating_sub(1);
            }
            if !self.is_grace(song, measure, beat) {
                self.measure = measure;
                self.beat = BeatIndex::new(beat);
                return true;
            }
        }
    }

    /// One beat on, skipping grace-note beats; from a measure's last beat to the next
    /// measure's first. Returns whether it moved (not at the end of the song).
    pub fn right(&mut self, song: &Song) -> bool {
        let mut measure = self.measure;
        let mut beat = self.beat.get();
        loop {
            beat = beat.saturating_add(1);
            let count = self.beats_in(song, measure);
            if beat >= count {
                let next = measure.get().saturating_add(1);
                if next >= song.measure_count() {
                    return false;
                }
                // An empty measure is a stop too: its beat 0 is where a note would go.
                measure = MeasureIndex::new(next);
                beat = 0;
            }
            if !self.is_grace(song, measure, beat) {
                self.measure = measure;
                self.beat = BeatIndex::new(beat);
                return true;
            }
        }
    }

    /// To the first timed beat of the track (the start of the song), keeping the string.
    /// Returns whether it moved.
    pub fn start_of_track(&mut self, song: &Song) -> bool {
        if song.measure_count() == 0 {
            return false;
        }
        let measure = MeasureIndex::new(0);
        let beats = self.beats_in(song, measure);
        let beat = (0..beats)
            .find(|&beat| !self.is_grace(song, measure, beat))
            .unwrap_or(0);
        self.jump_to(measure, beat)
    }

    /// To the last timed beat of the track's last measure, or its start when the measure is
    /// empty. Returns whether it moved.
    pub fn end_of_track(&mut self, song: &Song) -> bool {
        let Some(last) = song.measure_count().checked_sub(1) else {
            return false;
        };
        let measure = MeasureIndex::new(last);
        let beats = self.beats_in(song, measure);
        let beat = (0..beats)
            .rev()
            .find(|&beat| !self.is_grace(song, measure, beat))
            .unwrap_or(0);
        self.jump_to(measure, beat)
    }

    /// Sets measure and beat, reporting whether that changed the cursor.
    fn jump_to(&mut self, measure: MeasureIndex, beat: usize) -> bool {
        let moved = self.measure != measure || self.beat.get() != beat;
        self.measure = measure;
        self.beat = BeatIndex::new(beat);
        moved
    }

    /// To the first timed beat of the next (`forward`) or previous measure; backwards from
    /// inside a measure, to its own first beat first. Returns whether it moved.
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
        let measure = MeasureIndex::new(target);
        let count = self.beats_in(song, measure);
        let first = (0..count)
            .find(|&beat| !self.is_grace(song, measure, beat))
            .unwrap_or(0);
        self.measure = measure;
        self.beat = BeatIndex::new(first);
        true
    }

    /// One string up (towards string 1); from a staff's top string to the lowest string of
    /// the staff above it (a piano's bass to its treble), else stays.
    pub fn up(&mut self, song: &Song) -> bool {
        if self.string > 1 {
            self.string = self.string.saturating_sub(1);
            return true;
        }
        let Some(above) = self.staff.get().checked_sub(1) else {
            return false;
        };
        self.staff = StaffIndex::new(above);
        self.string = self.string_count(song);
        true
    }

    /// One string down; from a staff's lowest string to the top string of the staff below
    /// it, else stays.
    pub fn down(&mut self, song: &Song) -> bool {
        if self.string < self.string_count(song) {
            self.string = self.string.saturating_add(1);
            return true;
        }
        let staves = song
            .track(self.track)
            .map_or(1, |track| track.staff_count());
        let Some(below) = self.staff.get().checked_add(1).filter(|&at| at < staves) else {
            return false;
        };
        self.staff = StaffIndex::new(below);
        self.string = 1;
        true
    }
}
