//! Builders for varied songs in tests (feature `test-support`).
//!
//! The normal API only creates songs by opening a file. Tests in other crates (the command
//! layer, the CLI) also need empty, one-measure, multi-track and unusual-tuning scores without
//! a fixture file, so this module builds them from plain data. It is compiled only when the
//! `test-support` feature is enabled.

use gscore_gp::{
    Beat, BeatStatus, BendEffect, BendPoint, BendType, Duration as GpDuration, GraceEffect,
    GraceEffectTransition, Measure, MeasureHeader, Note as GpNote, NoteEffect, NoteType, SlideType,
    Song as GpSong, Track, Voice,
};

use crate::effects::SlideKind;
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

/// A one-measure song with a GP3-5 style grace note: a normal note on string 1 fret 3 with
/// a grace note at fret 5 attached to it.
pub fn song_with_attached_grace() -> Result<Song, ModelError> {
    let song = song(&[GUITAR_6], 1)?;
    let mut inner = song.inner().clone();
    let voice = inner
        .tracks
        .first_mut()
        .and_then(|track| track.measures.first_mut())
        .and_then(|measure| measure.voices.first_mut())
        .ok_or_else(|| ModelError::Invalid("the test song has no voice".to_owned()))?;
    if let Some(first) = voice.beats.first_mut() {
        let mut note = GpNote {
            value: 3,
            string: 1,
            kind: NoteType::Normal,
            velocity: 90,
            ..GpNote::default()
        };
        note.effect.grace = Some(GraceEffect {
            fret: 5,
            velocity: 90,
            duration: 8,
            is_dead: false,
            is_on_beat: false,
            transition: GraceEffectTransition::None,
        });
        first.notes = vec![note];
        first.status = BeatStatus::Normal;
    }
    Song::from_inner(inner)
}

/// A one-measure song with a GP6/GP7 grace beat (a 32nd) before its first note. The grace
/// note is string 1 fret 5; the main note is string 1 fret 3.
pub fn song_with_grace_beat() -> Result<Song, ModelError> {
    let song = song(&[GUITAR_6], 1)?;
    let mut inner = song.inner().clone();
    let mut grace_note = GpNote {
        value: 5,
        string: 1,
        kind: NoteType::Normal,
        velocity: 90,
        ..GpNote::default()
    };
    grace_note.effect.grace = Some(GraceEffect {
        fret: 5,
        velocity: 90,
        duration: 1,
        is_dead: false,
        is_on_beat: false,
        transition: GraceEffectTransition::None,
    });
    let grace = Beat {
        grace: true,
        status: BeatStatus::Normal,
        duration: GpDuration {
            value: 32,
            ..GpDuration::default()
        },
        notes: vec![grace_note],
        ..Beat::default()
    };
    let voice = inner
        .tracks
        .first_mut()
        .and_then(|track| track.measures.first_mut())
        .and_then(|measure| measure.voices.first_mut())
        .ok_or_else(|| ModelError::Invalid("the test song has no voice".to_owned()))?;
    if let Some(first) = voice.beats.first_mut() {
        first.notes = vec![GpNote {
            value: 3,
            string: 1,
            kind: NoteType::Normal,
            velocity: 90,
            ..GpNote::default()
        }];
        first.status = BeatStatus::Normal;
    }
    voice.beats.insert(0, grace);
    Song::from_inner(inner)
}

/// One quarter of [`song_with_graces`]: its grace notes and its notes, `(string, fret)`.
pub type GraceQuarter<'a> = (&'a [(u8, u8)], &'a [(u8, u8)]);

/// A one-measure 4/4 song on a 6-string guitar of quarter notes, each `(graces, notes)`:
/// a GP6/GP7 grace beat (a 32nd) holding `graces` (skipped when empty) before a quarter
/// holding `notes`. Notes are `(string, fret)`. Up to four quarters; the rest stay rests.
pub fn song_with_graces(quarters: &[GraceQuarter<'_>]) -> Result<Song, ModelError> {
    let song = song(&[GUITAR_6], 1)?;
    let mut inner = song.inner().clone();
    let note = |&(string, fret): &(u8, u8)| GpNote {
        value: i16::from(fret),
        string: i8::try_from(string).unwrap_or(1),
        kind: NoteType::Normal,
        velocity: 90,
        ..GpNote::default()
    };
    let mut beats = Vec::new();
    for (graces, notes) in quarters.iter().take(4) {
        if !graces.is_empty() {
            beats.push(Beat {
                grace: true,
                status: BeatStatus::Normal,
                duration: GpDuration {
                    value: 32,
                    ..GpDuration::default()
                },
                notes: graces
                    .iter()
                    .map(|grace| {
                        let mut grace_note = note(grace);
                        grace_note.effect.grace = Some(GraceEffect {
                            fret: i8::try_from(grace.1).unwrap_or(0),
                            velocity: 90,
                            duration: 1,
                            is_dead: false,
                            is_on_beat: false,
                            transition: GraceEffectTransition::None,
                        });
                        grace_note
                    })
                    .collect(),
                ..Beat::default()
            });
        }
        beats.push(Beat {
            status: BeatStatus::Normal,
            duration: GpDuration {
                value: 4,
                ..GpDuration::default()
            },
            notes: notes.iter().map(note).collect(),
            ..Beat::default()
        });
    }
    beats.extend(quarter_rests().into_iter().skip(quarters.len().min(4)));
    if let Some(voice) = inner
        .tracks
        .first_mut()
        .and_then(|track| track.measures.first_mut())
        .and_then(|measure| measure.voices.first_mut())
    {
        voice.beats = beats;
    }
    Song::from_inner(inner)
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
                kind: NoteType::Normal,
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

/// A bend for [`song_with_notes`]: how far it reaches and whether it releases back down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestBend {
    /// How far the bend reaches, in quarter tones (a whole step is 4).
    pub quarter_tones: i16,
    /// True for a release back to the unbent pitch.
    pub release: bool,
}

/// A note for [`song_with_notes`]: a valid string and fret, plus the effects playback reads.
#[derive(Debug, Clone, Copy)]
pub struct TestNote<'a> {
    /// String, 1-based (1 = highest).
    pub string: u8,
    /// Fret.
    pub fret: u8,
    /// A bend, if any.
    pub bend: Option<TestBend>,
    /// Slides, in file order.
    pub slides: &'a [SlideKind],
}

