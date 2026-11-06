//! The modelled guitars in the synth engine: a track given to them is played by the
//! strings instead of the SoundFont, in tune and in time; drums and other tracks stay on
//! the SoundFont; stopping, pausing and seeking silence the strings.
//!
//! The SoundFont here is a looped sine wave, so whatever has overtones comes from the
//! strings.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::sync::Arc;

use gscore_audio::testing::sine_soundfont;
use gscore_audio::{SoundBank, SynthEngine};
use gscore_model::Song;
use gscore_model::testing::{self, TestNote};
use gscore_model::{
    BeatIndex, BeatPosition, MeasureIndex, NoteEffect, StringNumber, TrackIndex, VoiceIndex,
};
use gscore_play::{PPQ, Sequence, StringKind, TrackEngine};

const RATE: u32 = 48_000;

fn bank() -> SoundBank {
    SoundBank::read(&mut sine_soundfont("Test Sine").as_slice()).unwrap()
}

/// Renders `seconds` of mono from `engine`.
fn render(engine: &mut SynthEngine, seconds: f32) -> Vec<f32> {
    let frames = (seconds * RATE as f32) as usize;
    let mut left = vec![0.0_f32; frames];
    let mut right = vec![0.0_f32; frames];
    engine.render(&mut left, &mut right);
    left.iter()
        .zip(&right)
        .map(|(l, r)| 0.5 * (l + r))
        .collect()
}

fn play(sequence: Sequence, seconds: f32) -> Vec<f32> {
    let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
    engine.load(Arc::new(sequence));
    engine.play();
    render(&mut engine, seconds)
}

/// The power at `frequency` (a windowed DFT bin).
fn power(samples: &[f32], frequency: f32) -> f64 {
    let n = samples.len() as f64;
    let w = std::f64::consts::TAU * f64::from(frequency) / f64::from(RATE);
    let (mut re, mut im) = (0.0, 0.0);
    for (i, &x) in samples.iter().enumerate() {
        let hann = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n).cos();
        re += f64::from(x) * hann * (w * i as f64).cos();
        im += f64::from(x) * hann * (w * i as f64).sin();
    }
    re * re + im * im
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

fn seconds(samples: &[f32], from: f32, to: f32) -> &[f32] {
    let at = |t: f32| ((t * RATE as f32) as usize).min(samples.len());
    &samples[at(from)..at(to)]
}

/// A whole note A3 (string 3, fret 2) at 120 BPM: two seconds.
fn a3() -> Song {
    testing::song_with_notes(&[(1, &[TestNote::new(3, 2)])]).unwrap()
}

#[test]
fn a_modelled_track_is_played_by_the_strings_not_the_soundfont() {
    let song = a3();
    let plain = Sequence::new(&song, None).unwrap();
    let modelled = plain.clone().with_modelled_guitars();
    assert!(matches!(
        modelled.performance.tracks[0].engine,
        TrackEngine::Modelled(StringKind::Steel)
    ));
    let a = 220.0;
    let sine = play(plain, 1.0);
    let strings = play(modelled, 1.0);
    let window = |out: &[f32]| seconds(out, 0.2, 0.9).to_vec();
    let (sine, strings) = (window(&sine), window(&strings));
    // The sine font has no overtones; the string has a strong second and third harmonic.
    let overtones = |out: &[f32]| (power(out, 2.0 * a) + power(out, 3.0 * a)) / power(out, a);
    assert!(
        overtones(&sine) < 1e-3,
        "the sine font is pure: {}",
        overtones(&sine)
    );
    assert!(
        overtones(&strings) > 1e-2,
        "the string is not: {}",
        overtones(&strings)
    );
    // In tune: the fundamental beats its neighbours a few cents away.
    let near = |cents: f32| power(&strings, a * (cents / 1200.0).exp2());
    assert!(near(0.0) > near(8.0) && near(0.0) > near(-8.0));
    assert!(rms(&strings) > 0.01, "loud enough: {}", rms(&strings));
}

#[test]
fn notes_start_on_time_and_a_damped_note_stops() {
    // A quarter rest, then a palm-muted quarter E2.
    let mut song =
        testing::song_with_notes(&[(4, &[]), (4, &[TestNote::new(6, 0)]), (2, &[])]).unwrap();
    let at = BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(1),
    };
    let _ = song
        .set_note_effect(
            at,
            StringNumber::new(6).unwrap(),
            NoteEffect::PalmMute(true),
        )
        .unwrap();
    let out = play(
        Sequence::new(&song, None).unwrap().with_modelled_guitars(),
        1.6,
    );
    // A quarter at 120 BPM is half a second.
    assert!(
        rms(seconds(&out, 0.0, 0.49)) < 1e-6,
        "silent before the note"
    );
    assert!(rms(seconds(&out, 0.5, 0.6)) > 1e-3, "struck on its beat");
    assert!(
        rms(seconds(&out, 1.3, 1.6)) < 1e-4,
        "palm muted: gone soon after"
    );
}

