//! Comparing two songs: what a save-and-reload cycle (or a conversion) changed.
//!
//! Compares what the model exposes: song information, tracks (name, instrument, strings,
//! tuning), measure time signatures, and every beat (kind, length, notes). Placeholder beats
//! and empty voices are ignored, because formats differ in how many voice slots they store.
//! Not compared yet: effects and other data the model has no accessors for.

use std::fmt;

use crate::duration::Duration;
use crate::error::ModelError;
use crate::position::VoiceIndex;
use crate::song::Song;
use crate::views::{BeatKind, MeasureView, NoteKind, TrackView, VoiceView};

/// One difference between two songs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    /// Where: "song", "track 2", "track 1, measure 3, voice 1, beat 2", ...
    pub location: String,
    /// What differs, with both values.
    pub detail: String,
}

impl fmt::Display for Difference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.detail)
    }
}

/// Everything that differs between `before` and `after`, in song order. Empty if the
/// compared content is equal.
pub fn differences(before: &Song, after: &Song) -> Vec<Difference> {
    let mut diff = Diffs(Vec::new());
    let song = "song";
    diff.check(song, "title", before.title(), after.title());
    diff.check(song, "subtitle", before.subtitle(), after.subtitle());
    diff.check(song, "artist", before.artist(), after.artist());
    diff.check(song, "album", before.album(), after.album());
    diff.check(song, "lyricist", before.lyricist(), after.lyricist());
    diff.check(song, "composer", before.composer(), after.composer());
    diff.check(song, "copyright", before.copyright(), after.copyright());
    diff.check(song, "tab author", before.tab_author(), after.tab_author());
    diff.check(
        song,
        "instructions",
        before.instructions(),
        after.instructions(),
    );
    diff.check(
        song,
        "notes",
        before.notes().collect::<Vec<_>>(),
        after.notes().collect::<Vec<_>>(),
    );
    diff.check(song, "tempo", before.tempo(), after.tempo());
    diff.check(
        song,
        "measures",
        before.measure_count(),
        after.measure_count(),
    );
    diff.check(song, "tracks", before.track_count(), after.track_count());
    for (a, b) in before.tracks().zip(after.tracks()) {
        compare_track(&mut diff, &a, &b);
    }
    diff.0
}

/// Collects differences; `check` records one if the two values differ.
struct Diffs(Vec<Difference>);

impl Diffs {
    fn check<T: PartialEq + fmt::Debug>(&mut self, location: &str, what: &str, a: T, b: T) {
        if a != b {
            self.0.push(Difference {
                location: location.to_owned(),
                detail: format!("{what} {a:?} became {b:?}"),
            });
        }
    }
}

fn compare_track(diff: &mut Diffs, a: &TrackView<'_>, b: &TrackView<'_>) {
    let at = a.index().to_string();
    diff.check(&at, "name", a.name(), b.name());
    diff.check(&at, "instrument", a.instrument_kind(), b.instrument_kind());
    diff.check(
        &at,
        "instrument name",
        a.instrument_name(),
        b.instrument_name(),
    );
    diff.check(&at, "strings", a.string_count(), b.string_count());
    diff.check(&at, "tuning", a.tuning().ok(), b.tuning().ok());
    diff.check(
        &at,
        "tuning name",
        a.tuning_name().ok(),
        b.tuning_name().ok(),
    );
    diff.check(&at, "measures", a.measures().len(), b.measures().len());
    for (ma, mb) in a.measures().zip(b.measures()) {
        compare_measure(diff, &at, &ma, &mb);
    }
}

fn compare_measure(diff: &mut Diffs, track: &str, a: &MeasureView<'_>, b: &MeasureView<'_>) {
    let at = format!("{track}, {}", a.index());
    diff.check(
        &at,
        "time signature",
        a.time_signature().ok(),
        b.time_signature().ok(),
    );
    let count = a.voices().len().max(b.voices().len());
    for index in (0..count).map(VoiceIndex::new) {
        let beats_a = beats(a.voice(index));
        let beats_b = beats(b.voice(index));
        let at = format!("{at}, {index}");
        diff.check(&at, "beats", beats_a.len(), beats_b.len());
        for (number, (ba, bb)) in (1_usize..).zip(beats_a.iter().zip(&beats_b)) {
            diff.check(&format!("{at}, beat {number}"), "beat", ba, bb);
        }
    }
}

/// What is compared of a beat: kind, length, and notes as (string, fret, kind), by string.
#[derive(Debug, PartialEq)]
struct BeatContent {
    kind: BeatKind,
    duration: Result<Duration, ModelError>,
    notes: Vec<(Option<u8>, Option<u8>, NoteKind)>,
}

/// The non-placeholder beats of a voice; a missing voice has none.
fn beats(voice: Option<VoiceView<'_>>) -> Vec<BeatContent> {
    let Some(voice) = voice else {
        return Vec::new();
    };
    voice
        .beats()
        .filter(|b| b.kind() != BeatKind::Empty)
        .map(|beat| {
            let mut notes: Vec<_> = beat
                .notes()
                .map(|n| {
                    (
                        n.string().ok().map(|s| s.get()),
                        n.fret().ok().map(|f| f.get()),
                        n.kind(),
                    )
                })
                .collect();
            notes.sort_by_key(|&(string, _, _)| string);
            BeatContent {
                kind: beat.kind(),
                duration: beat.duration(),
                notes,
            }
        })
        .collect()
}
