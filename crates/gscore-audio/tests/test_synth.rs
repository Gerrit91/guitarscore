//! The internal synthesizer, rendered offline (no audio device): timing, transport and
//! SoundFont handling. The SoundFont is built here: one preset with a looped sine wave, so
//! no third-party file is needed.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
// Integration tests are separate crates and see all of the library's dependencies.
#![allow(unused_crate_dependencies)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gscore_audio::testing::sine_soundfont;
use gscore_audio::{BLOCK, SoundBank, SynthEngine, SynthPlayer, find_soundfont_in};
use gscore_model::testing::{self, TestNote};
use gscore_play::{Channel, MidiEvent, PPQ, PlayState, Player, Sequence, TempoMap, TimedEvent};

const RATE: u32 = 44_100;

fn bank() -> SoundBank {
    SoundBank::read(&mut sine_soundfont("Test Sine").as_slice()).unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gscore-synth-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------------------
// Sequences and rendering
// ---------------------------------------------------------------------------

/// Notes (tick, key) of one quarter each on channel 0, at 120 BPM (a quarter is 500 ms).
fn notes(at: &[(u64, u8)]) -> Arc<Sequence> {
    let channel = Channel::new(0).unwrap();
    let quarter = u64::from(PPQ);
    let mut events: Vec<TimedEvent> = at
        .iter()
        .flat_map(|&(tick, key)| {
            [
                TimedEvent {
                    tick,
                    track: 0,
                    event: MidiEvent::NoteOn {
                        channel,
                        key,
                        velocity: 100,
                    },
                },
                TimedEvent {
                    tick: tick + quarter,
                    track: 0,
                    event: MidiEvent::NoteOff { channel, key },
                },
            ]
        })
        .collect();
    events.sort_by_key(|e| e.tick);
    let end = events.last().map_or(0, |e| e.tick);
    Arc::new(Sequence {
        tracks: Vec::new(),
        events,
        tempo: TempoMap::new(120),
        measures: Vec::new(),
        end,
        skipped_notes: 0,
        degraded_pitch_bends: 0,
    })
}

/// Renders `seconds` of mono (the mean of left and right).
fn render(engine: &mut SynthEngine, seconds: f64) -> Vec<f32> {
    let frames = (seconds * f64::from(RATE)) as usize;
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    engine.render(&mut left, &mut right);
    left.iter()
        .zip(&right)
        .map(|(l, r)| 0.5 * (l + r))
        .collect()
}

fn loudest(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |max, s| max.max(s.abs()))
}

/// The index of the first sample louder than a whisper.
fn onset(samples: &[f32]) -> Option<usize> {
    samples.iter().position(|s| s.abs() > 1e-3)
}

