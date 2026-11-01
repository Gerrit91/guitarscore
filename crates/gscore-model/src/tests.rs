//! Tests for the model: durations, validation, issues, navigation.

use crate::interop::from_gp;
use crate::test_support::*;
use crate::*;
use gscore_gp::Duration as GpDuration;

fn d(num: u64, den: u64) -> Duration {
    Duration::new(num, den).unwrap()
}

// --- Duration -----------------------------------------------------------------------------

#[test]
fn durations_are_exact_reduced_fractions() {
    assert_eq!(d(2, 8), d(1, 4));
    assert_eq!(d(2, 8).numerator(), 1);
    assert_eq!(d(2, 8).denominator(), 4);
    assert!(Duration::new(1, 0).is_err());
    assert!(d(1, 8) < d(1, 4));
    assert_eq!(d(1, 4).to_string(), "1/4");
}

#[test]
fn durations_from_note_values_dots_and_tuplets() {
    let cases = [
        (dur(1), d(1, 1)),
        (dur(4), d(1, 4)),
        (dur(128), d(1, 128)),
        (
            GpDuration {
                dotted: true,
                ..dur(8)
            },
            d(3, 16),
        ),
        (
            GpDuration {
                double_dotted: true,
                ..dur(4)
            },
            d(7, 16),
        ),
        // Triplet: 3 eighths in the time of 2.
        (
            GpDuration {
                tuplet_enters: 3,
                tuplet_times: 2,
                ..dur(8)
            },
            d(1, 12),
        ),
        // Quintuplet sixteenth: 5 in the time of 4.
        (
            GpDuration {
                tuplet_enters: 5,
                tuplet_times: 4,
                ..dur(16)
            },
            d(1, 20),
        ),
    ];
    for (gp, expected) in cases {
        assert_eq!(Duration::from_gp(&gp).unwrap(), expected, "{gp:?}");
    }
}

#[test]
fn three_triplet_eighths_make_exactly_a_quarter() {
    let triplet = Duration::from_gp(&GpDuration {
        tuplet_enters: 3,
        tuplet_times: 2,
        ..dur(8)
    })
    .unwrap();
    let sum = triplet
        .checked_add(triplet)
        .and_then(|s| s.checked_add(triplet))
        .unwrap();
    assert_eq!(sum, d(1, 4));
}

#[test]
fn invalid_durations_are_errors() {
    let bad = [
        dur(0),
        dur(3),
        dur(256),
        GpDuration {
            dotted: true,
            double_dotted: true,
            ..dur(4)
        },
        GpDuration {
            tuplet_enters: 0,
            ..dur(4)
        },
        GpDuration {
            tuplet_times: 0,
            ..dur(4)
        },
        GpDuration {
            tuplet_enters: 255,
            ..dur(4)
        },
    ];
    for gp in bad {
        assert!(Duration::from_gp(&gp).is_err(), "{gp:?}");
    }
}

#[test]
fn checked_add_reports_overflow() {
    let huge = d(u64::MAX, 1);
    assert_eq!(
        huge.checked_add(d(1, 1)),
        Err(ModelError::Overflow("a sum of durations"))
    );
}

// --- Pitch ------------------------------------------------------------------------------

#[test]
fn pitches_display_as_note_names() {
    let cases = [
        (0, "C-1"),
        (21, "A0"),
        (23, "B0"),
        (33, "A1"),
        (40, "E2"),
        (54, "F#3"),
        (60, "C4"),
        (64, "E4"),
        (127, "G9"),
    ];
    for (midi, name) in cases {
        assert_eq!(Pitch::new(midi).unwrap().to_string(), name);
    }
}

// --- TimeSignature ------------------------------------------------------------------------

#[test]
fn time_signatures_give_measure_lengths() {
    assert_eq!(TimeSignature::new(4, 4).unwrap().measure_length(), d(1, 1));
    assert_eq!(TimeSignature::new(3, 4).unwrap().measure_length(), d(3, 4));
    assert_eq!(TimeSignature::new(7, 8).unwrap().measure_length(), d(7, 8));
    assert_eq!(TimeSignature::new(6, 8).unwrap().to_string(), "6/8");
    for (num, den) in [(0, 4), (4, 0), (4, 3), (4, 256)] {
        assert!(TimeSignature::new(num, den).is_err(), "{num}/{den}");
    }
}

// --- Validation ---------------------------------------------------------------------------

#[test]
fn songs_for_several_instruments_load() {
    for (tuning, top_string) in [(BASS_4, 4), (GUITAR_6, 6), (GUITAR_7, 7), (GUITAR_8, 8)] {
        let gp = song(vec![track(
            "Instrument",
            tuning,
            vec![measure(
                (4, 4),
                vec![vec![beat(1, &[(1, 0), (top_string, 12)])]],
            )],
        )]);
        let song = from_gp(gp).unwrap();
        let track = song.track(TrackIndex::new(0)).unwrap();
        assert_eq!(track.string_count(), tuning.len());
        let pitches: Vec<i8> = track
            .tuning()
            .unwrap()
            .unwrap()
            .iter()
            .map(|t| i8::try_from(t.pitch.get()).unwrap())
            .collect();
        assert_eq!(pitches, tuning);
    }
}

