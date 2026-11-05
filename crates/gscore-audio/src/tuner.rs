//! A tuning tone at a chosen pitch on the default audio output: the fundamental with
//! overtones for low notes (a bare low sine is barely audible, and the ear finds a low
//! string's pitch by its overtones), a nearly pure tone for high ones.
//!
//! The device is opened on a thread of its own, so the UI never waits for it. The audio
//! callback reads two atomics (the frequency and whether it should sound) and renders a
//! faded sine, so starting and stopping never clicks. Like the synthesizer it does not
//! allocate, lock or do I/O in the callback, and a panic is contained and reported.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use gscore_play::PlayError;

/// How long the fade in and out lasts, in seconds.
const FADE: f32 = 0.012;
/// How loud the tone is: as loud as a sine at 0.25 of full scale, whatever its overtones
/// (comfortable, and it never clips).
const LEVEL: f32 = 0.25;
/// Overtones go up to about this pitch (hertz): a low E gets many, an A4 a few, the top
/// strings none.
const OVERTONE_CEILING: f32 = 2_400.0;
/// The most partials (the fundamental and its overtones) a tone has.
const MAX_PARTIALS: usize = 12;
/// How fast the overtones fall off: partial `k` has `1 / k^ROLL_OFF` of the fundamental's
/// amplitude (between a sawtooth's bright 1 and a softer string-like tone).
const ROLL_OFF: f32 = 1.3;
/// How often the worker checks whether it should end.
const POLL: Duration = Duration::from_millis(200);

/// What the worker and the audio callback share.
#[derive(Debug, Default)]
struct Shared {
    /// The frequency in hertz, as the bits of an `f32`.
    frequency: AtomicU32,
    /// Whether the tone should sound.
    playing: AtomicBool,
    /// The device failed; the callback stays silent.
    failed: AtomicBool,
    /// Why it failed, for the UI.
    error: Mutex<Option<String>>,
    /// The device's name, once open.
    device: Mutex<Option<String>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether `frequency` hertz is something the tuner should play.
fn audible(frequency: f32) -> bool {
    frequency.is_finite() && (20.0..=20_000.0).contains(&frequency)
}

/// How many partials a tone at `frequency` has: the fundamental and the overtones below
/// [`OVERTONE_CEILING`].
fn partials(frequency: f32) -> usize {
    if !(frequency.is_finite() && frequency > 0.0) {
        return 1;
    }
    ((OVERTONE_CEILING / frequency) as usize).clamp(1, MAX_PARTIALS)
}

/// The tone at `phase` (0 to 1 through one period) with `count` partials, scaled to the
/// loudness (RMS) of a sine of amplitude 1.
fn tone(phase: f32, count: usize) -> f32 {
    let mut sum = 0.0;
    let mut energy = 0.0;
    for k in 1..=count.max(1) {
        let partial = k as f32;
        let amplitude = partial.powf(-ROLL_OFF);
        sum += amplitude * (phase * partial * std::f32::consts::TAU).sin();
        energy += amplitude * amplitude;
    }
    sum / energy.sqrt()
}

/// Plays one tuning tone at a time (the tuner).
#[derive(Debug)]
pub struct TunerPlayer {
    shared: Arc<Shared>,
    /// Dropping it ends the worker (and the stream).
    quit: Option<SyncSender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl TunerPlayer {
    /// Opens the default output device in the background.
    pub fn start() -> Result<Self, PlayError> {
        let shared = Arc::new(Shared::default());
        let (quit, quit_receiver) = mpsc::sync_channel(1);
        let worker = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("gscore-tuner".to_owned())
            .spawn(move || run(&quit_receiver, &worker))
            .map_err(|e| PlayError::Device(format!("cannot start the tuner: {e}")))?;
        Ok(TunerPlayer {
            shared,
            quit: Some(quit),
            thread: Some(thread),
        })
    }

    /// Starts (or retunes) the tone at `frequency` hertz.
    pub fn play(&self, frequency: f32) -> Result<(), PlayError> {
        if !audible(frequency) {
            return Err(PlayError::Device(format!(
                "{frequency} Hz is outside the audible range"
            )));
        }
        self.shared
            .frequency
            .store(frequency.to_bits(), Ordering::Relaxed);
        self.shared.playing.store(true, Ordering::Release);
        Ok(())
    }

    /// Silences the tone (it fades out).
    pub fn stop(&self) {
        self.shared.playing.store(false, Ordering::Release);
    }

    /// Whether the tone should be sounding (it may still be fading out).
    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Acquire)
    }

    /// Why the device could not be opened, if it could not.
    pub fn error(&self) -> Option<String> {
        lock(&self.shared.error).clone()
    }

    /// The output device's name, once open.
    pub fn device(&self) -> Option<String> {
        lock(&self.shared.device).clone()
    }
}

impl Drop for TunerPlayer {
    fn drop(&mut self) {
        self.shared.playing.store(false, Ordering::Release);
        // Closing the channel ends the worker's wait; the stream is dropped there. The
        // handle is detached, not joined: opening a device can take a while and this runs
        // on the UI thread.
        self.quit.take();
        if let Some(thread) = self.thread.take() {
            drop(thread);
        }
    }
}

fn run(quit: &Receiver<()>, shared: &Arc<Shared>) {
    let stream = match open_stream(shared) {
        Ok(stream) => stream,
        Err(error) => {
            *lock(&shared.error) = Some(error.to_string());
            shared.failed.store(true, Ordering::Release);
            return;
        }
    };
    // Until the player is dropped (the quit channel closes).
    while let Err(RecvTimeoutError::Timeout) = quit.recv_timeout(POLL) {}
    drop(stream);
}

