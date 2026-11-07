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
            .tone(0, 26, gscore_play::StringKind::Nylon)
            .sound
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
    harness.get_by_label("Tone name").focus();
    harness
        .get_by_label("Tone name")
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
        .query_all_by_label("Tone")
        .find(|node| node.accesskit_node().role() == egui::accesskit::Role::ComboBox)
        .unwrap()
        .click();
    harness.run();
    harness.get_by_label("Natural Steel 1").click();
    harness.run();
    harness.get_by_label("Block 1: Reverb");
    assert_eq!(preset_shown(&harness), "Natural Steel 1");
}

#[test]
fn a_preset_can_be_saved_over_and_deleted_while_active() {
    let mut harness = app_with("basic/note_lengths.gp");
    harness.get_by_label("Sound chain").click();
    harness.run();
    // A block, saved under a name.
    harness.get_by_label("Block 1: +").click();
    harness.run();
    harness.get_by_label("Reverb").click();
    harness.run();
    harness.get_by_label("Save as…").click();
    harness.run();
    harness.get_by_label("Tone name").focus();
    harness.get_by_label("Tone name").type_text("Room 1");
    harness.run();
    harness.query_all_by_label("Save").last().unwrap().click();
    harness.run();
    assert_eq!(preset_shown(&harness), "Room 1");

    // Add a second block: the preset is still the one being worked on, now changed.
    harness.get_by_label("Block 2: +").click();
    harness.run();
    // The menu's entry (the selected block's header says Reverb too).
    harness
        .query_all_by_label("Reverb")
        .find(|node| node.accesskit_node().role() == egui::accesskit::Role::Button)
        .unwrap()
        .click();
    harness.run();
    assert_eq!(preset_shown(&harness), "Room 1 *");

    // Save over it: one preset of that name, with the chain as it is now.
    harness.get_by_label("Save tone").click();
    harness.run();
    assert_eq!(preset_shown(&harness), "Room 1");
    let presets = harness.state().settings().string_presets.clone();
    assert_eq!(
        presets
            .iter()
            .filter(|preset| preset.name == "Room 1")
            .count(),
        1
    );
    assert_eq!(
        presets
            .first()
            .and_then(|preset| preset.chain.blocks.get(1))
            .map(|block| block.choice),
        Some(gscore_app::chain_settings::BlockChoice::Reverb),
        "the saved chain took the block added after it was saved"
    );

    // And delete it while it is the active one.
    harness.get_by_label("Delete tone").click();
    harness.run();
    assert!(
        harness.state().settings().string_presets.is_empty(),
        "the preset is gone"
    );
    assert_eq!(preset_shown(&harness), "Changed");
}

