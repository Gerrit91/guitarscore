//! A 4-band parametric EQ block: a low cut, four bands (a low shelf, two bells and a high
//! shelf), and a high cut. The workhorse of a mix - a drum's body, a guitar's fizz, a
//! vocal's mud - and what the drum part tones are built from (`docs/handoff-drums.md`).
//!
//! ```text
//!  in ─► low cut ─► band 1 ─► band 2 ─► band 3 ─► band 4 ─► high cut ─► out
//!                   (shelf)    (bell)    (bell)    (shelf)
//! ```
//!
//! Each band has its own frequency and gain; the two bells have a width (`q`) as well, and
//! the shelves use the gentle fixed slope of [`Biquad::shelf`]. Each side has its own
//! filters; the knobs are one set for both. Nothing allocates, and the filters flush
//! denormals (`filters::flushed`).

use crate::filters::Biquad;

/// The bands of the EQ.
pub const BANDS: usize = 4;

/// The knobs of an EQ block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqKnobs {
    /// The low cut and the high cut, Hz (outside them everything goes).
    pub low_cut: f32,
    pub high_cut: f32,
    /// The four bands' centre (or shelf corner) frequencies, Hz.
    pub band_freq: [f32; BANDS],
    /// Their gains, dB.
    pub band_gain: [f32; BANDS],
    /// Their widths (`q`): the bells' width, the shelves' slope (1 is gentle).
    pub band_q: [f32; BANDS],
    /// Each band's shape. Band 1 is a low shelf or a bell, band 4 a high shelf or a bell,
    /// and the middle two are always bells ([`BandShape::of`] is each band's default).
    pub band_shape: [BandShape; BANDS],
}

impl Default for EqKnobs {
    fn default() -> Self {
        EqKnobs {
            low_cut: 30.0,
            high_cut: 18_000.0,
            band_freq: [120.0, 400.0, 2_500.0, 6_000.0],
            band_gain: [0.0; BANDS],
            band_q: [0.707, 1.0, 1.0, 0.707],
            band_shape: std::array::from_fn(BandShape::of),
        }
    }
}

/// What a band is: the first a low shelf, the last a high shelf, the middle two bells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandShape {
    LowShelf,
    Bell,
    HighShelf,
}

impl BandShape {
    /// The default shape of band `band` (0 to [`BANDS`] - 1): band 1 a low shelf, band 4 a
    /// high shelf, the middle two bells. The outer bands can be switched to bells with
    /// [`EqKnobs::band_shape`].
    pub fn of(band: usize) -> BandShape {
        if band == 0 {
            BandShape::LowShelf
        } else if band.saturating_add(1) == BANDS {
            BandShape::HighShelf
        } else {
            BandShape::Bell
        }
    }

    /// Its name, for the window.
    pub fn label(self) -> &'static str {
        match self {
            BandShape::LowShelf => "Low shelf",
            BandShape::Bell => "Bell",
            BandShape::HighShelf => "High shelf",
        }
    }
}

/// The filter of band `band`, from `knobs`.
fn band_filter(rate: f32, band: usize, knobs: &EqKnobs) -> Biquad {
    let (freq, gain, q) = (
        knobs.band_freq.get(band).copied().unwrap_or(1_000.0),
        knobs.band_gain.get(band).copied().unwrap_or(0.0),
        knobs.band_q.get(band).copied().unwrap_or(1.0),
    );
    match knobs.shape(band) {
        BandShape::LowShelf => Biquad::shelf(rate, freq, gain, false),
        BandShape::HighShelf => Biquad::shelf(rate, freq, gain, true),
        BandShape::Bell => Biquad::peaking(rate, freq, q, gain),
    }
}

/// A shape a band may actually take: the first band a low shelf or a bell, the last a high
/// shelf or a bell, the middle two always bells. A hand-edited file may hold any of them.
fn fitted_shape(band: usize, shape: BandShape) -> BandShape {
    if band == 0 {
        match shape {
            BandShape::Bell => BandShape::Bell,
            _ => BandShape::LowShelf,
        }
    } else if band.saturating_add(1) == BANDS {
        match shape {
            BandShape::Bell => BandShape::Bell,
            _ => BandShape::HighShelf,
        }
    } else {
        BandShape::Bell
    }
}

