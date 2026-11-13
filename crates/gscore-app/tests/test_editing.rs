//! Editing in the app, driven by key events as egui delivers them: the cursor, fret entry
//! (including characters that need Shift on some layouts), undo, saving, and the question
//! about unsaved changes.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gscore_app::{GuitarScoreApp, MeasureSelection};
use gscore_model::{
    BeatIndex, BeatPosition, Fade, Finger, Hand, MeasureIndex, NoteView, StringNumber,
    StrokeDirection, TrackIndex, VoiceIndex,
};

fn testdata(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative)
}

/// A private copy of a test file, so saving does not touch `testdata/`.
fn scratch_copy(name: &str, test: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("gscore-app-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(name);
    std::fs::copy(testdata(&format!("basic/{name}")), &file).unwrap();
    (dir, file)
}

/// Runs frames until background jobs are done.
fn settle(harness: &mut Harness<'static, GuitarScoreApp>) {
    let started = Instant::now();
    harness.run();
    while harness.state().is_busy() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run();
}

fn app_with(file: &Path) -> Harness<'static, GuitarScoreApp> {
    let file = file.to_path_buf();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        // Scrolling to the cursor animates over a few frames.
        .with_max_steps(64)
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(file)));
    settle(&mut harness);
    harness
}

/// A typed character, as the layout produced it, with the modifiers held for it.
fn type_char(harness: &mut Harness<'static, GuitarScoreApp>, text: &str, modifiers: Modifiers) {
    harness.event_modifiers(Event::Text(text.to_owned()), modifiers);
    harness.run();
}

fn press(harness: &mut Harness<'static, GuitarScoreApp>, key: Key) {
    harness.key_press(key);
    harness.run();
}

/// The fret on `string` at the cursor.
fn fret_at_cursor(app: &GuitarScoreApp, string: u8) -> Option<u8> {
    let document = app.document()?;
    let beat = document
        .song()
        .beat(document.editor().cursor().position())?;
    beat.notes()
        .find(|n| n.string().ok() == StringNumber::new(string).ok())
        .and_then(|n| n.fret().ok())
        .map(|f| f.get())
}

/// Puts the cursor on `beat` of `measure`, on `string`.
fn put_cursor(
    harness: &mut Harness<'static, GuitarScoreApp>,
    measure: usize,
    beat: usize,
    string: u8,
) {
    let document = harness.state_mut().document_mut().unwrap();
    let cursor = document.editor().cursor();
    document.editor_mut().set_cursor(gscore_edit::Cursor {
        measure: MeasureIndex::new(measure),
        beat: BeatIndex::new(beat),
        string,
        ..cursor
    });
}

/// The notes on a beat as (string, fret), highest string first.
fn notes_at(
    harness: &Harness<'static, GuitarScoreApp>,
    measure: usize,
    beat: usize,
) -> Vec<(u8, u8)> {
    let document = harness.state().document().unwrap();
    let at = BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(beat),
    };
    document.song().beat(at).map_or_else(Vec::new, |beat| {
        beat.notes()
            .map(|note| (note.string().unwrap().get(), note.fret().unwrap().get()))
            .collect()
    })
}

#[test]
fn keys_move_the_cursor_enter_frets_and_undo() {
    let (dir, file) = scratch_copy("note_lengths.gp", "keys");
    let mut harness = app_with(&file);
    // Measure 1 is one whole note, so Right goes on to measure 2.
    press(&mut harness, Key::ArrowRight);
    press(&mut harness, Key::ArrowDown);
    let cursor = harness.state().document().unwrap().editor().cursor();
    assert_eq!(
        (cursor.measure.get(), cursor.beat.get(), cursor.string),
        (1, 0, 2)
    );
    harness.get_by_label("Measure 2, beat 1, string 2");

    // AZERTY: the digit 7 needs Shift; the typed character is what counts.
    type_char(&mut harness, "7", Modifiers::SHIFT);
    assert_eq!(fret_at_cursor(harness.state(), 2), Some(7));
    assert!(harness.state().document().unwrap().is_dirty());
    harness.get_by_label("note_lengths.gp (changed)");

    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
    assert_ne!(fret_at_cursor(harness.state(), 2), Some(7));
    assert_eq!(harness.state().notice(), None);

    // A character no key is bound to does nothing; neither does a space typed as text.
    type_char(&mut harness, "q", Modifiers::NONE);
    assert_eq!(harness.state().notice(), None);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn plus_typed_with_or_without_shift_shortens_the_note() {
    let (dir, file) = scratch_copy("note_lengths.gp", "plus");
    let mut harness = app_with(&file);
    let value = |h: &Harness<'static, GuitarScoreApp>| {
        let document = h.state().document().unwrap();
        let beat = document
            .song()
            .beat(document.editor().cursor().position())
            .unwrap();
        beat.note_value().unwrap().value()
    };
    let before = value(&harness);
    // QWERTY: Shift+`=` types `+`; QWERTZ: the `+` key without Shift.
    type_char(&mut harness, "+", Modifiers::SHIFT);
    assert_eq!(value(&harness), before * 2);
    type_char(&mut harness, "+", Modifiers::NONE);
    assert_eq!(value(&harness), before * 4);
    type_char(&mut harness, "-", Modifiers::NONE);
    assert_eq!(value(&harness), before * 2);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn ctrl_s_saves_a_gp_file_in_place_with_a_backup() {
    let (dir, file) = scratch_copy("steel_guitar_dadgad.gp", "save");
    let mut harness = app_with(&file);
    type_char(&mut harness, "9", Modifiers::NONE);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::S);
    settle(&mut harness);
    let app = harness.state();
    assert_eq!(app.notice(), None);
    assert!(!app.document().unwrap().is_dirty());
    assert!(
        app.status()
            .unwrap()
            .starts_with("Saved steel_guitar_dadgad.gp")
    );
    assert!(dir.join("steel_guitar_dadgad.gp.bak").exists());

    let saved = gscore_session::open(&file).unwrap().song;
    let beat = saved
        .beat(app.document().unwrap().editor().cursor().position())
        .unwrap();
    assert!(
        beat.notes()
            .any(|n| n.fret().ok().map(|f| f.get()) == Some(9))
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_note_that_cannot_be_heard_does_not_interrupt_editing() {
    let (dir, file) = scratch_copy("note_lengths.gp", "silent");
    let mut harness = app_with(&file);
    // No output matches, so the first typed note cannot start it. Auditioning is a
    // convenience: the note is still entered, without a notice on every keystroke.
    harness
        .state_mut()
        .playback_mut()
        .choose_port(Some("no such port".to_owned()));
    type_char(&mut harness, "5", Modifiers::NONE);
    assert_eq!(harness.state().notice(), None);
    assert_eq!(fret_at_cursor(harness.state(), 1), Some(5));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn opening_another_file_opens_it_in_a_new_tab() {
    let (dir, file) = scratch_copy("note_lengths.gp", "tabs");
    let mut harness = app_with(&file);
    assert_eq!(harness.state().tab_count(), 1);
    type_char(&mut harness, "3", Modifiers::NONE);

    // Opening another file does not ask about unsaved changes: it opens in a new tab and
    // leaves the first one (with its unsaved edit) untouched.
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .open(testdata("basic/steel_guitar_dadgad.gp"), &ctx);
    settle(&mut harness);
    assert!(!harness.state().has_dialog());
    assert_eq!(harness.state().tab_count(), 2);
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Steel Guitar DADGAD"
    );
    // The first tab still holds its unsaved change.
    assert!(harness.state().tab_document(0).unwrap().is_dirty());
    assert_eq!(
        harness.state().tab_document(0).unwrap().display_title(),
        "Note Lengths"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_edit_menu_undoes_and_shows_the_keys() {
    let (dir, file) = scratch_copy("note_lengths.gp", "menu");
    let mut harness = app_with(&file);
    type_char(&mut harness, "4", Modifiers::NONE);
    harness.get_by_label("Edit").click();
    harness.run();
    // The entry's label includes its keys from the keymap.
    harness.get_by_label_contains("ctrl+z");
    harness.get_by_label_contains("Undo ctrl+z").click();
    harness.run();
    assert_ne!(fret_at_cursor(harness.state(), 1), Some(4));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn shift_arrows_select_beats_and_end_a_grid_selection() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    // A measure selected in the grid first.
    harness
        .state_mut()
        .set_selection(Some(MeasureSelection::new(0)));
    harness
        .state_mut()
        .document_mut()
        .unwrap()
        .editor_mut()
        .set_selection(Some(gscore_edit::Selection::Measures {
            anchor: MeasureIndex::new(0),
            focus: MeasureIndex::new(0),
            track: None,
        }));
    harness.run();

    for _ in 0..2 {
        harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
        harness.run();
    }
    let selection = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        document
            .editor()
            .selection()
            .map(|s| s.positions(document.song()).len())
    };
    assert_eq!(
        selection(&harness),
        Some(3),
        "the cursor's beat and two more"
    );
    assert!(
        harness.state().selection().is_none(),
        "the grid no longer shows a measure selection"
    );

    // A plain arrow ends the selection.
    press(&mut harness, Key::ArrowLeft);
    assert_eq!(selection(&harness), None);
}

#[test]
fn shift_up_and_down_extend_the_selection_over_the_lines() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    put_cursor(&mut harness, 0, 1, 1);
    harness.run();
    let layout = harness
        .state()
        .document()
        .unwrap()
        .shown_layout()
        .expect("the page was drawn");
    let places = layout.places_of(TrackIndex::new(0)).unwrap();
    assert!(places.systems.len() > 2, "the song needs a few lines");
    let system_of = |measure: MeasureIndex| places.measure(measure).unwrap().system;
    let focus = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        match document.editor().selection() {
            Some(gscore_edit::Selection::Beats { anchor, focus, .. }) => Some((anchor, focus)),
            _ => None,
        }
    };

    // Down: from the first line onto the second, then the third; the anchor stays.
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
    harness.run();
    let (anchor, first) = focus(&harness).expect("a beat selection");
    assert_eq!(anchor.measure, MeasureIndex::new(0));
    assert_eq!(system_of(first.measure), 1, "{first:?}");
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowDown);
    harness.run();
    let (_, second) = focus(&harness).unwrap();
    assert_eq!(system_of(second.measure), 2, "{second:?}");
    // The cursor goes along, as with Shift+Right.
    assert_eq!(
        harness
            .state()
            .document()
            .unwrap()
            .editor()
            .cursor()
            .measure,
        second.measure
    );
    // The selection holds every beat in between, over the line breaks.
    let count = {
        let document = harness.state().document().unwrap();
        document
            .editor()
            .selection()
            .unwrap()
            .positions(document.song())
            .len()
    };
    assert!(count > 2, "{count} beats");

    // Up again, line by line, back to where it started.
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowUp);
    harness.run();
    assert_eq!(focus(&harness).unwrap().1, first);
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowUp);
    harness.run();
    let (_, back) = focus(&harness).unwrap();
    assert_eq!(system_of(back.measure), 0);
    // Above the first line: on to the start of the track.
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowUp);
    harness.run();
    let (_, start) = focus(&harness).unwrap();
    assert_eq!(start.measure, MeasureIndex::new(0));
    assert_eq!(start.beat, BeatIndex::new(0));
}

