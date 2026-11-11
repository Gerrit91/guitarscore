//! An acoustic guitar's body, computed: how the box colours what the bridge drives into it,
//! and the wood and air ringing on after every note.
//!
//! Commuted synthesis takes a body's impulse response as the strings' excitation; every
//! part is linear, so the order can be swapped. Here the body runs once per track on the
//! sum of its strings: the same sound for a fraction of the cost, and no recording needed.
//!
//! ```text
//!           ┌─► low cut ─► radiation (broad bells and a shelf) ────────────┐
//!  strings ─┤                                                              ├─► mix ─► out
//!           └─► [ mode 1 ‖ mode 2 ‖ … ‖ mode 16 ] (narrow band-passes) ─► ring ┘
//! ```
//!
//! - **Radiation:** what the body does to every note, broadly: the air (Helmholtz) and the
//!   top's first mode make a full low end around 100-200 Hz, the box a dip in the boxy
//!   mids, the top's radiation a presence (steel) or a softer top end (nylon).
//! - **Modes:** sixteen resonances side by side, each a narrow band-pass that rings on
//!   for tens of milliseconds after the strings' energy passes it: the "box" a real body
//!   adds, which a broad curve alone cannot. The frequencies are typical measurements of
//!   the two body styles, rounded (the air near 100 Hz, the top near 190 Hz with the back
//!   a little above it, the cross and long dipoles near 400 and 550 Hz, and the plate's
//!   higher modes), their strength falling with frequency.
//!
//! The first version (2026-11-11) was eight narrow bells in series: each lifted only a
//! few hertz, so a strummed chord changed by under 2 dB in any octave and the owner heard
//! no difference between the bodies.
//!
//! Size scales every frequency (a smaller body sounds higher, a jumbo lower).

use crate::filters::Biquad;

/// How many broad radiation filters and ringing modes a body has.
const RADIATION: usize = 8;
const MODES: usize = 16;
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

/// One broad radiation filter: a bell (frequency, quality, gain) or a high shelf.
#[derive(Debug, Clone, Copy)]
enum Shape {
    Bell(f32, f32, f32),
    HighShelf(f32, f32),
}

/// A body's tables: its low cut (below the air resonance a body radiates little), the
/// radiation curve, the modes (frequency, quality, and how far each stands out of the
/// curve around it, dB), how strongly they ring, and the trim that keeps the body about as
/// loud as the strings alone.
struct Tables {
    low_cut: f32,
    radiation: [Shape; RADIATION],
    modes: [(f32, f32, f32); MODES],
    ring: f32,
    trim_db: f32,
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

