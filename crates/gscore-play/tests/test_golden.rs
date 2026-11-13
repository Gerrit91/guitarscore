//! The MIDI events playback sends, for the owner's test files and a small song per
//! effect, compared with the reference files in `tests/golden/`. They pin down what the
//! SoundFont and MIDI path plays, so a change in how sequences are built (M22: the
//! engine-neutral performance the MIDI is derived from) cannot change the sound unnoticed.
//!
//! If the output changes on purpose, regenerate the references with
//! `GSCORE_UPDATE_GOLDEN=1 cargo test -p gscore-play --test test_golden`, read the diff and
//! name the audible change in the commit message.
//!
//! Only files made for this repository go in here (`testdata/`, synthetic songs): never
//! songs of other bands, which are under copyright.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::fmt::Write as _;
use std::path::PathBuf;

use gscore_model::testing::{self, TestNote};
use gscore_model::{
    AccentKind, BeatEffect, BeatIndex, BeatPosition, BendKind, BendPoint, BendShape, Fret, Grace,
    GraceSpeed, Harmonic, HarmonicKind, Legato, MeasureIndex, NoteEffect, SlapKind, SlideKind,
    Song, StringNumber, Stroke, StrokeDirection, TrackIndex, TremoloSpeed, Trill, TrillSpeed,
    Vibrato, VoiceIndex, WhammyShape,
};
use gscore_play::{Click, MidiEvent, Sequence, count_in, lead_in};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn open(relative: &str) -> Song {
    let bytes = std::fs::read(crate_dir().join("../../testdata").join(relative)).unwrap();
    gscore_io::import_guitar_pro_from_bytes(&bytes)
        .unwrap()
        .song
}

/// Compares `text` with `tests/golden/<name>.txt`, or writes it when updating.
fn check(name: &str, text: &str) {
    let path = crate_dir().join("tests/golden").join(format!("{name}.txt"));
    if std::env::var_os("GSCORE_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}; create it with GSCORE_UPDATE_GOLDEN=1",
            path.display()
        )
    });
    if expected != text {
        let first = expected
            .lines()
            .zip(text.lines())
            .position(|(a, b)| a != b)
            .map_or_else(|| "length".to_owned(), |line| format!("line {}", line + 1));
        panic!(
            "{name}: the events differ from {} (first difference: {first}). If the change \
             is intended, regenerate with GSCORE_UPDATE_GOLDEN=1 and review the diff.",
            path.display()
        );
    }
}

/// One line per event, after a header with the tracks, tempo, measures and counters.
fn dump(sequence: &Sequence, out: &mut String) {
    for track in &sequence.tracks {
        let sound = track.sound;
        writeln!(
            out,
            "track {} ch{} program {} volume {} pan {} reverb {} chorus {} percussion {}",
            track.track.get(),
            track.channel.get(),
            sound.program,
            sound.volume,
            sound.pan,
            sound.reverb,
            sound.chorus,
            sound.percussion
        )
        .unwrap();
    }
    writeln!(
        out,
        "end {} skipped {} degraded {}",
        sequence.end, sequence.skipped_notes, sequence.degraded_pitch_bends
    )
    .unwrap();
    for &(tick, micros) in sequence.tempo.points() {
        writeln!(out, "tempo {tick} {micros}").unwrap();
    }
    for &(tick, micros) in sequence.tempo.pauses() {
        writeln!(out, "pause {tick} {micros}").unwrap();
    }
    for span in &sequence.measures {
        writeln!(
            out,
            "measure {} {}..{} pause {}",
            span.measure.get(),
            span.start,
            span.end,
            span.pause
        )
        .unwrap();
    }
    for event in &sequence.events {
        let (tick, track) = (event.tick, event.track);
        let line = match event.event {
            MidiEvent::NoteOn {
                channel,
                key,
                velocity,
            } => format!("on ch{} {key} {velocity}", channel.get()),
            MidiEvent::NoteOff { channel, key } => format!("off ch{} {key}", channel.get()),
            MidiEvent::ProgramChange { channel, program } => {
                format!("program ch{} {program}", channel.get())
            }
            MidiEvent::Control {
                channel,
                controller,
                value,
            } => format!("cc ch{} {controller} {value}", channel.get()),
            MidiEvent::PitchBend { channel, value } => {
                format!("bend ch{} {value}", channel.get())
            }
        };
        writeln!(out, "{tick} t{track} {line}").unwrap();
    }
}

