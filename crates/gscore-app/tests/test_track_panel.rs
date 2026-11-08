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
    open_sound_chain(&mut harness);
    // The first box is what plays the track; PM synthesis is the default for strings.
    harness.get_by_label("Block 0: Strings").click();
    harness.run();
    harness
        .query_all_by_label("MIDI synth")
        .next()
        .expect("the sound source switch");
    // Physical modelling shows the kind of string and its knobs.
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
    // The chain's output level is a knob of its own, after every block.
    harness.get_by_label("Chain output value").click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.get_by_label("Chain output value").type_text("6.0");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(
        (harness
            .state()
            .playback()
            .tone(0, 26, gscore_play::StringKind::Nylon)
            .chain
            .output_db
            - 6.0)
            .abs()
            < 1e-4,
        "the chain's output level is taken"
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
fn the_tone_table_folds_its_groups() {
    let mut harness = app_with("basic/steel_guitar_dadgad.gp");
    harness.get_by_label("Group: Acoustic").click();
    harness.run();
    assert!(
        harness.query_by_label("Steel Strummed").is_none(),
        "the group's rows are folded away"
    );
    // The other groups stay open.
    assert!(harness.query_by_label("Group: Clean").is_some());
    harness.get_by_label("Group: Acoustic").click();
    harness.run();
    assert!(harness.query_by_label("Steel Strummed").is_some());
}

#[test]
fn a_preset_brings_its_kind_of_string_back() {
    let mut harness = app_with("basic/note_lengths.gp");
    open_sound_chain(&mut harness);
    // A steel sound dialed in with a reverb behind it, saved.
    harness.get_by_label("Steel guitar").click();
    harness.run();
    let block = empty_block(&harness);
    harness.get_by_label(&block).click();
    harness.run();
    harness.get_by_label("Reverb").click();
    harness.run();
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

    // Another kind of string (electric): it starts on its presets built-in tone.
    harness.get_by_label("Block 0: Strings").click();
    harness.run();
    harness.get_by_label("Electric guitar").click();
    harness.run();
    assert_eq!(preset_shown(&harness), "Warm Jazz");

    // Choosing the steel preset brings back the steel strings and their chain, not just
    // values on the electric ones.
    choose_chain_tone(&mut harness, "Natural Steel 1");
    assert!(
        harness
            .query_all_by_label_contains("Reverb")
            .next()
            .is_some()
    );
    assert_eq!(preset_shown(&harness), "Natural Steel 1");
}

#[test]
fn a_preset_can_be_saved_over_and_deleted_while_active() {
    let mut harness = app_with("basic/note_lengths.gp");
    open_sound_chain(&mut harness);
    // A block, saved under a name.
    let first = empty_block_index(&harness);
    harness.get_by_label(&format!("Block {first}: +")).click();
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
    let second = empty_block(&harness);
    harness.get_by_label(&second).click();
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
            .and_then(|preset| preset.chain.blocks.get(first.saturating_sub(1)))
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

/// What the Sound chain window's Tone row shows.
fn preset_shown(harness: &Harness<'_, GuitarScoreApp>) -> String {
    harness
        .query_all_by_label_contains("Tone: ")
        .last()
        .and_then(|node| node.accesskit_node().label())
        .and_then(|label| label.strip_prefix("Tone: ").map(str::to_owned))
        .unwrap_or_default()
}

/// The first empty place on the board (1-based), a built-in tone fills some places.
fn empty_block_index(harness: &Harness<'_, GuitarScoreApp>) -> usize {
    for index in 1..=gscore_guitar::BLOCKS {
        if harness
            .query_by_label(&format!("Block {index}: +"))
            .is_some()
        {
            return index;
        }
    }
    panic!("the board has no empty place")
}

/// The label of the first empty place on the board.
fn empty_block(harness: &Harness<'_, GuitarScoreApp>) -> String {
    format!("Block {}: +", empty_block_index(harness))
}

/// Opens the Sound chain window and lets it settle (its first frame is laid out before the
/// window has its final position, so a click on a box needs a second frame).
fn open_sound_chain(harness: &mut Harness<'static, GuitarScoreApp>) {
    harness.get_by_label("Sound chain").click();
    harness.run();
    harness.run();
}

/// Clicks a label inside the Sound chain window (the track panel shows some of the same
/// labels, such as the source switch and the picker's saved tones).
fn click_in_chain(harness: &Harness<'_, GuitarScoreApp>, label: &str) {
    harness
        .query_all_by_label(label)
        .find(|node| node.rect().center().x < 700.0)
        .unwrap_or_else(|| {
            let all: Vec<_> = harness
                .query_all_by_label(label)
                .map(|node| node.rect())
                .collect();
            panic!("no {label} in the Sound chain window: {all:?}")
        })
        .click();
}

/// Chooses a tone in the Sound chain window's Tone list (the combo, then the row).
fn choose_chain_tone(harness: &mut Harness<'static, GuitarScoreApp>, name: &str) {
    click_in_chain(harness, "Tone");
    harness.run();
    click_in_chain(harness, name);
    harness.run();
}

#[test]
fn a_saved_chain_preset_brings_back_the_whole_chain() {
    use gscore_app::chain_settings::BlockChoice;

    let mut harness = app_with("basic/note_lengths.gp");
    open_sound_chain(&mut harness);
    // A computed reverb in the first empty place.
    let first = empty_block_index(&harness);
    harness.get_by_label(&format!("Block {first}: +")).click();
    harness.run();
    harness.get_by_label("Reverb").click();
    harness.run();
    harness.get_by_label(&format!("Block {first}: Reverb"));
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
        preset
            .chain
            .blocks
            .get(first.saturating_sub(1))
            .map(|block| block.choice),
        Some(BlockChoice::Reverb),
        "the chain is part of the preset"
    );

    // Remove the block from its context menu: the preset no longer matches.
    let block = harness
        .get_by_label(&format!("Block {first}: Reverb"))
        .rect();
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
    harness.get_by_label(&format!("Block {first}: +"));
    assert_ne!(preset_shown(&harness), "Hall 1");
    // Choosing the preset brings the block back.
    choose_chain_tone(&mut harness, "Hall 1");
    harness.get_by_label(&format!("Block {first}: Reverb"));
    assert_eq!(preset_shown(&harness), "Hall 1");

    // Every preset is listed, whatever kind of string it was saved for: switch the track
    // to another kind and the preset is still there, and choosing it comes back to its own.
    harness.get_by_label("Block 0: Strings").click();
    harness.run();
    harness.get_by_label("Nylon guitar").click();
    harness.run();
    empty_block(&harness);
    // The preset of the other kind is in the list, and brings its chain and kind back.
    choose_chain_tone(&mut harness, "Hall 1");
    harness.get_by_label(&format!("Block {first}: Reverb"));
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
    open_sound_chain(&mut harness);
    // An amp in the first empty place and a drive in the next one.
    let first = empty_block_index(&harness);
    harness.get_by_label(&format!("Block {first}: +")).click();
    harness.run();
    harness.get_by_label("Amp").click();
    harness.run();
    let second = empty_block_index(&harness);
    harness.get_by_label(&format!("Block {second}: +")).click();
    harness.run();
    harness.get_by_label("Drive").click();
    harness.run();
    harness.get_by_label(&format!("Block {first}: Amp"));
    harness.get_by_label(&format!("Block {second}: Drive"));
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
            .get(first.saturating_sub(1))
            .is_some_and(|block| block.choice == BlockChoice::Amp)
    })
    .expect("the blocks went into one chain");

    // The amp, dragged onto the drive: the drive slides back into the amp's place.
    drag_box(
        &mut harness,
        &format!("Block {first}: Amp"),
        &format!("Block {second}: Drive"),
        Some("sound_chain_dragging"),
    );
    harness.get_by_label(&format!("Block {second}: Amp"));
    let blocks = harness
        .state()
        .playback()
        .tone(0, 26, kind)
        .chain
        .blocks
        .clone();
    assert_eq!(
        blocks[first.saturating_sub(1)].choice,
        BlockChoice::Drive,
        "the drive slid back"
    );
    assert_eq!(
        blocks[second.saturating_sub(1)].choice,
        BlockChoice::Amp,
        "the amp is where it was let go"
    );

    // And back toward the strings: the other block slides the other way.
    drag_box(
        &mut harness,
        &format!("Block {second}: Amp"),
        &format!("Block {first}: Drive"),
        None,
    );
    harness.get_by_label(&format!("Block {first}: Amp"));
    let blocks = harness
        .state()
        .playback()
        .tone(0, 26, kind)
        .chain
        .blocks
        .clone();
    assert_eq!(blocks[first.saturating_sub(1)].choice, BlockChoice::Amp);
    assert_eq!(blocks[second.saturating_sub(1)].choice, BlockChoice::Drive);
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
        open_sound_chain(harness);
        // Forget the tone the song brought (Guitar Pro's RSE sound) so the knobs edit the
        // preset's tone, which is what this test is about.
        harness.get_by_label("Built-in").click();
        harness.run();
        // Played by the strings (if it is not yet), their box selected.
        if harness.query_by_label("Block 0: MIDI").is_some() {
            harness.get_by_label("Block 0: MIDI").click();
            harness.run();
            click_in_chain(harness, "PM Synthesis");
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
    // An electric guitar's strings with their pickup (a single coil at the bridge).
    open_sound_chain(&mut harness);
    harness.get_by_label("Block 0: Strings").click();
    harness.run();
    harness.snapshot("sound_chain_pickup");
    harness.get_by_label("Neck").click();
    harness.run();
    let neck = harness
        .state()
        .playback()
        .tone(0, 27, StringKind::Electric)
        .sound;
    assert_eq!(neck.model().pickup, gscore_guitar::Pickup::SingleCoil);
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
    harness.get_by_label("MIDI synth").click();
    harness.run();
    harness.get_by_label("Filter the sounds").click();
    harness.run();
    harness.get_by_label("Filter sounds").focus();
    harness
        .get_by_label("Filter sounds")
        .type_text("Distortion Guitar");
    harness.run();
    harness.get_by_label("30: Distortion Guitar").click();
    harness.run();
    dial(&mut harness, 2, "0.2");
    assert!((pick(&harness, 2, 30, StringKind::Electric) - 0.2).abs() < 1e-6);
    assert!(
        (pick(&harness, 0, 27, StringKind::Electric) - 0.4).abs() < 1e-6,
        "the first preset keeps its tone"
    );
}