#[test]
fn drums_and_other_tracks_stay_on_the_soundfont() {
    let song = testing::song_with_notes_on_tracks(2, &[(1, &[TestNote::new(3, 2)])]).unwrap();
    let mut sequence = Sequence::new(&song, None).unwrap().with_modelled_guitars();
    // Only the first track is modelled: the second plays the sine.
    Arc::make_mut(&mut sequence.performance).tracks[1].engine = TrackEngine::SoundFont;
    let mixed = play(sequence.clone(), 1.0);
    let mut strings_only = sequence.clone();
    let performance = Arc::make_mut(&mut strings_only.performance);
    performance.tracks[1].engine = TrackEngine::Modelled(StringKind::Steel);
    let both_modelled = play(strings_only, 1.0);
    let window = |out: &[f32]| seconds(out, 0.2, 0.9).to_vec();
    let a = 220.0;
    let overtone_share = |out: &[f32]| power(out, 2.0 * a) / power(out, a);
    // With the sine track on the SoundFont, the fundamental is relatively stronger.
    assert!(overtone_share(&window(&mixed)) < overtone_share(&window(&both_modelled)));
    assert!(rms(&window(&mixed)) > 0.01);
}

#[test]
fn stopping_and_seeking_silence_the_strings() {
    let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
    engine.load(Arc::new(
        Sequence::new(&a3(), None).unwrap().with_modelled_guitars(),
    ));
    engine.play();
    let playing = render(&mut engine, 0.5);
    assert!(rms(&playing[12_000..]) > 0.01);
    engine.stop();
    let stopped = render(&mut engine, 0.5);
    assert!(rms(seconds(&stopped, 0.2, 0.5)) < 1e-4, "stopped: silent");
    // Seeking past the note's start does not strike it.
    engine.seek(u64::from(PPQ));
    engine.play();
    let after_seek = render(&mut engine, 0.5);
    assert!(
        rms(&after_seek) < 1e-4,
        "a note that began earlier is not struck again"
    );
    // Playing from the start strikes it again.
    engine.stop();
    engine.play();
    assert!(rms(&render(&mut engine, 0.3)[4_800..]) > 0.01);
}

#[test]
fn a_bend_moves_the_string_pitch() {
    let song = testing::song_with_notes(&[(1, &[TestNote::new(3, 2).with_bend(4)])]).unwrap();
    let out = play(
        Sequence::new(&song, None).unwrap().with_modelled_guitars(),
        2.0,
    );
    // Up a whole step within a beat (half a second at 120 BPM), then held.
    let held = seconds(&out, 0.8, 1.8);
    let bent = 220.0 * (200.0_f32 / 1200.0).exp2();
    assert!(
        power(held, bent) > 10.0 * power(held, 220.0),
        "the string is bent up"
    );
}

#[test]
fn a_wav_of_a_modelled_song_is_written() {
    let sequence = Sequence::new(&a3(), None).unwrap().with_modelled_guitars();
    let mut bytes = Vec::new();
    let info = gscore_audio::write_wav(&mut bytes, &sequence, &bank()).unwrap();
    assert!(info.frames > 0);
    let data = &bytes[44..];
    assert!(data.iter().any(|&byte| byte != 0), "not silent");
}

#[test]
fn a_changed_string_model_is_heard_at_once() {
    let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
    engine.load(Arc::new(
        Sequence::new(&a3(), None).unwrap().with_modelled_guitars(),
    ));
    engine.play();
    let before = render(&mut engine, 0.3);
    let silent = gscore_guitar::StringModel {
        level: 0.0,
        ..gscore_audio::default_string_model(StringKind::Steel)
    };
    engine.set_string_model(StringKind::Steel, silent);
    let after = render(&mut engine, 0.3);
    assert!(rms(&before[4_800..]) > 0.01);
    assert!(
        rms(&after[4_800..]) < 1e-4,
        "the level knob took effect while ringing"
    );
    // Another kind is not touched.
    engine.set_string_model(
        StringKind::Bass,
        gscore_audio::default_string_model(StringKind::Bass),
    );
}

