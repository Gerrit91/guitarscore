//! The track panel: preset colors, the staves choice, note-name tuning, the action bar
//! (add, move, mute all, solo all) and the master row; and jumping during playback.

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

    harness.get_by_label("Color Blue").click();
    harness.run();
    assert_eq!(
        song(&harness).track(track).unwrap().color().rgb(),
        0x1E_88E5
    );
    undo(&mut harness);
    assert_eq!(song(&harness).track(track).unwrap().color(), before);

    harness.get_by_label("Tabs only").click();
    harness.run();
    assert_eq!(
        song(&harness).track(track).unwrap().staves(),
        TrackStaves::TabOnly
    );
    harness.get_by_label("Notes only").click();
    harness.run();
    assert_eq!(
        song(&harness).track(track).unwrap().staves(),
        TrackStaves::NotationOnly
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
    harness.get_by_label("String 1 note");
    // The first "+" is string 1's: a semitone up.
    harness.get_all_by_label("+").next().unwrap().click();
    harness.run();
    assert_eq!(pitch(&harness), "D#4");
}

#[test]
fn the_action_bar_adds_moves_mutes_and_solos_tracks() {
    let mut harness = app_with("basic/steel_guitar_dadgad.gp");
    harness.get_by_label("New track").click();
    harness.run();
    assert_eq!(song(&harness).track_count(), 2);
    assert_eq!(harness.state().view().track, 1, "the new track is shown");

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
        harness.state().view().track,
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

#[test]
fn jumping_while_playing_moves_the_playback_position() {
    // Plays to ALSA's silent "Midi Through" port; skipped where there is none.
    const THROUGH: &str = "Midi Through";
    let mut harness = app_with("basic/note_lengths.gp");
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
        return;
    };
    harness.state_mut().playback_mut().choose_port(Some(port));
    harness.get_by_label("Play").click();
    harness.step();
    let started = Instant::now();
    while harness.state().playback().state() != gscore_play::PlayState::Playing {
        assert!(started.elapsed() < Duration::from_secs(5), "did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    let song = song(&harness);
    let target = gscore_model::BeatPosition {
        track: TrackIndex::new(0),
        measure: gscore_model::MeasureIndex::new(song.measure_count() - 1),
        voice: gscore_model::VoiceIndex::new(0),
        beat: gscore_model::BeatIndex::new(0),
    };
    let tick = harness
        .state()
        .playback()
        .sequence()
        .unwrap()
        .tick_of(&song, target)
        .unwrap();
    harness
        .state_mut()
        .playback_mut()
        .jump_to(&song, target)
        .unwrap();
    // The player seeks on its own thread.
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
    harness.state_mut().playback_mut().stop().unwrap();
}
