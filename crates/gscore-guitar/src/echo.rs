//! A digital delay: echoes of the signal, as a lead guitar's. The time is free (ms) or a
//! note value synced to the song's tempo (as played, with the playback speed). The
//! repeats feed back through a low and a high cut, and can bounce between the sides
//! (ping-pong).
//!
//! ```text
//!  in ─┬───────────────────────────────────────────────────────┬─► + ─► out
//!      └─► + ─► [ line, left and right ] ─► low cut ─► high cut ─┴─► level
//!          ▲                                                │
//!          └──────────── feedback (crossed: ping-pong) ◄─────┘
//! ```
//!
//! The dry signal passes at full level and the echoes are added under it (Level), as on
//! a delay pedal. When the time changes (a knob, a new tempo) the read point glides to it
//! over a few tens of milliseconds instead of jumping, so nothing clicks.
//!
//! Ring-buffer index arithmetic only, bounded by the buffer length: the module allows the
//! arithmetic lint for that (no value can leave the buffer, and nothing panics).

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]

use crate::filters::Biquad;

/// The longest delay, seconds.
pub const MAX_DELAY_SECONDS: f32 = 2.5;
/// How fast the read point glides to a new time (time constant, seconds).
const GLIDE: f32 = 0.04;
/// Below this the echoes count as silent.
const SILENCE: f32 = 1e-6;
/// Quality of the cut filters (Butterworth).
const CUT_Q: f32 = 0.707;

/// A delay time as a note value against the beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DelayNote {
    Half,
    DottedQuarter,
    Quarter,
    QuarterTriplet,
    DottedEighth,
    #[default]
    Eighth,
    EighthTriplet,
    DottedSixteenth,
    Sixteenth,
    SixteenthTriplet,
}

impl DelayNote {
    /// Every note value, longest first, as a user interface shows them.
    pub const ALL: [DelayNote; 10] = [
        DelayNote::Half,
        DelayNote::DottedQuarter,
        DelayNote::Quarter,
        DelayNote::QuarterTriplet,
        DelayNote::DottedEighth,
        DelayNote::Eighth,
        DelayNote::EighthTriplet,
        DelayNote::DottedSixteenth,
        DelayNote::Sixteenth,
        DelayNote::SixteenthTriplet,
    ];

    /// Its length in quarter notes.
    pub fn quarters(self) -> f32 {
        match self {
            DelayNote::Half => 2.0,
            DelayNote::DottedQuarter => 1.5,
            DelayNote::Quarter => 1.0,
            DelayNote::QuarterTriplet => 2.0 / 3.0,
            DelayNote::DottedEighth => 0.75,
            DelayNote::Eighth => 0.5,
            DelayNote::EighthTriplet => 1.0 / 3.0,
            DelayNote::DottedSixteenth => 0.375,
            DelayNote::Sixteenth => 0.25,
            DelayNote::SixteenthTriplet => 1.0 / 6.0,
        }
    }

    /// Its short name: `1/8.` dotted, `1/8T` a triplet.
    pub fn label(self) -> &'static str {
        match self {
            DelayNote::Half => "1/2",
            DelayNote::DottedQuarter => "1/4.",
            DelayNote::Quarter => "1/4",
            DelayNote::QuarterTriplet => "1/4T",
            DelayNote::DottedEighth => "1/8.",
            DelayNote::Eighth => "1/8",
            DelayNote::EighthTriplet => "1/8T",
            DelayNote::DottedSixteenth => "1/16.",
            DelayNote::Sixteenth => "1/16",
            DelayNote::SixteenthTriplet => "1/16T",
        }
    }
}

