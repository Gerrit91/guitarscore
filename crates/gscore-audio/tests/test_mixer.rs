//! The engine's mixer: every track a strip (its source, its chain, then fader and pan, then
//! the reverb send), the buses into the master, and real meters. Played offline with the
//! test SoundFont (a sine), so levels can be compared exactly.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::sync::Arc;

use gscore_audio::testing::sine_soundfont;
use gscore_audio::{SoundBank, SynthEngine};
use gscore_guitar::{BLOCKS, BlockKind, BlockSettings, ChainSpec, ReverbKnobs};
use gscore_model::testing::{self, TestNote};
use gscore_model::{Song, TrackIndex};
use gscore_play::{Channel, MidiEvent, Sequence, controller};

const RATE: u32 = 48_000;

fn bank() -> SoundBank {
    SoundBank::read(&mut sine_soundfont("Test Sine").as_slice()).unwrap()
}

/// `tracks` guitar tracks, each playing a whole note A3 (two seconds).
fn song(tracks: usize) -> Song {
    testing::song_with_notes_on_tracks(tracks, &[(1, &[TestNote::new(3, 2)])]).unwrap()
}

fn engine(sequence: &Sequence) -> SynthEngine {
    let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
    engine.load(Arc::new(sequence.clone()));
    engine.play();
    engine
}

/// `seconds` of left and right.
fn render(engine: &mut SynthEngine, seconds: f32) -> (Vec<f32>, Vec<f32>) {
    let frames = (seconds * RATE as f32) as usize;
    let (mut left, mut right) = (vec![0.0_f32; frames], vec![0.0_f32; frames]);
    engine.render(&mut left, &mut right);
    (left, right)
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

/// The steady part of a note (after its attack).
fn steady(samples: &[f32]) -> &[f32] {
    &samples[RATE as usize / 4..RATE as usize]
}

#[test]
fn a_tracks_fader_and_pan_act_as_the_synthesizers_did() {
    // Full volume against half: the square law (64/127)², as General MIDI synthesizers
    // (and RustySynth before the mixer) apply it.
    let level = |volume: u8, pan: u8| {
        let mut song = song(1);
        let _ = song.set_track_volume(TrackIndex::new(0), volume).unwrap();
        let _ = song.set_track_pan(TrackIndex::new(0), pan).unwrap();
        let (left, right) = render(&mut engine(&Sequence::new(&song, None).unwrap()), 1.0);
        (rms(steady(&left)), rms(steady(&right)))
    };
    let (full, _) = level(127, 64);
    let (half, _) = level(64, 64);
    let expected = (64.0_f32 / 127.0).powi(2);
    assert!(
        (half / full - expected).abs() < 0.01,
        "{} against {expected}",
        half / full
    );
    // Hard left: nothing on the right, and as loud on the left as both sides in the
    // middle together (equal power).
    let (left, right) = level(127, 0);
    assert!(right < 1e-3 * left, "{right}");
    let (middle, _) = level(127, 64);
    assert!(
        (left / middle - std::f32::consts::SQRT_2).abs() < 0.02,
        "{}",
        left / middle
    );
}

#[test]
fn a_soundfont_track_has_its_own_chain() {
    // Two tracks, the second through an amp turned all the way down: only its meter and
    // its share of the mix drop.
    let song = song(2);
    let sequence = Sequence::new(&song, None).unwrap();
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Amp);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].amp.level_db = -24.0;
    settings[0].amp.voice = gscore_guitar::AmpVoice::Clean;
    settings[0].amp.gain = 0.0;
    let mut plain = engine(&sequence);
    let (plain_left, _) = render(&mut plain, 1.0);
    let mut chained = engine(&sequence);
    let chain = gscore_audio::prepare_chain(&spec, &settings, RATE);
    assert!(chained.set_chain(1, chain, settings).is_none());
    let (chained_left, _) = render(&mut chained, 1.0);
    let meters = chained.meters();
    let (first, _) = meters.track(0);
    let (second, _) = meters.track(1);
    assert!(first > 0.05, "the first track plays: {first}");
    assert!(
        second < 0.2 * first,
        "the second is turned down: {second} against {first}"
    );
    let ratio = rms(steady(&chained_left)) / rms(steady(&plain_left));
    assert!(
        (0.45..0.7).contains(&ratio),
        "about half the mix is left: {ratio}"
    );
}

