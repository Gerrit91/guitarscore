//! GuitarScore's fork of RustySynth (`vendor/rustysynth`): each MIDI channel rendered
//! apart, for the mixer. A channel's notes reach only its own output, the outputs added up
//! are exactly what the synthesizer plays without splitting, and a SoundFont still loads
//! (the sample data is read without `unsafe` now).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#![allow(unused_crate_dependencies)]

use std::sync::Arc;

use gscore_audio::testing::sine_soundfont;
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

const RATE: i32 = 48_000;
const CHANNELS: usize = Synthesizer::CHANNEL_COUNT;

fn font() -> Arc<SoundFont> {
    let bytes = sine_soundfont("Test Sine");
    Arc::new(SoundFont::new(&mut bytes.as_slice()).unwrap())
}

fn synth(split: bool) -> Synthesizer {
    let mut settings = SynthesizerSettings::new(RATE);
    settings.enable_reverb_and_chorus = false;
    settings.split_channels = split;
    let mut synth = Synthesizer::new(&font(), &settings).unwrap();
    // Channel 0 plays A3, channel 2 an E4 panned left; channel 1 stays silent.
    synth.note_on(0, 57, 100);
    synth.process_midi_message(2, 0xB0, 10, 0);
    synth.note_on(2, 64, 90);
    synth
}

fn energy(samples: &[f32]) -> f32 {
    samples.iter().map(|x| x * x).sum()
}

/// `frames` of each channel, rendered in uneven pieces (as the engine's callback asks).
fn channels(synth: &mut Synthesizer, frames: usize) -> Vec<[Vec<f32>; 2]> {
    let mut out: Vec<[Vec<f32>; 2]> = (0..CHANNELS)
        .map(|_| [vec![0.0; frames], vec![0.0; frames]])
        .collect();
    let mut at = 0;
    for piece in [37, 64, 5, 200].iter().cycle() {
        if at >= frames {
            break;
        }
        let end = (at + piece).min(frames);
        let (mut lefts, mut rights): (Vec<&mut [f32]>, Vec<&mut [f32]>) = out
            .iter_mut()
            .map(|[left, right]| (&mut left[at..end], &mut right[at..end]))
            .unzip();
        synth.render_channels(&mut lefts, &mut rights);
        at = end;
    }
    out
}

#[test]
fn each_channel_plays_into_its_own_output() {
    let mut split = synth(true);
    let out = channels(&mut split, 4_800);
    assert!(energy(&out[0][0]) > 1.0, "channel 0 sounds");
    assert!(energy(&out[2][0]) > 1.0, "channel 2 sounds");
    // Panned hard left: its right side is (nearly) silent.
    assert!(energy(&out[2][1]) < 0.01 * energy(&out[2][0]));
    for (channel, [left, right]) in out.iter().enumerate() {
        if channel != 0 && channel != 2 {
            assert_eq!(energy(left) + energy(right), 0.0, "channel {channel}");
        }
    }
}

#[test]
fn the_channels_add_up_to_the_unsplit_synthesizer() {
    let frames = 4_800;
    let mut plain = synth(false);
    let (mut left, mut right) = (vec![0.0_f32; frames], vec![0.0_f32; frames]);
    plain.render(&mut left, &mut right);
    let mut split = synth(true);
    let out = channels(&mut split, frames);
    for i in 0..frames {
        let sum_left: f32 = out.iter().map(|[l, _]| l[i]).sum();
        let sum_right: f32 = out.iter().map(|[_, r]| r[i]).sum();
        assert!((sum_left - left[i]).abs() < 1e-6, "left {i}");
        assert!((sum_right - right[i]).abs() < 1e-6, "right {i}");
    }
    // `render` on a split synthesizer plays the sum too.
    let mut split = synth(true);
    let (mut sum_left, mut sum_right) = (vec![0.0_f32; frames], vec![0.0_f32; frames]);
    split.render(&mut sum_left, &mut sum_right);
    assert_eq!(sum_left, left);
    assert_eq!(sum_right, right);
}

#[test]
fn a_synthesizer_that_is_not_split_gives_silent_channels() {
    let mut plain = synth(false);
    let out = channels(&mut plain, 256);
    assert!(out.iter().all(|[l, r]| energy(l) + energy(r) == 0.0));
}

#[test]
fn any_channel_can_play_drums() {
    let mut synth = synth(true);
    assert!(synth.is_percussion_channel(Synthesizer::PERCUSSION_CHANNEL));
    assert!(!synth.is_percussion_channel(3));
    synth.set_percussion_channel(3, true);
    assert!(synth.is_percussion_channel(3));
    // Notes on it still sound (the test SoundFont has no drum kit: its preset plays).
    synth.note_on(3, 40, 100);
    let out = channels(&mut synth, 2_400);
    assert!(energy(&out[3][0]) > 0.0);
    synth.set_percussion_channel(3, false);
    assert!(!synth.is_percussion_channel(3));
    // Out of range: nothing happens.
    synth.set_percussion_channel(99, true);
    assert!(!synth.is_percussion_channel(99));
}