#[test]
fn a_songs_resolved_preset_tone_survives_with_default_settings() {
    // A tone dialed in for a preset lives in this machine's settings. When the song is
    // saved, it gets the tone it actually plays, so another machine (with its own default
    // settings) hears the same.
    use gscore_play::StringKind;

    let dir = std::env::temp_dir().join(format!("gscore-preset-tone-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temp dir");
    let path = dir.join("song.gp");
    std::fs::copy(testdata("basic/multi_staff.gp"), &path).expect("a copy of the song");

    let open = |path: &std::path::Path| {
        let path = path.to_path_buf();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1200.0, 1100.0))
            .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(path.clone())));
        let started = Instant::now();
        while harness.state().is_busy() {
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "opening timed out"
            );
            harness.step();
            std::thread::sleep(Duration::from_millis(5));
        }
        harness.run();
        harness
    };
    let mut harness = open(&path);
    // Dial the pick position of the tone the first track's preset (27) plays.
    open_sound_chain(&mut harness);
    // The file's RSE sound gave the track an own tone; Built-in removes it so the knobs
    // edit the preset's tone, as this test checks.
    harness.get_by_label("Built-in").click();
    harness.run();
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
    harness.get_by_label("Pick position value").type_text("0.4");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    harness.get_by_label("Close").click();
    harness.run();
    // The tone is the preset's: this machine's settings, not the song's block.
    assert!(
        harness
            .state()
            .settings()
            .preset_tones
            .iter()
            .any(|tone| tone.program == 27)
    );
    // Saved, the song carries the tone it plays.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::S);
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "saving timed out"
        );
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run_steps(8);
    drop(harness);

    // A fresh app with default settings hears the tone, though it knows no preset tone.
    let reopened = open(&path);
    assert!(
        reopened
            .state()
            .settings()
            .preset_tones
            .iter()
            .all(|tone| tone.program != 27)
    );
    let tone = reopened
        .state()
        .playback()
        .tone(0, 27, StringKind::Electric);
    assert!(
        (tone.sound.pick_position - 0.4).abs() < 1e-6,
        "{:?}",
        tone.sound.pick_position
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_built_in_entry_forgets_a_presets_tone() {
    // Switching back to Built-in drops the local overwrite: the program's built-in tone
    // plays again.
    use gscore_play::StringKind;

    let mut harness = app_with("basic/multi_staff.gp");
    let built_in = harness
        .state()
        .playback()
        .default_tone_for(27, StringKind::Electric)
        .sound;
    open_sound_chain(&mut harness);
    harness.get_by_label("Block 0: Strings").click();
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

/// Types `text` into the track panel's picker filter and clicks the row `row`.
fn pick_in_track_panel(harness: &mut Harness<'static, GuitarScoreApp>, text: &str, row: &str) {
    harness.get_by_label("Filter the sounds").click();
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.get_by_label("Filter sounds").type_text(text);
    harness.run();
    harness.get_by_label(row).click();
    harness.run();
}

#[test]
fn a_tone_clicked_in_the_track_panel_belongs_to_that_track_and_survives_saving() {
    let dir = std::env::temp_dir().join(format!("gscore-tone-picker-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temp dir");
    let path = dir.join("song.gp");
    std::fs::copy(testdata("basic/multi_staff.gp"), &path).expect("a copy of the song");

    let open = |path: &std::path::Path| {
        let path = path.to_path_buf();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1200.0, 1100.0))
            .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(path.clone())));
        let started = Instant::now();
        while harness.state().is_busy() {
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "opening timed out"
            );
            harness.step();
            std::thread::sleep(Duration::from_millis(5));
        }
        harness.run();
        harness
    };
    let mut harness = open(&path);
    // Every string track may already play the tone its RSE sound brought; only the shown
    // track's choice may change.
    let before = harness.state().playback().track_tones().clone();
    // Choose a tone from the library for the shown (first) track.
    pick_in_track_panel(&mut harness, "Djent Tight", "Djent Tight");
    let chosen = harness
        .state()
        .playback()
        .track_tones()
        .get(&0)
        .cloned()
        .expect("the track has the tone");
    let after = harness.state().playback().track_tones().clone();
    assert_eq!(after.get(&1), before.get(&1), "only that track got it");
    // It travels in the song: a fresh app with default settings hears the same tone.
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::S);
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "saving timed out"
        );
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.run_steps(8);
    drop(harness);
    let reopened = open(&path);
    assert_eq!(
        reopened.state().playback().track_tones().get(&0),
        Some(&chosen),
        "the saved song carries the tone"
    );
    assert!(
        !reopened.state().playback().track_tones().contains_key(&1)
            || reopened.state().playback().track_tones().get(&1) == before.get(&1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn switching_the_source_keeps_the_program_and_the_tone() {
    let mut harness = app_with("basic/note_lengths.gp");
    // A modelled tone of the track's own.
    pick_in_track_panel(&mut harness, "Djent Tight", "Djent Tight");
    let chosen = harness
        .state()
        .playback()
        .track_tones()
        .get(&0)
        .cloned()
        .expect("the tone");
    // To the MIDI synth and another General MIDI program.
    harness.get_by_label("MIDI synth").click();
    harness.run();
    pick_in_track_panel(&mut harness, "24: Nylon", "24: Nylon Str Guitar");
    let program = |harness: &Harness<'static, GuitarScoreApp>| {
        harness
            .state()
            .document()
            .and_then(|document| {
                document
                    .song()
                    .track_sound(TrackIndex::new(0))
                    .map(|sound| sound.program)
            })
            .expect("the track")
    };
    assert_eq!(program(&harness), 24, "the MIDI synth's program changed");
    // And back: the modelled tone is still the one chosen, not the program's default.
    harness.get_by_label("PM Synthesis").click();
    harness.run();
    assert_eq!(
        harness.state().playback().track_tones().get(&0),
        Some(&chosen)
    );
    assert_eq!(program(&harness), 24, "the program is kept too");
    // The Sound chain window shows the same source.
    open_sound_chain(&mut harness);
    harness.get_by_label("Block 0: Strings");
}

#[test]
fn a_drum_track_lists_the_kits_and_cannot_be_played_by_pm() {
    let mut harness = app_with("basic/multi_staff.gp");
    harness.state_mut().set_shown_track(5);
    harness.run();
    assert!(
        harness.query_by_label("Group: Drum kits").is_some(),
        "the kit group"
    );
    assert!(
        harness.query_by_label("0: Standard").is_some(),
        "the standard kit"
    );
    let pm = harness
        .query_all_by_label("PM Synthesis")
        .find(|node| node.rect().center().x > 700.0)
        .expect("the track panel's source switch");
    assert!(pm.accesskit_node().is_disabled(), "no PM for a drum kit");
}

#[test]
fn an_external_synth_offers_a_port_a_channel_and_the_programs() {
    let mut harness = app_with("basic/note_lengths.gp");
    harness.get_by_label("MIDI out").click();
    harness.run();
    harness.get_by_label("Port");
    harness.get_by_label("Channel");
    assert!(
        harness.query_by_label("24: Nylon Str Guitar").is_some(),
        "the hardware synth gets the program table too"
    );
}