#[test]
fn tracks_without_a_real_tuning_report_none() {
    let mut gp = song(vec![
        track("Singer", GUITAR_6, vec![measure((4, 4), vec![])]),
        track("Drums", &[0; 6], vec![measure((4, 4), vec![])]),
        track("Guitar", GUITAR_6, vec![measure((4, 4), vec![])]),
    ]);
    gp.tracks[0].has_tuning = false;
    let mut voice = track("Voice", GUITAR_6, vec![measure((4, 4), vec![])]);
    voice.instrument_type = Some("voice".to_owned());
    voice.instrument_name = Some("Voice".to_owned());
    gp.tracks.push(voice);
    let song = from_gp(gp).unwrap();
    let tunings: Vec<bool> = song
        .tracks()
        .map(|t| t.tuning().unwrap().is_some())
        .collect();
    assert_eq!(tunings, [false, false, true, false]);
    let voice = song.track(TrackIndex::new(3)).unwrap();
    assert_eq!(voice.instrument_kind(), InstrumentKind::Other);
    assert_eq!(voice.instrument_name(), Some("Voice"));
    // The strings are still there for placing notes.
    assert!(song.tracks().all(|t| t.string_count() == 6));
}

#[test]
fn empty_song_is_valid() {
    let song = from_gp(song(vec![])).unwrap();
    assert_eq!(song.track_count(), 0);
    assert_eq!(song.measure_count(), 0);
    assert!(song.issues().is_empty());
}

#[test]
fn invalid_durations_in_a_song_are_rejected_with_location() {
    let gp = one_measure(vec![beat(4, &[]), beat(3, &[(1, 0)])]);
    let err = from_gp(gp).unwrap_err().to_string();
    assert!(err.contains("track 1: measure 1: voice 1, beat 2"), "{err}");
    assert!(err.contains("1/3"), "{err}");
}

/// Note-level problems found in real files (drum and piano tracks mapped onto tab strings)
/// must not stop a song from opening; they are reported as issues and kept unchanged.
#[test]
fn odd_notes_open_and_are_reported_as_issues() {
    type Notes = &'static [(i8, i16)];
    type Check = fn(&IssueKind) -> bool;
    let cases: [(Notes, Check); 6] = [
        (&[(1, -5)], |k| matches!(k, IssueKind::InvalidNote(_))),
        (&[(1, 128)], |k| matches!(k, IssueKind::InvalidNote(_))),
        (&[(0, 3)], |k| matches!(k, IssueKind::InvalidNote(_))),
        (&[(-2, 3)], |k| matches!(k, IssueKind::InvalidNote(_))),
        (&[(7, 0)], |k| {
            matches!(k, IssueKind::NoteOnMissingString { string, string_count: 6 }
                if string.get() == 7)
        }),
        (
            &[(1, 35), (1, 93), (1, 38)],
            |k| matches!(k, IssueKind::DuplicateString { string } if string.get() == 1),
        ),
    ];
    for (notes, expected) in cases {
        let song = from_gp(one_measure(vec![beat(1, notes)])).unwrap();
        let issues = song.issues();
        assert_eq!(issues.len(), 1, "{notes:?}: {issues:?}");
        assert!(expected(&issues[0].kind), "{notes:?}: {issues:?}");
        assert_eq!(issues[0].beat, Some(BeatIndex::new(0)));
        // The note itself is kept.
        let beat = song.beat(BeatPosition {
            track: TrackIndex::new(0),
            measure: MeasureIndex::new(0),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(0),
        });
        assert_eq!(beat.unwrap().notes().len(), notes.len());
    }
}

#[test]
fn issues_display_their_location() {
    let song = from_gp(one_measure(vec![beat(2, &[(2, 3), (2, 5), (3, -5)])])).unwrap();
    let text: Vec<String> = song.issues().iter().map(ToString::to_string).collect();
    assert_eq!(
        text,
        [
            "track 1, measure 1, voice 1: measure not filled (1/2 of 1/1)",
            "track 1, measure 1, voice 1, beat 1: several notes on string 2",
            "track 1, measure 1, voice 1, beat 1: fret -5",
        ]
    );
}

#[test]
fn bad_tracks_and_songs_are_rejected() {
    // Measure count differs from the song's.
    let mut gp = one_measure(vec![beat(1, &[])]);
    gp.measure_headers.push(Default::default());
    assert!(from_gp(gp).is_err());

    // No strings, and too many strings.
    for tuning in [&[][..], &[40; 13][..]] {
        let gp = song(vec![track("T", tuning, vec![measure((4, 4), vec![])])]);
        assert!(from_gp(gp).is_err(), "{} strings", tuning.len());
    }

    // Strings not numbered 1..=n.
    let mut gp = one_measure(vec![beat(1, &[])]);
    gp.tracks[0].strings[1].0 = 7;
    assert!(from_gp(gp).is_err());

    // Non-positive tempo and invalid time signature.
    let mut gp = one_measure(vec![beat(1, &[])]);
    gp.tempo = 0;
    assert!(from_gp(gp).is_err());
    let gp = song(vec![track("T", GUITAR_6, vec![measure((0, 4), vec![])])]);
    assert!(from_gp(gp).is_err());
}