#[test]
fn two_tracks_of_one_kind_keep_their_own_sound_and_chain() {
    use gscore_guitar::{BLOCKS, BlockKind, BlockSettings, ChainSpec};
    // Two steel-string tracks playing the same note (the owner's bug: two electric
    // tracks shared one sound, so dialing in the second changed the first).
    let song = testing::song_with_notes_on_tracks(2, &[(1, &[TestNote::new(3, 2)])]).unwrap();
    let sequence = Arc::new(Sequence::new(&song, None).unwrap().with_modelled_guitars());
    let silent = gscore_guitar::StringModel {
        level: 0.0,
        ..gscore_audio::default_string_model(StringKind::Steel)
    };
    let level = |setup: &dyn Fn(&mut SynthEngine)| {
        let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
        setup(&mut engine);
        engine.load(Arc::clone(&sequence));
        engine.play();
        rms(seconds(&render(&mut engine, 0.5), 0.1, 0.5))
    };
    let both = level(&|_| {});
    // The second track's own sound is silent: only the first is heard, and a change of the
    // kind's sound (what tracks without their own play) does not undo it.
    let first_only = level(&|engine| {
        engine.set_track_model(1, Some(silent));
        engine.set_string_model(
            StringKind::Steel,
            gscore_audio::default_string_model(StringKind::Steel),
        );
    });
    assert!(
        first_only > 0.3 * both && first_only < 0.8 * both,
        "{first_only} of {both}"
    );
    // Back to its kind's sound: both again.
    let again = level(&|engine| {
        engine.set_track_model(1, Some(silent));
        engine.set_track_model(1, None);
    });
    assert!((again - both).abs() < 0.01 * both, "{again} against {both}");
    // A chain on the first track only (an amp turned all the way down): the second plays
    // on unchanged, dry.
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Amp);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].amp.level_db = -24.0;
    let first_quiet = level(&|engine| {
        let chain = gscore_audio::prepare_chain(&spec, &settings, RATE);
        assert!(engine.set_chain(0, chain, settings).is_none());
    });
    assert!(
        first_quiet < 0.9 * both && first_quiet > 0.3 * both,
        "{first_quiet} of {both}"
    );
}

#[test]
fn a_beat_excerpt_plays_with_its_tracks_tone() {
    // A beat of the second track heard alone (a palette click) is the excerpt's only
    // track, yet it keeps the second track's tone, not the first one's.
    let song = testing::song_with_notes_on_tracks(2, &[(1, &[TestNote::new(3, 2)])]).unwrap();
    let at = BeatPosition {
        track: TrackIndex::new(1),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(0),
    };
    let excerpt = Arc::new(
        Sequence::beat_excerpt(&song, at)
            .unwrap()
            .unwrap()
            .with_modelled_guitars(),
    );
    let silent = gscore_guitar::StringModel {
        level: 0.0,
        ..gscore_audio::default_string_model(StringKind::Steel)
    };
    let level = |silenced: usize| {
        let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
        engine.set_track_model(silenced, Some(silent));
        engine.load(Arc::clone(&excerpt));
        engine.play();
        rms(seconds(&render(&mut engine, 0.5), 0.1, 0.5))
    };
    assert!(level(0) > 0.01, "the first track's tone does not apply");
    assert!(level(1) < 1e-4, "the second track's does");
}

#[test]
fn the_reverb_bus_carries_the_tracks_send() {
    use gscore_guitar::{BLOCKS, BlockKind, BlockSettings, ChainSpec, ReverbKnobs};
    // A palm-muted quarter whose track sends a lot into the reverb (the mixer's knob);
    // the strings alone are silent soon after.
    let mut song = testing::song_with_notes(&[(4, &[TestNote::new(3, 2)]), (2, &[])]).unwrap();
    let at = BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(0),
    };
    let _ = song
        .set_note_effect(
            at,
            StringNumber::new(3).unwrap(),
            NoteEffect::PalmMute(true),
        )
        .unwrap();
    let _ = song.set_track_reverb(TrackIndex::new(0), 120).unwrap();
    let sequence = Arc::new(Sequence::new(&song, None).unwrap().with_modelled_guitars());
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Reverb);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].reverb = ReverbKnobs {
        size: 0.8,
        damping: 0.3,
        mix: 1.0,
        ..ReverbKnobs::default()
    };
    // `None`: no bus; otherwise its level and mute (the track's send comes from the song).
    let tail = |bus: Option<(u8, bool)>| {
        let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
        if let Some((level, muted)) = bus {
            let chain = spec.prepare(RATE).instance(&settings);
            engine.set_reverb_bus(Some(Box::new(chain)));
            engine.set_reverb_bus_settings(settings, level, muted);
        }
        engine.load(Arc::clone(&sequence));
        engine.play();
        let out = render(&mut engine, 1.4);
        rms(seconds(&out, 0.9, 1.4))
    };
    let (dry, wet, quieter, muted) = (
        tail(None),
        tail(Some((127, false))),
        tail(Some((64, false))),
        tail(Some((127, true))),
    );
    assert!(dry < 1e-4, "the strings alone are silent: {dry}");
    assert!(wet > 10.0 * dry.max(1e-6), "the bus rings on: {wet}");
    assert!(quieter < 0.7 * wet, "the level scales it: {quieter} {wet}");
    assert!(muted < 1e-4, "muted: nothing comes back: {muted}");
}

