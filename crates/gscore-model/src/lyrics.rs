//! Lyrics: the up to five lines Guitar Pro keeps per measure (GP3-5) or on beats
//! (GP6/GP7), their syllables and the edit that changes one line.

use std::collections::BTreeSet;

use gscore_gp::Lyrics as GpLyrics;

use crate::error::ModelError;
use crate::position::{BeatIndex, BeatPosition, MeasureIndex, TrackIndex, VoiceIndex};
use crate::song::Song;
use crate::views::{BeatKind, NoteKind};

/// Most characters one lyric line may hold.
pub const MAX_LYRIC_LINE_CHARS: usize = 10_000;
/// How many lyric lines Guitar Pro stores.
pub const LYRIC_LINES: u8 = 5;

/// One lyric line: its index (0 is the first) and where its text starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLine {
    /// 0 to [`LYRIC_LINES`] - 1.
    pub line: u8,
    /// The measure the first syllable belongs to.
    pub start: MeasureIndex,
    /// The text, as stored (syllables separated by spaces, `+` joining two).
    pub text: String,
}

/// One syllable of a lyric line, on a beat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricSyllable {
    /// The line, 0 to [`LYRIC_LINES`] - 1.
    pub line: u8,
    /// The measure.
    pub measure: MeasureIndex,
    /// The beat the syllable is sung on.
    pub beat: BeatIndex,
    /// The syllable.
    pub text: String,
}

impl Song {
    /// The track the lyrics belong to (the file's choice; the first track when it names
    /// none or one that is not there).
    pub fn lyric_track(&self) -> TrackIndex {
        let choice = usize::from(self.inner().lyrics.track_choice);
        if choice < self.track_count() {
            TrackIndex::new(choice)
        } else {
            TrackIndex::new(0)
        }
    }

    /// The lyric lines with text, in line order. GP3-5 store the lines per measure; GP6/GP7
    /// store a syllable on each beat, from which the lines are rebuilt here.
    pub fn lyric_lines(&self) -> Vec<LyricLine> {
        let stored: Vec<LyricLine> = self
            .inner()
            .lyrics
            .lines
            .iter()
            .filter(|(_, _, text)| !text.is_empty())
            .map(|(line, start, text)| LyricLine {
                line: *line,
                start: MeasureIndex::new(usize::from(start.saturating_sub(1))),
                text: text.clone(),
            })
            .collect();
        // GP6/GP7: rebuild each line from its per-beat syllables; a line with a stored
        // text (GP3-5, or one edited here) keeps that text.
        let mut rebuilt: Vec<LyricLine> = Vec::new();
        for (measure, _, lyrics) in beat_lyrics(self, self.lyric_track()) {
            for (line, text) in lyrics.iter().enumerate() {
                let Ok(line) = u8::try_from(line) else {
                    continue;
                };
                if text.is_empty() {
                    continue;
                }
                // Words sharing a beat are joined with `+` in the typed text.
                let text = text.split_whitespace().collect::<Vec<_>>().join("+");
                match rebuilt.iter_mut().find(|entry| entry.line == line) {
                    Some(entry) => {
                        entry.text.push(' ');
                        entry.text.push_str(&text);
                    }
                    None => rebuilt.push(LyricLine {
                        line,
                        start: measure,
                        text,
                    }),
                }
            }
        }
        rebuilt.retain(|entry| !stored.iter().any(|known| known.line == entry.line));
        rebuilt.extend(stored);
        rebuilt.sort_by_key(|line| line.line);
        rebuilt
    }