#[test]
fn palette_keys_work_like_the_tiles() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    type_char(&mut harness, "5", Modifiers::NONE);
    let note = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        let at = document.editor().cursor().position();
        let beat = document.song().beat(at).unwrap();
        let palm = beat.notes().next().is_some_and(|n| n.is_palm_muted());
        (palm, beat.note_value().unwrap().value())
    };
    assert!(!note(&harness).0);
    // "p" is palm mute, as the tile's tooltip says; pressing it again takes it off.
    type_char(&mut harness, "p", Modifiers::NONE);
    assert!(note(&harness).0);
    type_char(&mut harness, "p", Modifiers::NONE);
    assert!(!note(&harness).0);
    // Ctrl+4 is the eighth note.
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Num4);
    harness.run();
    assert_eq!(note(&harness).1, 8);
}

#[test]
fn a_second_click_on_a_technique_removes_it_even_off_the_cursor_string() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    // A note on string 1; then the cursor moves to string 2, where the beat has no note.
    type_char(&mut harness, "5", Modifiers::NONE);
    press(&mut harness, Key::ArrowDown);
    let palm_muted = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        let at = document.editor().cursor().position();
        document
            .song()
            .beat(at)
            .unwrap()
            .notes()
            .next()
            .is_some_and(|n| n.is_palm_muted())
    };
    harness.get_by_label("Palm mute").click_accesskit();
    harness.run();
    assert!(palm_muted(&harness), "the first click applies it");
    harness.get_by_label("Palm mute").click_accesskit();
    harness.run();
    assert!(!palm_muted(&harness), "the second click removes it");
}

#[test]
fn the_advanced_effect_tiles_edit_the_note_and_beat() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    type_char(&mut harness, "5", Modifiers::NONE);
    let click = |harness: &mut Harness<'static, GuitarScoreApp>, label: &str| {
        harness.get_by_label(label).scroll_to_me();
        harness.run();
        harness.get_by_label(label).click_accesskit();
        harness.run();
    };
    let arpeggio = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        let at = document.editor().cursor().position();
        document.song().beat(at).unwrap().arpeggio()
    };
    let beat_flag = |harness: &Harness<'static, GuitarScoreApp>,
                     test: fn(&gscore_model::BeatView<'_>) -> bool| {
        let document = harness.state().document().unwrap();
        let at = document.editor().cursor().position();
        test(&document.song().beat(at).unwrap())
    };
    let fade = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        let at = document.editor().cursor().position();
        document.song().beat(at).unwrap().fade()
    };
    let note_flag = |harness: &Harness<'static, GuitarScoreApp>,
                     test: fn(&NoteView<'_>) -> bool| {
        let document = harness.state().document().unwrap();
        let cursor = document.editor().cursor();
        let beat = document.song().beat(cursor.position()).unwrap();
        beat.notes()
            .find(|note| note.string().unwrap().get() == cursor.string)
            .is_some_and(|note| test(&note))
    };

    click(&mut harness, "Arpeggio down");
    harness.get_by_label("OK").click();
    harness.run();
    assert_eq!(arpeggio(&harness), Some(StrokeDirection::Down));
    click(&mut harness, "Arpeggio down");
    harness.get_by_label("Remove").click();
    harness.run();
    assert_eq!(arpeggio(&harness), None, "Remove clears it");
    click(&mut harness, "Dead slapped");
    assert!(beat_flag(&harness, |beat| beat.is_dead_slapped()));
    click(&mut harness, "Rasgueado");
    assert!(beat_flag(&harness, |beat| beat.has_rasgueado()));
    click(&mut harness, "Left-hand tap");
    assert!(note_flag(&harness, |note| note.is_left_hand_tapped()));
    click(&mut harness, "String number");
    assert!(note_flag(&harness, |note| note.shows_string_number()));
    click(&mut harness, "Fade out");
    assert_eq!(fade(&harness), Some(Fade::Out));
    click(&mut harness, "Volume swell");
    assert_eq!(fade(&harness), Some(Fade::Swell));
    // The trill and tremolo tiles open their dialogs, like the bend and grace tiles.
    click(&mut harness, "Trill");
    click(&mut harness, "OK");
    assert!(note_flag(&harness, |note| note.trill().is_some()));
    click(&mut harness, "Tremolo picking");
    click(&mut harness, "OK");
    assert!(note_flag(&harness, |note| note.tremolo_speed().is_some()));
}