/// What the Sound chain window's preset list shows./// What the Sound chain window's preset list shows.
fn preset_shown(harness: &Harness<'_, GuitarScoreApp>) -> String {
    harness
        .query_all_by_label("Tone")
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
    harness.get_by_label("Tone name").focus();
    harness.get_by_label("Tone name").type_text("Hall 1");
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
        .query_all_by_label("Tone")
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
        .query_all_by_label("Tone")
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

/// Drags a block from the box labelled `from` to the box labelled `to`, as the mouse does:
/// press, move far enough that egui sees a drag and not a click, then let go over `to`.
/// With `shot`, the frame while the block hangs at the mouse is snapshotted.
fn drag_box(
    harness: &mut Harness<'static, GuitarScoreApp>,
    from: &str,
    to: &str,
    shot: Option<&str>,
) {
    let from = harness.get_by_label(from).rect().center();
    let to = harness.get_by_label(to).rect().center();
    harness.hover_at(from);
    harness.run();
    harness.drag_at(from);
    harness.step();
    // The drag only starts once the mouse has moved a little with the button down.
    harness.hover_at(from + egui::vec2(24.0, 0.0));
    harness.step();
    harness.hover_at(to);
    harness.step();
    if let Some(name) = shot {
        harness.snapshot(name);
    }
    // Released over the target, without taking the cursor away first: the drop is what
    // egui is asked about on this frame.
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();
}

#[test]
fn a_block_is_dragged_to_another_place_and_the_chain_follows() {
    use gscore_app::chain_settings::BlockChoice;
    use gscore_play::StringKind;

    let mut harness = app_with("basic/note_lengths.gp");
    harness.get_by_label("Sound chain").click();
    harness.run();
    // An amp in the first place and a drive in the second.
    harness.get_by_label("Block 1: +").click();
    harness.run();
    harness.get_by_label("Amp").click();
    harness.run();
    harness.get_by_label("Block 2: +").click();
    harness.run();
    harness.get_by_label("Drive").click();
    harness.run();
    harness.get_by_label("Block 1: Amp");
    harness.get_by_label("Block 2: Drive");
    // The chain that took the blocks (the kind of string the track's preset stands for).
    let kind = [
        StringKind::Nylon,
        StringKind::Steel,
        StringKind::Electric,
        StringKind::Bass,
    ]
    .into_iter()
    .find(|kind| {
        harness
            .state()
            .playback()
            .tone(0, 26, *kind)
            .chain
            .blocks
            .first()
            .is_some_and(|block| block.choice == BlockChoice::Amp)
    })
    .expect("the blocks went into one chain");

    // The amp, dragged past the drive into the empty third place: the drive slides back.
    drag_box(
        &mut harness,
        "Block 1: Amp",
        "Block 3: +",
        Some("sound_chain_dragging"),
    );
    harness.get_by_label("Block 3: Amp");
    let blocks = harness
        .state()
        .playback()
        .tone(0, 26, kind)
        .chain
        .blocks
        .clone();
    assert_eq!(blocks[0].choice, BlockChoice::Drive, "the drive slid back");
    assert_eq!(blocks[1].choice, BlockChoice::Empty);
    assert_eq!(
        blocks[2].choice,
        BlockChoice::Amp,
        "the amp is where it was let go"
    );
    assert_eq!(blocks[3].choice, BlockChoice::Empty);

    // And back toward the strings: the drive and the empty place slide the other way.
    drag_box(&mut harness, "Block 3: Amp", "Block 1: Drive", None);
    harness.get_by_label("Block 1: Amp");
    let blocks = harness
        .state()
        .playback()
        .tone(0, 26, kind)
        .chain
        .blocks
        .clone();
    assert_eq!(blocks[0].choice, BlockChoice::Amp);
    assert_eq!(blocks[1].choice, BlockChoice::Drive);
    assert_eq!(blocks[2].choice, BlockChoice::Empty);
}

#[test]
fn a_tone_belongs_to_the_track_preset() {
    // The owner: a tone dialed in for a preset plays on every track with that preset, in
    // every song (it is kept in the settings), and another preset keeps its own.
    use gscore_play::StringKind;

    let mut harness = app_with("basic/multi_staff.gp");
    // Dial the pick position of the tone the shown track's preset plays.
    let dial = |harness: &mut Harness<'static, GuitarScoreApp>, track: usize, value: &str| {
        harness.state_mut().set_shown_track(track);
        harness.run();
        harness.get_by_label("Sound chain").click();
        harness.run();
        // Played by the strings (if it is not yet), their box selected.
        if harness.query_by_label("Block 0: MIDI").is_some() {
            harness.get_by_label("Block 0: MIDI").click();
            harness.run();
            harness.get_by_label("PM Synthesis").click();
            harness.run();
        } else {
            harness.get_by_label("Block 0: Strings").click();
            harness.run();
        }
        harness.get_by_label("Pick position value").click();
        harness.run();
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        harness.get_by_label("Pick position value").type_text(value);
        harness.run();
        harness.key_press(egui::Key::Enter);
        harness.run();
        harness.get_by_label("Close").click();
        harness.run();
    };
    // The track whose preset is `program` (0, 2 and 4 in this song).
    let pick = |harness: &Harness<'static, GuitarScoreApp>, track: usize, program: u8, kind| {
        harness
            .state()
            .playback()
            .tone(track, program, kind)
            .sound
            .pick_position
    };
    let bass = harness
        .state()
        .playback()
        .tone(4, 33, StringKind::Bass)
        .sound;
    // An electric guitar's strings with their pickup (a humbucker at the bridge).
    harness.get_by_label("Sound chain").click();
    harness.run();
    harness.get_by_label("Block 0: MIDI").click();
    harness.run();
    harness.get_by_label("PM Synthesis").click();
    harness.run();
    harness.snapshot("sound_chain_pickup");
    harness.get_by_label("Neck").click();
    harness.run();
    let neck = harness
        .state()
        .playback()
        .tone(0, 27, StringKind::Electric)
        .sound;
    assert_eq!(neck.model().pickup, gscore_guitar::Pickup::Humbucker);
    assert_eq!(neck.pickup_position, gscore_guitar::pickup_positions::NECK);
    harness.get_by_label("Close").click();
    harness.run();

    // The first guitar's preset (27) gets a tone of its own: preset 27's, from now on.
    dial(&mut harness, 0, "0.4");
    assert!(
        (pick(&harness, 0, 27, StringKind::Electric) - 0.4).abs() < 1e-6,
        "{}",
        pick(&harness, 0, 27, StringKind::Electric)
    );
    // It is kept in the settings for that preset, so every song's 27 plays it.
    let stored = harness.state().settings().preset_tones.clone();
    let tone = stored
        .iter()
        .find(|tone| tone.program == 27)
        .expect("kept for preset 27");
    assert!(
        (tone.tone().expect("a known kind").sound.pick_position - 0.4).abs() < 1e-6,
        "the settings keep it"
    );
    // Another preset is not touched: the bass (33) keeps its own tone.
    assert_eq!(
        harness
            .state()
            .playback()
            .tone(4, 33, StringKind::Bass)
            .sound,
        bass
    );

    // A guitar on another preset (30) gets its own tone, and 27's stays as it was.
    harness.state_mut().set_shown_track(2);
    harness.run();
    harness.get_by_label("SoundFont preset").click();
    harness.run();
    harness
        .get_by_label("30: Distortion Guitar")
        .click_accesskit();
    harness.run();
    dial(&mut harness, 2, "0.2");
    assert!((pick(&harness, 2, 30, StringKind::Electric) - 0.2).abs() < 1e-6);
    assert!(
        (pick(&harness, 0, 27, StringKind::Electric) - 0.4).abs() < 1e-6,
        "the first preset keeps its tone"
    );
}