    /// The syllables of the lyrics at `track`: each line's syllables placed on beats. Empty
    /// if `track` is not the lyric track.
    pub fn lyric_syllables(&self, track: TrackIndex) -> Vec<LyricSyllable> {
        if track != self.lyric_track() {
            return Vec::new();
        }
        let on_beats = beat_lyrics(self, track);
        if on_beats
            .iter()
            .any(|(_, _, lyrics)| lyrics.iter().any(|text| !text.is_empty()))
        {
            return on_beats
                .into_iter()
                .flat_map(|(measure, beat, lyrics)| {
                    lyrics
                        .into_iter()
                        .enumerate()
                        .filter(|(_, text)| !text.is_empty())
                        .filter_map(move |(line, text)| {
                            Some(LyricSyllable {
                                line: u8::try_from(line).ok()?,
                                measure,
                                beat,
                                text,
                            })
                        })
                })
                .collect();
        }
        let count = self.measure_count();
        let mut placed = Vec::new();
        for line in self.lyric_lines() {
            for (measure, beat, text) in
                place_tokens(self, track, line.start, lyric_tokens(&line.text), count)
            {
                placed.push(LyricSyllable {
                    line: line.line,
                    measure,
                    beat,
                    text,
                });
            }
        }
        placed
    }

    /// Copies the lyrics between the two places Guitar Pro keeps them: the per-measure
    /// lines (GP3-5) and the per-beat syllables (GP6/GP7). Export calls this so a file in
    /// either format keeps its lyrics in the other.
    pub fn synchronize_lyrics(&mut self) {
        let lines = self.lyric_lines();
        if lines.is_empty() {
            return;
        }
        let track = self.lyric_track();
        let count = self.measure_count();
        let mut placed: Vec<(u8, MeasureIndex, BeatIndex, String)> = Vec::new();
        for entry in &lines {
            for (measure, beat, text) in
                place_tokens(self, track, entry.start, lyric_tokens(&entry.text), count)
            {
                placed.push((entry.line, measure, beat, text));
            }
        }
        // GP6/GP7 lyrics already sit on their beats, where Guitar Pro put them: keep them
        // (placing the joined text again could move syllables), and only fill in the
        // per-measure lines GP3-5 need.
        let on_beats = has_beat_syllables(self, track);
        let track_choice = u8::try_from(track.get()).unwrap_or(0);
        let inner = self.inner_mut();
        inner.lyrics.lines.clear();
        for entry in &lines {
            inner.lyrics.lines.push((
                entry.line,
                u16::try_from(entry.start.get().saturating_add(1)).unwrap_or(u16::MAX),
                entry.text.clone(),
            ));
        }
        inner.lyrics.lines.sort_by_key(|(line, _, _)| *line);
        inner.lyrics.track_choice = track_choice;
        if on_beats {
            return;
        }
        let Some(gp_track) = inner.tracks.get_mut(track.get()) else {
            return;
        };
        for measure in &mut gp_track.measures {
            for voice in &mut measure.voices {
                for beat in &mut voice.beats {
                    beat.lyrics.clear();
                }
            }
        }
        for (line, measure, beat, text) in placed {
            let Some(beat) = gp_track
                .measures
                .get_mut(measure.get())
                .and_then(|measure| measure.voices.first_mut())
                .and_then(|voice| voice.beats.get_mut(beat.get()))
            else {
                continue;
            };
            let line = usize::from(line);
            if beat.lyrics.len() <= line {
                beat.lyrics.resize(line.saturating_add(1), String::new());
            }
            if let Some(slot) = beat.lyrics.get_mut(line) {
                *slot = text;
            }
        }
    }