#[test]
#[ignore = "needs a SoundFont: GSCORE_SF2=/path/to.sf2 cargo test --release -p gscore-audio --test test_split_channels -- --ignored"]
fn a_large_soundfont_loads_as_before() {
    // Prints the load time and a checksum of the sample data, to compare with the
    // crates.io release (which read the samples with `unsafe`).
    let path = std::env::var("GSCORE_SF2").unwrap();
    let start = std::time::Instant::now();
    let mut file = std::io::BufReader::new(std::fs::File::open(&path).unwrap());
    let font = SoundFont::new(&mut file).unwrap();
    let checksum = font.get_wave_data().iter().fold(0_u64, |sum, &s| {
        sum.wrapping_mul(31).wrapping_add(s as u16 as u64)
    });
    println!(
        "{} samples, checksum {checksum:x}, loaded in {:?}",
        font.get_wave_data().len(),
        start.elapsed()
    );
}

#[test]
#[ignore = "needs a SoundFont with drums: GSCORE_SF2=/path/to.sf2 cargo test --release -p gscore-audio --test test_split_channels -- --ignored"]
fn a_second_drum_channel_plays_the_same_kit() {
    // A kick (key 36) on channel 10 and on channel 4 made percussion sound the same; on
    // channel 4 as a melodic channel it is a piano note.
    let path = std::env::var("GSCORE_SF2").unwrap();
    let mut file = std::io::BufReader::new(std::fs::File::open(&path).unwrap());
    let font = Arc::new(SoundFont::new(&mut file).unwrap());
    let kick = |channel: usize, drums: bool| {
        let mut settings = SynthesizerSettings::new(RATE);
        settings.enable_reverb_and_chorus = false;
        settings.split_channels = true;
        let mut synth = Synthesizer::new(&font, &settings).unwrap();
        synth.set_percussion_channel(channel, drums);
        synth.note_on(channel as i32, 36, 110);
        let out = channels(&mut synth, 9_600);
        out[channel][0].clone()
    };
    let ten = kick(9, true);
    let four = kick(3, true);
    let piano = kick(3, false);
    assert!(energy(&ten) > 1.0);
    let difference: f32 = ten.iter().zip(&four).map(|(a, b)| (a - b).abs()).sum();
    assert!(difference < 1e-3, "the same kick: {difference}");
    let other: f32 = ten.iter().zip(&piano).map(|(a, b)| (a - b).abs()).sum();
    assert!(other > 1.0, "a melodic channel plays something else");
    println!("same kit on channel 4 (difference {difference}), piano differs by {other}");
}

#[test]
fn a_drum_channels_parts_add_up_to_its_channel() {
    // The fork sorts a percussion channel's voices by drum part (kick, snare, hi-hat, ...)
    // as well as summing them into the channel: the mixer gives each part its own chain.
    let bytes = gscore_audio::testing::sine_drum_soundfont("Test Drums");
    let font = Arc::new(SoundFont::new(&mut bytes.as_slice()).unwrap());
    let mut settings = SynthesizerSettings::new(RATE);
    settings.enable_reverb_and_chorus = false;
    settings.split_channels = true;
    let mut synth = Synthesizer::new(&font, &settings).unwrap();
    synth.set_percussion_channel(9, true);
    // A kick, a snare and a hi-hat, each on its own part.
    for key in [36, 38, 42] {
        synth.note_on(9, key, 100);
    }
    let frames = 64;
    let mut sounded = [false; rustysynth::PARTS];
    for _ in 0..40 {
        let mut out: Vec<[Vec<f32>; 2]> = (0..CHANNELS)
            .map(|_| [vec![0.0; frames], vec![0.0; frames]])
            .collect();
        {
            let (mut lefts, mut rights): (Vec<&mut [f32]>, Vec<&mut [f32]>) = out
                .iter_mut()
                .map(|[left, right]| (&mut left[..], &mut right[..]))
                .unzip();
            synth.render_channels(&mut lefts, &mut rights);
        }
        for (side, rendered) in out.get(9).into_iter().flatten().enumerate() {
            for (index, channel) in rendered.iter().enumerate() {
                let mut sum = 0.0_f32;
                for part in 0..rustysynth::PARTS {
                    let block = synth.part_block(9, part, side);
                    sum += block.get(index).copied().unwrap_or(0.0);
                }
                assert!((sum - channel).abs() < 1e-6, "{sum} against {channel}");
            }
        }
        for (part, heard) in sounded.iter_mut().enumerate() {
            if energy(synth.part_block(9, part, 0)) > 0.0 {
                *heard = true;
            }
        }
    }
    // The kick, snare and hi-hat landed in their own parts, and nowhere else.
    assert!(sounded[0] && sounded[1] && sounded[2], "{sounded:?}");
    assert!(!sounded[3] && !sounded[4] && !sounded[5], "{sounded:?}");
    // A melodic channel has no parts (its sound is only in its channel).
    assert!(synth.part_block(0, 0, 0).iter().all(|&x| x == 0.0));
}