#[test]
fn the_tie_beat_text_and_fingering_tiles_work() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    type_char(&mut harness, "5", Modifiers::NONE);
    press(&mut harness, Key::ArrowRight);
    type_char(&mut harness, "5", Modifiers::NONE);
    harness.run();

    let note_kind = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        let cursor = document.editor().cursor();
        let beat = document.song().beat(cursor.position()).unwrap();
        beat.notes()
            .find(|note| note.string().unwrap().get() == cursor.string)
            .map(|note| note.kind())
            .unwrap()
    };
    harness.get_by_label("Tie beat").scroll_to_me();
    harness.run();
    harness.get_by_label("Tie beat").click_accesskit();
    harness.run();
    assert_eq!(note_kind(&harness), gscore_model::NoteKind::Tie);

    // The fingering tile opens a dialog; choosing a finger and OK sets it.
    let finger = |harness: &Harness<'static, GuitarScoreApp>, hand: Hand| {
        let document = harness.state().document().unwrap();
        let cursor = document.editor().cursor();
        let beat = document.song().beat(cursor.position()).unwrap();
        beat.notes()
            .find(|note| note.string().unwrap().get() == cursor.string)
            .and_then(|note| note.finger(hand))
    };
    harness.get_by_label("Left-hand fingering…").scroll_to_me();
    harness.run();
    harness
        .get_by_label("Left-hand fingering…")
        .click_accesskit();
    harness.run();
    // A click sets the finger at once; the window stays open, following the cursor.
    harness.get_by_label("Little").click_accesskit();
    harness.run();
    assert_eq!(finger(&harness, Hand::Left), Some(Finger::Little));
    // Keys: the arrows move to the previous note, a digit fingers it (not a fret).
    press(&mut harness, Key::ArrowLeft);
    assert_eq!(finger(&harness, Hand::Left), None);
    type_char(&mut harness, "2", Modifiers::NONE);
    harness.run();
    assert_eq!(finger(&harness, Hand::Left), Some(Finger::Middle));
    assert_eq!(
        fret_at_cursor(harness.state(), 1),
        Some(5),
        "the fret stays"
    );
    // The window shows the cursor note's finger.
    use egui_kittest::kittest::NodeT as _;
    assert!(
        harness.get_by_label("Middle").accesskit_node().toggled()
            == Some(egui::accesskit::Toggled::True)
    );
    press(&mut harness, Key::ArrowRight);
    assert!(
        harness.get_by_label("Little").accesskit_node().toggled()
            == Some(egui::accesskit::Toggled::True)
    );
    // Backspace takes the finger off; Escape closes the window.
    press(&mut harness, Key::Backspace);
    assert_eq!(finger(&harness, Hand::Left), None);
    press(&mut harness, Key::Escape);
    assert!(
        harness
            .query_by_label("Close Left-hand fingering")
            .is_none()
    );

    // The beat text tile opens its dialog; Cancel leaves the beat unchanged.
    harness.get_by_label("Beat text…").scroll_to_me();
    harness.run();
    harness.get_by_label("Beat text…").click_accesskit();
    harness.run();
    harness.get_by_label("Beat text");
    harness.get_by_label("Cancel").click_accesskit();
    harness.run();
    assert!(
        harness.query_by_label("Beat text").is_none(),
        "the dialog closed"
    );
}

#[test]
fn the_lyrics_and_chord_diagram_dialogs_create_them() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    type_char(&mut harness, "5", Modifiers::NONE);
    harness.run();

    // Lyrics: the first line, starting at the cursor's bar.
    harness.get_by_label("Lyrics…").scroll_to_me();
    harness.run();
    harness.get_by_label("Lyrics…").click_accesskit();
    harness.run();
    harness.get_by_label("Lyric text").click();
    harness.run();
    harness.get_by_label("Lyric text").type_text("one two");
    harness.run();
    harness.get_by_label("Apply").click_accesskit();
    harness.run();
    let song = harness.state().document().unwrap().song();
    let lines = song.lyric_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].text, "one two");
    assert_eq!(
        song.lyric_syllables(TrackIndex::new(0))
            .iter()
            .map(|syllable| syllable.text.as_str())
            .collect::<Vec<_>>(),
        ["one", "two"]
    );

    // Chord diagram: it starts from the beat's notes (fret 5 on the highest string), and
    // a click on the drawn grid adds a finger (its middle: the fourth string, fret 3).
    harness.get_by_label("Chord diagram…").scroll_to_me();
    harness.run();
    harness.get_by_label("Chord diagram…").click_accesskit();
    harness.run();
    harness.get_by_label("Chord diagram grid").click();
    harness.run();
    harness.get_by_label("Apply").click_accesskit();
    harness.run();
    let document = harness.state().document().unwrap();
    let cursor = document.editor().cursor();
    let chord = document
        .song()
        .beat(cursor.position())
        .and_then(|beat| beat.chord_diagram())
        .expect("the beat has a chord diagram");
    assert_eq!(chord.name, "Chord");
    assert_eq!(chord.frets.len(), 6, "the track's six strings");
    assert!(chord.frets.contains(&(5, 5)), "{:?}", chord.frets);
    assert!(chord.frets.contains(&(3, 3)), "{:?}", chord.frets);
}

#[test]
fn lyrics_show_in_the_score_while_typed_and_cancel_takes_them_back() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    let syllables = |harness: &Harness<'static, GuitarScoreApp>| {
        let song = harness.state().document().unwrap().song();
        song.lyric_syllables(TrackIndex::new(0))
            .iter()
            .map(|syllable| syllable.text.clone())
            .collect::<Vec<_>>()
    };
    let depth = |harness: &Harness<'static, GuitarScoreApp>| {
        harness.state().document().unwrap().editor().undo_depth()
    };
    let before = depth(&harness);
    // Shift+L opens the dialog.
    type_char(&mut harness, "L", Modifiers::SHIFT);
    harness.get_by_label("Lyric text").click();
    harness.run();
    harness.get_by_label("Lyric text").type_text("one two");
    harness.run();
    // In the score before Apply, as one undo step.
    assert_eq!(syllables(&harness), ["one", "two"]);
    assert_eq!(depth(&harness), before + 1);
    harness.get_by_label("Lyric text").type_text(" three");
    harness.run();
    assert_eq!(syllables(&harness), ["one", "two", "three"]);
    assert_eq!(depth(&harness), before + 1);
    // Cancel takes it all back.
    harness.get_by_label("Cancel").click_accesskit();
    harness.run();
    assert!(syllables(&harness).is_empty());
    assert_eq!(depth(&harness), before);

    // Apply keeps it, in one step; a new dialog starts a new step.
    type_char(&mut harness, "L", Modifiers::SHIFT);
    harness.get_by_label("Lyric text").click();
    harness.run();
    harness.get_by_label("Lyric text").type_text("la");
    harness.run();
    harness.get_by_label("Apply").click_accesskit();
    harness.run();
    assert_eq!(syllables(&harness), ["la"]);
    assert_eq!(depth(&harness), before + 1);
}

#[test]
fn the_player_tells_whether_the_bar_is_full() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    harness.run();
    harness.get_by_label("Bar full: 4 of 4 beats");
    // One more beat makes the bar too long.
    press(&mut harness, Key::Insert);
    harness.run();
    assert!(
        harness.query_by_label_contains("Bar too long:").is_some(),
        "the overfull bar is reported"
    );
}

/// The sounding pitch of the note on `string` at the cursor.
fn pitch_at_cursor(app: &GuitarScoreApp, string: u8) -> Option<u8> {
    let document = app.document()?;
    let cursor = document.editor().cursor();
    let track = document.song().track(cursor.track)?;
    let note = track
        .measure(cursor.measure)?
        .voice(cursor.voice)?
        .beat(cursor.beat)?
        .notes()
        .find(|note| note.string().ok() == StringNumber::new(string).ok())?;
    track.sounding_pitch(&note).ok().map(|pitch| pitch.get())
}