/// The knobs of a delay block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DelayKnobs {
    /// Whether the time follows the tempo (`note`) or is free (`time_ms`).
    pub sync: bool,
    pub note: DelayNote,
    pub time_ms: f32,
    /// How much of each echo comes back as the next, 0 one echo to 0.95 a long trail.
    pub feedback: f32,
    /// The echoes' level under the dry signal, 0 none to 1 as loud.
    pub level: f32,
    /// Low cut and high cut on the echoes, Hz (each repeat passes them again: darker and
    /// thinner as they fade).
    pub low_cut: f32,
    pub high_cut: f32,
    /// How much the echoes bounce between the sides, 0 in the middle to 1 ping-pong.
    pub ping_pong: f32,
}

impl Default for DelayKnobs {
    fn default() -> Self {
        DelayKnobs {
            sync: true,
            note: DelayNote::default(),
            time_ms: 350.0,
            feedback: 0.35,
            level: 0.35,
            low_cut: 120.0,
            high_cut: 6_000.0,
            ping_pong: 0.0,
        }
    }
}

impl DelayKnobs {
    /// Every value in its range (a hand-edited settings file can hold anything).
    pub fn sanitized(self) -> Self {
        let fit = |value: f32, (low, high): (f32, f32), fallback: f32| {
            if value.is_finite() {
                value.clamp(low, high)
            } else {
                fallback
            }
        };
        let d = DelayKnobs::default();
        let ranges = crate::knob_ranges::DELAY_MS;
        DelayKnobs {
            sync: self.sync,
            note: self.note,
            time_ms: fit(self.time_ms, ranges, d.time_ms),
            feedback: fit(self.feedback, crate::knob_ranges::FEEDBACK, d.feedback),
            level: fit(self.level, crate::knob_ranges::MIX, d.level),
            low_cut: fit(self.low_cut, crate::knob_ranges::LOW_CUT, d.low_cut),
            high_cut: fit(self.high_cut, crate::knob_ranges::HIGH_CUT, d.high_cut),
            ping_pong: fit(self.ping_pong, crate::knob_ranges::WIDTH, d.ping_pong),
        }
    }

    /// The delay time in seconds at `bpm` quarter notes a minute (within the line's
    /// length).
    pub fn seconds(&self, bpm: f32) -> f32 {
        let seconds = if self.sync {
            let bpm = if bpm.is_finite() && bpm > 0.0 {
                bpm
            } else {
                120.0
            };
            self.note.quarters() * 60.0 / bpm
        } else {
            self.time_ms / 1_000.0
        };
        seconds.clamp(0.001, MAX_DELAY_SECONDS)
    }
}

/// One side's line.
#[derive(Debug, Clone)]
struct Line {
    buffer: Vec<f32>,
    at: usize,
    cuts: [Biquad; 2],
}

impl Line {
    fn new(frames: usize) -> Self {
        Line {
            buffer: vec![0.0; frames.max(4)],
            at: 0,
            cuts: [Biquad::default(); 2],
        }
    }

    /// What was written `delay` samples ago (linear interpolation), through the cuts.
    fn read(&mut self, delay: f32) -> f32 {
        let length = self.buffer.len();
        let delay = delay.clamp(1.0, (length - 2) as f32);
        let whole = delay.floor();
        let fraction = delay - whole;
        let first = (self.at + length - whole as usize) % length;
        let second = (first + length - 1) % length;
        let raw = self.buffer[first] * (1.0 - fraction) + self.buffer[second] * fraction;
        let [low, high] = &mut self.cuts;
        high.process(low.process(raw))
    }

    fn write(&mut self, x: f32) {
        let length = self.buffer.len();
        self.buffer[self.at] = x;
        self.at = (self.at + 1) % length;
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
        for cut in &mut self.cuts {
            cut.reset();
        }
    }
}

/// The delay of one chain block.
#[derive(Debug, Clone)]
pub(crate) struct Echo {
    rate: f32,
    knobs: DelayKnobs,
    bpm: f32,
    lines: [Line; 2],
    /// The read point now and where it glides, samples.
    delay: f32,
    target: f32,
    glide: f32,
    /// Samples since an echo above silence was written, to know when the trail is over.
    quiet: usize,
}

