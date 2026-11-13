//! How long the whole engine takes for one second of a busy song: 15 guitar tracks of
//! modelled strings strumming six-string chords, each through a full chain (compressor,
//! amp, cabinet, EQ), sending into the reverb, compressor and chorus buses, a drum track
//! with a chain on each of its kit's parts, and a master chain. (A computed reverb on every
//! track as well costs about 14 ms each: rooms belong on the reverb bus.) Rendered by
//! `SynthEngine` as the audio thread does, in 64-frame blocks.
//!
//! ```text
//! cargo run --release -p gscore-audio --example engine_bench [-- --check]
//! ```
//!
//! `cargo xtask bench` runs it with `--check`, which fails when the engine takes more than
//! [`BUDGET`] for a second of audio. The SoundFont is the test sine: a voice costs the
//! synthesizer the same whatever its samples are.

// Times and counts are small and positive; a measurement tool.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    unused_crate_dependencies
)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use gscore_audio::testing::sine_soundfont;
use gscore_audio::{Bus, ChainKnobs, SoundBank, SynthEngine};
use gscore_guitar::{BLOCKS, BlockKind, BlockSettings, ChainSpec, built_in_cabinet};
use gscore_model::testing::{self, TestNote};
use gscore_model::{Song, TrackIndex};
use gscore_play::{MidiEvent, Sequence, StringKind};

/// One second of the whole engine on the song below: half of one core, leaving the other
/// half for the UI, the device and slower machines (M22: a 16-track song in real time with
/// headroom).
const BUDGET: Duration = Duration::from_millis(500);
const RATE: u32 = 48_000;
const BLOCK: usize = 64;
const TRACKS: usize = 16;
/// The last track is the drum kit.
const DRUMS: usize = TRACKS - 1;
/// The kit's parts (kick, snare, hi-hat, toms, cymbals, the rest).
const PARTS: usize = 6;
const RUNS: usize = 5;

/// Sixteen tracks strumming an open E chord on every quarter; the last is a drum track.
fn song() -> Song {
    let chord = [
        TestNote::new(1, 0),
        TestNote::new(2, 0),
        TestNote::new(3, 1),
        TestNote::new(4, 2),
        TestNote::new(5, 2),
        TestNote::new(6, 0),
    ];
    let beats: Vec<(u16, &[TestNote<'_>])> = (0..4).map(|_| (4, &chord[..])).collect();
    let mut song = testing::song_with_notes_on_tracks(TRACKS, &beats).expect("a test song");
    for track in 0..TRACKS {
        let at = TrackIndex::new(track);
        let _ = song.set_track_reverb(at, 40).expect("reverb send");
        let _ = song.set_track_chorus(at, 30).expect("chorus send");
    }
    let _ = song
        .set_track_drums(TrackIndex::new(DRUMS), true)
        .expect("a drum track");
    song
}

fn chain(blocks: [Option<BlockKind>; BLOCKS]) -> (Box<gscore_guitar::Chain>, ChainKnobs) {
    let settings = [BlockSettings::default(); BLOCKS];
    let spec = ChainSpec { blocks };
    let chain = Box::new(spec.prepare(RATE).instance(&settings, 0.0));
    (
        chain,
        ChainKnobs {
            settings,
            output_db: 0.0,
        },
    )
}

/// An engine playing `sequence` with every chain and bus in place.
fn engine(bank: &SoundBank, sequence: &Arc<Sequence>) -> SynthEngine {
    let mut engine = SynthEngine::new(bank, RATE).expect("an engine");
    engine.load(Arc::clone(sequence));
    for track in 0..DRUMS {
        let (chain, knobs) = chain([
            Some(BlockKind::Compressor),
            Some(BlockKind::Amp),
            Some(BlockKind::Impulse(built_in_cabinet())),
            Some(BlockKind::Eq),
            None,
        ]);
        let _ = engine.set_chain(track, Some(chain), knobs);
        engine.set_comp_send(track, 64);
    }
    for part in 0..PARTS {
        let (chain, knobs) = chain([
            Some(BlockKind::Eq),
            Some(BlockKind::Compressor),
            None,
            None,
            None,
        ]);
        let _ = engine.set_part_chain(DRUMS, part, Some(chain), knobs);
    }
    let (drum_bus, knobs) = chain([Some(BlockKind::Compressor), None, None, None, None]);
    let _ = engine.set_chain(DRUMS, Some(drum_bus), knobs);
    for (bus, block) in [
        (Bus::Reverb, BlockKind::Reverb),
        (Bus::Compressor, BlockKind::Compressor),
        (Bus::Chorus, BlockKind::Chorus),
    ] {
        let (chain, knobs) = chain([Some(block), None, None, None, None]);
        let _ = engine.set_bus(bus, Some(chain));
        engine.set_bus_settings(bus, knobs, 100, false);
    }
    let (master, knobs) = chain([
        Some(BlockKind::Eq),
        Some(BlockKind::Compressor),
        None,
        None,
        None,
    ]);
    let _ = engine.set_master_chain(Some(master), knobs);
    engine.play();
    engine
}

/// How long one second takes, from the start of the song, with a kick, snare and hi-hat
/// played on the kit on every eighth note.
fn second(engine: &mut SynthEngine, kit: gscore_play::Channel) -> Duration {
    let (mut left, mut right) = ([0.0_f32; BLOCK], [0.0_f32; BLOCK]);
    let blocks = RATE as usize / BLOCK;
    let eighth = blocks / 4;
    let start = Instant::now();
    for index in 0..blocks {
        if index % eighth == 0 {
            for key in [36, 38, 42] {
                engine.send_event(MidiEvent::NoteOn {
                    channel: kit,
                    key,
                    velocity: 100,
                });
            }
        }
        engine.render(&mut left, &mut right);
    }
    std::hint::black_box((&left, &right));
    start.elapsed()
}

fn main() {
    let check = std::env::args().any(|arg| arg == "--check");
    let bank = SoundBank::read(&mut sine_soundfont("Test Sine").as_slice()).expect("the font");
    let song = song();
    let sequence = Arc::new(
        Sequence::new(&song, None)
            .expect("a sequence")
            .with_modelled_guitars_where(|_| Some(StringKind::Electric)),
    );
    let kit = sequence.tracks[DRUMS].channel;
    let mut times: Vec<Duration> = (0..RUNS)
        .map(|_| second(&mut engine(&bank, &sequence), kit))
        .collect();
    times.sort();
    let median = times[RUNS / 2];
    println!(
        "whole engine, {} modelled tracks + drums, chains, buses and master:\n  \
         one second of audio {:8.3} ms  (budget {:.3} ms)",
        DRUMS,
        median.as_secs_f64() * 1000.0,
        BUDGET.as_secs_f64() * 1000.0
    );
    if check && median > BUDGET {
        eprintln!("whole engine: one second of audio took over the budget");
        std::process::exit(1);
    }
}
