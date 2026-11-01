//! Builders for Guitar Pro song data in tests.

use gscore_gp::Duration as GpDuration;
use gscore_gp::{
    Beat, BeatStatus, Measure, MeasureHeader, Note, NoteType, Song, TimeSignature, Track, Voice,
};

/// Standard tunings (MIDI pitches, string 1 first) for several instruments.
pub(crate) const BASS_4: &[i8] = &[43, 38, 33, 28];
pub(crate) const GUITAR_6: &[i8] = &[64, 59, 55, 50, 45, 40];
pub(crate) const GUITAR_7: &[i8] = &[64, 59, 55, 50, 45, 40, 35];
pub(crate) const GUITAR_8: &[i8] = &[64, 59, 55, 50, 45, 40, 35, 30];

/// A duration of note value `1/value`.
pub(crate) fn dur(value: u16) -> GpDuration {
    GpDuration {
        value,
        ..GpDuration::default()
    }
}

/// A beat of note value `1/value` with notes as `(string, fret)`.
pub(crate) fn beat(value: u16, notes: &[(i8, i16)]) -> Beat {
    beat_with(dur(value), notes)
}

/// A beat with the given duration and notes as `(string, fret)`.
pub(crate) fn beat_with(duration: GpDuration, notes: &[(i8, i16)]) -> Beat {
    Beat {
        duration,
        notes: notes
            .iter()
            .map(|&(string, value)| Note {
                string,
                value,
                // `Note::default()` is a rest note.
                kind: NoteType::Normal,
                ..Note::default()
            })
            .collect(),
        ..Beat::default()
    }
}

/// A rest of note value `1/value`.
pub(crate) fn rest(value: u16) -> Beat {
    Beat {
        status: BeatStatus::Rest,
        ..beat(value, &[])
    }
}

/// A placeholder beat, as Guitar Pro stores in unused voices.
pub(crate) fn empty_beat() -> Beat {
    Beat {
        status: BeatStatus::Empty,
        ..beat(4, &[])
    }
}

/// A measure in `numerator/denominator` time with the given voices.
pub(crate) fn measure(time: (i8, u16), voices: Vec<Vec<Beat>>) -> Measure {
    Measure {
        time_signature: TimeSignature {
            numerator: time.0,
            denominator: dur(time.1),
            ..TimeSignature::default()
        },
        voices: voices
            .into_iter()
            .map(|beats| Voice {
                beats,
                ..Voice::default()
            })
            .collect(),
        ..Measure::default()
    }
}

/// A track with the given tuning (string 1 first) and measures.
pub(crate) fn track(name: &str, tuning: &[i8], measures: Vec<Measure>) -> Track {
    Track {
        name: name.to_owned(),
        strings: (1_i8..).zip(tuning.iter().copied()).collect(),
        measures,
        ..Track::default()
    }
}

/// A song with the given tracks and one measure header per measure of the first track.
pub(crate) fn song(tracks: Vec<Track>) -> Song {
    let measure_count = tracks.first().map_or(0, |t| t.measures.len());
    Song {
        name: "Test song".to_owned(),
        measure_headers: vec![MeasureHeader::default(); measure_count],
        tracks,
        ..Song::default()
    }
}

/// A one-track, one-measure 4/4 song on a 6-string guitar holding `beats`.
pub(crate) fn one_measure(beats: Vec<Beat>) -> Song {
    song(vec![track(
        "Guitar",
        GUITAR_6,
        vec![measure((4, 4), vec![beats])],
    )])
}