fn step_until(
    harness: &mut Harness<'static, GuitarScoreApp>,
    timeout: Duration,
    done: impl Fn(&GuitarScoreApp) -> bool,
) -> bool {
    let started = Instant::now();
    while !done(harness.state()) {
        if started.elapsed() > timeout {
            return false;
        }
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

#[test]
fn an_entered_fret_is_heard_and_undo_is_not() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = std::env::temp_dir().join(format!("gscore-audition-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let font = dir.join("sine.sf2");
    std::fs::write(&font, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    harness.state_mut().playback_mut().use_soundfont(font);

    // The first fret starts the synthesizer; it is heard once the SoundFont is loaded.
    type_char(&mut harness, "5", Modifiers::NONE);
    let ready = step_until(&mut harness, Duration::from_secs(10), |app| {
        !app.playback().is_loading() || app.playback().error().is_some()
    });
    assert!(ready, "the synthesizer neither loaded nor failed");
    if let Some(error) = harness.state().playback().error() {
        eprintln!("skipped: no usable audio device ({error})");
        std::fs::remove_dir_all(dir).ok();
        return;
    }
    assert_eq!(harness.state().notice(), None);

    // A fret on another string sounds at that note's pitch, then stops.
    press(&mut harness, Key::ArrowDown);
    type_char(&mut harness, "7", Modifiers::NONE);
    let string = harness.state().document().unwrap().editor().cursor().string;
    let expected = pitch_at_cursor(harness.state(), string);
    assert!(expected.is_some());
    assert_eq!(harness.state().playback().auditioning(), expected);
    let ended = step_until(&mut harness, Duration::from_secs(2), |app| {
        app.playback().auditioning().is_none()
    });
    assert!(ended, "the entered note kept sounding");

    // Undoing the fret is silent.
    type_char(&mut harness, "z", Modifiers::COMMAND);
    harness.run();
    assert_eq!(harness.state().playback().auditioning(), None);

    // With the setting off, nothing sounds.
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Settings… ctrl+,").click();
    harness.run();
    harness.get_by_label("Editor").click();
    harness.run();
    harness
        .get_by_label("Play notes as they are entered")
        .click();
    harness.run();
    harness.get_by_label("Close Settings").click();
    harness.run();
    type_char(&mut harness, "3", Modifiers::NONE);
    assert_eq!(harness.state().playback().auditioning(), None);
    assert_eq!(fret_at_cursor(harness.state(), string), Some(3));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn an_effect_chosen_in_the_palette_plays_its_beat_and_play_still_plays_the_song() {
    // Plays a sine-wave SoundFont on the default audio device; skipped where there is none.
    let dir = std::env::temp_dir().join(format!("gscore-preview-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let font = dir.join("sine.sf2");
    std::fs::write(&font, gscore_audio::testing::sine_soundfont("Sine")).unwrap();
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    harness.state_mut().playback_mut().use_soundfont(font);
    type_char(&mut harness, "5", Modifiers::NONE);
    let ready = step_until(&mut harness, Duration::from_secs(10), |app| {
        !app.playback().is_loading() || app.playback().error().is_some()
    });
    assert!(ready, "the synthesizer neither loaded nor failed");
    if let Some(error) = harness.state().playback().error() {
        eprintln!("skipped: no usable audio device ({error})");
        std::fs::remove_dir_all(dir).ok();
        return;
    }

    // The vibrato tile sets the effect and plays the beat with it; the song stays stopped.
    harness.get_by_label("Vibrato").scroll_to_me();
    harness.run();
    harness.get_by_label("Vibrato").click();
    harness.run();
    assert!(harness.state().playback().previewing());
    let excerpt = harness
        .state()
        .playback()
        .preview_sequence()
        .unwrap()
        .clone();
    for event in &excerpt.events {
        if matches!(
            event.event,
            gscore_play::MidiEvent::PitchBend { .. } | gscore_play::MidiEvent::NoteOn { .. }
        ) {
            eprintln!("excerpt {} {:?}", event.tick, event.event);
        }
    }
    let shaking = excerpt.events.iter().any(|event| {
        matches!(event.event, gscore_play::MidiEvent::PitchBend { value, .. } if value > 8192 + 200)
    });
    assert!(shaking, "the excerpt has no vibrato: {:?}", excerpt.events);
    assert_eq!(
        harness.state().playback().state(),
        gscore_play::PlayState::Stopped
    );
    assert_eq!(harness.state().notice(), None);

    // Listen in the bend editor plays the drawn bend without applying it.
    let depth = harness.state().document().unwrap().editor().undo_depth();
    harness.get_by_label("Bend").click();
    harness.run();
    harness.get_by_label("Listen").click();
    harness.run();
    let excerpt = harness
        .state()
        .playback()
        .preview_sequence()
        .unwrap()
        .clone();
    let bent = excerpt.events.iter().any(|event| {
        matches!(event.event, gscore_play::MidiEvent::PitchBend { value, .. } if value > 8192 + 2000)
    });
    assert!(bent, "the excerpt bends");
    assert_eq!(
        harness.state().document().unwrap().editor().undo_depth(),
        depth,
        "listening changes nothing"
    );
    harness.get_by_label("Cancel").click();
    harness.run();

    // Play ends the excerpt and plays the song.
    harness.get_by_label("Play").click();
    harness.step();
    assert!(!harness.state().playback().previewing());
    let playing = step_until(&mut harness, Duration::from_secs(5), |app| {
        app.playback().state() == gscore_play::PlayState::Playing
    });
    assert!(playing, "the song did not start after the excerpt");
    // A paused song gives way to a heard beat.
    harness.step();
    harness.get_by_label("Pause").click();
    harness.step();
    harness.get_by_label("Vibrato").click();
    harness.run();
    assert!(
        harness.state().playback().previewing(),
        "heard while paused"
    );
    harness.get_by_label("Stop").click();
    harness.step();
    assert!(!harness.state().playback().previewing());

    // The dynamic the beat already has (mezzo-forte, the default) changes nothing but is
    // still heard.
    let depth = harness.state().document().unwrap().editor().undo_depth();
    harness.get_by_label("mf (mezzo-forte)").scroll_to_me();
    harness.run();
    harness.get_by_label("mf (mezzo-forte)").click();
    harness.run();
    assert!(harness.state().playback().previewing());
    assert_eq!(
        harness.state().document().unwrap().editor().undo_depth(),
        depth
    );
    harness.get_by_label("Stop").click();
    harness.step();

    // Undo is quiet.
    type_char(&mut harness, "z", Modifiers::COMMAND);
    harness.run();
    assert!(!harness.state().playback().previewing());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn the_rest_tile_enters_a_rest_of_the_chosen_length_in_an_empty_bar() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    harness.get_by_label("New song").click();
    settle(&mut harness);
    let at = |measure: usize| BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(measure),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(0),
    };
    let rest = |harness: &Harness<'static, GuitarScoreApp>, measure: usize| {
        let song = harness.state().document().unwrap().song();
        let beat = song.beat(at(measure)).unwrap();
        (beat.kind(), beat.duration().unwrap())
    };
    // Choose an eighth, then add a bar: it starts with a whole rest.
    harness.get_by_label("1/8").click();
    harness.run();
    harness.get_by_label("Bar").click();
    harness.run();
    harness.get_by_label("Insert bar after").click_accesskit();
    harness.run();
    let document = harness.state_mut().document_mut().unwrap();
    let mut cursor = document.editor().cursor();
    cursor.measure = MeasureIndex::new(1);
    cursor.beat = BeatIndex::new(0);
    document.editor_mut().set_cursor(cursor);
    harness.run();
    let whole = gscore_model::Duration::new(1, 1).unwrap();
    assert_eq!(rest(&harness, 1), (gscore_model::BeatKind::Rest, whole));
    // The rest tile makes it a rest of the chosen length.
    harness.get_by_label("Rest").click();
    harness.run();
    let eighth = gscore_model::Duration::new(1, 8).unwrap();
    assert_eq!(rest(&harness, 1), (gscore_model::BeatKind::Rest, eighth));
}

#[test]
fn a_second_click_on_the_rest_tile_brings_the_notes_back() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    let at = harness
        .state()
        .document()
        .unwrap()
        .editor()
        .cursor()
        .position();
    let beat = |harness: &Harness<'static, GuitarScoreApp>| {
        let song = harness.state().document().unwrap().song();
        let beat = song.beat(at).unwrap();
        (beat.kind(), beat.duration().unwrap(), beat.notes().count())
    };
    let before = beat(&harness);
    assert_eq!(before.0, gscore_model::BeatKind::Normal);
    assert!(before.2 > 0, "the first beat has notes");
    harness.get_by_label("Rest").click();
    harness.run();
    assert_eq!(beat(&harness), (gscore_model::BeatKind::Rest, before.1, 0));
    // Not a rest twice as long: the notes as they were.
    harness.get_by_label("Rest").click();
    harness.run();
    assert_eq!(beat(&harness), before);
    // And a rest again.
    harness.get_by_label("Rest").click();
    harness.run();
    assert_eq!(beat(&harness), (gscore_model::BeatKind::Rest, before.1, 0));
}

#[test]
fn the_harmonic_tile_sets_an_artificial_harmonic_with_its_touch_point() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    type_char(&mut harness, "5", Modifiers::NONE);
    harness.run();
    harness.get_by_label("Harmonic").scroll_to_me();
    harness.run();
    harness.get_by_label("Harmonic").click();
    harness.run();
    harness.get_by_label("Artificial").click();
    harness.run();
    harness.get_by_label("+12").click();
    harness.run();
    assert!(
        harness.query_by_label_contains("2nd partial").is_some(),
        "the editor names what the harmonic sounds"
    );
    harness.get_by_label("OK").click();
    harness.run();
    let document = harness.state().document().unwrap();
    let cursor = document.editor().cursor();
    let song = document.song();
    let beat = song.beat(cursor.position()).unwrap();
    let note = beat
        .notes()
        .find(|note| note.string().unwrap().get() == cursor.string)
        .unwrap();
    assert_eq!(
        note.harmonic(),
        Some(gscore_model::Harmonic {
            kind: gscore_model::HarmonicKind::Artificial,
            node: Some(17),
        })
    );
}

#[test]
fn notes_go_into_the_third_and_fourth_voice() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    for (label, voice) in [("Voice 3", 2), ("Voice 4", 3)] {
        harness.get_by_label(label).scroll_to_me();
        harness.run();
        harness.get_by_label(label).click();
        harness.run();
        let cursor = harness.state().document().unwrap().editor().cursor();
        assert_eq!(cursor.voice.get(), voice);
        type_char(&mut harness, "7", Modifiers::NONE);
        harness.run();
        let document = harness.state().document().unwrap();
        let cursor = document.editor().cursor();
        let beat = document
            .song()
            .beat(cursor.position())
            .expect("the voice's beat");
        assert!(beat.notes().any(|note| note.fret().unwrap().get() == 7));
        assert_eq!(harness.state().notice(), None);
    }
    // The page still lays out with four voices.
    assert!(harness.render().is_ok());
}

#[test]
fn the_bend_tile_opens_the_editor_on_a_full_step_bend() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    type_char(&mut harness, "5", Modifiers::NONE);
    let bend = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        let at = document.editor().cursor().position();
        let beat = document.song().beat(at).unwrap();
        beat.notes()
            .next()
            .and_then(|n| n.bend_shape())
            .map(|shape| {
                shape
                    .points
                    .iter()
                    .map(|p| (p.position, p.value))
                    .collect::<Vec<_>>()
            })
    };
    let click = |harness: &mut Harness<'static, GuitarScoreApp>, label: &str| {
        harness.get_by_label(label).click_accesskit();
        harness.run();
    };

    // The tile opens the editor; nothing changes until OK, and Cancel changes nothing.
    click(&mut harness, "Bend");
    harness.get_by_label("Bend editor");
    assert_eq!(bend(&harness), None, "opening adds no bend yet");
    click(&mut harness, "Cancel");
    assert!(harness.query_by_label("Bend editor").is_none());
    assert_eq!(bend(&harness), None, "Cancel adds no bend");

    // By default it is a full-step bend.
    click(&mut harness, "Bend");
    click(&mut harness, "OK");
    assert_eq!(bend(&harness), Some(vec![(0, 0), (6, 4), (12, 4)]));
    assert!(
        harness.query_by_label("Bend editor").is_none(),
        "OK closes the editor"
    );

    // Opened again, it shows the note's bend; a preset replaces it.
    click(&mut harness, "Bend");
    click(&mut harness, "Preset Pre-Bend");
    click(&mut harness, "OK");
    assert_eq!(bend(&harness), Some(vec![(0, 4), (12, 4)]));

    click(&mut harness, "Bend");
    click(&mut harness, "Remove");
    assert_eq!(bend(&harness), None, "Remove takes the bend off");
}