impl Echo {
    /// A delay for `sample_rate` with `knobs`. Allocates (off the audio thread).
    pub(crate) fn new(sample_rate: f32, knobs: DelayKnobs) -> Self {
        let rate = sample_rate.max(1.0);
        let frames = (rate * MAX_DELAY_SECONDS).ceil() as usize + 4;
        let mut echo = Echo {
            rate,
            knobs,
            bpm: 120.0,
            lines: [Line::new(frames), Line::new(frames)],
            delay: 0.0,
            target: 0.0,
            glide: 1.0 - (-1.0 / (GLIDE * rate)).exp(),
            quiet: usize::MAX,
        };
        echo.set_knobs(knobs);
        echo.delay = echo.target;
        echo
    }

    /// New knob values; the time glides there.
    pub(crate) fn set_knobs(&mut self, knobs: DelayKnobs) {
        let knobs = knobs.sanitized();
        let low = Biquad::highpass(self.rate, knobs.low_cut, CUT_Q);
        let high = Biquad::lowpass(self.rate, knobs.high_cut, CUT_Q);
        for line in &mut self.lines {
            let [low_cut, high_cut] = &mut line.cuts;
            *low_cut = low_cut.retuned(low);
            *high_cut = high_cut.retuned(high);
        }
        self.knobs = knobs;
        self.retarget();
    }

    /// The tempo the song plays at now, quarter notes a minute (with the playback speed):
    /// a synced time follows it.
    pub(crate) fn set_tempo(&mut self, bpm: f32) {
        if bpm.is_finite() && bpm > 0.0 && (bpm - self.bpm).abs() > 1e-3 {
            self.bpm = bpm;
            self.retarget();
        }
    }

    fn retarget(&mut self) {
        self.target = self.knobs.seconds(self.bpm) * self.rate;
    }

    /// Whether echoes are still on their way (the strings may be silent by then).
    pub(crate) fn ringing(&self) -> bool {
        (self.quiet as f32) < (MAX_DELAY_SECONDS + 0.1) * self.rate
    }

    /// One stereo sample; `stereo` false when both sides carry the same mono signal.
    pub(crate) fn process(&mut self, left: f32, right: f32, stereo: bool) -> (f32, f32) {
        self.delay += (self.target - self.delay) * self.glide;
        let [line_l, line_r] = &mut self.lines;
        let echo_l = line_l.read(self.delay);
        let echo_r = line_r.read(self.delay);
        let knobs = self.knobs;
        let pong = knobs.ping_pong;
        // What comes back into each side: its own echo, or the other side's for ping-pong.
        let back_l = (echo_l * (1.0 - pong) + echo_r * pong) * knobs.feedback;
        let back_r = (echo_r * (1.0 - pong) + echo_l * pong) * knobs.feedback;
        // Ping-pong starts on the left: the right hears the input less, the bounce more.
        let (in_l, in_r) = if stereo {
            (left, right * (1.0 - pong))
        } else {
            (left, left * (1.0 - pong))
        };
        let write_l = in_l + back_l;
        let write_r = in_r + back_r;
        if !(write_l.is_finite() && write_r.is_finite()) {
            // A broken value empties the lines instead of echoing on.
            self.clear();
            return (0.0, 0.0);
        }
        line_l.write(write_l);
        line_r.write(write_r);
        self.quiet = if write_l.abs().max(write_r.abs()) > SILENCE {
            0
        } else {
            self.quiet.saturating_add(1)
        };
        (left + echo_l * knobs.level, right + echo_r * knobs.level)
    }

