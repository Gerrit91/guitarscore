//! The track panel (preset colors, the staves choice, note-name tuning), the song grid's
//! action bar (add, move, mute all, solo all) and master row, and jumping during playback
//! from the page or the grid.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable};
use gscore_app::GuitarScoreApp;
use gscore_model::{MasterMix, Song, TrackIndex, TrackStaves};

fn testdata(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(relative)
}

fn app_with(file: &str) -> Harness<'static, GuitarScoreApp> {
    let file = testdata(file);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 1100.0))
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(file)));
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(started.elapsed() < Duration::from_secs(30), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run();
    harness
}

fn song(harness: &Harness<'static, GuitarScoreApp>) -> Song {
    harness.state().document().unwrap().song().clone()
}

fn undo(harness: &mut Harness<'static, GuitarScoreApp>) {
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    harness.run();
}

#[test]
fn a_preset_color_and_the_staves_are_one_click_each() {
    let mut harness = app_with("basic/steel_guitar_dadgad.gp");
    let track = TrackIndex::new(0);
    let before = song(&harness).track(track).unwrap().color();

    harness.get_by_label("Track color").click();
    harness.run();
    harness.get_by_label("Color Blue").click();
    harness.run();
    assert_eq!(
        song(&harness).track(track).unwrap().color().rgb(),
        0x1E_88E5
    );
    undo(&mut harness);
    assert_eq!(song(&harness).track(track).unwrap().color(), before);

    harness.get_by_label("Tablature").click();
    harness.run();
    assert_eq!(
        song(&harness).track(track).unwrap().staves(),
        TrackStaves::NOTATION_ONLY
    );
    harness.get_by_label("Slash notation").click();
    harness.run();
    assert_eq!(
        song(&harness).track(track).unwrap().staves(),
        TrackStaves::NOTATION_AND_SLASH
    );
    harness.get_by_label("Standard notation").click();
    harness.run();
    assert_eq!(
        song(&harness).track(track).unwrap().staves(),
        TrackStaves::SLASH_ONLY
    );
}

#[test]
fn strings_are_tuned_by_note_name() {
    let mut harness = app_with("basic/steel_guitar_dadgad.gp");
    let pitch = |harness: &Harness<'static, GuitarScoreApp>| {
        let song = song(harness);
        let tuning = song
            .track(TrackIndex::new(0))
            .unwrap()
            .tuning()
            .unwrap()
            .unwrap();
        tuning[0].pitch.to_string()
    };
    // DADGAD: string 1 is D4, shown by its name.
    assert_eq!(pitch(&harness), "D4");
    harness.get_by_label("Tuning: DADGAD…").click();
    harness.run();
    harness.get_by_label("String 1 note");
    // The first "+" is string 1's: a semitone up.
    harness.get_all_by_label("+").next().unwrap().click();
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    assert_eq!(pitch(&harness), "D#4");
}

#[test]
fn the_action_bar_adds_moves_mutes_and_solos_tracks() {
    let mut harness = app_with("basic/steel_guitar_dadgad.gp");
    harness.get_by_label("New track").click();
    harness.run();
    assert_eq!(song(&harness).track_count(), 1, "the popup comes first");
    harness.get_by_label("Track name");
    harness.get_by_label("Create track").click();
    harness.run();
    assert_eq!(song(&harness).track_count(), 2);
    assert_eq!(harness.state().shown_track(), 1, "the new track is shown");

    let name = |harness: &Harness<'static, GuitarScoreApp>, index| {
        song(harness)
            .track(TrackIndex::new(index))
            .unwrap()
            .name()
            .to_owned()
    };
    let new_name = name(&harness, 1);
    harness.get_by_label("Move track up").click();
    harness.run();
    assert_eq!(name(&harness, 0), new_name);
    assert_eq!(
        harness.state().shown_track(),
        0,
        "the moved track stays shown"
    );

    harness.get_by_label("Mute all").click();
    harness.run();
    assert!(song(&harness).tracks().all(|track| track.is_muted()));
    harness.get_by_label("Mute all").click();
    harness.run();
    assert!(song(&harness).tracks().all(|track| !track.is_muted()));
    undo(&mut harness);
    assert!(
        song(&harness).tracks().all(|track| track.is_muted()),
        "one undo step for all tracks"
    );

    harness.get_by_label("Solo all").click();
    harness.run();
    assert!(song(&harness).tracks().all(|track| track.is_soloed()));
}