#[test]
fn the_whammy_tile_opens_the_editor_on_a_dip() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    type_char(&mut harness, "5", Modifiers::NONE);
    let whammy = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        let at = document.editor().cursor().position();
        document
            .song()
            .beat(at)
            .unwrap()
            .whammy_shape()
            .map(|shape| {
                shape
                    .points
                    .iter()
                    .map(|p| (p.position, p.value))
                    .collect::<Vec<_>>()
            })
    };
    let click = |harness: &mut Harness<'static, GuitarScoreApp>, label: &str| {
        harness.get_by_label(label).click_accesskit();
        harness.run();
    };

    click(&mut harness, "Whammy bar");
    harness.get_by_label("Whammy bar editor");
    click(&mut harness, "Cancel");
    assert_eq!(whammy(&harness), None, "Cancel adds no whammy");

    // By default it is a dip of two steps.
    click(&mut harness, "Whammy bar");
    click(&mut harness, "OK");
    assert_eq!(whammy(&harness), Some(vec![(0, 0), (6, -8), (12, 0)]));

    click(&mut harness, "Whammy bar");
    click(&mut harness, "Preset Release down");
    click(&mut harness, "OK");
    assert_eq!(whammy(&harness), Some(vec![(0, 8), (12, 0)]));

    click(&mut harness, "Whammy bar");
    click(&mut harness, "Remove");
    assert_eq!(whammy(&harness), None, "Remove takes the whammy off");
}

#[test]
fn the_score_and_the_menu_bar_edit_bars_and_tracks() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    let counts = |harness: &Harness<'static, GuitarScoreApp>| {
        let song = harness.state().document().unwrap().song();
        (song.measure_count(), song.track_count())
    };
    let (measures, tracks) = counts(&harness);
    let menu = |harness: &mut Harness<'static, GuitarScoreApp>, path: &[&str]| {
        // Near the top of the page: the middle of the (tall) page can be scrolled away.
        let page = harness.get_by_label("Score page").rect();
        let at = egui::pos2(page.center().x, page.top() + 120.0);
        harness.event(Event::PointerMoved(at));
        for pressed in [true, false] {
            harness.event(Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
        harness.run();
        // Entries with keys carry them in their name ("Delete bar ctrl+Delete"). The menu
        // bar has buttons of the same names; the open popup's come last.
        for label in path {
            harness
                .query_all_by_label(label)
                .last()
                .unwrap_or_else(|| harness.get_by_label_contains(label))
                .click_accesskit();
            harness.run();
        }
    };

    menu(&mut harness, &["Bar", "Insert bar after"]);
    assert_eq!(counts(&harness), (measures + 1, tracks), "a bar is added");
    menu(&mut harness, &["Bar", "Delete bar"]);
    assert_eq!(counts(&harness), (measures, tracks), "and deleted again");

    menu(&mut harness, &["Track", "Duplicate track"]);
    assert_eq!(counts(&harness), (measures, tracks + 1), "a track is added");
    assert_eq!(harness.state().shown_track(), 1, "the copy is shown");
    menu(&mut harness, &["Track", "Move up"]);
    assert_eq!(
        harness.state().shown_track(),
        0,
        "the moved track stays shown"
    );
    menu(&mut harness, &["Track", "Delete track"]);
    assert_eq!(counts(&harness), (measures, tracks));

    // The menu bar has the same menus.
    harness.get_all_by_label("Bar").next().unwrap().click();
    harness.run();
    harness.get_by_label("Insert bar after").click_accesskit();
    harness.run();
    assert_eq!(
        counts(&harness),
        (measures + 1, tracks),
        "the menu bar adds a bar"
    );
}

#[test]
fn the_view_menu_switches_between_the_five_score_views() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    assert_eq!(
        harness.state().view().score_view,
        gscore_app::ScoreView::PageVertical,
        "pages by default"
    );
    harness.get_by_label("Page 1 of 1");
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Screen view (vertical)").click();
    harness.run();
    assert_eq!(
        harness.state().view().score_view,
        gscore_app::ScoreView::ScreenVertical
    );
    assert!(harness.query_by_label("Page 1 of 1").is_none());
    // The screen fills the room between the panels exactly: it does not reach under the
    // track panel, so nothing scrolls sideways.
    let surface = harness.get_by_label("Score page").rect();
    let panel = harness.get_by_label("Tablature").rect();
    assert!(
        surface.right() < panel.left(),
        "the screen ({surface:?}) stays left of the track panel ({panel:?})"
    );
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Page grid").click();
    harness.run();
    assert_eq!(
        harness.state().view().score_view,
        gscore_app::ScoreView::PageGrid
    );
    assert_eq!(
        harness.state().settings().score_view,
        gscore_app::ScoreView::PageGrid,
        "remembered in the settings"
    );
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Screen view (horizontal)").click();
    harness.run();
    assert_eq!(
        harness.state().view().score_view,
        gscore_app::ScoreView::ScreenHorizontal
    );
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Page view (vertical)").click();
    harness.run();
    assert_eq!(
        harness.state().view().score_view,
        gscore_app::ScoreView::PageVertical
    );
}

