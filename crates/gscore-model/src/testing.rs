//! Builders for varied songs in tests (feature `test-support`).
//!
//! The normal API only creates songs by opening a file. Tests in other crates (the command
//! layer, the CLI) also need empty, one-measure, multi-track and unusual-tuning scores without
//! a fixture file, so this module builds them from plain data. It is compiled only when the
//! `test-support` feature is enabled.

use gscore_gp::{Beat, BeatStatus, Measure, MeasureHeader, Song as GpSong, Track, Voice};

use crate::error::ModelError;
use crate::song::Song;

/// Standard tuning of a 6-string guitar, string 1 (highest) first.
pub const GUITAR_6: &[i8] = &[64, 59, 55, 50, 45, 40];
/// Standard tuning of a 7-string guitar.
pub const GUITAR_7: &[i8] = &[64, 59, 55, 50, 45, 40, 35];
/// Standard tuning of an 8-string guitar.
pub const GUITAR_8: &[i8] = &[64, 59, 55, 50, 45, 40, 35, 30];
/// Standard tuning of a 4-string bass.
pub const BASS_4: &[i8] = &[43, 38, 33, 28];

/// A song with no tracks and no measures.
pub fn empty() -> Song {
    Song::default()
}

/// A 4/4 song with one track per tuning and `measures` measures. Every measure has one voice
/// of four quarter-note rests, so it is exactly filled and ready for note entry.
pub fn song(tunings: &[&[i8]], measures: usize) -> Result<Song, ModelError> {
    let template = MeasureHeader::default();
    let time_signature = template.time_signature.clone();
    let key_signature = template.key_signature.clone();
    let measure_headers = vec![template; measures];

    let mut tracks = Vec::with_capacity(tunings.len());
    for (track_index, tuning) in tunings.iter().enumerate() {
        let strings = (1_i8..).zip(tuning.iter().copied()).collect();
        let measures = (0..measures)
            .map(|measure_index| Measure {
                track_index,
                header_index: measure_index,
                time_signature: time_signature.clone(),
                key_signature: key_signature.clone(),
                voices: vec![Voice {
                    beats: quarter_rests(),
                    ..Voice::default()
                }],
                ..Measure::default()
            })
            .collect();
        tracks.push(Track {
            name: format!("Track {}", track_index.saturating_add(1)),
            strings,
            measures,
            ..Track::default()
        });
    }

    Song::from_inner(GpSong {
        measure_headers,
        tracks,
        ..GpSong::default()
    })
}

/// A one-track, one-measure 4/4 guitar song.
pub fn one_measure() -> Result<Song, ModelError> {
    song(&[GUITAR_6], 1)
}

/// A one-measure 4/4 song on a 6-string guitar whose only beat (a whole note) holds `notes`
/// as a file stores them, `(string, fret)`, including values the model reports as issues
/// (negative frets, strings the guitar does not have).
pub fn one_beat_with_raw_notes(notes: &[(i8, i16)]) -> Result<Song, ModelError> {
    let song = song(&[GUITAR_6], 1)?;
    let mut inner = song.inner().clone();
    let beat = Beat {
        duration: gscore_gp::Duration {
            value: 1,
            ..gscore_gp::Duration::default()
        },
        notes: notes
            .iter()
            .map(|&(string, value)| gscore_gp::Note {
                string,
                value,
                // `Note::default()` is a rest note.
                kind: gscore_gp::NoteType::Normal,
                ..gscore_gp::Note::default()
            })
            .collect(),
        ..Beat::default()
    };
    for voice in inner
        .tracks
        .iter_mut()
        .flat_map(|t| t.measures.iter_mut())
        .flat_map(|m| m.voices.iter_mut())
    {
        voice.beats = vec![beat.clone()];
    }
    Song::from_inner(inner)
}

/// A 4/4 guitar song of `count` measures with repeat signs: each mark is
/// `(measure, repeat start, repeat end with this many repeats or -1, alternative endings
/// bit set)`. Several marks for one measure combine.
pub fn song_with_repeats(
    count: usize,
    marks: &[(usize, bool, i8, u8)],
) -> Result<Song, ModelError> {
    let song = song(&[GUITAR_6], count)?;
    let mut inner = song.inner().clone();
    for &(measure, start, end, endings) in marks {
        let header = inner.measure_headers.get_mut(measure).ok_or_else(|| {
            ModelError::Invalid(format!("no measure {measure} for a repeat mark"))
        })?;
        header.repeat_open |= start;
        if end >= 0 {
            header.repeat_close = end;
        }
        header.repeat_alternative |= endings;
    }
    Song::from_inner(inner)
}

/// Four quarter-note rests.
fn quarter_rests() -> Vec<Beat> {
    (0..4)
        .map(|_| Beat {
            duration: gscore_gp::Duration {
                value: 4,
                ..gscore_gp::Duration::default()
            },
            status: BeatStatus::Rest,
            ..Beat::default()
        })
        .collect()
}