/// The frequency, from the zero crossings of `samples`.
fn frequency(samples: &[f32]) -> f64 {
    let crossings = samples
        .windows(2)
        .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
        .count();
    crossings as f64 * f64::from(RATE) / samples.len() as f64
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn a_soundfont_is_read_and_bad_files_are_refused() {
    let bank = bank();
    assert_eq!(bank.name(), "Test Sine");
    assert_eq!(bank.preset_count(), 1);
    assert!(format!("{bank:?}").contains("Test Sine"));

    let refused = SoundBank::read(&mut b"RIFF\0\0\0\0WAVE".as_slice()).unwrap_err();
    assert!(refused.to_string().contains("SoundFont"), "{refused}");

    let dir = temp_dir("bad");
    let missing = SoundBank::open(&dir.join("missing.sf2")).unwrap_err();
    assert!(missing.to_string().contains("missing.sf2"), "{missing}");
    let text = dir.join("notes.sf2");
    std::fs::write(&text, "not a soundfont").unwrap();
    let broken = SoundBank::open(&text).unwrap_err();
    assert!(broken.to_string().contains("notes.sf2"), "{broken}");

    assert!(SynthEngine::new(&bank, 8_000).is_err(), "too low a rate");
}

#[test]
fn notes_sound_at_their_time_and_pitch() {
    let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
    assert_eq!(engine.sample_rate(), RATE);
    // Before anything is loaded or played: silence.
    assert_eq!(loudest(&render(&mut engine, 0.1)), 0.0);

    // A4 on the second beat, at 500 ms.
    let quarter = u64::from(PPQ);
    assert!(engine.load(notes(&[(quarter, 69)])).is_none());
    engine.play();
    let audio = render(&mut engine, 0.9);
    let start = onset(&audio).expect("the note sounds");
    let expected = RATE as usize / 2;
    assert!(
        start >= expected && start <= expected + BLOCK,
        "onset at sample {start}, expected {expected} (within one block)"
    );
    let hz = frequency(&audio[start + 2_000..start + 2_000 + 8_820]);
    assert!((hz - 440.0).abs() < 10.0, "A4 sounds at {hz} Hz");
    assert_eq!(engine.state(), PlayState::Playing);

    // An octave higher sounds at twice the frequency.
    let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
    let _ = engine.load(notes(&[(0, 81)]));
    engine.play();
    let audio = render(&mut engine, 0.3);
    let hz = frequency(&audio[2_000..]);
    assert!((hz - 880.0).abs() < 20.0, "A5 sounds at {hz} Hz");
}

#[test]
fn the_end_stops_and_play_starts_over() {
    let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
    let _ = engine.load(notes(&[(0, 69)]));
    engine.play();
    let _ = render(&mut engine, 0.6);
    assert_eq!(engine.state(), PlayState::Stopped, "the end was reached");
    assert_eq!(engine.position(), u64::from(PPQ));
    // The release fades out.
    let _ = render(&mut engine, 2.0);
    assert!(loudest(&render(&mut engine, 0.1)) < 1e-3);
    // Playing again starts from the beginning.
    engine.play();
    assert!(loudest(&render(&mut engine, 0.2)) > 0.05);
}

#[test]
fn pause_keeps_the_position_and_stop_rewinds() {
    let quarter = u64::from(PPQ);
    let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
    let _ = engine.load(notes(&[(0, 69), (quarter, 69), (2 * quarter, 69)]));
    engine.play();
    let _ = render(&mut engine, 0.75);
    engine.pause();
    let paused_at = engine.position();
    assert_eq!(engine.state(), PlayState::Paused);
    assert!(
        paused_at > quarter && paused_at < 2 * quarter,
        "{paused_at}"
    );
    // Paused: the notes fade out and the position stays.
    let _ = render(&mut engine, 2.0);
    assert!(loudest(&render(&mut engine, 0.1)) < 1e-3);
    assert_eq!(engine.position(), paused_at);

    // Resuming continues from there: the third note comes after the rest of beat two.
    engine.play();
    let audio = render(&mut engine, 0.5);
    let remaining = (2 * quarter - paused_at) as f64 / quarter as f64 * 0.5;
    let start = onset(&audio).unwrap();
    let expected = (remaining * f64::from(RATE)) as usize;
    assert!(
        start.abs_diff(expected) <= BLOCK + 1,
        "{start} vs {expected}"
    );

    engine.stop();
    assert_eq!((engine.state(), engine.position()), (PlayState::Stopped, 0));
}

#[test]
fn seeking_and_tempo_change_the_timing() {
    let quarter = u64::from(PPQ);
    let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
    let _ = engine.load(notes(&[(0, 69), (2 * quarter, 69)]));
    // Seek past the first note: the second one comes after one quarter (500 ms).
    engine.seek(quarter);
    assert_eq!(engine.state(), PlayState::Paused);
    engine.play();
    let start = onset(&render(&mut engine, 0.8)).unwrap();
    assert!(start.abs_diff(RATE as usize / 2) <= BLOCK, "{start}");

    // Twice as fast: the second note comes after 500 ms instead of a second.
    let mut fast = SynthEngine::new(&bank(), RATE).unwrap();
    let _ = fast.load(notes(&[(2 * quarter, 69)]));
    fast.set_tempo_scale(2.0);
    fast.play();
    let start = onset(&render(&mut fast, 0.8)).unwrap();
    assert!(start.abs_diff(RATE as usize / 2) <= BLOCK, "{start}");
}

#[test]
fn interleaved_output_fills_every_channel_and_clips() {
    let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
    // Many loud notes at once go beyond full scale before clipping.
    let chord: Vec<(u64, u8)> = (60..76).map(|key| (0, key)).collect();
    let _ = engine.load(notes(&chord));
    engine.play();

    // Odd lengths and channel counts: mono, stereo, 5.1.
    for channels in [1, 2, 6] {
        let mut out = vec![0.0_f32; channels * 1_001];
        engine.render_interleaved(&mut out, channels, |s| s);
        assert!(out.iter().all(|s| s.abs() <= 1.0));
        assert!(loudest(&out) > 0.05, "{channels} channel(s) sound");
        if channels == 6 {
            assert!(
                out.chunks(6)
                    .all(|frame| frame[2..].iter().all(|&s| s == 0.0))
            );
        }
    }
    // Integer output.
    let mut ints = vec![0_i16; 2 * 512];
    engine.render_interleaved(&mut ints, 2, |s| (s * f32::from(i16::MAX)) as i16);
    assert!(ints.iter().any(|&s| s != 0));
    // Unequal slices: the extra part of the longer one is silence.
    let mut left = vec![1.0; 100];
    let mut right = vec![1.0; 50];
    engine.render(&mut left, &mut right);
    assert!(left[50..].iter().all(|&s| s == 0.0));
}

#[test]
fn a_song_plays_through_the_synth() {
    // A real sequence from the model: two quarter notes on a guitar track.
    let song = testing::song_with_notes(&[
        (4, &[TestNote::new(5, 0)][..]),
        (4, &[TestNote::new(1, 5)][..]),
    ])
    .unwrap();
    let sequence = Arc::new(Sequence::new(&song, None).unwrap());
    let mut engine = SynthEngine::new(&bank(), RATE).unwrap();
    let _ = engine.load(Arc::clone(&sequence));
    engine.play();
    let seconds = sequence.tempo.micros_at(sequence.end) as f64 / 1e6 + 0.1;
    let audio = render(&mut engine, seconds);
    assert!(loudest(&audio) > 0.05, "the song sounds");
    assert_eq!(engine.state(), PlayState::Stopped);
    // Loading another sequence hands back the old one.
    assert!(engine.load(sequence).is_some());
}

#[test]
fn soundfonts_are_found_in_the_usual_places() {
    let root = temp_dir("find-root");
    let home = temp_dir("find-home");
    // Nothing installed.
    assert_eq!(find_soundfont_in(None, Some(&home), &root, None), None);
    // The variable wins, even for a missing file.
    let named = PathBuf::from("/nowhere/mine.sf2");
    assert_eq!(
        find_soundfont_in(Some(named.clone()), Some(&home), &root, None),
        Some(named)
    );

    // Any .sf2 in a system folder (not .sf3, which the synthesizer cannot read).
    let sf2 = root.join("usr/share/sounds/sf2");
    std::fs::create_dir_all(&sf2).unwrap();
    std::fs::write(sf2.join("MuseScore_General.sf3"), "").unwrap();
    assert_eq!(find_soundfont_in(None, None, &root, None), None);
    std::fs::write(sf2.join("Zebra.sf2"), "").unwrap();
    std::fs::write(sf2.join("Alpha.SF2"), "").unwrap();
    assert_eq!(
        find_soundfont_in(None, None, &root, None),
        Some(sf2.join("Alpha.SF2"))
    );
    // A well-known file beats the folder search.
    let arch = root.join("usr/share/soundfonts");
    std::fs::create_dir_all(&arch).unwrap();
    std::fs::write(arch.join("FluidR3_GM.sf2"), "").unwrap();
    assert_eq!(
        find_soundfont_in(None, None, &root, None),
        Some(arch.join("FluidR3_GM.sf2"))
    );
    // The user's own folder beats everything but the variable.
    let user = home.join(".local/share/soundfonts");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::write(user.join("Mine.sf2"), "").unwrap();
    assert_eq!(
        find_soundfont_in(None, Some(&home), &root, None),
        Some(user.join("Mine.sf2"))
    );
}

#[test]
fn a_soundfont_bundled_next_to_the_executable_is_used() {
    let root = temp_dir("bundled-root");
    let home = temp_dir("bundled-home");
    let bin = temp_dir("bundled-bin");
    // A release archive puts FluidR3_GM.sf2 beside the binary.
    let bundled = bin.join("FluidR3_GM.sf2");
    std::fs::write(&bundled, "").unwrap();
    let exe = bin.join("guitarscore");
    assert_eq!(
        find_soundfont_in(None, None, &root, Some(&exe)),
        Some(bundled.clone())
    );
    // A SoundFont the user put in their own folder still wins over the bundled default.
    let user = home.join(".local/share/soundfonts");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::write(user.join("Mine.sf2"), "").unwrap();
    assert_eq!(
        find_soundfont_in(None, Some(&home), &root, Some(&exe)),
        Some(user.join("Mine.sf2"))
    );
    let _ = std::fs::remove_dir_all(bin);
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(home);
}

/// Waits until `done` or a few seconds have passed.
fn wait_for(player: &SynthPlayer, done: impl Fn(&SynthPlayer) -> bool) {
    let started = Instant::now();
    while !done(player) && started.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn the_player_reports_a_bad_soundfont_without_blocking() {
    let dir = temp_dir("player");
    let path = dir.join("broken.sf2");
    std::fs::write(&path, "not a soundfont").unwrap();
    let mut player = SynthPlayer::start(&path).unwrap();
    assert_eq!(player.soundfont(), path.as_path());
    // Commands are queued while it loads.
    let _ = player.load(notes(&[(0, 69)]));
    wait_for(&player, |p| !p.is_loading());
    let error = player.error().expect("the file is refused");
    assert!(error.contains("broken.sf2"), "{error}");
    assert_eq!(player.state(), PlayState::Stopped);
    assert!(player.play().is_err(), "a failed synth says so on play");
    assert_eq!(player.device(), None);
}

/// End to end through the default audio device, at zero volume would be nicer, but a short
/// note is fine. Skipped where there is no audio device (CI, containers) or when
/// `GSCORE_NO_AUDIO` is set.
#[test]
fn the_player_plays_on_the_default_device() {
    if std::env::var_os("GSCORE_NO_AUDIO").is_some() {
        eprintln!("skipped: GSCORE_NO_AUDIO is set");
        return;
    }
    let dir = temp_dir("device");
    let path: &Path = &dir.join("sine.sf2");
    std::fs::write(path, sine_soundfont("Sine")).unwrap();
    let mut player = SynthPlayer::start(path).unwrap();
    wait_for(&player, |p| !p.is_loading());
    if let Some(error) = player.error() {
        eprintln!("skipped: no usable audio device ({error})");
        return;
    }
    assert_eq!(player.bank_name().as_deref(), Some("Sine"));
    assert!(player.device().is_some());
    // A very short, quiet sequence: one 16th note at the lowest velocity.
    let mut sequence = (*notes(&[(0, 69)])).clone();
    for event in &mut sequence.events {
        if let MidiEvent::NoteOn { velocity, .. } = &mut event.event {
            *velocity = 1;
        }
    }
    sequence.events.last_mut().unwrap().tick = u64::from(PPQ) / 4;
    sequence.end = u64::from(PPQ) / 4;
    player.load(Arc::new(sequence)).unwrap();
    player.play().unwrap();
    wait_for(&player, |p| p.state() == PlayState::Playing);
    wait_for(&player, |p| p.state() == PlayState::Stopped);
    assert_eq!(player.position(), u64::from(PPQ) / 4);
    assert_eq!(player.error(), None);
}