#[test]
fn a_room_chain_rings_on_after_a_damped_note() {
    use gscore_guitar::{BLOCKS, BlockKind, BlockSettings, ChainSpec, ImpulseResponse};
    // A palm-muted quarter: the strings alone are silent soon after.
    let mut song = testing::song_with_notes(&[(4, &[TestNote::new(3, 2)]), (2, &[])]).unwrap();
    let at = BeatPosition {
        track: TrackIndex::new(0),
        measure: MeasureIndex::new(0),
        voice: VoiceIndex::new(0),
        beat: BeatIndex::new(0),
    };
    let _ = song
        .set_note_effect(
            at,
            StringNumber::new(3).unwrap(),
            NoteEffect::PalmMute(true),
        )
        .unwrap();
    let sequence = Arc::new(Sequence::new(&song, None).unwrap().with_modelled_guitars());
    // `None`: no chain; `Some(length)`: the room at that length against the recording's.
    let tail = |chain: Option<f32>| {
        let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
        if let Some(length) = chain {
            // A two-second room that decays slowly.
            let room: Vec<f32> = (0..96_000)
                .map(|i| ((i * 7919 % 1000) as f32 / 500.0 - 1.0) * (-(i as f32) / 30_000.0).exp())
                .collect();
            let mut spec = ChainSpec::default();
            spec.blocks[1] = Some(BlockKind::room(
                Arc::new(ImpulseResponse::from_samples(&room, RATE).unwrap()),
                length,
            ));
            let mut settings = [BlockSettings::default(); BLOCKS];
            settings[1].room.mix = 0.6;
            let chain = gscore_audio::prepare_chain(&spec, &settings, RATE);
            assert!(chain.is_some());
            assert!(engine.set_chain(0, chain, settings).is_none());
        }
        engine.load(Arc::clone(&sequence));
        engine.play();
        let out = render(&mut engine, 1.4);
        rms(seconds(&out, 1.0, 1.4))
    };
    let (dry, wet, longer) = (tail(None), tail(Some(1.0)), tail(Some(4.0)));
    assert!(dry < 1e-4, "the strings alone are silent: {dry}");
    assert!(wet > 10.0 * dry.max(1e-6), "the room rings on: {wet}");
    assert!(
        longer > wet,
        "four times the length still rings on where the recording is gone: {longer} against \
         {wet}"
    );
}

#[test]
#[ignore = "timing, run in release: cargo test --release -p gscore-audio --test test_guitars -- --ignored"]
fn a_long_tail_after_stop_costs_what_playing_does() {
    use gscore_guitar::{BLOCKS, BlockKind, BlockSettings, ChainSpec, ReverbKnobs};
    let mut song = a3();
    let _ = song.set_track_reverb(TrackIndex::new(0), 120).unwrap();
    let sequence = Arc::new(Sequence::new(&song, None).unwrap().with_modelled_guitars());
    let mut spec = ChainSpec::default();
    spec.blocks[0] = Some(BlockKind::Reverb);
    let mut settings = [BlockSettings::default(); BLOCKS];
    settings[0].reverb = ReverbKnobs {
        size: 1.0,
        decay: 20.0,
        mix: 0.5,
        ..ReverbKnobs::default()
    };
    let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
    let chain = gscore_audio::prepare_chain(&spec, &settings, RATE);
    let _ = engine.set_chain(0, chain, settings);
    let bus = spec.prepare(RATE).instance(&settings);
    engine.set_reverb_bus(Some(Box::new(bus)));
    engine.set_reverb_bus_settings(settings, 127, false);
    engine.load(Arc::clone(&sequence));
    engine.play();
    let mut left = vec![0.0_f32; 512];
    let mut right = vec![0.0_f32; 512];
    let seconds_of = |engine: &mut SynthEngine, left: &mut [f32], right: &mut [f32]| {
        let start = std::time::Instant::now();
        for _ in 0..(RATE as usize / 512) {
            engine.render(left, right);
        }
        start.elapsed().as_secs_f32() * 100.0
    };
    for second in 0..2 {
        println!(
            "playing {second}: {:.2} %",
            seconds_of(&mut engine, &mut left, &mut right)
        );
    }
    engine.stop();
    for second in 0..30 {
        println!(
            "stopped {second}: {:.2} %",
            seconds_of(&mut engine, &mut left, &mut right)
        );
    }
}
