//! The string's delay line: a ring buffer read at a fractional delay with third-order
//! Lagrange interpolation, so the pitch is exact and can glide (bends, slides, vibrato)
//! without steps.

/// A ring buffer of samples. Its length is a power of two, fixed when it is made.
#[derive(Debug, Clone)]
pub(crate) struct DelayLine {
    buffer: Vec<f32>,
    mask: usize,
    /// Where the next sample goes.
    write: usize,
}

/// The shortest delay [`DelayLine::read`] handles: the interpolator needs one sample after
/// the read position and two before it.
pub(crate) const MIN_DELAY: f32 = 2.0;

impl DelayLine {
    /// A line holding at least `samples` of history (plus room for the interpolator).
    pub(crate) fn new(samples: usize) -> Self {
        let length = samples.saturating_add(8).next_power_of_two();
        DelayLine {
            buffer: crate::convolver::touched(length, 0.0, 1.0),
            mask: length.saturating_sub(1),
            write: 0,
        }
    }

    /// The longest delay that can be read.
    pub(crate) fn max_delay(&self) -> f32 {
        self.buffer.len().saturating_sub(4) as f32
    }

    /// Appends `sample` as the newest.
    pub(crate) fn push(&mut self, sample: f32) {
        if let Some(slot) = self.buffer.get_mut(self.write) {
            *slot = sample;
        }
        self.write = self.write.wrapping_add(1) & self.mask;
    }

    /// The sample `back` places before the next one to be written (1 is the newest).
    fn at(&self, back: usize) -> f32 {
        let index = self.write.wrapping_sub(back) & self.mask;
        self.buffer.get(index).copied().unwrap_or(0.0)
    }

    /// The signal `delay` samples before the next one to be written, interpolated
    /// (`delay` is clamped to [`MIN_DELAY`] and [`DelayLine::max_delay`]).
    pub(crate) fn read(&self, delay: f32) -> f32 {
        let delay = if delay.is_finite() {
            delay.clamp(MIN_DELAY, self.max_delay())
        } else {
            MIN_DELAY
        };
        let whole = delay.floor();
        let fraction = delay - whole;
        let back = whole as usize;
        // Four points around the read position; `t` is the position from the first.
        let p0 = self.at(back.saturating_sub(1));
        let p1 = self.at(back);
        let p2 = self.at(back.saturating_add(1));
        let p3 = self.at(back.saturating_add(2));
        let t = 1.0 + fraction;
        let h0 = -(t - 1.0) * (t - 2.0) * (t - 3.0) / 6.0;
        let h1 = t * (t - 2.0) * (t - 3.0) / 2.0;
        let h2 = -t * (t - 1.0) * (t - 3.0) / 2.0;
        let h3 = t * (t - 1.0) * (t - 2.0) / 6.0;
        h0 * p0 + h1 * p1 + h2 * p2 + h3 * p3
    }

    /// Changes the `count` samples to be read next (the newest `count`, oldest first) with
    /// `change(position, old)`: how an excitation is added to the vibrating string.
    pub(crate) fn reshape(&mut self, count: usize, mut change: impl FnMut(usize, f32) -> f32) {
        let count = count.min(self.buffer.len());
        for position in 0..count {
            let back = count.saturating_sub(position);
            let index = self.write.wrapping_sub(back) & self.mask;
            if let Some(sample) = self.buffer.get_mut(index) {
                *sample = change(position, *sample);
            }
        }
    }

    /// Silence throughout.
    pub(crate) fn clear(&mut self) {
        self.buffer.fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn whole_delays_read_exact_samples_and_fractions_interpolate() {
        let mut line = DelayLine::new(16);
        for value in 0..10 {
            line.push(value as f32);
        }
        // The newest is 9, one back.
        assert!((line.read(2.0) - 8.0).abs() < 1e-6);
        assert!((line.read(5.0) - 5.0).abs() < 1e-6);
        // A straight line is interpolated exactly.
        assert!((line.read(3.5) - 6.5).abs() < 1e-5);
        assert!(line.read(f32::NAN).is_finite());
    }
}