#[test]
fn staff_tiles_open_dialogs_for_repeats_barlines_and_breaks() {
    let mut harness = app_with(&testdata("basic/ornaments.gp"));
    let measure = MeasureIndex::new(1);
    {
        let document = harness.state_mut().document_mut().unwrap();
        let mut cursor = document.editor().cursor();
        cursor.measure = measure;
        cursor.beat = BeatIndex::new(0);
        document.editor_mut().set_cursor(cursor);
    }
    harness.run();
    let song = |harness: &Harness<'static, GuitarScoreApp>| {
        harness.state().document().unwrap().song().clone()
    };
    let before_repeat = song(&harness).repeat(measure).unwrap_or_default();
    let before_double_bar = song(&harness).double_bar(measure);
    let before_break = song(&harness)
        .track(TrackIndex::new(0))
        .unwrap()
        .measure(measure)
        .unwrap()
        .system_break();
    let before_feel = song(&harness).feel(measure);
    let before_fermata = song(&harness).fermata(measure);

    // Start repeat, end repeat and alternative endings have their own tiles.
    harness.get_by_label("Start repeat sign").scroll_to_me();
    harness.run();
    harness.get_by_label("Start repeat sign").click();
    harness.run();
    harness.get_by_label("Start repeat at this bar").click();
    harness.get_by_label("OK").click();
    harness.run();

    harness.get_by_label("End repeat sign").scroll_to_me();
    harness.run();
    harness.get_by_label("End repeat sign").click();
    harness.run();
    harness.get_by_label("End the repeat at this bar").click();
    harness
        .get_by_label("Repeat count")
        .set_value_accesskit(2.0);
    harness.get_by_label("OK").click();
    harness.run();

    harness.get_by_label("Alternative endings").scroll_to_me();
    harness.run();
    harness.get_by_label("Alternative endings").click();
    harness.run();
    harness.get_by_label("Ending 1").click();
    harness.get_by_label("OK").click();
    harness.run();
    assert_eq!(
        song(&harness).repeat(measure),
        Some(gscore_model::Repeat {
            start: true,
            end_repeats: Some(2),
            endings: 1,
        })
    );

    // Swing and fermata.
    harness.get_by_label("Swing feel").scroll_to_me();
    harness.run();
    harness.get_by_label("Swing feel").click();
    harness.run();
    harness.get_by_label("Triplet eighths (swing)").click();
    harness.get_by_label("OK").click();
    harness.run();
    assert_eq!(
        song(&harness).feel(measure),
        Some(gscore_model::Feel::Eighth)
    );

    harness.get_by_label("Fermata").scroll_to_me();
    harness.run();
    harness.get_by_label("Fermata").click();
    harness.run();
    harness.get_by_label("Long").click();
    harness.get_by_label("OK").click();
    harness.run();
    assert_eq!(
        song(&harness).fermata(measure),
        Some(gscore_model::Fermata::Long)
    );

    // A double barline.
    harness.get_by_label("Double barline").scroll_to_me();
    harness.run();
    harness.get_by_label("Double barline").click();
    harness.run();
    assert_eq!(song(&harness).double_bar(measure), Some(true));

    // A page break, from the staff tile.
    harness.get_by_label("Break").scroll_to_me();
    harness.run();
    harness.get_by_label("Break").click();
    harness.run();
    harness.get_by_label("Start a new page here").click();
    harness.get_by_label("OK").click();
    harness.run();
    assert_eq!(
        song(&harness)
            .track(TrackIndex::new(0))
            .unwrap()
            .measure(measure)
            .unwrap()
            .system_break(),
        gscore_model::SystemBreak::Page
    );

    // Everything was one command per dialog, so each undo takes one back.
    for _ in 0..7 {
        harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
        harness.run();
    }
    assert_eq!(song(&harness).repeat(measure), Some(before_repeat));
    assert_eq!(song(&harness).double_bar(measure), before_double_bar);
    assert_eq!(song(&harness).feel(measure), before_feel);
    assert_eq!(song(&harness).fermata(measure), before_fermata);
    assert_eq!(
        song(&harness)
            .track(TrackIndex::new(0))
            .unwrap()
            .measure(measure)
            .unwrap()
            .system_break(),
        before_break
    );
}

#[test]
fn breaks_bars_per_line_and_the_page_setup_are_edited_from_the_menus() {
    let mut harness = app_with(&testdata("basic/ornaments.gp"));
    let song = |harness: &Harness<'static, GuitarScoreApp>| {
        harness.state().document().unwrap().song().clone()
    };
    let track = TrackIndex::new(0);
    let break_at = |harness: &Harness<'static, GuitarScoreApp>, measure: usize| {
        song(harness)
            .track(track)
            .unwrap()
            .measures()
            .nth(measure)
            .unwrap()
            .system_break()
    };
    // Guitar Pro's three bars a line come in as line breaks.
    assert_eq!(break_at(&harness, 3), gscore_model::SystemBreak::Line);
    harness.get_by_label("Page 1 of 1");

    // A page break before bar 7, from the menu bar's Bar menu.
    {
        let document = harness.state_mut().document_mut().unwrap();
        let mut cursor = document.editor().cursor();
        cursor.measure = MeasureIndex::new(6);
        cursor.beat = BeatIndex::new(0);
        document.editor_mut().set_cursor(cursor);
    }
    harness.run();
    let menu = |harness: &mut Harness<'static, GuitarScoreApp>, title: &str, entry: &str| {
        harness.get_all_by_label(title).next().unwrap().click();
        harness.run();
        harness.get_by_label_contains(entry).click_accesskit();
        harness.run();
    };
    menu(&mut harness, "Bar", "Page break before this bar");
    assert_eq!(break_at(&harness, 6), gscore_model::SystemBreak::Page);
    harness.get_by_label("Page 2 of 2");
    // Ticked now: clicking again removes it.
    menu(&mut harness, "Bar", "Page break before this bar");
    assert_eq!(break_at(&harness, 6), gscore_model::SystemBreak::None);

    // Two bars a line, from the Track menu.
    harness.get_all_by_label("Track").next().unwrap().click();
    harness.run();
    harness.get_by_label("Bars per line").click_accesskit();
    harness.run();
    harness.get_by_label("2").click_accesskit();
    harness.run();
    assert_eq!(
        song(&harness).track(track).unwrap().bars_per_system(),
        Some(2)
    );

    // Landscape from the settings window's Page layout tab: one undo step.
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Settings… ctrl+,").click();
    harness.run();
    harness.get_by_label("Page layout").click();
    harness.run();
    harness.get_by_label("Page preview");
    harness.get_by_label("Landscape").click();
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    assert!(song(&harness).page_setup().is_landscape());
    harness.get_by_label("Close Settings").click();
    harness.run();
    harness.get_by_label("Edit").click();
    harness.run();
    harness.get_by_label_contains("Undo ctrl+z").click();
    harness.run();
    assert!(
        !song(&harness).page_setup().is_landscape(),
        "undone in one step"
    );
}

/// Asks the window to close, as the window manager's close button does.
fn request_window_close(harness: &mut Harness<'static, GuitarScoreApp>) {
    harness
        .input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    harness.run();
}

