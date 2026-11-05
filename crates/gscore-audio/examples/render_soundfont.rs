//! Renders a few General MIDI programs with the internal synthesizer and reports their
//! level, then writes the result as a WAV so two SoundFonts can be compared by ear.
//!
//! ```text
//! cargo run -p gscore-audio --example render_soundfont -- OUT.wav FONT.sf2
//! ```
//!
//! This is a development tool: the browser build's bundled SoundFont must not be near
//! silent or badly filtered, and a modulator that the synthesizer does not implement shows
//! up here as a level far from the others (or as silence).

// Levels and sample indices are small, positive and bounded; the casts are exact enough for
// a measurement tool.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    unused_crate_dependencies
)]

use std::io::Write;
use std::path::Path;

use gscore_audio::{SoundBank, SynthEngine};
use gscore_play::{Channel, MidiEvent};

/// One program and the note that shows it.
struct Probe {
    name: &'static str,
    program: u8,
    key: u8,
    percussion: bool,
}

const PROBES: &[Probe] = &[
    Probe {
        name: "piano",
        program: 0,
        key: 60,
        percussion: false,
    },
    Probe {
        name: "nylon guitar",
        program: 24,
        key: 60,
        percussion: false,
    },
    Probe {
        name: "steel guitar",
        program: 25,
        key: 60,
        percussion: false,
    },
    Probe {
        name: "clean electric",
        program: 27,
        key: 60,
        percussion: false,
    },
    Probe {
        name: "overdriven",
        program: 29,
        key: 55,
        percussion: false,
    },
    Probe {
        name: "distortion",
        program: 30,
        key: 55,
        percussion: false,
    },
    Probe {
        name: "acoustic bass",
        program: 32,
        key: 40,
        percussion: false,
    },
    Probe {
        name: "finger bass",
        program: 33,
        key: 40,
        percussion: false,
    },
    Probe {
        name: "slap bass",
        program: 36,
        key: 40,
        percussion: false,
    },
    Probe {
        name: "strings",
        program: 48,
        key: 60,
        percussion: false,
    },
    Probe {
        name: "choir",
        program: 52,
        key: 60,
        percussion: false,
    },
    Probe {
        name: "warm pad",
        program: 89,
        key: 60,
        percussion: false,
    },
    Probe {
        name: "drums (kick)",
        program: 0,
        key: 36,
        percussion: true,
    },
];

const SAMPLE_RATE: u32 = 44_100;
const NOTE_SECONDS: f32 = 1.2;
const TAIL_SECONDS: f32 = 0.4;
const GAP_SECONDS: f32 = 0.2;

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let out = args
        .next()
        .ok_or("usage: render_soundfont OUT.wav FONT.sf2")?;
    let font = args
        .next()
        .ok_or("usage: render_soundfont OUT.wav FONT.sf2")?;
    let bank = SoundBank::open(Path::new(&font)).map_err(|e| e.to_string())?;
    let mut engine = SynthEngine::new(&bank, SAMPLE_RATE).map_err(|e| e.to_string())?;
    println!("font: {} ({} presets)", bank.name(), bank.preset_count());

    let mut samples: Vec<f32> = Vec::new();
    println!(
        "{:20} {:>8} {:>8} {:>10}",
        "program", "rms dB", "peak dB", "audible s"
    );
    for probe in PROBES {
        let channel = if probe.percussion {
            Channel::PERCUSSION
        } else {
            Channel::new(0).ok_or("no MIDI channel")?
        };
        if !probe.percussion {
            engine.send_event(MidiEvent::ProgramChange {
                channel,
                program: probe.program,
            });
        }
        engine.send_event(MidiEvent::NoteOn {
            channel,
            key: probe.key,
            velocity: 100,
        });
        let note = render(&mut engine, NOTE_SECONDS);
        engine.send_event(MidiEvent::NoteOff {
            channel,
            key: probe.key,
        });
        let tail = render(&mut engine, TAIL_SECONDS);
        let (rms, peak, audible) = levels(&note);
        println!(
            "{:20} {:8.1} {:8.1} {:10.2}",
            probe.name,
            to_db(rms),
            to_db(peak),
            audible
        );
        samples.extend(note);
        samples.extend(tail);
        samples.extend(std::iter::repeat_n(
            0.0,
            frames(GAP_SECONDS).saturating_mul(2),
        ));
    }
    write_wav(Path::new(&out), &samples)?;
    println!("wrote {out} ({:.1} s)", seconds(&samples));
    Ok(())
}

/// Renders `seconds` of stereo samples (interleaved).
fn render(engine: &mut SynthEngine, seconds: f32) -> Vec<f32> {
    let frames = frames(seconds);
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    engine.render(&mut left, &mut right);
    interleave(&left, &right)
}

fn frames(seconds: f32) -> usize {
    let frames = (seconds * SAMPLE_RATE as f32).max(0.0);
    usize::try_from(frames as u64).unwrap_or(0)
}

fn interleave(left: &[f32], right: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(left.len().saturating_mul(2));
    for (l, r) in left.iter().zip(right) {
        out.push(*l);
        out.push(*r);
    }
    out
}

/// Root-mean-square level, peak and the seconds above a small threshold.
fn levels(samples: &[f32]) -> (f32, f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut sum = 0.0_f64;
    let mut peak = 0.0_f32;
    let mut audible = 0_usize;
    for sample in samples {
        sum += f64::from(*sample) * f64::from(*sample);
        peak = peak.max(sample.abs());
        if sample.abs() > 0.0005 {
            audible = audible.saturating_add(1);
        }
    }
    let count = f64::from(u32::try_from(samples.len()).unwrap_or(u32::MAX));
    let rms = (sum / count.max(1.0)).sqrt() as f32;
    let seconds = audible as f32 / (SAMPLE_RATE as f32 * 2.0);
    (rms, peak, seconds)
}

fn to_db(level: f32) -> f32 {
    if level <= 0.0 {
        -120.0
    } else {
        20.0 * level.log10()
    }
}

fn seconds(samples: &[f32]) -> f32 {
    samples.len() as f32 / (SAMPLE_RATE as f32 * 2.0)
}

/// 16-bit PCM WAV, stereo.
fn write_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    let data_bytes = u32::try_from(samples.len().saturating_mul(2)).unwrap_or(u32::MAX);
    let mut file = std::fs::File::create(path)
        .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    let mut header = Vec::with_capacity(44);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&data_bytes.saturating_add(36).to_le_bytes());
    header.extend_from_slice(b"WAVEfmt ");
    header.extend_from_slice(&16_u32.to_le_bytes());
    header.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    header.extend_from_slice(&2_u16.to_le_bytes()); // stereo
    header.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    header.extend_from_slice(&SAMPLE_RATE.saturating_mul(4).to_le_bytes());
    header.extend_from_slice(&4_u16.to_le_bytes());
    header.extend_from_slice(&16_u16.to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&data_bytes.to_le_bytes());
    file.write_all(&header)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    let mut data = Vec::with_capacity(samples.len().saturating_mul(2));
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
        data.extend_from_slice(&value.to_le_bytes());
    }
    file.write_all(&data)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}
