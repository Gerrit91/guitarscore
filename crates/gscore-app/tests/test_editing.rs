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
    harness.get_by_label("Little").click_accesskit();
    harness.run();
    harness.get_by_label("OK").click_accesskit();
    harness.run();
    assert_eq!(finger(&harness, Hand::Left), Some(Finger::Little));

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
    harness.get_by_label("Settings…").click();
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
        matches!(event.event, gscore_play::MidiEvent::PitchBend { value, .. } if value > 8192 + 500)
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
fn the_view_menu_switches_between_pages_and_the_endless_page() {
    let mut harness = app_with(&testdata("basic/note_lengths.gp"));
    assert!(harness.state().view().page_view, "pages by default");
    harness.get_by_label("Page 1 of 1");
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Endless view").click();
    harness.run();
    assert!(!harness.state().view().page_view);
    assert!(harness.query_by_label("Page 1 of 1").is_none());
    assert!(
        !harness.state().settings().page_view,
        "remembered in the settings"
    );
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label("Page view").click();
    harness.run();
    assert!(harness.state().view().page_view);
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
    harness.get_by_label("Barline").scroll_to_me();
    harness.run();
    harness.get_by_label("Barline").click();
    harness.run();
    harness
        .get_by_label("End this bar with a double barline")
        .click();
    harness.get_by_label("OK").click();
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
    harness.get_by_label("Settings…").click();
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
    let tuplet_at_cursor = |harness: &Harness<'static, GuitarScoreApp>| {
        let document = harness.state().document().unwrap();
        let cursor = document.editor().cursor();
        let beat = document.song().beat(cursor.position()).unwrap();
        beat.note_value().unwrap().tuplet()
    };
    harness.get_by_label("Sextuplet").click();
    harness.run();
    assert_eq!(tuplet_at_cursor(&harness), Some((6, 4)));
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
}