    /// Silence in both lines.
    pub(crate) fn clear(&mut self) {
        for line in &mut self.lines {
            line.clear();
        }
        self.quiet = usize::MAX;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    /// An impulse through `echo`, `seconds` long, both sides.
    fn impulse(echo: &mut Echo, seconds: f32) -> (Vec<f32>, Vec<f32>) {
        let frames = (seconds * RATE) as usize;
        let (mut left, mut right) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
        for i in 0..frames {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (l, r) = echo.process(x, x, false);
            left.push(l);
            right.push(r);
        }
        (left, right)
    }

    /// The index of the loudest sample after `from`.
    fn loudest(signal: &[f32], from: usize) -> usize {
        (from..signal.len())
            .max_by(|&a, &b| signal[a].abs().total_cmp(&signal[b].abs()))
            .unwrap_or(0)
    }

    #[test]
    fn a_synced_echo_lands_on_the_note_value_at_the_tempo() {
        let knobs = DelayKnobs {
            sync: true,
            note: DelayNote::DottedEighth,
            low_cut: 20.0,
            high_cut: 20_000.0,
            ..DelayKnobs::default()
        };
        let mut echo = Echo::new(RATE, knobs);
        echo.set_tempo(100.0);
        echo.delay = echo.target;
        let (left, _) = impulse(&mut echo, 1.0);
        // A dotted eighth at 100 BPM: 0.45 s.
        let expected = (0.45 * RATE) as usize;
        let at = loudest(&left, 10);
        assert!(at.abs_diff(expected) < 4, "{at} against {expected}");
        // Free time ignores the tempo.
        let free = DelayKnobs {
            sync: false,
            time_ms: 200.0,
            ..knobs
        };
        let mut echo = Echo::new(RATE, free);
        echo.set_tempo(60.0);
        let (left, _) = impulse(&mut echo, 0.5);
        assert!(loudest(&left, 10).abs_diff(9_600) < 4);
    }

    #[test]
    fn the_repeats_fade_and_ping_pong_alternates() {
        let knobs = DelayKnobs {
            sync: false,
            time_ms: 100.0,
            feedback: 0.5,
            level: 1.0,
            low_cut: 20.0,
            high_cut: 20_000.0,
            ping_pong: 1.0,
            ..DelayKnobs::default()
        };
        let mut echo = Echo::new(RATE, knobs);
        let (left, right) = impulse(&mut echo, 0.45);
        let at = |ms: usize| (ms * RATE as usize / 1_000).saturating_sub(40);
        let peak = |signal: &[f32], ms: usize| {
            signal[at(ms)..at(ms) + 80]
                .iter()
                .fold(0.0_f32, |p, x| p.max(x.abs()))
        };
        // First echo left, the second right, the third left again, each quieter.
        assert!(peak(&left, 100) > 0.5 && peak(&right, 100) < 0.01);
        assert!(peak(&right, 200) > 0.2 && peak(&left, 200) < 0.01);
        assert!(peak(&left, 300) > 0.1 && peak(&left, 300) < peak(&left, 100));
        assert!(echo.ringing());
        // Without feedback the trail ends after one echo, and the delay knows it.
        let mut once = Echo::new(
            RATE,
            DelayKnobs {
                feedback: 0.0,
                ..knobs
            },
        );
        let _ = impulse(&mut once, MAX_DELAY_SECONDS + 0.5);
        assert!(!once.ringing());
    }

    #[test]
    fn a_new_tempo_glides_and_broken_values_stay_silent() {
        let mut echo = Echo::new(RATE, DelayKnobs::default());
        echo.set_tempo(120.0);
        let before = echo.target;
        echo.set_tempo(60.0);
        assert!((echo.target - 2.0 * before).abs() < 1.0, "twice as long");
        let _ = echo.process(0.0, 0.0, false);
        assert!(echo.delay < echo.target, "gliding, not jumping");
        echo.set_tempo(f32::NAN);
        // A broken input clears the lines (the chain mutes the broken sample itself).
        let _ = echo.process(f32::NAN, 0.0, true);
        let (l, r) = echo.process(0.0, 0.0, true);
        assert!(l.is_finite() && r.is_finite());
    }
}
