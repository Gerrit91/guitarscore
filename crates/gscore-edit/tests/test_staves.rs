//! Editing a track's further staves: positions, the cursor, typing, undo, copy and paste,
//! and the journal's command lines, all on a piano's second (bass) staff.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(unused_crate_dependencies)]

mod common;

use gscore_edit::{Action, ActionId, BeatSpot, Command, Cursor, Outcome, Selection, TabEditor};
use gscore_model::testing;
use gscore_model::{
    BeatIndex, BeatPosition, MeasureIndex, Song, StaffIndex, StringNumber, TrackIndex, VoiceIndex,
};

/// The piano (a track with two staves) and the guitar after it.
fn editor() -> TabEditor {
    TabEditor::new(testing::song_with_two_staves().unwrap(), TrackIndex::new(0))
}

fn plain(editor: &mut TabEditor, id: ActionId) -> Outcome {
    editor
        .perform(Action::Plain(id), 0)
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// The cursor on `staff` of the piano, at its first beat.
fn on_staff(staff: usize, string: u8) -> Cursor {
    Cursor {
        track: TrackIndex::new(0),
        staff: StaffIndex::new(staff),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(0),
        string,
    }
}

/// Notes as (string, fret) pairs in order.
type Notes = Vec<(u8, u8)>;

/// The notes of the first beat on both staves, in order.
fn notes(song: &Song) -> (Notes, Notes) {
    let read = |staff: usize| {
        let piano = song.track(TrackIndex::new(0)).unwrap();
        piano
            .staff(StaffIndex::new(staff))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .voices()
            .next()
            .unwrap()
            .beats()
            .next()
            .unwrap()
            .notes()
            .map(|note| (note.string().unwrap().get(), note.fret().unwrap().get()))
            .collect()
    };
    (read(0), read(1))
}

#[test]
fn the_cursor_walks_from_one_staff_to_the_next_and_back() {
    let mut e = editor();
    // Down the treble staff's strings, then onto the bass staff's top string.
    for _ in 0..5 {
        assert_eq!(plain(&mut e, ActionId::CursorDownString), Outcome::Moved);
    }
    let c = e.cursor();
    assert_eq!((c.staff.get(), c.string), (0, 6));
    assert_eq!(plain(&mut e, ActionId::CursorDownString), Outcome::Moved);
    let c = e.cursor();
    assert_eq!(c.staff.get(), 1, "the piano's bass staff");
    assert_eq!(c.string, 1, "its highest string");
    // The bass staff has five strings: down to the last, then back up to the treble.
    for _ in 0..4 {
        assert_eq!(plain(&mut e, ActionId::CursorDownString), Outcome::Moved);
    }
    assert_eq!(e.cursor().string, 5);
    assert_eq!(plain(&mut e, ActionId::CursorDownString), Outcome::Nothing);
    assert_eq!(plain(&mut e, ActionId::CursorUpString), Outcome::Moved);
    assert_eq!((e.cursor().staff.get(), e.cursor().string), (1, 4));
    // Up through the bass staff, then to the treble staff's lowest string.
    for _ in 0..3 {
        plain(&mut e, ActionId::CursorUpString);
    }
    assert_eq!((e.cursor().staff.get(), e.cursor().string), (1, 1));
    assert_eq!(plain(&mut e, ActionId::CursorUpString), Outcome::Moved);
    assert_eq!((e.cursor().staff.get(), e.cursor().string), (0, 6));
    assert_eq!(plain(&mut e, ActionId::CursorUpString), Outcome::Moved);
    assert_eq!((e.cursor().staff.get(), e.cursor().string), (0, 5));
    assert_eq!(e.revision(), 0, "moving never edits");
}

#[test]
fn the_cursor_is_pulled_onto_a_staff_the_track_has() {
    let song = testing::song(&[testing::GUITAR_6], 2).unwrap();
    let mut e = TabEditor::new(song, TrackIndex::new(0));
    // A guitar has one staff: a cursor from a two-staff track is pulled back to it.
    e.set_cursor(Cursor {
        staff: StaffIndex::new(1),
        string: 9,
        ..e.cursor()
    });
    assert_eq!(e.cursor().staff, StaffIndex::new(0), "pulled back");
    assert_eq!(e.cursor().string, 6, "and onto a string it has");
}

#[test]
fn typing_a_fret_writes_on_the_staff_the_cursor_is_on() {
    let mut e = editor();
    let before = notes(e.song());
    e.set_cursor(on_staff(1, 3));
    e.perform(Action::Digit(2), 1000).unwrap();
    let after = notes(e.song());
    assert_eq!(after.0, before.0, "the treble staff is untouched");
    assert_ne!(after.1, before.1, "the bass staff got the note");
    // Undo takes it off again, on the right staff only.
    plain(&mut e, ActionId::Undo);
    assert_eq!(notes(e.song()), before);
    // And typing on the treble staff leaves the bass alone.
    e.set_cursor(on_staff(0, 2));
    e.perform(Action::Digit(4), 2000).unwrap();
    let after = notes(e.song());
    assert_eq!(after.1, before.1);
    assert_ne!(after.0, before.0);
}

#[test]
fn a_command_on_the_second_staff_journals_its_staff_and_replays() {
    let mut e = editor();
    let at = BeatPosition {
        track: TrackIndex::new(0),
        staff: StaffIndex::new(1),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(0),
    };
    let command = Command::SetFret {
        at,
        string: StringNumber::new(2).unwrap(),
        fret: gscore_model::Fret::new(11).unwrap(),
    };
    let line = command.to_line();
    assert!(line.contains("staff=2"), "{line}");
    let parsed = gscore_edit::parse_line(&line).unwrap().unwrap();
    assert_eq!(parsed, command);
    // A line without a staff (an older journal) means the track's first staff.
    let old = line.replace("staff=2 ", "");
    let parsed = gscore_edit::parse_line(&old).unwrap().unwrap();
    assert_eq!(parsed.position().unwrap().staff, StaffIndex::new(0));

    // Replaying the recorded line gives the same song as applying it.
    let mut direct = e.song().clone();
    let _ = command.apply(&mut direct).unwrap();
    e.replay(gscore_edit::Recorded::parse(&line).unwrap().unwrap())
        .unwrap();
    assert_eq!(
        gscore_model::differences(&direct, e.song()),
        Vec::new(),
        "the journal line replays the edit"
    );
}

#[test]
fn a_selection_on_the_second_staff_covers_its_beats_only() {
    let mut e = editor();
    e.set_cursor(on_staff(1, 1));
    let spot = BeatSpot::of(e.cursor());
    e.set_selection(Some(Selection::Beats {
        track: TrackIndex::new(0),
        staff: StaffIndex::new(1),
        voice: VoiceIndex::new(0),
        anchor: spot,
        focus: spot,
    }));
    let positions = e.selection().unwrap().positions(e.song());
    assert!(!positions.is_empty());
    assert!(
        positions.iter().all(|at| at.staff == StaffIndex::new(1)),
        "{positions:?}"
    );
    // Give the bass staff a fret of its own, copy its beat and paste it on the treble
    // staff: the note arrives there.
    e.set_cursor(on_staff(1, 1));
    e.perform(Action::Digit(5), 1000).unwrap();
    e.set_selection(Some(Selection::Beats {
        track: TrackIndex::new(0),
        staff: StaffIndex::new(1),
        voice: VoiceIndex::new(0),
        anchor: spot,
        focus: spot,
    }));
    let before = notes(e.song());
    assert_eq!(before.1, [(1, 5)]);
    plain(&mut e, ActionId::Copy);
    e.set_cursor(on_staff(0, 1));
    plain(&mut e, ActionId::Paste);
    let after = notes(e.song());
    assert_eq!(after.1, before.1, "the bass staff keeps its notes");
    assert_ne!(after.0, before.0, "the treble staff got the paste");
}

#[test]
fn every_command_works_on_the_second_staff_or_refuses_cleanly() {
    // The model checks the staff on every edit through a position; the edits that address
    // a position all go through it.
    for (name, song) in common::test_songs() {
        for info in gscore_edit::COMMANDS {
            let has_position = info.params.iter().any(|param| param.name == "measure")
                && info.params.iter().any(|param| param.name == "beat");
            if !has_position {
                continue;
            }
            let mut args: Vec<String> = info
                .params
                .iter()
                .map(|param| {
                    let value = match param.name {
                        "measure" | "beat" | "voice" => "1",
                        "track" => "1",
                        "staff" => "2",
                        "string" => "1",
                        "fret" => "3",
                        "duration" => "1/4",
                        _ => "1",
                    };
                    format!("{}={value}", param.name)
                })
                .collect();
            if !args.iter().any(|arg| arg.starts_with("staff=")) {
                args.push("staff=2".to_owned());
            }
            let line = format!("{} {}", info.id, args.join(" "));
            let Ok(Some(command)) = gscore_edit::parse_line(&line) else {
                continue;
            };
            let mut edited = song.clone();
            match command.apply(&mut edited) {
                Ok(change) => {
                    // An edit on the second staff must not touch the first.
                    if name == "two staves" && command.position().is_some() {
                        let _ = edited.issues();
                    }
                    edited.revert(&change).unwrap();
                    assert!(
                        gscore_model::differences(&song, &edited).is_empty(),
                        "{name}: reverting {} from \"{line}\" changed the song",
                        info.id
                    );
                }
                Err(_) => {
                    assert!(gscore_model::differences(&song, &edited).is_empty());
                }
            }
        }
    }
}

#[test]
fn copying_a_measure_takes_every_staff_and_pastes_them_together() {
    let mut song = testing::song_with_two_staves().unwrap();
    let _ = song.insert_measure(MeasureIndex::new(1)).unwrap();
    let mut e = TabEditor::new(song, TrackIndex::new(0));
    // The new bar is rests on both staves.
    let notes_in = |song: &Song, staff: usize, measure: usize| {
        let piano = song.track(TrackIndex::new(0)).unwrap();
        piano
            .staff(StaffIndex::new(staff))
            .unwrap()
            .measure(MeasureIndex::new(measure))
            .unwrap()
            .voices()
            .next()
            .unwrap()
            .beats()
            .next()
            .unwrap()
            .notes()
            .count()
    };
    assert_eq!((notes_in(e.song(), 0, 1), notes_in(e.song(), 1, 1)), (0, 0));
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(0),
        focus: MeasureIndex::new(0),
        track: None,
    }));
    plain(&mut e, ActionId::Copy);
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(1),
        focus: MeasureIndex::new(1),
        track: None,
    }));
    plain(&mut e, ActionId::Paste);
    // Both of the piano's staves got the copied bar, and so did the guitar after it.
    assert_eq!((notes_in(e.song(), 0, 1), notes_in(e.song(), 1, 1)), (1, 1));
}

