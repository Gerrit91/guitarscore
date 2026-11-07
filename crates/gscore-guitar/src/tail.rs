//! Longer (or shorter) rooms: an impulse response whose decay is stretched by a factor.
//!
//! The decay rate is measured from the energy decay curve (Schroeder's backward
//! integration), fitted between -5 and -25 dB. The recording keeps its start (the direct
//! sound, the early reflections, the beginning of the tail) under an envelope that turns
//! its decay into the new one; where the recording fades out, a tail of noise takes over:
//! coloured like the room's late sound (noise filtered by a short window of the response
//! there), decaying at the new rate and growing darker as it goes, as a room's highs die
//! first. Noise is fully diffuse, and each channel gets its own, so the tail stays wide.
//! Everything is deterministic: the same response and factor give the same samples.
//!
//! Runs off the audio thread (when a chain is prepared); index arithmetic here is bounded
//! by the response's length.
#![allow(clippy::arithmetic_side_effects)]

use crate::filters::Noise;
use crate::impulse::{ImpulseError, ImpulseResponse};

/// The factors allowed: half as long to four times as long.
pub const LENGTH_RANGE: (f32, f32) = (0.5, 4.0);
/// The longest response made, in seconds.
pub const MAX_LENGTHENED_SECONDS: f32 = 16.0;
/// Where the decay is fitted on the energy decay curve, dB.
const FIT: (f32, f32) = (-5.0, -25.0);
/// The recording is followed down to this level (dB on the energy decay curve) before the
/// synthetic tail takes over...
const HANDOVER_DB: f32 = -30.0;
/// ...over a crossfade this long, in seconds.
const CROSSFADE: f32 = 0.08;
/// The window of the late response that colours the noise, in seconds.
const COLOUR_WINDOW: f32 = 0.0025;
/// The tail ends this far down, dB.
const END_DB: f32 = -80.0;
/// How much darker the tail grows: its lowpass falls from this many Hz at the handover to
/// [`DARKEST`] at the end.
const BRIGHTEST: f32 = 12_000.0;
const DARKEST: f32 = 1_500.0;

/// The energy decay curve of `samples` in dB (0 at the start), by backward integration.
fn decay_curve(samples: &[f32]) -> Vec<f32> {
    let mut remaining: f64 = samples.iter().map(|&x| f64::from(x) * f64::from(x)).sum();
    let total = remaining.max(1e-30);
    samples
        .iter()
        .map(|&x| {
            let db = 10.0 * (remaining.max(1e-30) / total).log10();
            remaining -= f64::from(x) * f64::from(x);
            db as f32
        })
        .collect()
}

/// The decay rate in dB per second (negative), fitted on `curve` between [`FIT`]'s levels;
/// `None` when the curve does not reach them.
fn decay_rate(curve: &[f32], rate: f32) -> Option<f32> {
    let start = curve.iter().position(|&db| db <= FIT.0)?;
    let end = curve.iter().position(|&db| db <= FIT.1)?;
    let span = end.checked_sub(start).filter(|&span| span > 0)? as f32 / rate;
    let drop = curve.get(end)? - curve.get(start)?;
    Some(drop / span).filter(|slope| slope.is_finite() && *slope < 0.0)
}