    fn tables(self) -> Tables {
        match self {
            // Fitted to the owner's reference, an impulse response of a Furch Vintage 1
            // dreadnought (measured 2026-11-12, only these figures taken from it): the air
            // near 95 Hz and the top and back around 200-280 Hz up, a deep scoop at
            // 400 Hz, the mids well down, a notch near 2 kHz, a strong presence from 3 to
            // 9 kHz and little above 10 kHz; its narrow resonances, each standing a few dB
            // out of the curve, ring for only a few milliseconds.
            BodyKind::None | BodyKind::Dreadnought => Tables {
                low_cut: 28.0,
                radiation: [
                    Shape::Bell(95.0, 3.5, 13.0),
                    Shape::Bell(238.0, 1.8, 11.0),
                    Shape::Bell(400.0, 2.8, -17.0),
                    Shape::Bell(900.0, 0.9, -11.0),
                    Shape::Bell(2_050.0, 3.0, -13.0),
                    Shape::Bell(4_100.0, 2.0, 12.0),
                    Shape::Bell(7_500.0, 1.3, 9.0),
                    Shape::HighShelf(9_500.0, -8.0),
                ],
                modes: [
                    (479.0, 9.0, 6.0),
                    (554.0, 7.0, 3.0),
                    (806.0, 5.0, 4.0),
                    (879.0, 11.0, 6.0),
                    (1_208.0, 12.0, 2.5),
                    (1_356.0, 17.0, 7.0),
                    (1_660.0, 17.0, 8.0),
                    (1_759.0, 7.0, 8.0),
                    (2_635.0, 8.0, 5.0),
                    (2_792.0, 9.0, 2.0),
                    (3_320.0, 17.0, 6.0),
                    (4_064.0, 12.0, 4.0),
                    (4_833.0, 17.0, 6.0),
                    (6_451.0, 7.0, 2.0),
                    (7_671.0, 7.0, 2.6),
                    (8_863.0, 12.0, 6.0),
                ],
                ring: 1.0,
                trim_db: -4.0,
            },
            BodyKind::Classical => Tables {
                low_cut: 62.0,
                radiation: [
                    Shape::Bell(100.0, 1.0, 6.0),
                    Shape::Bell(200.0, 1.3, 4.5),
                    Shape::Bell(600.0, 1.0, -4.0),
                    Shape::Bell(1_800.0, 1.0, 1.0),
                    Shape::Bell(1_000.0, 1.0, 0.0),
                    Shape::Bell(1_000.0, 1.0, 0.0),
                    Shape::Bell(1_000.0, 1.0, 0.0),
                    Shape::HighShelf(4_000.0, -4.0),
                ],
                modes: [
                    (96.0, 18.0, 8.9),
                    (188.0, 24.0, 8.9),
                    (240.0, 22.0, 7.1),
                    (300.0, 26.0, 6.1),
                    (410.0, 28.0, 6.1),
                    (480.0, 28.0, 5.1),
                    (600.0, 30.0, 5.1),
                    (700.0, 30.0, 4.3),
                    (800.0, 32.0, 3.9),
                    (950.0, 32.0, 3.6),
                    (1_100.0, 34.0, 3.2),
                    (1_300.0, 34.0, 2.9),
                    (1_500.0, 36.0, 2.4),
                    (1_800.0, 36.0, 2.0),
                    (2_100.0, 38.0, 1.6),
                    (2_500.0, 38.0, 1.3),
                ],
                ring: 1.0,
                trim_db: -4.0,
            },
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
    radiation: [Biquad; RADIATION],
    modes: [Biquad; MODES],
    /// Each mode's gain, the ring level included.
    gains: [f32; MODES],
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
            radiation: [Biquad::default(); RADIATION],
            modes: [Biquad::default(); MODES],
            gains: [0.0; MODES],
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
        let tables = kind.tables();
        let rate = self.rate;
        let nyquist = rate * 0.45;
        self.low_cut = self
            .low_cut
            .retuned(Biquad::highpass(rate, tables.low_cut * scale, 0.707));
        for (filter, shape) in self.radiation.iter_mut().zip(tables.radiation) {
            let design = match shape {
                Shape::Bell(frequency, q, gain) => {
                    Biquad::peaking(rate, (frequency * scale).min(nyquist), q, gain)
                }
                // The top's high end does not move with the body's size.
                Shape::HighShelf(frequency, gain) => {
                    Biquad::shelf(rate, frequency.min(nyquist), gain, true)
                }
            };
            *filter = filter.retuned(design);
        }
        for ((filter, gain), (frequency, q, db)) in
            self.modes.iter_mut().zip(&mut self.gains).zip(tables.modes)
        {
            *filter = filter.retuned(Biquad::bandpass(rate, (frequency * scale).min(nyquist), q));
            // A mode rung from the curve adds to it in phase at its centre: standing `db`
            // out of it takes `10^(db/20) - 1` of it.
            *gain = tables.ring * (10.0_f32.powf(db / 20.0) - 1.0).max(0.0);
        }
        self.trim = 10.0_f32.powf(tables.trim_db / 20.0);
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
            let cut = self.low_cut.process(dry);
            let radiated = self
                .radiation
                .iter_mut()
                .fold(cut, |value, filter| filter.process(value));
            let ring: f32 = self
                .modes
                .iter_mut()
                .zip(&self.gains)
                .map(|(mode, gain)| mode.process(radiated) * gain)
                .sum();
            let wet = (radiated + ring) * self.trim;
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
        for filter in self.radiation.iter_mut().chain(self.modes.iter_mut()) {
            filter.reset();
        }
        self.quiet = usize::MAX;
    }
}