fn sequence_text(song: &Song) -> String {
    let mut out = String::new();
    dump(&Sequence::new(song, None).unwrap(), &mut out);
    out
}

/// Every beat of every track and voice, played alone as the palette's preview plays it.
fn excerpts_text(song: &Song) -> String {
    let mut out = String::new();
    for track in 0..song.track_count() {
        let view = song.track(TrackIndex::new(track)).unwrap();
        for measure in 0..song.measure_count() {
            let Some(bar) = view.measure(MeasureIndex::new(measure)) else {
                continue;
            };
            for voice in bar.voices() {
                for beat in 0..voice.beats().count() {
                    let at = BeatPosition {
                        staff: gscore_model::StaffIndex::new(0),
                        track: TrackIndex::new(track),
                        measure: MeasureIndex::new(measure),
                        voice: voice.index(),
                        beat: BeatIndex::new(beat),
                    };
                    let Some(excerpt) = Sequence::beat_excerpt(song, at).unwrap() else {
                        continue;
                    };
                    writeln!(
                        out,
                        "## excerpt track {track} measure {measure} voice {} beat {beat}",
                        voice.index().get()
                    )
                    .unwrap();
                    dump(&excerpt, &mut out);
                }
            }
        }
    }
    out
}

const OWNER_FILES: [&str; 6] = [
    "steel_guitar_dadgad",
    "note_lengths",
    "ornaments",
    "advanced_ornaments",
    "staff",
    "multi_staff",
];

#[test]
fn owner_files_play_as_their_golden_events() {
    for name in OWNER_FILES {
        let gp = open(&format!("basic/{name}.gp"));
        check(&format!("{name}.gp"), &sequence_text(&gp));
        check(&format!("{name}.excerpts"), &excerpts_text(&gp));
        for format in ["gpx", "gp5"] {
            let path = format!("basic/{name}.{format}");
            if crate_dir().join("../../testdata").join(&path).exists() {
                check(&format!("{name}.{format}"), &sequence_text(&open(&path)));
            }
        }
    }
}

#[test]
fn metronome_and_count_in_play_as_their_golden_events() {
    let song = open("basic/ornaments.gp");
    let mut out = String::new();
    let click = Click::default();
    let with_click = Sequence::with_click(&song, None, Some(click)).unwrap();
    dump(&with_click, &mut out);
    let at = BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(2),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(0),
    };
    let clicks = count_in(&song, at, 2, click).unwrap();
    out.push_str("## count-in\n");
    dump(&clicks, &mut out);
    let start = with_click.tick_of(&song, at).unwrap();
    out.push_str("## lead-in\n");
    dump(&lead_in(&with_click, &clicks, start), &mut out);
    check("ornaments.click", &out);
}

fn at(beat: usize) -> BeatPosition {
    BeatPosition {
        staff: gscore_model::StaffIndex::new(0),
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(beat),
    }
}

fn string(number: u8) -> StringNumber {
    StringNumber::new(number).unwrap()
}

fn points(points: &[(u8, i8)]) -> Vec<BendPoint> {
    points
        .iter()
        .map(|&(position, value)| BendPoint { position, value })
        .collect()
}

/// Four quarter-note chords (strings 4-6) and a run on string 3, for effects to go on.
fn base_song() -> Song {
    let chord = [
        TestNote::new(6, 3),
        TestNote::new(5, 5),
        TestNote::new(4, 5),
    ];
    testing::song_with_notes(&[
        (4, &chord),
        (4, &chord),
        (8, &[TestNote::new(3, 5)]),
        (8, &[TestNote::new(3, 7)]),
        (8, &[TestNote::new(3, 9)]),
        (8, &[TestNote::new(3, 7)]),
    ])
    .unwrap()
}