#[test]
fn a_save_that_fails_while_closing_the_window_asks_again_on_the_next_close() {
    let (dir, file) = scratch_copy("note_lengths.gp", "close-fail");
    let mut harness = app_with(&file);
    type_char(&mut harness, "3", Modifiers::NONE);
    assert!(harness.state().document().unwrap().is_dirty());

    // Saving into a folder that cannot be written fails.
    let set_mode = |mode| {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    set_mode(0o555);
    if std::fs::write(dir.join("probe"), b"").is_ok() {
        // Running as root: the folder is writable anyway; nothing to test.
        set_mode(0o755);
        std::fs::remove_dir_all(&dir).unwrap();
        return;
    }

    request_window_close(&mut harness);
    assert!(
        harness.state().has_dialog(),
        "closing asks about the changes"
    );
    // The dialog's Save (the toolbar has a Save button too; the dialog is drawn last).
    harness.get_all_by_label("Save").last().unwrap().click();
    settle(&mut harness);
    assert!(harness.state().notice().unwrap().starts_with("Not saved"));
    assert!(harness.state().document().unwrap().is_dirty());
    assert!(!harness.state().has_dialog());

    // The window stayed open, and closing it again asks again (it must not just close and
    // throw the changes and their recovery journal away).
    request_window_close(&mut harness);
    assert!(harness.state().has_dialog(), "the next close asks again");

    set_mode(0o755);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_file_chosen_while_another_opens_waits_its_turn() {
    let (dir, file) = scratch_copy("note_lengths.gp", "queue");
    std::fs::copy(
        testdata("basic/steel_guitar_dadgad.gp"),
        dir.join("steel_guitar_dadgad.gp"),
    )
    .unwrap();
    let mut harness = app_with(&file);
    let ctx = harness.ctx.clone();
    // Two opens in a row: the second waits for the first instead of racing it.
    harness
        .state_mut()
        .open(testdata("basic/steel_guitar_dadgad.gp"), &ctx);
    harness
        .state_mut()
        .open(dir.join("steel_guitar_dadgad.gp"), &ctx);
    // Asking for a waiting file again does not queue it twice.
    harness
        .state_mut()
        .open(dir.join("steel_guitar_dadgad.gp"), &ctx);
    settle(&mut harness);
    assert_eq!(harness.state().tab_count(), 3);
    assert_eq!(harness.state().active_tab(), Some(2));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn shift_g_opens_the_grace_note_dialog_and_ok_sets_the_chosen_speed() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    type_char(&mut harness, "5", Modifiers::NONE);
    harness.run();
    type_char(&mut harness, "G", Modifiers::SHIFT);
    harness.run();
    harness.get_by_label("Slow").click();
    harness.run();
    harness.get_by_label("On the beat").click();
    harness.run();
    harness.get_by_label("2 below").click();
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    let grace_at_cursor = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        let cursor = document.editor().cursor();
        let beat = document.song().beat(cursor.position()).unwrap();
        beat.notes()
            .find(|note| note.string().unwrap().get() == cursor.string)
            .unwrap()
            .grace()
    };
    assert_eq!(
        grace_at_cursor(&harness),
        Some(gscore_model::Grace {
            fret: gscore_model::Fret::new(3).unwrap(),
            speed: gscore_model::GraceSpeed::Slow,
            on_beat: true,
            dead: false,
            hammer: false,
        })
    );
    // The palette tile reopens the dialog on it; Remove takes the grace note off.
    harness.get_by_label("Grace note").scroll_to_me();
    harness.run();
    harness.get_by_label("Grace note").click();
    harness.run();
    harness.get_by_label("Remove").click();
    harness.run();
    assert_eq!(grace_at_cursor(&harness), None);
}

#[test]
fn the_sextuplet_tile_and_the_tuplet_dialog_set_the_beats_tuplet() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    type_char(&mut harness, "5", Modifiers::NONE);
    harness.run();
    // An eighth note, so taking a tuplet off again must leave an eighth note: the tuplet
    // choice changes the tuplet only, not the written value.
    harness.get_by_label("1/8").click();
    harness.run();
    let value_at_cursor = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        let cursor = document.editor().cursor();
        let beat = document.song().beat(cursor.position()).unwrap();
        beat.note_value().unwrap().value()
    };
    assert_eq!(value_at_cursor(&harness), 8);
    let tuplet_at_cursor = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        let cursor = document.editor().cursor();
        let beat = document.song().beat(cursor.position()).unwrap();
        beat.note_value().unwrap().tuplet()
    };
    harness.get_by_label("Sextuplet").click();
    harness.run();
    assert_eq!(tuplet_at_cursor(&harness), Some((6, 4)));
    assert_eq!(value_at_cursor(&harness), 8, "the written value is kept");
    // The dialog opens on the current tuplet; a common one is a click away.
    harness.get_by_label("Tuplet…").click();
    harness.run();
    harness.get_by_label("Septuplet").click();
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    assert_eq!(tuplet_at_cursor(&harness), Some((7, 4)));
    // Steppers reach any tuplet: 7:4 becomes 8:4... and 9:4 is not a common one.
    harness.get_by_label("Tuplet…").click();
    harness.run();
    harness.get_by_label("More: Notes").click();
    harness.run();
    harness.get_by_label("More: Notes").click();
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    assert_eq!(tuplet_at_cursor(&harness), Some((9, 4)));
    // Remove takes the tuplet off.
    harness.get_by_label("Tuplet…").click();
    harness.run();
    harness.get_by_label("Remove").click();
    harness.run();
    assert_eq!(tuplet_at_cursor(&harness), None);
    assert_eq!(
        value_at_cursor(&harness),
        8,
        "removing the tuplet restores the original note value"
    );
}

#[test]
fn clefs_and_free_time_are_edited_from_the_staff_tiles() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    let measure = MeasureIndex::new(0);
    {
        let document = harness.state_mut().document_mut().unwrap();
        let mut cursor = document.editor().cursor();
        cursor.measure = measure;
        document.editor_mut().set_cursor(cursor);
    }
    harness.run();

    harness.get_by_label("Clef").scroll_to_me();
    harness.run();
    harness.get_by_label("Clef").click();
    harness.run();
    harness.get_by_label("Bass clef").click();
    harness.get_by_label("OK").click();
    harness.run();
    let clef = || {
        harness
            .state()
            .document()
            .unwrap()
            .song()
            .track(TrackIndex::new(0))
            .unwrap()
            .measure(measure)
            .unwrap()
            .clef()
    };
    assert_eq!(clef(), Some(gscore_model::Clef::Bass));

    // Free time is a switch tile: one click, no dialog.
    harness.get_by_label("Free time").scroll_to_me();
    harness.run();
    harness.get_by_label("Free time").click();
    harness.run();
    assert_eq!(
        harness
            .state()
            .document()
            .unwrap()
            .song()
            .free_time(measure),
        Some(true)
    );

    // Both are ordinary undoable commands.
    let _ = harness
        .state_mut()
        .document_mut()
        .unwrap()
        .editor_mut()
        .undo();
    harness.run();
    assert_eq!(
        harness
            .state()
            .document()
            .unwrap()
            .song()
            .free_time(measure),
        Some(false)
    );
}

#[test]
fn bar_repetitions_are_switch_tiles() {
    use gscore_model::SimileMark;
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    let move_to = |harness: &mut Harness<'static, GuitarScoreApp>, measure: usize| {
        let document = harness.state_mut().document_mut().unwrap();
        let mut cursor = document.editor().cursor();
        cursor.measure = MeasureIndex::new(measure);
        document.editor_mut().set_cursor(cursor);
        harness.run();
    };
    let simile = |harness: &Harness<'static, GuitarScoreApp>, measure: usize| {
        harness
            .state()
            .document()
            .unwrap()
            .song()
            .track(TrackIndex::new(0))
            .unwrap()
            .measure(MeasureIndex::new(measure))
            .unwrap()
            .simile_mark()
    };
    let enabled = |harness: &Harness<'static, GuitarScoreApp>, label: &str| {
        use egui_kittest::kittest::NodeT as _;
        !harness.get_by_label(label).accesskit_node().is_disabled()
    };

    // The first bar has nothing before it to repeat; the second has one bar, not two.
    move_to(&mut harness, 0);
    harness.get_by_label("Bar repeat").scroll_to_me();
    harness.run();
    assert!(!enabled(&harness, "Bar repeat"));
    assert!(!enabled(&harness, "Two-bar repeat"));
    move_to(&mut harness, 1);
    assert!(enabled(&harness, "Bar repeat"));
    assert!(!enabled(&harness, "Two-bar repeat"));

    move_to(&mut harness, 2);
    harness.get_by_label("Bar repeat").click();
    harness.run();
    assert_eq!(simile(&harness, 2), Some(SimileMark::Simple));
    harness.get_by_label("Bar repeat").click();
    harness.run();
    assert_eq!(simile(&harness, 2), None);

    harness.get_by_label("Two-bar repeat").click();
    harness.run();
    assert_eq!(simile(&harness, 2), Some(SimileMark::FirstOfDouble));
    assert_eq!(simile(&harness, 3), Some(SimileMark::SecondOfDouble));
    // A one-bar repeat on the pair's second half takes the whole pair's place.
    move_to(&mut harness, 3);
    harness.get_by_label("Bar repeat").click();
    harness.run();
    assert_eq!(simile(&harness, 2), None);
    assert_eq!(simile(&harness, 3), Some(SimileMark::Simple));
    // Clicking a pair's second half clears both halves.
    move_to(&mut harness, 2);
    harness.get_by_label("Two-bar repeat").click();
    harness.run();
    assert_eq!(simile(&harness, 3), Some(SimileMark::SecondOfDouble));
    move_to(&mut harness, 3);
    harness.get_by_label("Two-bar repeat").click();
    harness.run();
    assert_eq!(simile(&harness, 2), None);
    assert_eq!(simile(&harness, 3), None);
}