#[test]
fn the_meters_show_the_real_levels() {
    let mut song = song(2);
    let _ = song.set_track_volume(TrackIndex::new(1), 0).unwrap();
    let mut engine = engine(&Sequence::new(&song, None).unwrap());
    let (left, right) = render(&mut engine, 0.5);
    let meters = engine.meters();
    let (first_left, first_right) = meters.track(0);
    assert!(first_left > 0.05 && (first_left - first_right).abs() < 1e-3);
    assert_eq!(
        meters.track(1),
        (0.0, 0.0),
        "a track at volume 0 shows nothing"
    );
    // The master's peak is the loudest sample of the last moments of the mix.
    let (master, _) = meters.master();
    let tail = left[left.len() - 2_400..]
        .iter()
        .fold(0.0_f32, |peak, x| peak.max(x.abs()));
    assert!(
        (master - tail).abs() < 0.05 * tail,
        "{master} against {tail}"
    );
    assert!(right.iter().all(|x| x.is_finite()));
    // Stopped, the meters fall back toward silence.
    engine.stop();
    let _ = render(&mut engine, 4.0);
    let (after, _) = engine.meters().track(0);
    assert!(after < 0.01 * first_left, "{after}");
}

#[test]
fn a_soundfont_tracks_reverb_send_reaches_the_bus() {
    // The track sends to the reverb bus with its Reverb knob (MIDI CC 91): the synth's
    // own reverb is off now, so without the bus nothing rings on after the note.
    let mut song = testing::song_with_notes(&[(4, &[TestNote::new(3, 2)]), (2, &[])]).unwrap();
    let _ = song.set_track_reverb(TrackIndex::new(0), 120).unwrap();
    let sequence = Sequence::new(&song, None).unwrap();
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Reverb);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].reverb = ReverbKnobs {
        size: 0.8,
        decay: 4.0,
        mix: 1.0,
        ..ReverbKnobs::default()
    };
    let tail = |bus: bool| {
        let mut engine = engine(&sequence);
        if bus {
            engine.set_reverb_bus(Some(Box::new(spec.prepare(RATE).instance(&settings))));
            engine.set_reverb_bus_settings(settings, 127, false);
        }
        let (left, _) = render(&mut engine, 2.0);
        (
            rms(&left[RATE as usize + RATE as usize / 2..]),
            engine.meters().reverb().0,
        )
    };
    let (dry, _) = tail(false);
    let (wet, bus_meter) = tail(true);
    assert!(
        wet > 3.0 * dry.max(1e-6),
        "the bus rings on: {wet} against {dry}"
    );
    assert!(bus_meter > 0.0, "the bus's meter moves");
}

#[test]
fn a_live_fader_move_is_heard_at_once() {
    let song = song(1);
    let sequence = Sequence::new(&song, None).unwrap();
    let channel = sequence.tracks[0].channel;
    let mut engine = engine(&sequence);
    let (before, _) = render(&mut engine, 0.5);
    engine.send_event(MidiEvent::Control {
        channel,
        controller: controller::VOLUME,
        value: 0,
    });
    let (after, _) = render(&mut engine, 0.3);
    assert!(rms(&before[12_000..]) > 0.01);
    assert!(
        rms(&after[4_800..]) < 1e-4,
        "the fader closed: {}",
        rms(&after[4_800..])
    );
}

#[test]
fn tracks_sharing_a_channel_are_one_strip() {
    // Guitar Pro lets two tracks share a MIDI channel: the synthesizer makes one signal of
    // them. The first track's strip carries it (both notes); the second's stays silent,
    // and either track's volume moves the shared strip.
    let song = song(2);
    let mut sequence = Sequence::new(&song, None).unwrap();
    let shared = sequence.tracks[0].channel;
    let other = sequence.tracks[1].channel;
    sequence.tracks[1].channel = shared;
    let retarget = |channel: &mut Channel| {
        if *channel == other {
            *channel = shared;
        }
    };
    for event in &mut sequence.events {
        match &mut event.event {
            MidiEvent::NoteOn { channel, .. }
            | MidiEvent::NoteOff { channel, .. }
            | MidiEvent::ProgramChange { channel, .. }
            | MidiEvent::Control { channel, .. }
            | MidiEvent::PitchBend { channel, .. } => retarget(channel),
        }
    }
    let mut engine = engine(&sequence);
    let (left, _) = render(&mut engine, 1.0);
    let meters = engine.meters();
    assert!(meters.track(0).0 > 0.05, "the shared strip plays");
    assert_eq!(
        meters.track(1),
        (0.0, 0.0),
        "the linked track has no signal of its own"
    );
    assert!(rms(steady(&left)) > 0.01);
}
