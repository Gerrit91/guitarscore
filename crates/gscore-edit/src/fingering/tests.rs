//! The fingering's unit tests.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use super::*;

fn string(n: u8) -> StringNumber {
    StringNumber::new(n).unwrap()
}

fn place(s: u8, fret: u8) -> Place {
    Place {
        string: string(s),
        fret: Fret::new(fret).unwrap(),
    }
}

/// Standard tuning, string 1 (E4) to string 6 (E2), 24 frets.
fn guitar() -> Neck {
    Neck::new(
        [64, 59, 55, 50, 45, 40]
            .iter()
            .zip(1..)
            .map(|(pitch, n)| (string(n), *pitch))
            .collect(),
        24,
    )
}

#[test]
fn open_strings_come_first_without_a_hand_position() {
    let neck = guitar();
    assert_eq!(neck.place_note(40, None, &[]).unwrap(), place(6, 0));
    assert_eq!(neck.place_note(64, None, &[]).unwrap(), place(1, 0));
    // G2 is fret 3 of the low E string, not fret -2 of anything.
    assert_eq!(neck.place_note(43, None, &[]).unwrap(), place(6, 3));
}

#[test]
fn notes_stay_near_the_hand() {
    let neck = guitar();
    // At the 7th fret, E4 is fret 5 of the B string rather than the open E string.
    assert_eq!(neck.place_note(64, Some(7.0), &[]).unwrap(), place(2, 5));
    // At the 12th, A3 (57) is fret 12 of the A string... or 7 of the D string; the
    // hand picks the one at its position.
    assert_eq!(neck.place_note(57, Some(12.0), &[]).unwrap(), place(5, 12));
}

#[test]
fn chords_get_one_string_each_within_a_span() {
    let neck = guitar();
    // An open E major chord: E2 B2 E3 G#3 B3 E4.
    let chord = neck.place_chord(&[40, 47, 52, 56, 59, 64], None).unwrap();
    assert_eq!(
        chord,
        [
            place(6, 0),
            place(5, 2),
            place(4, 2),
            place(3, 1),
            place(2, 0),
            place(1, 0)
        ]
    );
    // A barre A major at the 5th fret when the hand is there: A2 E3 A3 C#4 E4 A4.
    let barre = neck
        .place_chord(&[45, 52, 57, 61, 64, 69], Some(5.0))
        .unwrap();
    assert_eq!(barre[0], place(6, 5));
    let frets: Vec<u8> = barre.iter().map(|p| p.fret.get()).collect();
    assert!(span(&frets) <= MAX_SPAN, "{barre:?}");
}

#[test]
fn unplayable_pitches_and_chords_are_errors() {
    let neck = guitar();
    assert_eq!(
        neck.place_note(30, None, &[]),
        Err(FingeringError::OutOfRange {
            pitch: 30,
            lowest: 40,
            highest: 88
        })
    );
    // The low E is taken: nothing else plays E2.
    assert_eq!(
        neck.place_note(40, None, &[place(6, 0)]),
        Err(FingeringError::NoFreeString { pitch: 40 })
    );
    assert!(matches!(
        neck.place_chord(&[40, 41, 42, 43, 44, 45, 46], None),
        Err(FingeringError::TooManyNotes { .. })
    ));
    // F2 and F5 cannot be held together (an open E2 and E5 could: open strings take
    // no finger).
    assert!(neck.place_chord(&[40, 76], None).is_ok());
    assert_eq!(
        neck.place_chord(&[41, 77], None),
        Err(FingeringError::ChordTooWide)
    );
    assert!(Neck::new(Vec::new(), 24).place_note(60, None, &[]).is_err());
    assert!(!FingeringError::ChordTooWide.to_string().is_empty());
}

#[test]
fn a_note_moves_to_the_next_string_with_its_pitch() {
    let neck = guitar();
    // E4: open string 1, fret 5 on 2, 9 on 3, 14 on 4, 19 on 5, 24 on 6.
    let mut at = place(1, 0);
    let mut seen = vec![at];
    for _ in 0..5 {
        at = neck.other_string(at, true, &[]).unwrap();
        assert_eq!(neck.pitch(at), Some(64));
        seen.push(at);
    }
    assert_eq!(at, place(6, 24));
    // It wraps around to string 1 again, and goes back the other way.
    assert_eq!(neck.other_string(at, true, &[]), Some(place(1, 0)));
    assert_eq!(
        neck.other_string(place(1, 0), false, &[]),
        Some(place(6, 24))
    );
    // A taken string is skipped.
    assert_eq!(
        neck.other_string(place(1, 0), true, &[place(2, 3)]),
        Some(place(3, 9))
    );
}

#[test]
fn pitch_names_have_octaves() {
    assert_eq!(pitch_name(60), "C4");
    assert_eq!(pitch_name(40), "E2");
    assert_eq!(pitch_name(0), "C-1");
}