    /// Sets one lyric line: where its text starts and the text itself. An empty text
    /// removes the line. The syllables are written to the beats too, so GP6/GP7 keep them.
    /// Returns the change for undo.
    pub fn set_lyric_line(
        &mut self,
        line: u8,
        start: MeasureIndex,
        text: &str,
    ) -> Result<crate::edit::Change, ModelError> {
        if line >= LYRIC_LINES {
            return Err(ModelError::Invalid(format!(
                "there are only {LYRIC_LINES} lyric lines"
            )));
        }
        if text.chars().count() > MAX_LYRIC_LINE_CHARS {
            return Err(ModelError::Invalid(format!(
                "the lyric line is longer than {MAX_LYRIC_LINE_CHARS} characters"
            )));
        }
        if !text.is_empty() && start.get() >= self.measure_count() {
            return Err(ModelError::Invalid(format!("{start}: no such measure")));
        }
        let track = self.lyric_track();
        let mut lines = self.lyric_lines();
        lines.retain(|entry| entry.line != line);
        if !text.is_empty() {
            lines.push(LyricLine {
                line,
                start,
                text: text.to_owned(),
            });
        }
        lines.sort_by_key(|entry| entry.line);

        // Place the lines on the beats, as the beats will store them. When the beats
        // already hold syllables (GP6/GP7, or an earlier edit), only the edited line is
        // placed again, so the other lines keep their beats.
        let only_this_line = has_beat_syllables(self, track);
        let count = self.measure_count();
        let mut placed: Vec<(u8, MeasureIndex, BeatIndex, String)> = Vec::new();
        for entry in lines
            .iter()
            .filter(|entry| !only_this_line || entry.line == line)
        {
            for (measure, beat, text) in
                place_tokens(self, track, entry.start, lyric_tokens(&entry.text), count)
            {
                placed.push((entry.line, measure, beat, text));
            }
        }
        let previous_track = self.inner().lyrics.track_choice;
        let previous = self
            .inner()
            .lyrics
            .lines
            .iter()
            .find(|(number, _, _)| *number == line)
            .map(|(_, measure, text)| (*measure, text.clone()));
        // Everything whose beat lyrics this edit may change, to put back on undo.
        let mut affected: BTreeSet<(usize, usize)> = placed
            .iter()
            .map(|(_, measure, beat, _)| (measure.get(), beat.get()))
            .collect();
        for (measure, beat, lyrics) in beat_lyrics(self, track) {
            if lyrics.iter().any(|text| !text.is_empty()) {
                affected.insert((measure.get(), beat.get()));
            }
        }
        let previous_beats: Vec<(BeatPosition, Vec<String>)> = affected
            .into_iter()
            .filter_map(|(measure, beat)| {
                let at = BeatPosition {
                    staff: crate::StaffIndex::new(0),
                    track,
                    measure: MeasureIndex::new(measure),
                    voice: VoiceIndex::new(0),
                    beat: BeatIndex::new(beat),
                };
                Some((at, self.beat(at)?.lyrics().to_vec()))
            })
            .collect();

        let text = text.to_owned();
        let start_measure = u16::try_from(start.get().saturating_add(1)).unwrap_or(u16::MAX);
        let track_choice = u8::try_from(track.get()).unwrap_or(0);
        let inner = self.inner_mut();
        set_line(&mut inner.lyrics, line, start_measure, &text);
        inner.lyrics.track_choice = track_choice;
        let Some(gp_track) = inner.tracks.get_mut(track.get()) else {
            return Err(ModelError::Invalid(format!("the song has no {track}")));
        };
        let edited = usize::from(line);
        for measure in &mut gp_track.measures {
            for voice in &mut measure.voices {
                for beat in &mut voice.beats {
                    if only_this_line {
                        if let Some(slot) = beat.lyrics.get_mut(edited) {
                            slot.clear();
                        }
                        while beat.lyrics.last().is_some_and(String::is_empty) {
                            beat.lyrics.pop();
                        }
                    } else {
                        beat.lyrics.clear();
                    }
                }
            }
        }
        for (line, measure, beat, text) in placed {
            let Some(beat) = gp_track
                .measures
                .get_mut(measure.get())
                .and_then(|measure| measure.voices.first_mut())
                .and_then(|voice| voice.beats.get_mut(beat.get()))
            else {
                continue;
            };
            let line = usize::from(line);
            if beat.lyrics.len() <= line {
                beat.lyrics.resize(line.saturating_add(1), String::new());
            }
            if let Some(slot) = beat.lyrics.get_mut(line) {
                *slot = text;
            }
        }
        Ok(crate::edit::Change::lyric_line(
            line,
            previous,
            previous_track,
            previous_beats,
        ))
    }
}