/// Opens the default output device and starts a stream rendering the tone.
fn open_stream(shared: &Arc<Shared>) -> Result<Stream, PlayError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| PlayError::Device("no audio output device for the tuner".to_owned()))?;
    let name = device
        .description()
        .map(|d| d.name().to_owned())
        .unwrap_or_else(|_| "the default audio output".to_owned());
    let supported = device
        .default_output_config()
        .map_err(|e| PlayError::Device(format!("{name}: {e}")))?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let parts = Parts {
        shared: Arc::clone(shared),
        sample_rate: config.sample_rate as f32,
        channels: usize::from(config.channels),
        phase: 0.0,
        gain: 0.0,
    };
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, parts),
        SampleFormat::F64 => build::<f64>(&device, &config, parts),
        SampleFormat::I16 => build::<i16>(&device, &config, parts),
        SampleFormat::I32 => build::<i32>(&device, &config, parts),
        SampleFormat::U16 => build::<u16>(&device, &config, parts),
        SampleFormat::U8 => build::<u8>(&device, &config, parts),
        other => Err(PlayError::Device(format!(
            "{name} wants {other} samples, which the tuner cannot produce"
        ))),
    }
    .map_err(|e| PlayError::Device(format!("{name}: {e}")))?;
    stream
        .play()
        .map_err(|e| PlayError::Device(format!("{name}: {e}")))?;
    *lock(&shared.device) = Some(name);
    Ok(stream)
}

/// State the audio callback owns.
struct Parts {
    shared: Arc<Shared>,
    sample_rate: f32,
    channels: usize,
    /// Position in the sine, 0 to 1.
    phase: f32,
    /// The current fade gain, 0 to 1.
    gain: f32,
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut parts: Parts,
) -> Result<Stream, PlayError>
where
    T: SizedSample + FromSample<f32>,
{
    let callback_errors = Arc::clone(&parts.shared);
    let error_errors = Arc::clone(&parts.shared);
    let mut broken = false;
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                if broken {
                    data.fill(T::EQUILIBRIUM);
                    return;
                }
                let rendered = catch_unwind(AssertUnwindSafe(|| callback(&mut parts, data)));
                if rendered.is_err() {
                    broken = true;
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        *lock(&callback_errors.error) =
                            Some("the tuner's audio callback panicked".to_owned());
                        callback_errors.failed.store(true, Ordering::Release);
                    }));
                }
            },
            move |error| {
                *lock(&error_errors.error) = Some(format!("audio output error: {error}"));
                error_errors.failed.store(true, Ordering::Release);
            },
            None,
        )
        .map_err(|e| PlayError::Device(format!("{e}")))
}

fn callback<T>(parts: &mut Parts, data: &mut [T])
where
    T: SizedSample + FromSample<f32>,
{
    let playing = parts.shared.playing.load(Ordering::Acquire)
        && !parts.shared.failed.load(Ordering::Acquire);
    let target = if playing { 1.0 } else { 0.0 };
    if !playing && parts.gain <= 0.0 {
        data.fill(T::EQUILIBRIUM);
        parts.phase = 0.0;
        return;
    }
    let frequency = if playing {
        f32::from_bits(parts.shared.frequency.load(Ordering::Relaxed))
    } else {
        0.0
    };
    let step = 1.0 / (FADE * parts.sample_rate.max(1.0));
    let increment = frequency / parts.sample_rate.max(1.0);
    let count = partials(frequency);
    for frame in data.chunks_mut(parts.channels.max(1)) {
        if parts.gain < target {
            parts.gain = (parts.gain + step).min(target);
        } else {
            parts.gain = (parts.gain - step).max(target);
        }
        let sample = tone(parts.phase, count) * parts.gain * LEVEL;
        for slot in frame.iter_mut() {
            *slot = T::from_sample(sample);
        }
        parts.phase = (parts.phase + increment).fract().abs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_notes_get_overtones_and_high_ones_stay_pure() {
        // Low E on a guitar, a bass's B, A4 and a high E.
        assert!(partials(82.41) >= 10);
        assert_eq!(partials(30.87), MAX_PARTIALS);
        assert!((2..=6).contains(&partials(440.0)));
        assert_eq!(partials(1_318.5), 1);
        // One partial is the plain sine.
        assert!((tone(0.25, 1) - 1.0).abs() < 1e-6);
        // However many overtones, the tone is as loud as the sine and stays in range.
        for count in [1, 4, MAX_PARTIALS] {
            let samples: Vec<f32> = (0..4_000)
                .map(|i| tone(i as f32 / 4_000.0, count))
                .collect();
            let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
            assert!(
                (rms - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01,
                "{count}: {rms}"
            );
            let peak = samples.iter().fold(0.0_f32, |a, b| a.max(b.abs()));
            assert!(peak * LEVEL < 1.0, "{count}: {peak}");
        }
    }

    #[test]
    fn only_audible_frequencies_are_accepted() {
        for frequency in [20.0, 82.41, 440.0, 20_000.0] {
            assert!(audible(frequency), "{frequency}");
        }
        for frequency in [0.0, 10.0, 20_001.0, f32::NAN, f32::INFINITY] {
            assert!(!audible(frequency), "{frequency}");
        }
    }
}
