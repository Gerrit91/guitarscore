//! A stereo room reverb after Jezar's Freeverb (public domain): eight damped comb filters
//! and four allpasses per side, the right side's delays a little longer for width. The
//! delay lengths are scaled from 44.1 kHz to the engine's rate; nothing allocates after
//! [`Reverb::new`].

/// Comb and allpass lengths at 44.1 kHz, and how much longer the right side's are.
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;
/// Freeverb's scalings.
const INPUT_GAIN: f32 = 0.015;
const WET_SCALE: f32 = 3.0;
const ROOM_SCALE: f32 = 0.28;
const ROOM_OFFSET: f32 = 0.7;
const DAMP_SCALE: f32 = 0.4;

#[derive(Debug, Clone)]
struct Comb {
    buffer: Vec<f32>,
    at: usize,
    store: f32,
}

impl Comb {
    fn new(length: usize) -> Self {
        Comb {
            buffer: vec![0.0; length.max(1)],
            at: 0,
            store: 0.0,
        }
    }

    fn process(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let Some(slot) = self.buffer.get_mut(self.at) else {
            return 0.0;
        };
        let output = *slot;
        self.store = output * (1.0 - damp) + self.store * damp;
        *slot = input + self.store * feedback;
        self.at = self.at.saturating_add(1);
        if self.at >= self.buffer.len() {
            self.at = 0;
        }
        output
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.store = 0.0;
    }
}

#[derive(Debug, Clone)]
struct Allpass {
    buffer: Vec<f32>,
    at: usize,
}

impl Allpass {
    fn new(length: usize) -> Self {
        Allpass {
            buffer: vec![0.0; length.max(1)],
            at: 0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let Some(slot) = self.buffer.get_mut(self.at) else {
            return input;
        };
        let delayed = *slot;
        *slot = input + delayed * 0.5;
        self.at = self.at.saturating_add(1);
        if self.at >= self.buffer.len() {
            self.at = 0;
        }
        delayed - input
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
    }
}

/// The reverb of one track.
#[derive(Debug, Clone)]
pub(crate) struct Reverb {
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<Allpass>; 2],
}

impl Reverb {
    pub(crate) fn new(sample_rate: u32) -> Self {
        let scale = sample_rate as f32 / 44_100.0;
        let length = |base: usize, extra: usize| {
            ((base.saturating_add(extra)) as f32 * scale).round() as usize
        };
        let side = |extra: usize| {
            (
                COMBS.iter().map(|&c| Comb::new(length(c, extra))).collect(),
                ALLPASSES
                    .iter()
                    .map(|&a| Allpass::new(length(a, extra)))
                    .collect(),
            )
        };
        let (left_combs, left_allpasses) = side(0);
        let (right_combs, right_allpasses) = side(SPREAD);
        Reverb {
            combs: [left_combs, right_combs],
            allpasses: [left_allpasses, right_allpasses],
        }
    }

    /// The wet signal for one input sample pair: `size` and `damping` from 0 to 1.
    pub(crate) fn process(&mut self, left: f32, right: f32, size: f32, damping: f32) -> (f32, f32) {
        let input = (left + right) * INPUT_GAIN;
        let feedback = size.clamp(0.0, 1.0) * ROOM_SCALE + ROOM_OFFSET;
        let damp = damping.clamp(0.0, 1.0) * DAMP_SCALE;
        let mut out = [0.0_f32; 2];
        for (side, sum) in out.iter_mut().enumerate() {
            let (Some(combs), Some(allpasses)) =
                (self.combs.get_mut(side), self.allpasses.get_mut(side))
            else {
                continue;
            };
            let mut value: f32 = combs
                .iter_mut()
                .map(|comb| comb.process(input, feedback, damp))
                .sum();
            for allpass in allpasses.iter_mut() {
                value = allpass.process(value);
            }
            *sum = value * WET_SCALE;
        }
        (out[0], out[1])
    }

    pub(crate) fn clear(&mut self) {
        for comb in self.combs.iter_mut().flatten() {
            comb.clear();
        }
        for allpass in self.allpasses.iter_mut().flatten() {
            allpass.clear();
        }
    }
}