/// Puts `text` on `line`, creating the line entry when it is missing.
pub(crate) fn set_line(lyrics: &mut GpLyrics, line: u8, start: u16, text: &str) {
    if let Some(entry) = lyrics
        .lines
        .iter_mut()
        .find(|(number, _, _)| *number == line)
    {
        entry.1 = start;
        entry.2 = text.to_owned();
        return;
    }
    if !text.is_empty() {
        lyrics.lines.push((line, start, text.to_owned()));
        lyrics.lines.sort_by_key(|(number, _, _)| *number);
    }
}

/// Restores a lyric line after undo, including the syllables on the beats.
pub(crate) fn restore_line(
    song: &mut Song,
    line: u8,
    previous: &Option<(u16, String)>,
    previous_track: u8,
    previous_beats: &[(BeatPosition, Vec<String>)],
) {
    // The edit (or several merged into one undo step) may have written syllables on any
    // beat of the lyric track: clear them all, then put back every beat that had lyrics.
    let track = song.lyric_track();
    let lyrics = song.inner_mut();
    if let Some(gp_track) = lyrics.tracks.get_mut(track.get()) {
        for measure in &mut gp_track.measures {
            for voice in &mut measure.voices {
                for beat in &mut voice.beats {
                    beat.lyrics.clear();
                }
            }
        }
    }
    lyrics.lyrics.track_choice = previous_track;
    match previous {
        Some((start, text)) => set_line(&mut lyrics.lyrics, line, *start, text),
        None => lyrics.lyrics.lines.retain(|(number, _, _)| *number != line),
    }
    for (at, texts) in previous_beats {
        let Some(beat) = lyrics
            .tracks
            .get_mut(at.track.get())
            .and_then(|track| track.measures.get_mut(at.measure.get()))
            .and_then(|measure| measure.voices.get_mut(at.voice.get()))
            .and_then(|voice| voice.beats.get_mut(at.beat.get()))
        else {
            continue;
        };
        beat.lyrics.clone_from(texts);
    }
}

/// Every beat of `track` with lyric texts (including empty lines), in measure order.
///
/// A track's lyrics are written on its first staff only (as Guitar Pro shows them), so
/// this and the edits above work on the track's own measures.
fn beat_lyrics(song: &Song, track: TrackIndex) -> Vec<(MeasureIndex, BeatIndex, Vec<String>)> {
    let mut found = Vec::new();
    let Some(view) = song.track(track) else {
        return found;
    };
    for measure in view.measures() {
        for voice in measure.voices() {
            for beat in voice.beats() {
                let lyrics = beat.lyrics();
                if !lyrics.is_empty() {
                    found.push((measure.index(), beat.index(), lyrics.to_vec()));
                }
            }
        }
    }
    found
}

/// Whether any beat of `track` holds a lyric syllable.
fn has_beat_syllables(song: &Song, track: TrackIndex) -> bool {
    beat_lyrics(song, track)
        .iter()
        .any(|(_, _, lyrics)| lyrics.iter().any(|text| !text.is_empty()))
}

/// Places `tokens` on the beats from `start` on, skipping rests and grace notes.
fn place_tokens(
    song: &Song,
    track: TrackIndex,
    start: MeasureIndex,
    tokens: Vec<String>,
    count: usize,
) -> Vec<(MeasureIndex, BeatIndex, String)> {
    let mut placed = Vec::new();
    if tokens.is_empty() {
        return placed;
    }
    let mut next = tokens.into_iter();
    for index in start.get().min(count)..count {
        let measure = MeasureIndex::new(index);
        let Some(view) = song.track(track).and_then(|track| track.measure(measure)) else {
            break;
        };
        let Some(voice) = view.voice(VoiceIndex::new(0)) else {
            continue;
        };
        for beat in voice.beats() {
            // Like Guitar Pro: no syllable on a rest, a grace note or a beat that only
            // continues tied notes.
            let tied =
                beat.notes().len() > 0 && beat.notes().all(|note| note.kind() == NoteKind::Tie);
            if beat.kind() == BeatKind::Empty
                || beat.kind() == BeatKind::Rest
                || beat.is_grace()
                || tied
            {
                continue;
            }
            let Some(token) = next.next() else {
                break;
            };
            // An empty syllable (a tab) leaves the beat without one.
            if !token.is_empty() {
                placed.push((measure, beat.index(), token));
            }
        }
        if next.len() == 0 {
            break;
        }
    }
    placed
}