// --- Issues (measure fill) ----------------------------------------------------------------

fn issues_of(time: (i8, u16), voices: Vec<Vec<gscore_gp::Beat>>) -> Vec<Issue> {
    let gp = song(vec![track("T", GUITAR_6, vec![measure(time, voices)])]);
    from_gp(gp).unwrap().issues()
}

#[test]
fn filled_measures_have_no_issues() {
    let triplet = GpDuration {
        tuplet_enters: 3,
        tuplet_times: 2,
        ..dur(8)
    };
    let full_measures = [
        (
            (4, 4),
            vec![beat(4, &[(1, 0)]), rest(4), beat(2, &[(2, 3)])],
        ),
        ((3, 4), vec![beat(4, &[]), beat(4, &[]), beat(4, &[])]),
        ((7, 8), (0..7).map(|_| beat(8, &[])).collect()),
        (
            (2, 4),
            vec![
                beat_with(triplet.clone(), &[]),
                beat_with(triplet.clone(), &[]),
                beat_with(triplet, &[]),
                beat(4, &[]),
            ],
        ),
    ];
    for (time, beats) in full_measures {
        assert_eq!(issues_of(time, vec![beats]), [], "{time:?}");
    }
}

#[test]
fn unfilled_and_overfilled_measures_are_reported() {
    let under = issues_of((3, 4), vec![vec![beat(4, &[]), beat(4, &[])]]);
    assert_eq!(
        under,
        [Issue {
            track: TrackIndex::new(0),
            measure: MeasureIndex::new(0),
            voice: VoiceIndex::new(0),
            beat: None,
            kind: IssueKind::Underfull {
                expected: d(3, 4),
                actual: d(1, 2)
            },
        }]
    );
    let over = issues_of((4, 4), vec![vec![beat(1, &[]), beat(8, &[])]]);
    assert!(matches!(
        over.as_slice(),
        [Issue {
            kind: IssueKind::Overfull { .. },
            ..
        }]
    ));
}

#[test]
fn empty_voices_are_not_issues() {
    let issues = issues_of((4, 4), vec![vec![beat(1, &[])], vec![empty_beat()], vec![]]);
    assert_eq!(issues, []);
}

// --- Navigation ---------------------------------------------------------------------------

#[test]
fn beats_are_found_by_position() {
    let gp = song(vec![
        track(
            "Lead",
            GUITAR_6,
            vec![measure((4, 4), vec![vec![beat(1, &[])]])],
        ),
        track(
            "Bass",
            BASS_4,
            vec![measure(
                (4, 4),
                vec![vec![beat(2, &[]), beat(2, &[(4, 3)])]],
            )],
        ),
    ]);
    let song = from_gp(gp).unwrap();
    let pos = BeatPosition {
        track: TrackIndex::new(1),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(1),
    };
    let beat = song.beat(pos).unwrap();
    let note = beat.notes().next().unwrap();
    assert_eq!(note.string().unwrap(), StringNumber::new(4).unwrap());
    assert_eq!(note.fret().unwrap(), Fret::new(3).unwrap());
    assert_eq!(note.kind(), NoteKind::Normal);
    assert_eq!(beat.duration().unwrap(), d(1, 2));
    assert_eq!(pos.to_string(), "track 2, measure 1, voice 1, beat 2");

    for out_of_range in [
        BeatPosition {
            track: TrackIndex::new(2),
            ..pos
        },
        BeatPosition {
            measure: MeasureIndex::new(1),
            ..pos
        },
        BeatPosition {
            voice: VoiceIndex::new(1),
            ..pos
        },
        BeatPosition {
            beat: BeatIndex::new(2),
            ..pos
        },
    ] {
        assert!(song.beat(out_of_range).is_none(), "{out_of_range}");
    }
}

#[test]
fn unicode_metadata_is_kept() {
    let mut gp = one_measure(vec![beat(1, &[])]);
    gp.name = "Ünïcödé — 曲 🎸".to_owned();
    gp.tracks[0].name = "Gitarre ä".to_owned();
    let song = from_gp(gp).unwrap();
    assert_eq!(song.title(), "Ünïcödé — 曲 🎸");
    assert_eq!(song.tracks().next().unwrap().name(), "Gitarre ä");
    assert_eq!(song.tempo(), 120);
}

#[test]
fn unknown_fields_survive_wrapping() {
    let mut gp = one_measure(vec![beat(1, &[])]);
    gp.copyright = "© owner".to_owned();
    gp.tracks[0].color = 0x00ff00;
    let song = from_gp(gp).unwrap();
    let inner = interop::as_gp(&song);
    assert_eq!(inner.copyright, "© owner");
    assert_eq!(inner.tracks[0].color, 0x00ff00);
}