#[test]
fn cutting_a_measure_clears_every_staff() {
    let mut song = testing::song_with_two_staves().unwrap();
    let _ = song.insert_measure(MeasureIndex::new(1)).unwrap();
    let mut e = TabEditor::new(song, TrackIndex::new(0));
    e.set_selection(Some(Selection::Measures {
        anchor: MeasureIndex::new(0),
        focus: MeasureIndex::new(0),
        track: Some(TrackIndex::new(0)),
    }));
    let cut = plain(&mut e, ActionId::Cut);
    assert_eq!(cut, Outcome::Edited);
    let piano = e.song().track(TrackIndex::new(0)).unwrap();
    for staff in piano.all_staves() {
        let notes: usize = staff
            .measure(MeasureIndex::new(0))
            .unwrap()
            .voices()
            .map(|voice| {
                voice
                    .beats()
                    .map(|beat| beat.notes().count())
                    .sum::<usize>()
            })
            .sum();
        assert_eq!(notes, 0, "{} keeps notes", staff.index());
    }
    // One undo puts every staff back.
    plain(&mut e, ActionId::Undo);
    let piano = e.song().track(TrackIndex::new(0)).unwrap();
    assert_eq!(
        piano
            .staff(StaffIndex::new(1))
            .unwrap()
            .measure(MeasureIndex::new(0))
            .unwrap()
            .voices()
            .next()
            .unwrap()
            .beats()
            .next()
            .unwrap()
            .notes()
            .count(),
        1
    );
}
