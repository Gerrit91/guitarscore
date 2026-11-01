//! End to end through a real MIDI port: plays to ALSA's "Midi Through" port and listens on
//! the same port, so nothing sounds. Skipped (with a message) where that port does not
//! exist, e.g. on systems without the ALSA sequencer.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gscore_audio::{MidiOutPlayer, midi_output_ports};
use gscore_model::testing::{self, GUITAR_6};
use gscore_model::{
    BeatIndex, BeatPosition, Fret, MeasureIndex, StringNumber, TrackIndex, VoiceIndex,
};
use gscore_play::{PlayState, Player, Sequence};

const THROUGH: &str = "Midi Through";

/// Four quarter notes: open strings 6, 5, 4, 3 (E2 A2 D3 G3 = 40 45 50 55).
fn four_notes() -> Arc<Sequence> {
    let mut song = testing::song(&[GUITAR_6], 1).unwrap();
    for (beat, string) in [(0, 6), (1, 5), (2, 4), (3, 3)] {
        let at = BeatPosition {
            track: TrackIndex::new(0),
            measure: MeasureIndex::new(0),
            voice: VoiceIndex::new(0),
            beat: BeatIndex::new(beat),
        };
        let _change = song
            .set_note(
                at,
                StringNumber::new(string).unwrap(),
                Fret::new(0).unwrap(),
            )
            .unwrap();
    }
    Arc::new(Sequence::new(&song, None).unwrap())
}

#[test]
fn a_sequence_arrives_in_order_at_a_midi_port() {
    let Ok(ports) = midi_output_ports() else {
        eprintln!("skipped: no MIDI system");
        return;
    };
    if !ports.iter().any(|p| p.contains(THROUGH)) {
        eprintln!("skipped: no \"{THROUGH}\" port in {ports:?}");
        return;
    }

    // Listen on the through port.
    let input = midir::MidiInput::new("GuitarScore test").unwrap();
    let port = input
        .ports()
        .into_iter()
        .find(|p| input.port_name(p).is_ok_and(|n| n.contains(THROUGH)))
        .unwrap();
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let _listening = input
        .connect(
            &port,
            "listen",
            move |_, message, _| {
                let _ = tx.send(message.to_vec());
            },
            (),
        )
        .unwrap();

    let mut player = MidiOutPlayer::connect(Some(THROUGH)).unwrap();
    player.load(four_notes()).unwrap();
    player.set_tempo_scale(4.0).unwrap(); // one 4/4 bar at 120 BPM: 0.5 s
    let started = Instant::now();
    player.play().unwrap();

    let mut keys = Vec::new();
    let mut silenced = false;
    while started.elapsed() < Duration::from_secs(5) && !silenced {
        if let Ok(message) = rx.recv_timeout(Duration::from_millis(100)) {
            match message.as_slice() {
                [status, key, velocity] if status & 0xF0 == 0x90 && *velocity > 0 => {
                    keys.push(*key);
                }
                // All notes off at the end.
                [status, 123, 0] if status & 0xF0 == 0xB0 => silenced = true,
                _ => {}
            }
        }
    }
    let elapsed = started.elapsed();
    assert_eq!(keys, [40, 45, 50, 55]);
    assert!(silenced, "the end silences the channel");
    assert!(
        elapsed >= Duration::from_millis(350) && elapsed < Duration::from_secs(3),
        "{elapsed:?}"
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while player.state() != PlayState::Stopped && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(player.state(), PlayState::Stopped);
    assert_eq!(player.error(), None);
}

#[test]
fn an_unknown_port_is_an_error_not_a_panic() {
    let Err(error) = MidiOutPlayer::connect(Some("no such port 1234567")) else {
        panic!("connected to a port that does not exist");
    };
    assert!(error.to_string().contains("no such port") || error.to_string().contains("MIDI"));
}