type Edit = fn(&mut Song);

fn note_edit(song: &mut Song, beat: usize, on: u8, effect: NoteEffect) {
    let _ = song.set_note_effect(at(beat), string(on), effect).unwrap();
}

fn beat_edit(song: &mut Song, beat: usize, effect: BeatEffect) {
    let _ = song.set_beat_effect(at(beat), effect).unwrap();
}

/// One small song per effect (or group of effects that belong together).
const EFFECTS: &[(&str, Edit)] = &[
    ("plain", |_| {}),
    ("ghost_accent_heavy", |song| {
        note_edit(song, 0, 6, NoteEffect::Ghost(true));
        note_edit(song, 0, 5, NoteEffect::Accent(Some(AccentKind::Normal)));
        note_edit(song, 0, 4, NoteEffect::Accent(Some(AccentKind::Heavy)));
    }),
    ("palm_mute_staccato", |song| {
        for on in [4, 5, 6] {
            note_edit(song, 0, on, NoteEffect::PalmMute(true));
            note_edit(song, 1, on, NoteEffect::Staccato(true));
        }
    }),
    ("let_ring", |song| {
        for on in [4, 5, 6] {
            note_edit(song, 0, on, NoteEffect::LetRing(true));
        }
        note_edit(song, 2, 3, NoteEffect::LetRing(true));
        note_edit(song, 5, 3, NoteEffect::LetRing(true));
    }),
    ("hammer_legato_tap", |song| {
        note_edit(song, 2, 3, NoteEffect::Hammer(true));
        note_edit(song, 3, 3, NoteEffect::Hammer(true));
        beat_edit(
            song,
            4,
            BeatEffect::Legato(Some(Legato {
                origin: false,
                destination: true,
            })),
        );
        note_edit(song, 5, 3, NoteEffect::LeftHandTap(true));
    }),
    ("bends", |song| {
        let up = BendShape::new(BendKind::Up, points(&[(0, 0), (6, 4), (12, 4)])).unwrap();
        let drawn =
            BendShape::new(BendKind::Up, points(&[(0, 0), (4, 4), (8, 0), (12, 4)])).unwrap();
        let release =
            BendShape::new(BendKind::Release, points(&[(0, 4), (6, 4), (12, 0)])).unwrap();
        note_edit(song, 2, 3, NoteEffect::Bend(Some(up)));
        note_edit(song, 3, 3, NoteEffect::Bend(Some(drawn)));
        note_edit(song, 4, 3, NoteEffect::Bend(Some(release)));
    }),
    ("slides", |song| {
        note_edit(song, 2, 3, NoteEffect::Slide(SlideKind::ShiftSlideTo, true));
        note_edit(
            song,
            3,
            3,
            NoteEffect::Slide(SlideKind::IntoFromBelow, true),
        );
        note_edit(
            song,
            4,
            3,
            NoteEffect::Slide(SlideKind::LegatoSlideTo, true),
        );
        note_edit(song, 5, 3, NoteEffect::Slide(SlideKind::OutDownwards, true));
        note_edit(
            song,
            0,
            6,
            NoteEffect::Slide(SlideKind::IntoFromAbove, true),
        );
        note_edit(song, 1, 6, NoteEffect::Slide(SlideKind::OutUpwards, true));
    }),
    ("vibrato_whammy", |song| {
        note_edit(song, 2, 3, NoteEffect::Vibrato(Some(Vibrato::Slight)));
        note_edit(song, 3, 3, NoteEffect::Vibrato(Some(Vibrato::Wide)));
        beat_edit(song, 4, BeatEffect::Vibrato(Some(Vibrato::Wide)));
        let dive = WhammyShape::new(points(&[(0, 0), (6, -8), (12, -8)])).unwrap();
        beat_edit(song, 0, BeatEffect::Whammy(Some(dive)));
    }),
    ("trill_tremolo", |song| {
        note_edit(
            song,
            0,
            6,
            NoteEffect::Trill(Some(Trill {
                fret: Some(5),
                speed: TrillSpeed::ThirtySecond,
            })),
        );
        note_edit(
            song,
            1,
            5,
            NoteEffect::TremoloPicking(Some(TremoloSpeed::Sixteenth)),
        );
        note_edit(
            song,
            2,
            3,
            NoteEffect::TremoloPicking(Some(TremoloSpeed::ThirtySecond)),
        );
    }),
    ("strokes", |song| {
        beat_edit(song, 0, BeatEffect::PickStroke(Some(StrokeDirection::Up)));
        beat_edit(
            song,
            1,
            BeatEffect::BrushStroke(Some(Stroke {
                direction: StrokeDirection::Down,
                speed: Stroke::BRUSH_SPEED,
                on_beat: false,
            })),
        );
    }),
    ("arpeggio_rasgueado", |song| {
        beat_edit(
            song,
            0,
            BeatEffect::Arpeggio(Some(Stroke {
                direction: StrokeDirection::Up,
                speed: 8,
                on_beat: true,
            })),
        );
        beat_edit(song, 1, BeatEffect::Rasgueado(true));
    }),
    ("slap_pop_tap_dead_slap", |song| {
        beat_edit(song, 0, BeatEffect::Slap(Some(SlapKind::Slapping)));
        beat_edit(song, 1, BeatEffect::DeadSlapped(true));
        beat_edit(song, 2, BeatEffect::Slap(Some(SlapKind::Popping)));
        beat_edit(song, 3, BeatEffect::Slap(Some(SlapKind::Tapping)));
    }),
    ("fades", |song| {
        beat_edit(song, 0, BeatEffect::FadeIn(true));
        beat_edit(song, 1, BeatEffect::FadeOut(true));
        beat_edit(song, 2, BeatEffect::VolumeSwell(true));
    }),
    ("pick_slides", |song| {
        note_edit(
            song,
            2,
            3,
            NoteEffect::Slide(SlideKind::PickSlideDown, true),
        );
        note_edit(song, 5, 3, NoteEffect::Slide(SlideKind::PickSlideUp, true));
    }),
    ("graces", |song| {
        let grace = |on_beat, dead, hammer| Grace {
            fret: Fret::new(2).unwrap(),
            speed: GraceSpeed::Normal,
            on_beat,
            dead,
            hammer,
        };
        note_edit(
            song,
            1,
            6,
            NoteEffect::Grace(Some(grace(false, false, true))),
        );
        note_edit(
            song,
            3,
            3,
            NoteEffect::Grace(Some(grace(true, false, false))),
        );
        note_edit(
            song,
            4,
            3,
            NoteEffect::Grace(Some(grace(false, true, false))),
        );
    }),
    ("harmonics", |song| {
        let harmonic = |kind| Harmonic { kind, node: None };
        note_edit(
            song,
            2,
            3,
            NoteEffect::Harmonic(Some(harmonic(HarmonicKind::Natural))),
        );
        note_edit(
            song,
            3,
            3,
            NoteEffect::Harmonic(Some(harmonic(HarmonicKind::Pinch))),
        );
    }),
];

#[test]
fn every_effect_plays_as_its_golden_events() {
    for (name, edit) in EFFECTS {
        let mut song = base_song();
        edit(&mut song);
        let mut out = sequence_text(&song);
        out.push_str(&excerpts_text(&song));
        check(&format!("effect_{name}"), &out);
    }
}

/// More tracks than MIDI channels: bends that find no channel to borrow are counted.
#[test]
fn crowded_channels_play_as_their_golden_events() {
    let bent = [TestNote::new(3, 5).with_bend(4)];
    let song = testing::song_with_notes_on_tracks(16, &[(4, &bent), (4, &bent)]).unwrap();
    check("crowded_channels", &sequence_text(&song));
}
