//! `StringVoice::render` and everything the audio thread calls on a voice (pluck, bend,
//! release, level) never allocate: the delay line is made once, when the voice is.
//!
//! The global allocator counts allocations for this test binary only.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::alloc::System;

use gscore_guitar::{Attack, GuitarModel, Pluck, Release, StringVoice, frequency};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn playing_a_string_never_allocates() {
    let mut voices: Vec<StringVoice> = [
        GuitarModel::Nylon,
        GuitarModel::Steel,
        GuitarModel::Electric,
        GuitarModel::Bass,
    ]
    .iter()
    .map(|model| StringVoice::new(48_000.0, 20.0, model.strings()))
    .collect();
    let mut block = [0.0_f32; 64];

    let region = Region::new(GLOBAL);
    for (index, voice) in voices.iter_mut().enumerate() {
        for step in 0..400_usize {
            match step % 50 {
                0 => voice.pluck(Pluck {
                    frequency: frequency(40 + index as u8 * 5, 0.0),
                    velocity: 0.8,
                    attack: Attack::Pick,
                    palm_mute: false,
                    dead: false,
                    harmonic: false,
                }),
                10 => voice.pluck(Pluck {
                    frequency: frequency(42 + index as u8 * 5, 0.0),
                    velocity: 0.6,
                    attack: Attack::Legato,
                    palm_mute: false,
                    dead: false,
                    harmonic: false,
                }),
                20 => voice.set_bend(150.0),
                30 => voice.set_level(0.5),
                40 => voice.release(Release::Damped),
                _ => {}
            }
            block.fill(0.0);
            voice.render(&mut block);
        }
    }
    let stats = region.change();
    assert_eq!(stats.allocations, 0, "{stats:?}");
    assert_eq!(stats.reallocations, 0, "{stats:?}");
}