impl<'a> TestNote<'a> {
    /// A plain note.
    pub fn new(string: u8, fret: u8) -> Self {
        TestNote {
            string,
            fret,
            bend: None,
            slides: &[],
        }
    }

    /// The same note with an upward bend.
    pub fn with_bend(mut self, quarter_tones: i16) -> Self {
        self.bend = Some(TestBend {
            quarter_tones,
            release: false,
        });
        self
    }

    /// The same note with a bend that releases back down.
    pub fn with_release_bend(mut self, quarter_tones: i16) -> Self {
        self.bend = Some(TestBend {
            quarter_tones,
            release: true,
        });
        self
    }

    /// The same note with slides.
    pub fn with_slides(mut self, slides: &'a [SlideKind]) -> Self {
        self.slides = slides;
        self
    }
}

/// A one-measure 4/4 song whose first voice has the given beats, each
/// `(note value, notes)`. The note values need not fill the measure; the model reports an
/// under- or overfilled measure as an issue, which playback ignores.
pub fn song_with_notes(beats: &[(u16, &[TestNote<'_>])]) -> Result<Song, ModelError> {
    song_with_notes_on_tracks(1, beats)
}

/// The same song on `track_count` six-string guitar tracks, all with the same beats. Used to
/// fill every MIDI channel when testing how playback degrades.
pub fn song_with_notes_on_tracks(
    track_count: usize,
    beats: &[(u16, &[TestNote<'_>])],
) -> Result<Song, ModelError> {
    let tunings: Vec<&[i8]> = (0..track_count).map(|_| GUITAR_6).collect();
    let song = song(&tunings, 1)?;
    let mut inner = song.inner().clone();
    let beats = beats
        .iter()
        .map(|&(value, notes)| {
            let notes = notes
                .iter()
                .map(note_for_playback)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Beat {
                duration: gscore_gp::Duration {
                    value,
                    ..gscore_gp::Duration::default()
                },
                status: BeatStatus::Normal,
                notes,
                ..Beat::default()
            })
        })
        .collect::<Result<Vec<_>, ModelError>>()?;
    for voice in inner
        .tracks
        .iter_mut()
        .flat_map(|track| track.measures.iter_mut())
        .flat_map(|measure| measure.voices.iter_mut())
    {
        voice.beats = beats.clone();
    }
    Song::from_inner(inner)
}

/// The Guitar Pro note [`song_with_notes`] builds for one [`TestNote`].
fn note_for_playback(note: &TestNote<'_>) -> Result<GpNote, ModelError> {
    let string = i8::try_from(note.string)
        .map_err(|_| ModelError::Invalid(format!("test note on string {}", note.string)))?;
    let mut effect = NoteEffect::default();
    if let Some(bend) = note.bend {
        let value = i8::try_from(bend.quarter_tones).map_err(|_| {
            ModelError::Invalid(format!("test bend of {} quarter tones", bend.quarter_tones))
        })?;
        effect.bend = Some(BendEffect {
            kind: if bend.release {
                BendType::ReleaseDown
            } else {
                BendType::Bend
            },
            // As Guitar Pro draws them: a bend is up by the middle and held, a release
            // starts bent and comes back down.
            points: if bend.release {
                vec![(0, value), (12, 0)]
            } else {
                vec![(0, 0), (6, value), (12, value)]
            }
            .into_iter()
            .map(|(position, value)| BendPoint {
                position,
                value,
                ..BendPoint::default()
            })
            .collect(),
            ..BendEffect::default()
        });
    }
    effect.slides = note.slides.iter().copied().map(slide_type).collect();
    Ok(GpNote {
        string,
        value: i16::from(note.fret),
        kind: NoteType::Normal,
        effect,
        ..GpNote::default()
    })
}

/// This crate's slide kind as the Guitar Pro data stores it.
fn slide_type(kind: SlideKind) -> SlideType {
    match kind {
        SlideKind::IntoFromAbove => SlideType::IntoFromAbove,
        SlideKind::IntoFromBelow => SlideType::IntoFromBelow,
        SlideKind::ShiftSlideTo => SlideType::ShiftSlideTo,
        SlideKind::LegatoSlideTo => SlideType::LegatoSlideTo,
        SlideKind::OutDownwards => SlideType::OutDownwards,
        SlideKind::OutUpwards => SlideType::OutUpWards,
        SlideKind::PickSlideDown => SlideType::PickSlideDown,
        SlideKind::PickSlideUp => SlideType::PickSlideUp,
    }
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

/// A 4/4 guitar song of `count` measures with navigation signs at the given measures.
pub fn song_with_directions(
    count: usize,
    marks: &[(usize, crate::marks::Direction)],
) -> Result<Song, ModelError> {
    let song = song(&[GUITAR_6], count)?;
    let mut inner = song.inner().clone();
    for &(measure, direction) in marks {
        let header = inner
            .measure_headers
            .get_mut(measure)
            .ok_or_else(|| ModelError::Invalid(format!("no measure {measure} for a direction")))?;
        header.direction = Some(direction.to_gp());
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
