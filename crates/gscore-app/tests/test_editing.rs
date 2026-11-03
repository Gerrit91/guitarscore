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
use gscore_app::GuitarScoreApp;
use gscore_model::StringNumber;

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
fn opening_another_file_with_unsaved_changes_asks_first() {
    let (dir, file) = scratch_copy("note_lengths.gp", "unsaved");
    let mut harness = app_with(&file);
    type_char(&mut harness, "3", Modifiers::NONE);
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .open(testdata("basic/steel_guitar_dadgad.gp"), &ctx);
    harness.run();
    assert!(harness.state().has_dialog());
    harness.get_by_label("Unsaved changes");
    // Keys do not edit while the question is open.
    type_char(&mut harness, "5", Modifiers::NONE);
    assert_eq!(fret_at_cursor(harness.state(), 1), Some(3));

    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(!harness.state().has_dialog());
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Note Lengths"
    );

    harness
        .state_mut()
        .open(testdata("basic/steel_guitar_dadgad.gp"), &ctx);
    harness.run();
    harness.get_by_label("Discard changes").click();
    settle(&mut harness);
    assert_eq!(
        harness.state().document().unwrap().display_title(),
        "Steel Guitar DADGAD"
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
    harness.state_mut().view_mut().select_measure(0, false);
    harness
        .state_mut()
        .document_mut()
        .unwrap()
        .editor_mut()
        .set_selection(Some(gscore_edit::Selection::Measures {
            anchor: gscore_model::MeasureIndex::new(0),
            focus: gscore_model::MeasureIndex::new(0),
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
        harness.state().view().selection.is_none(),
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
    assert_eq!(harness.state().view().track, 1, "the copy is shown");
    menu(&mut harness, &["Track", "Move up"]);
    assert_eq!(
        harness.state().view().track,
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
fn breaks_bars_per_line_and_the_page_setup_are_edited_from_the_menus() {
    let mut harness = app_with(&testdata("basic/ornaments.gp"));
    let song = |harness: &Harness<'static, GuitarScoreApp>| {
        harness.state().document().unwrap().song().clone()
    };
    let track = gscore_model::TrackIndex::new(0);
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
        cursor.measure = gscore_model::MeasureIndex::new(6);
        cursor.beat = gscore_model::BeatIndex::new(0);
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