/// A split synthesizer on the stereo choke kit, channel 10 playing drums.
fn choke_kit() -> Synthesizer {
    let bytes = gscore_audio::testing::stereo_choke_drum_soundfont("Test Kit");
    let font = Arc::new(SoundFont::new(&mut bytes.as_slice()).unwrap());
    let mut settings = SynthesizerSettings::new(RATE);
    settings.enable_reverb_and_chorus = false;
    settings.split_channels = true;
    let mut synth = Synthesizer::new(&font, &settings).unwrap();
    synth.set_percussion_channel(9, true);
    synth
}

#[test]
fn a_stereo_hit_in_a_choke_group_sounds_on_both_sides_and_chokes_the_last() {
    // FluidR3's hi-hats are two regions (left and right) in one exclusive class: the
    // second region took the first one's voice, and the hi-hat sounded on one side only.
    let mut synth = choke_kit();
    synth.note_on(9, 42, 100);
    let out = channels(&mut synth, 4_800);
    let (left, right) = (energy(&out[9][0]), energy(&out[9][1]));
    assert!(left > 1.0 && right > 1.0, "both sides: {left} {right}");
    assert!((left / right - 1.0).abs() < 0.1, "balanced: {left} {right}");
    assert_eq!(synth.active_voice_count(), 2, "one voice per side");
    // An open hi-hat (another key of the group) chokes the closed one: still two voices.
    synth.note_on(9, 46, 100);
    let _ = channels(&mut synth, 960);
    assert_eq!(synth.active_voice_count(), 2, "the last hit choked");
}

#[test]
fn a_drum_rings_out_past_its_note_off_and_a_new_hit_lets_the_last_go() {
    // The written length of a drum note does not cut it (a crash on a sixteenth rings); a
    // melodic channel still releases at its note-off.
    let tail = |drums: bool| {
        let mut synth = choke_kit();
        synth.set_percussion_channel(0, false);
        let channel = if drums { 9 } else { 0 };
        synth.note_on(channel, 49, 100);
        let _ = channels(&mut synth, 960);
        synth.note_off(channel, 49);
        let out = channels(&mut synth, 14_400);
        energy(&out[channel as usize][0][9_600..])
    };
    assert!(tail(true) > 1.0, "the drum rings on: {}", tail(true));
    assert!(tail(false) < 1e-3 * tail(true), "a melodic note stops");
    // Struck again, the drum's last hit is let go: the voices do not pile up.
    let mut synth = choke_kit();
    for _ in 0..20 {
        synth.note_on(9, 38, 100);
        let _ = channels(&mut synth, 480);
    }
    assert!(
        synth.active_voice_count() <= 4,
        "{}",
        synth.active_voice_count()
    );
}

#[test]
fn a_drum_channels_parts_line_up_with_calls_of_any_size() {
    // The engine's audio callback need not ask for whole blocks: the parts must be those
    // of exactly the samples a call gave, or the drums drop and repeat bits.
    let bytes = gscore_audio::testing::sine_drum_soundfont("Test Drums");
    let font = Arc::new(SoundFont::new(&mut bytes.as_slice()).unwrap());
    let mut settings = SynthesizerSettings::new(RATE);
    settings.enable_reverb_and_chorus = false;
    settings.split_channels = true;
    let mut synth = Synthesizer::new(&font, &settings).unwrap();
    synth.set_percussion_channel(9, true);
    synth.note_on(9, 36, 100);
    synth.note_on(9, 42, 80);
    for frames in [37_usize, 64, 5, 50, 13, 64, 1, 63].iter().cycle().take(40) {
        let mut out: Vec<[Vec<f32>; 2]> = (0..CHANNELS)
            .map(|_| [vec![0.0; *frames], vec![0.0; *frames]])
            .collect();
        {
            let (mut lefts, mut rights): (Vec<&mut [f32]>, Vec<&mut [f32]>) = out
                .iter_mut()
                .map(|[left, right]| (&mut left[..], &mut right[..]))
                .unzip();
            synth.render_channels(&mut lefts, &mut rights);
        }
        for (side, rendered) in out[9].iter().enumerate() {
            assert_eq!(synth.part_block(9, 0, side).len(), *frames);
            for (index, channel) in rendered.iter().enumerate() {
                let sum: f32 = (0..rustysynth::PARTS)
                    .map(|part| synth.part_block(9, part, side)[index])
                    .sum();
                assert!(
                    (sum - channel).abs() < 1e-6,
                    "{frames}: {sum} against {channel}"
                );
            }
        }
    }
}
