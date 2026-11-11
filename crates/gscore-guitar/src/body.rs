//! An acoustic guitar's body, computed: the air in the box and the wood's main modes as
//! resonances on the strings' sound, the way the body colours what the bridge drives
//! into it.
//!
//! Commuted synthesis takes a body's impulse response as the strings' excitation; every
//! part is linear, so the order can be swapped. Here the body runs once per track on the
//! sum of its strings: the same sound for a fraction of the cost, and no recording needed.
//!
//! ```text
//!  strings ─┬───────────────────────────────────────────────────────┬─► mix ─► out
//!           └─► low cut ─► [ air ─► top ─► back ─► … modes ] ─► trim ─┘
//! ```
//!
//! Each mode is a narrow bell (a peaking filter): it lifts its band and rings on for a
//! moment after the string's energy passes, as a real body does. The tables are typical
//! measurements of the two body styles, rounded: the air (Helmholtz) resonance near
//! 100 Hz, the top's first mode near 190 Hz with the back a little above it, the top's
//! cross and long dipoles around 400 and 550 Hz, and weaker modes higher up. Size scales
//! every frequency (a smaller body sounds higher, a jumbo lower).

use crate::filters::Biquad;

/// How many modes a body has at most.
const MODES: usize = 8;
/// Below the air resonance a body radiates little: its low cut, as a share of the air
/// mode's frequency.
const LOW_CUT_SHARE: f32 = 0.7;
/// Size 0 (a parlour) to 1 (a jumbo) scales the modes by this much up and down.
const SIZE_SPAN: f32 = 1.25;

/// The kind of body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BodyKind {
    /// No body (an electric's solid body, a bass).
    #[default]
    None,
    /// A steel-string dreadnought: a strong, deep air resonance and a bright top.
    Dreadnought,
    /// A classical guitar: a lighter top, warmer and rounder.
    Classical,
}

impl BodyKind {
    /// Every kind, in the order a user interface shows them.
    pub const ALL: [BodyKind; 3] = [BodyKind::None, BodyKind::Dreadnought, BodyKind::Classical];

    /// Its name.
    pub fn label(self) -> &'static str {
        match self {
            BodyKind::None => "None",
            BodyKind::Dreadnought => "Dreadnought",
            BodyKind::Classical => "Classical",
        }
    }

    /// The modes: frequency (Hz), quality, lift (dB); and the trim (dB) that keeps the
    /// body about as loud as the strings alone.
    fn modes(self) -> ([(f32, f32, f32); MODES], f32) {
        match self {
            BodyKind::None => ([(1_000.0, 1.0, 0.0); MODES], 0.0),
            BodyKind::Dreadnought => (
                [
                    (100.0, 14.0, 10.0),
                    (195.0, 20.0, 8.0),
                    (228.0, 18.0, 4.0),
                    (390.0, 22.0, 3.0),
                    (550.0, 22.0, 3.5),
                    (830.0, 20.0, 2.0),
                    (1_250.0, 16.0, 2.0),
                    // The top's radiation: a broad presence.
                    (2_600.0, 1.2, 2.5),
                ],
                -0.7,
            ),
            BodyKind::Classical => (
                [
                    (96.0, 12.0, 8.0),
                    (188.0, 18.0, 8.5),
                    (240.0, 16.0, 4.0),
                    (410.0, 20.0, 3.0),
                    (600.0, 20.0, 2.0),
                    (950.0, 18.0, 2.0),
                    (1_500.0, 14.0, 1.0),
                    // Warmer than a steel string: a soft dip in the upper mids.
                    (2_200.0, 1.0, -1.5),
                ],
                -0.7,
            ),
        }
    }
}

/// A body on one track's strings.
#[derive(Debug, Clone)]
pub struct Body {
    rate: f32,
    kind: BodyKind,
    mix: f32,
    trim: f32,
    low_cut: Biquad,
    modes: [Biquad; MODES],
    /// Samples since the body's output was last above silence, to stop computing it.
    quiet: usize,
}

/// Below this the body counts as silent...
const SILENCE: f32 = 1e-6;
/// ...and after this long (seconds) its resonances have died away.
const QUIET_HOLD: f32 = 1.0;

impl Body {
    /// A body at `sample_rate`, none yet.
    pub fn new(sample_rate: f32) -> Self {
        Body {
            rate: sample_rate.max(1.0),
            kind: BodyKind::None,
            mix: 0.0,
            trim: 1.0,
            low_cut: Biquad::default(),
            modes: [Biquad::default(); MODES],
            quiet: usize::MAX,
        }
    }

    /// Sets the kind, how much of the body is heard (`mix`, 0 to 1) and its size (0 small
    /// to 1 large, 0.5 the kind's own), keeping what rings.
    pub fn set(&mut self, kind: BodyKind, mix: f32, size: f32) {
        let mix = if mix.is_finite() {
            mix.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let size = if size.is_finite() {
            size.clamp(0.0, 1.0)
        } else {
            0.5
        };
        if kind != self.kind {
            self.reset();
        }
        self.kind = kind;
        self.mix = mix;
        if kind == BodyKind::None {
            return;
        }
        // A smaller body sounds higher: size 0 scales up by the span, 1 down.
        let scale = SIZE_SPAN.powf(1.0 - 2.0 * size);
        let (modes, trim_db) = kind.modes();
        let nyquist = self.rate * 0.45;
        for (filter, (frequency, q, gain)) in self.modes.iter_mut().zip(modes) {
            let frequency = (frequency * scale).min(nyquist);
            *filter = filter.retuned(Biquad::peaking(self.rate, frequency, q, gain));
        }
        let air = modes.first().map_or(100.0, |mode| mode.0) * scale;
        self.low_cut =
            self.low_cut
                .retuned(Biquad::highpass(self.rate, air * LOW_CUT_SHARE, 0.707));
        self.trim = 10.0_f32.powf(trim_db / 20.0);
    }

    /// Whether the body changes the sound at all.
    pub fn is_on(&self) -> bool {
        self.kind != BodyKind::None && self.mix > 0.0
    }

    /// Runs `samples` (the track's strings, mono) through the body, in place.
    pub fn process(&mut self, samples: &mut [f32]) {
        if !self.is_on() {
            return;
        }
        let incoming = samples.iter().fold(0.0_f32, |peak, x| peak.max(x.abs()));
        if incoming <= SILENCE && (self.quiet as f32) > QUIET_HOLD * self.rate {
            // Silent in and rung out: nothing to compute.
            return;
        }
        let mut peak = 0.0_f32;
        for sample in samples.iter_mut() {
            let dry = *sample;
            let wet = self
                .modes
                .iter_mut()
                .fold(self.low_cut.process(dry), |value, mode| mode.process(value))
                * self.trim;
            let out = dry + (wet - dry) * self.mix;
            peak = peak.max(out.abs());
            *sample = if out.is_finite() { out } else { 0.0 };
        }
        if !peak.is_finite() {
            self.reset();
            return;
        }
        self.quiet = if peak > SILENCE {
            0
        } else {
            self.quiet.saturating_add(samples.len())
        };
    }

    /// Silence in every resonance.
    pub fn reset(&mut self) {
        self.low_cut.reset();
        for mode in &mut self.modes {
            mode.reset();
        }
        self.quiet = usize::MAX;
    }
}