#[test]
fn the_master_row_sets_the_volume_of_all_tracks() {
    let mut harness = app_with("basic/note_lengths.gp");
    // The file has Guitar Pro's default master, which leaves every track as it is.
    let before = song(&harness).master_mix();
    assert_eq!(before, MasterMix::default());
    harness
        .get_by_label("Master volume")
        .set_value_accesskit(64.0);
    harness.run();
    assert_eq!(song(&harness).master_mix().volume, 64);
    let played = song(&harness).played_sound(TrackIndex::new(0)).unwrap();
    let own = song(&harness).track_sound(TrackIndex::new(0)).unwrap();
    assert_eq!(
        u16::from(played.volume),
        u16::from(own.volume) * 64 / u16::from(gscore_model::MASTER_UNITY)
    );
    undo(&mut harness);
    assert_eq!(song(&harness).master_mix(), before);
    assert_eq!(
        harness
            .get_by_label("Master volume")
            .accesskit_node()
            .numeric_value(),
        Some(f64::from(gscore_model::MASTER_UNITY))
    );
}

/// Starts playback to ALSA's silent "Midi Through" port; false (test skipped) without one.
fn start_playing(harness: &mut Harness<'static, GuitarScoreApp>) -> bool {
    const THROUGH: &str = "Midi Through";
    let playback = harness.state_mut().playback_mut();
    let port = playback.refresh_ports().ok().and_then(|()| {
        playback
            .ports()
            .iter()
            .find(|p| p.contains(THROUGH))
            .cloned()
    });
    let Some(port) = port else {
        eprintln!("skipped: no \"{THROUGH}\" MIDI port");
        return false;
    };
    harness.state_mut().playback_mut().choose_port(Some(port));
    harness.get_by_label("Play").click();
    harness.step();
    let started = Instant::now();
    while harness.state().playback().state() != gscore_play::PlayState::Playing {
        assert!(started.elapsed() < Duration::from_secs(5), "did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    true
}

/// The first beat of `measure` in track 1.
fn measure_start(measure: usize) -> gscore_model::BeatPosition {
    gscore_model::BeatPosition {
        track: TrackIndex::new(0),
        measure: gscore_model::MeasureIndex::new(measure),
        voice: gscore_model::VoiceIndex::new(0),
        beat: gscore_model::BeatIndex::new(0),
    }
}

/// Waits until playback has reached `at` (the player seeks on its own thread).
fn wait_for_position(harness: &Harness<'static, GuitarScoreApp>, at: gscore_model::BeatPosition) {
    let song = song(harness);
    let tick = harness
        .state()
        .playback()
        .sequence()
        .unwrap()
        .tick_of(&song, at)
        .unwrap();
    let jumped = Instant::now();
    while harness.state().playback().position() < tick {
        assert!(jumped.elapsed() < Duration::from_secs(5), "did not jump");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        harness.state().playback().state(),
        gscore_play::PlayState::Playing,
        "it plays on from there"
    );
}

#[test]
fn jumping_while_playing_moves_the_playback_position() {
    let mut harness = app_with("basic/note_lengths.gp");
    if !start_playing(&mut harness) {
        return;
    }
    let song = song(&harness);
    let target = measure_start(song.measure_count() - 1);
    harness
        .state_mut()
        .playback_mut()
        .jump_to(&song, target)
        .unwrap();
    wait_for_position(&harness, target);
    harness.state_mut().playback_mut().stop().unwrap();
}

#[test]
fn a_click_in_the_song_grid_while_playing_jumps_there() {
    let mut harness = app_with("basic/note_lengths.gp");
    if !start_playing(&mut harness) {
        return;
    }
    // The third measure, first track: 24 px square cells under a 26 px ruler.
    let rect = harness.get_by_label("Song grid").rect();
    let pos = rect.left_top() + egui::vec2(24.0 * 2.0 + 12.0, 12.0);
    harness.hover_at(pos);
    harness.drag_at(pos);
    harness.drop_at(pos);
    harness.step();
    wait_for_position(&harness, measure_start(2));
    harness.state_mut().playback_mut().stop().unwrap();
}

#[test]
fn the_sound_section_opens_the_sound_chain_and_the_strings_sound() {
    let mut harness = app_with("basic/note_lengths.gp");
    // The track panel's Sound section opens the Sound chain with its stomp box.
    harness.get_by_label("Sound chain").click();
    harness.run();
    // The first box is what plays the track; a Guitar Pro file starts on the MIDI synth.
    harness.get_by_label("Block 0: MIDI").click();
    harness.run();
    harness.get_by_label("Played by");
    assert!(
        harness.query_all_by_label("Brilliance").next().is_none(),
        "the SoundFont plays it: no string knobs"
    );
    // Physical modelling brings the kind of string and its knobs into the same window.
    harness.get_by_label("PM Synthesis").click();
    harness.run();
    harness.snapshot("sound_chain_strings");
    for knob in [
        "Pick position",
        "Roundness",
        "Brilliance",
        "Bridge",
        "Second vibration",
    ] {
        assert!(harness.query_all_by_label(knob).next().is_some(), "{knob}");
    }
    harness.get_by_label("Nylon guitar").click();
    harness.run();
    // Clicking a knob's value lets one type a number.
    harness.get_by_label("Pick position value").click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.get_by_label("Pick position value").type_text("0.4");
    harness.run();
    eprintln!(
        "FIELD TEXT {:?}",
        harness
            .get_by_label("Pick position value")
            .accesskit_node()
            .value()
    );
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(
        (harness
            .state()
            .playback()
            .string_sounds()
            .get(gscore_play::StringKind::Nylon)
            .pick_position
            - 0.4)
            .abs()
            < 1e-6,
        "the typed value is taken"
    );
    // Back to the MIDI synth: the knobs go.
    harness
        .query_all_by_label_contains("MIDI synth")
        .next()
        .unwrap()
        .click();
    harness.run();
    assert!(harness.query_all_by_label("Brilliance").next().is_none());
}

#[test]
fn a_preset_brings_its_kind_of_string_back() {
    let mut harness = app_with("basic/note_lengths.gp");
    harness.get_by_label("Sound chain").click();
    harness.run();
    // A steel sound dialed in (PM synthesis, steel strings) with a reverb behind it, saved.
    harness.get_by_label("Block 0: MIDI").click();
    harness.run();
    harness.get_by_label("PM Synthesis").click();
    harness.run();
    harness.get_by_label("Steel guitar").click();
    harness.run();
    harness.get_by_label("Block 1: +").click();
    harness.run();
    harness.get_by_label("Reverb").click();
    harness.run();
    harness.get_by_label("Block 1: Reverb");
    harness.get_by_label("Save as…").click();
    harness.run();
    harness.get_by_label("Preset name").focus();
    harness
        .get_by_label("Preset name")
        .type_text("Natural Steel 1");
    harness.run();
    harness.query_all_by_label("Save").last().unwrap().click();
    harness.run();
    assert_eq!(preset_shown(&harness), "Natural Steel 1");

    // Another kind of string (electric): its chain is empty.
    harness.get_by_label("Block 0: Strings").click();
    harness.run();
    harness.get_by_label("Electric guitar").click();
    harness.run();
    harness.get_by_label("Block 1: +");
    assert_eq!(preset_shown(&harness), "Built-in");

    // Choosing the steel preset brings back the steel strings and their chain, not just
    // values on the electric ones.
    harness
        .query_all_by_label("Preset")
        .find(|node| node.accesskit_node().role() == egui::accesskit::Role::ComboBox)
        .unwrap()
        .click();
    harness.run();
    harness.get_by_label("Natural Steel 1").click();
    harness.run();
    harness.get_by_label("Block 1: Reverb");
    assert_eq!(preset_shown(&harness), "Natural Steel 1");
}

/// What the Sound chain window's preset list shows.
fn preset_shown(harness: &Harness<'_, GuitarScoreApp>) -> String {
    harness
        .query_all_by_label("Preset")
        .find(|node| node.accesskit_node().role() == egui::accesskit::Role::ComboBox)
        .and_then(|node| node.value())
        .unwrap_or_default()
}

#[test]
fn a_saved_chain_preset_brings_back_the_whole_chain() {
    use gscore_app::chain_settings::BlockChoice;

    let mut harness = app_with("basic/note_lengths.gp");
    harness.get_by_label("Sound chain").click();
    harness.run();
    // A computed reverb in the first place.
    harness.get_by_label("Block 1: +").click();
    harness.run();
    harness.get_by_label("Reverb").click();
    harness.run();
    harness.get_by_label("Block 1: Reverb");
    // Save the chain (with the string sound it was dialed in with) under a name.
    harness.get_by_label("Save as…").click();
    harness.run();
    harness.get_by_label("Preset name").focus();
    harness.get_by_label("Preset name").type_text("Hall 1");
    harness.run();
    // The preset's Save (the toolbar has a Save too).
    harness.query_all_by_label("Save").last().unwrap().click();
    harness.run();
    assert_eq!(preset_shown(&harness), "Hall 1");
    let preset = harness
        .state()
        .settings()
        .string_presets
        .iter()
        .find(|preset| preset.name == "Hall 1")
        .cloned()
        .expect("the preset is kept");
    assert_eq!(
        preset.chain.blocks.first().map(|block| block.choice),
        Some(BlockChoice::Reverb),
        "the chain is part of the preset"
    );

    // Remove the block from its context menu: the preset no longer matches.
    let block = harness.get_by_label("Block 1: Reverb").rect();
    harness.hover_at(block.center());
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: block.center(),
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    harness.run();
    harness.get_by_label("Remove").click();
    harness.run();
    harness.get_by_label("Block 1: +");
    assert_eq!(preset_shown(&harness), "Built-in");
    // Choosing the preset brings the block back.
    harness
        .query_all_by_label("Preset")
        .find(|node| node.accesskit_node().role() == egui::accesskit::Role::ComboBox)
        .unwrap()
        .click();
    harness.run();
    harness.get_by_label("Hall 1").click();
    harness.run();
    harness.get_by_label("Block 1: Reverb");
    assert_eq!(preset_shown(&harness), "Hall 1");

    // Every preset is listed, whatever kind of string it was saved for: switch the track
    // to another kind and the preset is still there, and choosing it comes back to its own.
    harness.get_by_label("Block 0: MIDI").click();
    harness.run();
    harness.get_by_label("PM Synthesis").click();
    harness.run();
    harness.get_by_label("Block 0: Strings").click();
    harness.run();
    harness.get_by_label("Nylon guitar").click();
    harness.run();
    harness.get_by_label("Block 1: +");
    harness
        .query_all_by_label("Preset")
        .find(|node| node.accesskit_node().role() == egui::accesskit::Role::ComboBox)
        .unwrap()
        .click();
    harness.run();
    // The preset of the other kind is in the list, and brings its chain and kind back.
    harness.get_by_label("Hall 1").click();
    harness.run();
    harness.get_by_label("Block 1: Reverb");
    assert_eq!(preset_shown(&harness), "Hall 1");
}