#[test]
fn the_built_in_entry_forgets_a_presets_tone() {
    // Switching back to Built-in drops the local overwrite: the kind's default plays again.
    use gscore_play::StringKind;

    let mut harness = app_with("basic/multi_staff.gp");
    let built_in = harness
        .state()
        .playback()
        .string_sounds()
        .get(StringKind::Electric);
    harness.get_by_label("Sound chain").click();
    harness.run();
    harness.get_by_label("Block 0: MIDI").click();
    harness.run();
    harness.get_by_label("PM Synthesis").click();
    harness.run();
    harness.get_by_label("Pick position value").click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.get_by_label("Pick position value").type_text("0.4");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    let changed = harness
        .state()
        .playback()
        .tone(0, 27, StringKind::Electric)
        .sound;
    assert_ne!(changed, built_in, "the preset has a tone of its own now");
    // The tone row offers Built-in: the preset's own tone is forgotten.
    harness
        .query_all_by_label("Tone")
        .find(|node| node.accesskit_node().role() == egui::accesskit::Role::ComboBox)
        .expect("the tone row")
        .click();
    harness.run();
    harness.get_by_label("Built-in").click();
    harness.run();
    assert!(
        harness
            .state()
            .settings()
            .preset_tones
            .iter()
            .all(|tone| tone.program != 27),
        "the overwrite is gone from the settings"
    );
    assert_eq!(
        harness
            .state()
            .playback()
            .tone(0, 27, StringKind::Electric)
            .sound,
        built_in,
        "the built-in tone plays again"
    );
}