/// An EQ's response at any frequency, without its audio state: each band's gain and the
/// sum of them all. The Sound chain window draws its curve from this; the filters
/// themselves use the same coefficients, so the picture is what is heard.
#[derive(Debug, Clone, Copy)]
pub struct EqResponse {
    rate: f32,
    low_cut: Biquad,
    high_cut: Biquad,
    bands: [Biquad; BANDS],
}

impl EqResponse {
    /// The response of `knobs` for `sample_rate`. Allocates nothing.
    pub fn new(sample_rate: f32, knobs: EqKnobs) -> Self {
        let rate = sample_rate.max(1.0);
        let knobs = knobs.sanitized();
        EqResponse {
            rate,
            low_cut: Biquad::highpass(rate, knobs.low_cut, 0.707),
            high_cut: Biquad::lowpass(rate, knobs.high_cut, 0.707),
            bands: std::array::from_fn(|band| band_filter(rate, band, &knobs)),
        }
    }

    /// The gain at `frequency` Hz of one filter, in dB (clamped to something printable).
    fn db(filter: Biquad, rate: f32, frequency: f32) -> f32 {
        let frequency = frequency.clamp(1.0, rate * 0.499);
        let gain = filter.gain_at(rate, frequency);
        if gain.is_finite() && gain > 1e-12 {
            20.0 * gain.log10()
        } else {
            -240.0
        }
    }

    /// The low cut's own gain at `frequency` Hz, in dB.
    pub fn low_cut_db(&self, frequency: f32) -> f32 {
        Self::db(self.low_cut, self.rate, frequency)
    }

    /// The high cut's own gain at `frequency` Hz, in dB.
    pub fn high_cut_db(&self, frequency: f32) -> f32 {
        Self::db(self.high_cut, self.rate, frequency)
    }

    /// Band `band`'s own gain at `frequency` Hz, in dB (0 for a band out of range).
    pub fn band_db(&self, band: usize, frequency: f32) -> f32 {
        self.bands
            .get(band)
            .copied()
            .map_or(0.0, |filter| Self::db(filter, self.rate, frequency))
    }

    /// Every band, both cuts and their sum at `frequency` Hz, in dB.
    pub fn total_db(&self, frequency: f32) -> f32 {
        let frequency = frequency.clamp(1.0, self.rate * 0.499);
        let mut gain = self.low_cut.gain_at(self.rate, frequency);
        gain *= self.high_cut.gain_at(self.rate, frequency);
        for band in self.bands {
            gain *= band.gain_at(self.rate, frequency);
        }
        if gain.is_finite() && gain > 1e-12 {
            20.0 * gain.log10()
        } else {
            -240.0
        }
    }
}

/// One side of the EQ.
#[derive(Debug, Clone)]
pub(crate) struct Eq {
    rate: f32,
    knobs: EqKnobs,
    low_cut: Biquad,
    high_cut: Biquad,
    bands: [Biquad; BANDS],
}

impl Eq {
    /// An EQ for `sample_rate` with `knobs`. Allocates nothing.
    pub(crate) fn new(sample_rate: f32, knobs: EqKnobs) -> Self {
        let mut eq = Eq {
            rate: sample_rate.max(1.0),
            knobs: knobs.sanitized(),
            low_cut: Biquad::default(),
            high_cut: Biquad::default(),
            bands: [Biquad::default(); BANDS],
        };
        eq.set_knobs(knobs);
        eq
    }

    /// New knob values, keeping what it is playing (no allocation).
    pub(crate) fn set_knobs(&mut self, knobs: EqKnobs) {
        let knobs = knobs.sanitized();
        let rate = self.rate;
        self.low_cut = self
            .low_cut
            .retuned(Biquad::highpass(rate, knobs.low_cut, 0.707));
        self.high_cut = self
            .high_cut
            .retuned(Biquad::lowpass(rate, knobs.high_cut, 0.707));
        for (band, filter) in self.bands.iter_mut().enumerate() {
            *filter = filter.retuned(band_filter(rate, band, &knobs));
        }
        self.knobs = knobs;
    }

    /// Silence in its filters.
    pub(crate) fn reset(&mut self) {
        self.low_cut.reset();
        self.high_cut.reset();
        for band in &mut self.bands {
            band.reset();
        }
    }