#[test]
fn a_selection_in_the_score_can_be_looped() {
    use egui_kittest::kittest::NodeT as _;
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    let enabled = |harness: &Harness<'static, GuitarScoreApp>| {
        !harness
            .get_by_label("Loop selection")
            .accesskit_node()
            .is_disabled()
    };
    // With nothing selected, Loop repeats the whole song.
    assert!(enabled(&harness), "a song is enough to loop");
    // Select beats in the score until the selection reaches the next bar.
    for _ in 0..6 {
        harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
        harness.run();
    }
    let selection = harness
        .state()
        .document()
        .unwrap()
        .editor()
        .selection()
        .expect("a selection");
    assert!(selection.last_measure() > selection.first_measure());
    assert!(enabled(&harness), "the score's selection can be looped");
    // A selection of several beats turns Loop on by itself.
    assert!(harness.state().playback().loop_selection());
    // Turned off, it stays off while the selection grows.
    harness.get_by_label("Loop selection").click();
    harness.run();
    assert!(!harness.state().playback().loop_selection());
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
    harness.run();
    assert!(!harness.state().playback().loop_selection());
    // A new selection turns it on again.
    harness.key_press(Key::ArrowRight);
    harness.run();
    assert!(
        harness
            .state()
            .document()
            .unwrap()
            .editor()
            .selection()
            .is_none()
    );
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
    harness.run();
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
    harness.run();
    assert!(harness.state().playback().loop_selection());
}

#[test]
fn shift_h_opens_the_history() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    assert!(harness.query_by_label("Close History").is_none());
    type_char(&mut harness, "H", Modifiers::SHIFT);
    harness.run();
    harness.get_by_label("Close History");
}

#[test]
fn ctrl_c_and_ctrl_v_copy_one_note_and_a_beat_selection() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    // A note on the cursor's string, with an effect: Copy takes it without a selection.
    put_cursor(&mut harness, 0, 0, 1);
    type_char(&mut harness, "2", Modifiers::NONE);
    type_char(&mut harness, "p", Modifiers::NONE);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::C);
    harness.run();
    press(&mut harness, Key::ArrowDown);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::V);
    harness.run();
    assert_eq!(harness.state().notice(), None);
    assert_eq!(notes_at(&harness, 0, 0), vec![(6, 0), (1, 2), (2, 2)]);
    assert!(
        harness
            .state()
            .document()
            .unwrap()
            .song()
            .beat(BeatPosition {
                track: TrackIndex::new(0),
                measure: MeasureIndex::new(0),
                voice: VoiceIndex::new(0),
                beat: BeatIndex::new(0),
            })
            .unwrap()
            .notes()
            .find(|note| note.string().unwrap().get() == 2)
            .unwrap()
            .is_palm_muted(),
        "the copied effect came along"
    );

    // Two beats selected in the score: Copy and Paste replace whole beats.
    put_cursor(&mut harness, 1, 0, 1);
    for _ in 0..2 {
        harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
        harness.run();
    }
    assert!(
        harness
            .state()
            .document()
            .unwrap()
            .editor()
            .selection()
            .is_some()
    );
    harness.key_press_modifiers(Modifiers::COMMAND, Key::C);
    harness.run();
    put_cursor(&mut harness, 4, 0, 1);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::V);
    harness.run();
    assert_eq!(harness.state().notice(), None);
    assert_eq!(notes_at(&harness, 1, 0), notes_at(&harness, 4, 0));
    assert_eq!(notes_at(&harness, 1, 1), notes_at(&harness, 4, 1));
}

/// The text the last frame put on the system clipboard, if any.
fn copied_text(harness: &Harness<'static, GuitarScoreApp>) -> Option<String> {
    harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
}

/// Ctrl+C and Ctrl+V as a desktop delivers them: egui's window layer turns Ctrl+C into a
/// Copy event (no key press), and Ctrl+V into a Paste event only when the system
/// clipboard holds text, with that text.
#[test]
fn copy_and_paste_work_as_the_desktop_sends_them() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    put_cursor(&mut harness, 0, 0, 1);
    type_char(&mut harness, "2", Modifiers::NONE);
    harness.event(Event::Copy);
    harness.step();
    // The copy reaches the system clipboard, so the desktop sends the next Ctrl+V on.
    let text = copied_text(&harness).expect("Copy puts text on the system clipboard");
    assert_eq!(text, "GuitarScore: fret 2");
    harness.run();
    put_cursor(&mut harness, 1, 0, 3);
    harness.event(Event::Paste(text));
    harness.run();
    assert_eq!(harness.state().notice(), None);
    assert!(
        notes_at(&harness, 1, 0).contains(&(3, 2)),
        "{:?}",
        notes_at(&harness, 1, 0)
    );

    // A beat selection the same way: Copy, then Paste with the clipboard's text.
    put_cursor(&mut harness, 1, 0, 1);
    for _ in 0..2 {
        harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
        harness.run();
    }
    harness.event(Event::Copy);
    harness.step();
    let text = copied_text(&harness).expect("Copy puts text on the system clipboard");
    assert_eq!(text, "GuitarScore: 3 beats");
    harness.run();
    put_cursor(&mut harness, 4, 0, 1);
    harness.event(Event::Paste(text));
    harness.run();
    assert_eq!(harness.state().notice(), None);
    for beat in 0..3 {
        assert_eq!(notes_at(&harness, 1, beat), notes_at(&harness, 4, beat));
    }

    // Cut puts its text there too (a plain arrow ends the pasted selection first).
    press(&mut harness, Key::ArrowLeft);
    put_cursor(&mut harness, 1, 0, 3);
    harness.event(Event::Cut);
    harness.step();
    assert_eq!(
        copied_text(&harness).as_deref(),
        Some("GuitarScore: fret 2")
    );
}

#[test]
fn the_edit_menu_copies_and_pastes_a_note_and_a_beat_selection() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    put_cursor(&mut harness, 0, 0, 1);
    type_char(&mut harness, "3", Modifiers::NONE);
    harness.get_by_label("Edit").click();
    harness.run();
    harness.get_by_label_contains("Copy ctrl+c").click();
    harness.run();
    press(&mut harness, Key::ArrowDown);
    harness.get_by_label("Edit").click();
    harness.run();
    harness.get_by_label_contains("Paste ctrl+v").click();
    harness.run();
    assert_eq!(notes_at(&harness, 0, 0), vec![(6, 0), (1, 3), (2, 3)]);

    put_cursor(&mut harness, 1, 0, 1);
    for _ in 0..2 {
        harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
        harness.run();
    }
    harness.get_by_label("Edit").click();
    harness.run();
    harness.get_by_label_contains("Copy ctrl+c").click();
    harness.run();
    put_cursor(&mut harness, 4, 0, 1);
    harness.get_by_label("Edit").click();
    harness.run();
    harness.get_by_label_contains("Paste ctrl+v").click();
    harness.run();
    assert_eq!(notes_at(&harness, 1, 0), notes_at(&harness, 4, 0));
    assert_eq!(notes_at(&harness, 1, 1), notes_at(&harness, 4, 1));
}

#[test]
fn a_paste_that_needs_more_bars_says_so() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    let document = harness.state_mut().document_mut().unwrap();
    document
        .editor_mut()
        .set_selection(Some(gscore_edit::Selection::Beats {
            track: TrackIndex::new(0),
            voice: VoiceIndex::new(0),
            anchor: gscore_edit::BeatSpot {
                measure: MeasureIndex::new(7),
                beat: BeatIndex::new(0),
            },
            focus: gscore_edit::BeatSpot {
                measure: MeasureIndex::new(8),
                beat: BeatIndex::new(0),
            },
        }));
    harness.key_press_modifiers(Modifiers::COMMAND, Key::C);
    harness.run();
    let last = harness
        .state()
        .document()
        .unwrap()
        .song()
        .measure_count()
        .saturating_sub(1);
    put_cursor(&mut harness, last, 0, 1);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::V);
    harness.run();
    let notice = harness.state().notice().unwrap_or_default();
    assert!(notice.contains("insert measures first"), "{notice}");
}