impl ImpulseResponse {
    /// This response with its decay time scaled by `factor` (see the module). A factor
    /// of 1 gives it back unchanged.
    pub fn lengthened(&self, factor: f32) -> Result<ImpulseResponse, ImpulseError> {
        let factor = if factor.is_finite() {
            factor.clamp(LENGTH_RANGE.0, LENGTH_RANGE.1)
        } else {
            1.0
        };
        if (factor - 1.0).abs() < 1e-3 {
            return Ok(self.clone());
        }
        let rate = self.sample_rate() as f32;
        let channels: Vec<Vec<f32>> = (0..self.channel_count())
            .map(|index| self.channel_at_rate(index, self.sample_rate()))
            .collect();
        // One decay for all channels, from their sum.
        let length = channels.first().map_or(0, Vec::len);
        let mut sum = vec![0.0_f32; length];
        for channel in &channels {
            for (total, &x) in sum.iter_mut().zip(channel) {
                *total += x;
            }
        }
        let curve = decay_curve(&sum);
        let slope = decay_rate(&curve, rate).unwrap_or(-60.0 / self.seconds().max(0.1));
        let new_slope = slope / factor;
        let handover = curve
            .iter()
            .position(|&db| db <= HANDOVER_DB)
            .unwrap_or(length)
            .min(length);
        // The new length: down to END_DB at the new rate from the handover's level.
        // Its level there under the new envelope (which raises a longer room's later parts).
        let handover_db = curve.get(handover).copied().unwrap_or(HANDOVER_DB)
            + (new_slope - slope) * handover as f32 / rate;
        let tail_seconds = ((END_DB - handover_db) / new_slope).max(0.0);
        let total = (handover as f32 + tail_seconds * rate)
            .min(MAX_LENGTHENED_SECONDS * rate)
            .max(1.0) as usize;
        let fade = ((CROSSFADE * rate) as usize).max(1);
        let window = ((COLOUR_WINDOW * rate) as usize).max(8);
        let out: Vec<Vec<f32>> = channels
            .iter()
            .enumerate()
            .map(|(index, channel)| {
                stretch(
                    channel,
                    Stretch {
                        rate,
                        slope,
                        new_slope,
                        handover,
                        total,
                        fade,
                        window,
                        seed: 0x2545_F491_u32.wrapping_add(index as u32 * 0x9E37_79B9),
                        synthesize: factor > 1.0,
                    },
                )
            })
            .collect();
        ImpulseResponse::from_channels_long(out, self.sample_rate())
    }
}

/// What [`stretch`] needs to know.
struct Stretch {
    rate: f32,
    slope: f32,
    new_slope: f32,
    handover: usize,
    total: usize,
    fade: usize,
    window: usize,
    seed: u32,
    synthesize: bool,
}