    /// One sample through the EQ.
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let x = self.low_cut.process(x);
        let mut x = x;
        for band in &mut self.bands {
            x = band.process(x);
        }
        self.high_cut.process(x)
    }
}

impl EqKnobs {
    /// The shape of `band`: what was chosen, fitted to what that band may be.
    pub fn shape(&self, band: usize) -> BandShape {
        let shape = self
            .band_shape
            .get(band)
            .copied()
            .unwrap_or(BandShape::of(band));
        fitted_shape(band, shape)
    }

    /// Every value in its range (a hand-edited settings file can hold anything).
    pub fn sanitized(self) -> Self {
        let fit = |value: f32, (low, high): (f32, f32), fallback: f32| {
            if value.is_finite() {
                value.clamp(low, high)
            } else {
                fallback
            }
        };
        let default = EqKnobs::default();
        let mut band_freq = default.band_freq;
        let mut band_gain = default.band_gain;
        let mut band_q = default.band_q;
        for (band, ((freq, gain), q)) in band_freq
            .iter_mut()
            .zip(band_gain.iter_mut())
            .zip(band_q.iter_mut())
            .enumerate()
        {
            let (default_freq, default_q) = (
                default.band_freq.get(band).copied().unwrap_or(1_000.0),
                default.band_q.get(band).copied().unwrap_or(1.0),
            );
            *freq = fit(
                self.band_freq.get(band).copied().unwrap_or(default_freq),
                crate::knob_ranges::BAND_FREQ,
                default_freq,
            );
            *gain = fit(
                self.band_gain.get(band).copied().unwrap_or(0.0),
                crate::knob_ranges::EQ_GAIN,
                0.0,
            );
            *q = fit(
                self.band_q.get(band).copied().unwrap_or(default_q),
                crate::knob_ranges::BAND_Q,
                default_q,
            );
        }
        let band_shape = std::array::from_fn(|band| {
            fitted_shape(
                band,
                self.band_shape
                    .get(band)
                    .copied()
                    .unwrap_or_else(|| BandShape::of(band)),
            )
        });
        EqKnobs {
            low_cut: fit(self.low_cut, crate::knob_ranges::LOW_CUT, default.low_cut),
            high_cut: fit(
                self.high_cut,
                crate::knob_ranges::HIGH_CUT,
                default.high_cut,
            ),
            band_freq,
            band_gain,
            band_q,
            band_shape,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

    use std::f32::consts::TAU;

    use super::*;

    const RATE: f32 = 48_000.0;

    /// The gain the EQ gives a sine of `hz` (dB), measured after the filters settle.
    fn gain_db(knobs: EqKnobs, hz: f32) -> f32 {
        let mut eq = Eq::new(RATE, knobs);
        let frames = 48_000;
        let mut peak = 0.0_f32;
        for i in 0..frames {
            let x = (TAU * hz * i as f32 / RATE).sin();
            let y = eq.process(x);
            if i > frames / 2 {
                peak = peak.max(y.abs());
            }
        }
        20.0 * peak.max(1e-12).log10()
    }

    #[test]
    fn every_band_boosts_and_cuts_where_it_sits() {
        let flat = EqKnobs {
            low_cut: 20.0,
            high_cut: 20_000.0,
            ..EqKnobs::default()
        };
        for band in 0..BANDS {
            let mut knobs = flat;
            knobs.band_gain[band] = 12.0;
            // A shelf gives half its gain at its corner, so it is measured well inside it.
            let at = match BandShape::of(band) {
                BandShape::LowShelf => knobs.band_freq[band] / 4.0,
                BandShape::HighShelf => knobs.band_freq[band] * 2.0,
                BandShape::Bell => knobs.band_freq[band],
            };
            assert!(
                gain_db(knobs, at) > 8.0,
                "band {band} at {at} Hz: {}",
                gain_db(knobs, at)
            );
            knobs.band_gain[band] = -12.0;
            assert!(
                gain_db(knobs, at) < -8.0,
                "band {band} at {at} Hz cut: {}",
                gain_db(knobs, at)
            );
        }
    }

    #[test]
    fn a_bells_width_decides_how_wide_it_reaches() {
        // A narrow bell at 1 kHz leaves 2 kHz alone; a wide one still lifts it.
        let narrow = EqKnobs {
            low_cut: 20.0,
            high_cut: 20_000.0,
            band_freq: [100.0, 1_000.0, 4_000.0, 10_000.0],
            band_gain: [0.0, 12.0, 0.0, 0.0],
            band_q: [0.707, 6.0, 1.0, 0.707],
            ..EqKnobs::default()
        };
        let wide = EqKnobs {
            band_q: [0.707, 0.4, 1.0, 0.707],
            ..narrow
        };
        let (narrow_at_2k, wide_at_2k) = (gain_db(narrow, 2_000.0), gain_db(wide, 2_000.0));
        assert!(
            wide_at_2k > narrow_at_2k + 3.0,
            "{wide_at_2k} against {narrow_at_2k}"
        );
    }

    #[test]
    fn the_cuts_and_shelves_do_what_they_say() {
        // A low cut at 300 Hz takes a 60 Hz sine down hard.
        let cut = EqKnobs {
            low_cut: 300.0,
            ..EqKnobs::default()
        };
        assert!(gain_db(cut, 60.0) < -12.0, "{}", gain_db(cut, 60.0));
        // The low shelf (band 1 at 120 Hz): 12 dB up below, little above.
        let shelf = EqKnobs {
            low_cut: 20.0,
            high_cut: 20_000.0,
            band_gain: [12.0, 0.0, 0.0, 0.0],
            ..EqKnobs::default()
        };
        assert!(gain_db(shelf, 40.0) > 8.0, "{}", gain_db(shelf, 40.0));
        assert!((gain_db(shelf, 1_000.0)).abs() < 1.5);
        // The high shelf (band 4 at 6 kHz) lifts the highs only.
        let highs = EqKnobs {
            low_cut: 20.0,
            high_cut: 20_000.0,
            band_gain: [0.0, 0.0, 0.0, 12.0],
            ..EqKnobs::default()
        };
        assert!(
            gain_db(highs, 12_000.0) > 8.0,
            "{}",
            gain_db(highs, 12_000.0)
        );
        assert!((gain_db(highs, 500.0)).abs() < 1.5);
        // A high cut at 2 kHz takes the highs away.
        let dark = EqKnobs {
            low_cut: 20.0,
            high_cut: 2_000.0,
            ..EqKnobs::default()
        };
        assert!(gain_db(dark, 8_000.0) < -12.0, "{}", gain_db(dark, 8_000.0));
    }

    #[test]
    fn the_drawn_response_is_the_filters_gain() {
        let knobs = EqKnobs {
            low_cut: 80.0,
            high_cut: 12_000.0,
            band_freq: [100.0, 500.0, 2_500.0, 9_000.0],
            band_gain: [6.0, -4.0, 3.0, -2.0],
            band_q: [0.707, 2.0, 1.0, 0.707],
            ..EqKnobs::default()
        };
        let eq = Eq::new(RATE, knobs);
        let response = EqResponse::new(RATE, knobs);
        for hz in [
            30.0, 60.0, 100.0, 250.0, 500.0, 1_000.0, 2_500.0, 5_000.0, 9_000.0, 15_000.0, 19_000.0,
        ] {
            let expected = eq.low_cut.gain_at(RATE, hz)
                * eq.high_cut.gain_at(RATE, hz)
                * eq.bands
                    .iter()
                    .map(|filter| filter.gain_at(RATE, hz))
                    .product::<f32>();
            let expected_db = 20.0 * expected.log10();
            assert!(
                (response.total_db(hz) - expected_db).abs() < 0.01,
                "{hz} Hz: {} against {expected_db}",
                response.total_db(hz)
            );
            // And each band's own curve is that filter alone.
            for band in 0..BANDS {
                let own = 20.0 * eq.bands[band].gain_at(RATE, hz).log10();
                assert!(
                    (response.band_db(band, hz) - own).abs() < 0.01,
                    "band {band} at {hz} Hz"
                );
            }
        }
        // The same gain a rendered sine gets (a bell's centre, well settled).
        let measured = gain_db(
            EqKnobs {
                band_gain: [0.0, 12.0, 0.0, 0.0],
                ..knobs
            },
            500.0,
        );
        let drawn = EqResponse::new(
            RATE,
            EqKnobs {
                band_gain: [0.0, 12.0, 0.0, 0.0],
                ..knobs
            },
        )
        .total_db(500.0);
        assert!((measured - drawn).abs() < 0.5, "{measured} against {drawn}");
    }

    #[test]
    fn the_outer_bands_switch_between_shelf_and_bell() {
        let flat = EqKnobs {
            low_cut: 20.0,
            high_cut: 20_000.0,
            ..EqKnobs::default()
        };
        let defaults = EqKnobs::default();
        for band in 0..BANDS {
            assert_eq!(defaults.shape(band), BandShape::of(band));
        }
        // Band 1 as a shelf lifts everything below its corner; as a bell it lifts around
        // the corner and leaves 30 Hz mostly alone.
        let shelf = EqKnobs {
            band_gain: [12.0, 0.0, 0.0, 0.0],
            ..flat
        };
        let bell = EqKnobs {
            band_shape: [
                BandShape::Bell,
                BandShape::Bell,
                BandShape::Bell,
                BandShape::HighShelf,
            ],
            ..shelf
        };
        assert!(gain_db(shelf, 30.0) > 8.0, "{}", gain_db(shelf, 30.0));
        assert!(gain_db(bell, 30.0) < 4.0, "{}", gain_db(bell, 30.0));
        assert!(gain_db(bell, 120.0) > 8.0, "{}", gain_db(bell, 120.0));
        // Band 4 the same the other way: a bell at 6 kHz is not a shelf above it.
        let highs = EqKnobs {
            band_gain: [0.0, 0.0, 0.0, 12.0],
            ..flat
        };
        let high_bell = EqKnobs {
            band_shape: [
                BandShape::LowShelf,
                BandShape::Bell,
                BandShape::Bell,
                BandShape::Bell,
            ],
            ..highs
        };
        assert!(
            gain_db(highs, 12_000.0) > 8.0,
            "{}",
            gain_db(highs, 12_000.0)
        );
        assert!(
            gain_db(high_bell, 12_000.0) < gain_db(highs, 12_000.0) - 3.0,
            "{} against {}",
            gain_db(high_bell, 12_000.0),
            gain_db(highs, 12_000.0)
        );
        assert!(
            gain_db(high_bell, 6_000.0) > 8.0,
            "{}",
            gain_db(high_bell, 6_000.0)
        );
        // The middle bands stay bells, whatever a hand-edited file says.
        assert_eq!(
            EqKnobs {
                band_shape: [
                    BandShape::LowShelf,
                    BandShape::LowShelf,
                    BandShape::HighShelf,
                    BandShape::HighShelf
                ],
                ..flat
            }
            .sanitized()
            .shape(1),
            BandShape::Bell
        );
    }

    #[test]
    fn a_flat_eq_leaves_the_signal_alone_and_never_goes_nan() {
        let flat = EqKnobs {
            low_cut: 20.0,
            high_cut: 20_000.0,
            ..EqKnobs::default()
        };
        assert!((gain_db(flat, 1_000.0)).abs() < 0.2);
        // A broken settings file: every value fitted into its range.
        let broken = EqKnobs {
            low_cut: f32::NAN,
            high_cut: 1e9,
            band_freq: [f32::NAN, -5.0, 1e9, f32::INFINITY],
            band_gain: [1e9, f32::NAN, -1e9, 0.0],
            band_q: [0.0, f32::NAN, 1e9, -1.0],
            band_shape: [
                BandShape::HighShelf,
                BandShape::LowShelf,
                BandShape::HighShelf,
                BandShape::LowShelf,
            ],
        }
        .sanitized();
        assert!(broken.low_cut.is_finite() && broken.high_cut.is_finite());
        for band in 0..BANDS {
            assert!(broken.band_freq[band].is_finite());
            assert!(broken.band_gain[band].is_finite());
            assert!(broken.band_q[band] > 0.0);
        }
        // Only the outer bands may be shelves, each only its own kind.
        assert_eq!(broken.shape(0), BandShape::LowShelf);
        assert_eq!(broken.shape(1), BandShape::Bell);
        assert_eq!(broken.shape(2), BandShape::Bell);
        assert_eq!(broken.shape(3), BandShape::HighShelf);
        let mut eq = Eq::new(RATE, broken);
        for _ in 0..10_000 {
            let y = eq.process(0.5);
            assert!(y.is_finite(), "{y}");
        }
        eq.reset();
        assert_eq!(eq.process(0.0), 0.0);
    }
}