/// The syllables of `text`: words split at whitespace, `[...]` hidden text left out and a
/// `+` joining the word it follows to the next one (on one beat, with a space between).
pub(crate) fn lyric_tokens(text: &str) -> Vec<String> {
    let mut visible = String::with_capacity(text.len());
    let mut depth = 0_u32;
    for character in text.chars() {
        match character {
            '[' => depth = depth.saturating_add(1),
            ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => visible.push(character),
            _ => {}
        }
    }
    let mut tokens = Vec::new();
    let mut current = String::new();
    // A tab skips a beat: an empty syllable. Other white space only separates.
    for word in visible.split_inclusive('\t').flat_map(|piece| {
        let skip = piece.ends_with('\t');
        piece
            .split_whitespace()
            .map(Some)
            .chain(skip.then_some(None))
    }) {
        let Some(word) = word else {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            tokens.push(String::new());
            continue;
        };
        // `a+b` and `a+ b` put both words on one beat, with a space between them.
        let join_next = word.ends_with('+');
        for (index, part) in word.split('+').filter(|part| !part.is_empty()).enumerate() {
            if index > 0 || !current.is_empty() {
                current.push(' ');
            }
            current.push_str(part);
        }
        if !join_next && !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use crate::Song;

    use super::*;

    fn song_of(count: usize) -> Song {
        let measures = (0..count)
            .map(|_| {
                crate::test_support::measure(
                    (4, 4),
                    vec![vec![
                        crate::test_support::beat(4, &[(1, 5)]),
                        crate::test_support::beat(4, &[(1, 5)]),
                        crate::test_support::beat(4, &[(1, 5)]),
                        crate::test_support::beat(4, &[(1, 5)]),
                    ]],
                )
            })
            .collect();
        crate::interop::from_gp(crate::test_support::song(vec![crate::test_support::track(
            "Guitar",
            crate::test_support::GUITAR_6,
            measures,
        )]))
        .unwrap()
    }

    #[test]
    fn syllables_are_split_placed_and_hidden_text_is_left_out() {
        assert_eq!(lyric_tokens("Hel+lo [hidden] world"), ["Hel lo", "world"]);
        assert_eq!(lyric_tokens("in+ the dark"), ["in the", "dark"]);
        assert_eq!(
            lyric_tokens("one\t\ttwo three"),
            ["one", "", "", "two", "three"]
        );
        assert_eq!(lyric_tokens("one \t two"), ["one", "", "two"]);
        assert_eq!(lyric_tokens("one  two\nthree"), ["one", "two", "three"]);
        assert_eq!(lyric_tokens("[only hidden]"), Vec::<String>::new());

        let mut song = song_of(2);
        assert_eq!(song.lyric_track(), TrackIndex::new(0));
        let _change = song
            .set_lyric_line(0, MeasureIndex::new(0), "one two three four five")
            .unwrap();
        let placed = song.lyric_syllables(TrackIndex::new(0));
        assert_eq!(placed.len(), 5);
        assert_eq!(
            placed
                .iter()
                .map(|syllable| {
                    (
                        syllable.measure.get(),
                        syllable.beat.get(),
                        syllable.text.as_str(),
                    )
                })
                .collect::<Vec<_>>(),
            [
                (0, 0, "one"),
                (0, 1, "two"),
                (0, 2, "three"),
                (0, 3, "four"),
                (1, 0, "five"),
            ]
        );
        // The other track has none.
        assert!(song.lyric_syllables(TrackIndex::new(1)).is_empty());
        // The syllables are on the beats for GP6/GP7 too.
        assert_eq!(
            song.beat(BeatPosition {
                staff: crate::StaffIndex::new(0),
                track: TrackIndex::new(0),
                measure: MeasureIndex::new(0),
                voice: VoiceIndex::new(0),
                beat: BeatIndex::new(1),
            })
            .unwrap()
            .lyrics(),
            ["two"]
        );
    }

    #[test]
    fn a_lyric_line_undoes_to_its_previous_text() {
        let mut song = song_of(2);
        let change = song
            .set_lyric_line(1, MeasureIndex::new(0), "first")
            .unwrap();
        assert_eq!(song.lyric_lines().len(), 1);
        song.revert(&change).unwrap();
        assert!(song.lyric_lines().is_empty());
        assert!(
            song.lyric_syllables(TrackIndex::new(0)).is_empty(),
            "the beats are cleared too"
        );
    }

    #[test]
    fn beats_and_gp7_files_keep_the_lyrics() {
        let mut song = song_of(1);
        let _change = song
            .set_lyric_line(0, MeasureIndex::new(0), "Hel lo")
            .unwrap();
        let _second = song
            .set_lyric_line(1, MeasureIndex::new(0), "second line")
            .unwrap();
        let before = song.lyric_syllables(TrackIndex::new(0));
        assert_eq!(before.len(), 4);
        // Writing and reading a GP7 file keeps the syllables on the beats.
        let bytes = song.inner().write_gp().unwrap();
        let mut reread = gscore_gp::Song::default();
        reread.read_gp(&bytes).unwrap();
        let after = crate::interop::from_gp(reread).unwrap();
        assert_eq!(after.lyric_syllables(TrackIndex::new(0)), before);
        assert_eq!(after.lyric_lines().len(), 2);
    }

    #[test]
    fn legacy_measure_lines_are_copied_to_the_beats_for_gp7() {
        // A GP3-5 song stores only the five lines, not syllables on the beats.
        let mut gp = crate::test_support::song(vec![crate::test_support::track(
            "Guitar",
            crate::test_support::GUITAR_6,
            vec![crate::test_support::measure(
                (4, 4),
                vec![vec![
                    crate::test_support::beat(4, &[(1, 5)]),
                    crate::test_support::beat(4, &[(1, 5)]),
                    crate::test_support::beat(4, &[(1, 5)]),
                    crate::test_support::beat(4, &[(1, 5)]),
                ]],
            )],
        )]);
        gp.lyrics.lines = vec![(0, 1, "one two".to_owned())];
        let mut song = crate::interop::from_gp(gp).unwrap();
        assert!(
            song.beat(BeatPosition {
                staff: crate::StaffIndex::new(0),
                track: TrackIndex::new(0),
                measure: MeasureIndex::new(0),
                voice: VoiceIndex::new(0),
                beat: BeatIndex::new(1),
            })
            .unwrap()
            .lyrics()
            .is_empty(),
            "nothing on the beats before the export"
        );
        song.synchronize_lyrics();
        assert_eq!(
            song.beat(BeatPosition {
                staff: crate::StaffIndex::new(0),
                track: TrackIndex::new(0),
                measure: MeasureIndex::new(0),
                voice: VoiceIndex::new(0),
                beat: BeatIndex::new(1),
            })
            .unwrap()
            .lyrics(),
            ["two"]
        );
    }

    /// Words of `song`'s lyric track as `(measure, beat, line, text)`.
    fn placed(song: &Song) -> Vec<(usize, usize, u8, String)> {
        song.lyric_syllables(song.lyric_track())
            .into_iter()
            .map(|syllable| {
                (
                    syllable.measure.get(),
                    syllable.beat.get(),
                    syllable.line,
                    syllable.text,
                )
            })
            .collect()
    }

    /// Two measures whose second beat of measure 0 continues a tied note, with GP7-style
    /// syllables already on beats 0 and 3 of measure 0 and beat 0 of measure 1 (beat 2 of
    /// measure 0 sounds but has none, as Guitar Pro allows).
    fn song_with_beat_syllables() -> Song {
        let mut tied = crate::test_support::beat(4, &[(1, 5)]);
        for note in &mut tied.notes {
            note.kind = gscore_gp::NoteType::Tie;
        }
        let mut first = vec![
            crate::test_support::beat(4, &[(1, 5)]),
            tied,
            crate::test_support::beat(4, &[(1, 5)]),
            crate::test_support::beat(4, &[(1, 5)]),
        ];
        first[0].lyrics = vec!["one".to_owned(), "uno".to_owned()];
        first[3].lyrics = vec!["two".to_owned()];
        let mut second = vec![
            crate::test_support::beat(4, &[(1, 5)]),
            crate::test_support::beat(4, &[(1, 5)]),
            crate::test_support::beat(4, &[(1, 5)]),
            crate::test_support::beat(4, &[(1, 5)]),
        ];
        second[0].lyrics = vec!["three".to_owned(), "dos".to_owned()];
        crate::interop::from_gp(crate::test_support::song(vec![crate::test_support::track(
            "Vocals",
            crate::test_support::GUITAR_6,
            vec![
                crate::test_support::measure((4, 4), vec![first]),
                crate::test_support::measure((4, 4), vec![second]),
            ],
        )]))
        .unwrap()
    }

    #[test]
    fn synchronizing_keeps_syllables_on_their_beats() {
        let mut song = song_with_beat_syllables();
        let before = placed(&song);
        song.synchronize_lyrics();
        assert_eq!(placed(&song), before);
        // The GP3-5 lines are filled in from the beats.
        assert_eq!(song.inner().lyrics.lines.len(), 2);
    }

    #[test]
    fn editing_one_line_leaves_the_other_lines_on_their_beats() {
        let mut song = song_with_beat_syllables();
        let _change = song
            .set_lyric_line(1, MeasureIndex::new(0), "a b c")
            .unwrap();
        let line_of = |song: &Song, line: u8| {
            placed(song)
                .into_iter()
                .filter(|entry| entry.2 == line)
                .map(|(measure, beat, _, text)| (measure, beat, text))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            line_of(&song, 0),
            [
                (0, 0, "one".to_owned()),
                (0, 3, "two".to_owned()),
                (1, 0, "three".to_owned())
            ]
        );
        // The edited line skips the tied beat, like Guitar Pro.
        assert_eq!(
            line_of(&song, 1),
            [
                (0, 0, "a".to_owned()),
                (0, 2, "b".to_owned()),
                (0, 3, "c".to_owned())
            ]
        );
        // Both lines are still listed.
        assert_eq!(song.lyric_lines().len(), 2);
    }

    #[test]
    fn a_tab_leaves_a_beat_without_a_syllable() {
        let mut song = song_of(1);
        let _change = song
            .set_lyric_line(0, MeasureIndex::new(0), "one\ttwo three")
            .unwrap();
        assert_eq!(
            placed(&song),
            [
                (0, 0, 0, "one".to_owned()),
                (0, 2, 0, "two".to_owned()),
                (0, 3, 0, "three".to_owned())
            ]
        );
    }

    #[test]
    fn undo_after_merged_edits_restores_every_beat() {
        // The lyrics dialog merges what is typed into one step: undo must also clear the
        // beats the later edits wrote, not only those of the first.
        let mut song = song_of(2);
        let first = song.set_lyric_line(0, MeasureIndex::new(0), "a").unwrap();
        let _later = song
            .set_lyric_line(0, MeasureIndex::new(0), "a b c d e f")
            .unwrap();
        song.revert(&first).unwrap();
        assert!(placed(&song).is_empty(), "{:?}", placed(&song));
        assert!(song.lyric_lines().is_empty());
    }
}