/// One channel stretched: the recording under the new envelope up to the handover, then
/// (when longer) the synthetic tail, crossfaded.
fn stretch(channel: &[f32], s: Stretch) -> Vec<f32> {
    // The envelope that turns the old decay into the new one, in dB per second.
    let gain_slope = s.new_slope - s.slope;
    let envelope = |n: usize| 10.0_f32.powf(gain_slope * (n as f32 / s.rate) / 20.0);
    let mut out = vec![0.0_f32; s.total];
    let keep = if s.synthesize {
        s.handover.saturating_add(s.fade).min(channel.len())
    } else {
        channel.len()
    };
    for (n, (slot, &x)) in out.iter_mut().zip(channel).take(keep).enumerate() {
        let fade_out = if s.synthesize && n >= s.handover {
            1.0 - n.saturating_sub(s.handover) as f32 / s.fade as f32
        } else {
            1.0
        };
        *slot = x * envelope(n) * fade_out.clamp(0.0, 1.0);
    }
    if !s.synthesize {
        return out;
    }
    // The level of the recording just before the handover, under the new envelope (over
    // a few windows, so one reflection does not decide it).
    let measure = s.handover.saturating_sub(s.fade.max(s.window));
    let level = rms(channel.get(measure..s.handover).unwrap_or_default()) * envelope(s.handover);
    let before = s.handover.saturating_sub(s.window);
    // The colour: the late response's window, applied to the noise as a short filter.
    let colour: Vec<f32> = channel
        .get(before..s.handover)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let hann = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / s.window as f32).cos();
            x * hann
        })
        .collect();
    let mut noise = Noise::new(s.seed);
    let mut history = vec![0.0_f32; colour.len().max(1)];
    let mut at = 0_usize;
    let mut dark = 0.0_f32;
    let tail_length = s.total.saturating_sub(s.handover).max(1) as f32;
    let mut tail = Vec::with_capacity(s.total.saturating_sub(s.handover));
    for n in s.handover..s.total {
        let white = noise.next();
        if let Some(slot) = history.get_mut(at) {
            *slot = white;
        }
        // FIR with the colour window (the newest noise against its first sample).
        let mut coloured = 0.0_f32;
        for (k, &c) in colour.iter().enumerate() {
            let index = (at + history.len() - k % history.len()) % history.len();
            coloured += c * history.get(index).copied().unwrap_or(0.0);
        }
        at = (at + 1) % history.len();
        // Darker as it goes.
        let progress = (n - s.handover) as f32 / tail_length;
        let cutoff = BRIGHTEST * (DARKEST / BRIGHTEST).powf(progress);
        let k = 1.0 - (-std::f32::consts::TAU * cutoff / s.rate).exp();
        dark += (coloured - dark) * k;
        let decay = 10.0_f32.powf(s.new_slope * ((n - s.handover) as f32 / s.rate) / 20.0);
        tail.push(dark * decay);
    }
    // Scaled to start at the recording's level, faded in across the crossfade.
    let start = rms(tail.get(..s.fade.min(tail.len())).unwrap_or_default());
    let scale = if start > 1e-12 { level / start } else { 0.0 };
    for (i, (slot, value)) in out.iter_mut().skip(s.handover).zip(tail).enumerate() {
        let fade_in = (i as f32 / s.fade as f32).min(1.0);
        *slot += value * scale * fade_in;
    }
    out
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )]

    use super::*;

    const RATE: u32 = 48_000;

    /// A synthetic room: noise decaying 60 dB in `t60` seconds, two channels.
    fn room(t60: f32, seconds: f32) -> ImpulseResponse {
        let length = (seconds * RATE as f32) as usize;
        let channel = |seed: u32| {
            let mut noise = Noise::new(seed);
            (0..length)
                .map(|n| noise.next() * 10.0_f32.powf(-3.0 * n as f32 / RATE as f32 / t60))
                .collect::<Vec<f32>>()
        };
        ImpulseResponse::from_channels(vec![channel(1), channel(2)], RATE).unwrap()
    }

    fn t60(response: &ImpulseResponse) -> f32 {
        let mono = response.mono_at_rate(RATE);
        let slope = decay_rate(&decay_curve(&mono), RATE as f32).unwrap();
        -60.0 / slope
    }

    #[test]
    fn a_longer_room_decays_slower_and_lasts_longer() {
        // Recorded down to about -75 dB.
        let original = room(1.2, 1.5);
        let longer = original.lengthened(2.0).unwrap();
        let measured = t60(&original);
        assert!(
            (measured - 1.2).abs() < 0.15,
            "the fit reads the room: {measured}"
        );
        let stretched = t60(&longer);
        assert!(
            (stretched / measured - 2.0).abs() < 0.3,
            "twice the decay time: {stretched} against {measured}"
        );
        assert!(longer.seconds() > original.seconds() * 1.3);
        assert_eq!(longer.channel_count(), 2);
        let shorter = original.lengthened(0.5).unwrap();
        assert!(t60(&shorter) < measured * 0.65);
        assert_eq!(original.lengthened(1.0).unwrap(), original);
    }

    #[test]
    fn the_tail_takes_over_at_the_recordings_level_and_repeats() {
        let original = room(0.8, 1.5);
        let longer = original.lengthened(3.0).unwrap();
        let samples = longer.channel_at_rate(0, RATE);
        assert!(samples.iter().all(|x| x.is_finite()));
        // Around the handover the level glides on: no step of more than 3 dB between
        // neighbouring 20 ms windows over the first two seconds.
        let window = 960;
        let levels: Vec<f32> = samples
            .chunks(window)
            .take(100)
            .map(|chunk| 10.0 * (chunk.iter().map(|x| x * x).sum::<f32>() / window as f32).log10())
            .collect();
        for pair in levels.windows(2) {
            assert!((pair[1] - pair[0]).abs() < 3.0, "a jump: {pair:?}");
        }
        assert_eq!(original.lengthened(3.0).unwrap(), longer, "deterministic");
    }
}
