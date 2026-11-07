//! Impulse responses read from WAV files: a guitar body, a room, later a cabinet. The file
//! is input from the user's disk, so it is read defensively: every length is checked, a
//! damaged file is an error and never a panic, and the response is capped at
//! [`MAX_SECONDS`].

/// The longest response kept, in seconds (a body is a fraction of a second, a room or a
/// hall a few seconds; longer ones are cut with a short fade).
pub const MAX_SECONDS: f32 = 6.0;
/// The fade at a cut end, in seconds.
const CUT_FADE: f32 = 0.05;

/// Why a file is not a usable impulse response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpulseError(pub String);

impl std::fmt::Display for ImpulseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ImpulseError {}

fn error(message: &str) -> ImpulseError {
    ImpulseError(message.to_owned())
}

/// An impulse response: one or two channels at their own rate, scaled to unit energy (so
/// a response leaves the level about where it was; the block's gain adjusts it). A
/// cabinet or a body is played mono; a room keeps its two channels.
#[derive(Debug, Clone, PartialEq)]
pub struct ImpulseResponse {
    channels: Vec<Vec<f32>>,
    sample_rate: u32,
}

/// The sample formats read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Pcm,
    Float,
}

impl ImpulseResponse {
    /// A mono response from samples at `sample_rate` (for tests and generated responses).
    pub fn from_samples(samples: &[f32], sample_rate: u32) -> Result<Self, ImpulseError> {
        ImpulseResponse::from_channels(vec![samples.to_vec()], sample_rate)
    }

    /// A response of one or two channels (more are cut to two) at `sample_rate`.
    pub fn from_channels(channels: Vec<Vec<f32>>, sample_rate: u32) -> Result<Self, ImpulseError> {
        ImpulseResponse::with_limit(channels, sample_rate, MAX_SECONDS)
    }

    /// Like [`ImpulseResponse::from_channels`] for a lengthened response, which may be
    /// longer than a file's.
    pub(crate) fn from_channels_long(
        channels: Vec<Vec<f32>>,
        sample_rate: u32,
    ) -> Result<Self, ImpulseError> {
        ImpulseResponse::with_limit(channels, sample_rate, crate::tail::MAX_LENGTHENED_SECONDS)
    }

    fn with_limit(
        channels: Vec<Vec<f32>>,
        sample_rate: u32,
        seconds: f32,
    ) -> Result<Self, ImpulseError> {
        if !(8_000..=384_000).contains(&sample_rate) {
            return Err(error("the sample rate is not usable"));
        }
        let keep = (seconds * sample_rate as f32) as usize;
        let fade = (CUT_FADE * sample_rate as f32) as usize;
        let mut channels: Vec<Vec<f32>> = channels
            .into_iter()
            .take(2)
            .map(|channel| {
                let mut samples: Vec<f32> = channel
                    .into_iter()
                    .take(keep)
                    .map(|x| if x.is_finite() { x } else { 0.0 })
                    .collect();
                if samples.len() == keep {
                    let fade = fade.min(samples.len());
                    let start = samples.len().saturating_sub(fade);
                    for (i, sample) in samples.iter_mut().skip(start).enumerate() {
                        *sample *= 1.0 - i as f32 / fade.max(1) as f32;
                    }
                }
                samples
            })
            .collect();
        let count = channels.len().max(1) as f32;
        let energy: f32 = channels.iter().flatten().map(|x| x * x).sum::<f32>() / count;
        if !(energy.is_finite() && energy > 1e-12) {
            return Err(error("the impulse response is silent"));
        }
        let scale = 1.0 / energy.sqrt();
        for sample in channels.iter_mut().flatten() {
            *sample *= scale;
        }
        Ok(ImpulseResponse {
            channels,
            sample_rate,
        })
    }

    /// Reads a WAV file: 16-, 24- or 32-bit PCM or 32- or 64-bit float, one or two
    /// channels kept (of more, the first two).
    pub fn from_wav(bytes: &[u8]) -> Result<Self, ImpulseError> {
        let not_wav = || error("not a WAV file");
        if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
            return Err(not_wav());
        }
        let mut at = 12_usize;
        let mut format: Option<(Format, u16, u32, u16)> = None;
        let mut data: Option<&[u8]> = None;
        while let Some(header) = bytes.get(at..at.saturating_add(8)) {
            let id = header.get(0..4).unwrap_or_default();
            let size = u32_at(header, 4).ok_or_else(not_wav)? as usize;
            let body_start = at.saturating_add(8);
            // A file cut short still gives what it has of its last chunk.
            let body_end = body_start.saturating_add(size).min(bytes.len());
            let body = bytes.get(body_start..body_end).unwrap_or_default();
            match id {
                b"fmt " => format = Some(read_format(body)?),
                b"data" => data = Some(body),
                _ => {}
            }
            // Chunks are padded to an even length.
            at = body_start.saturating_add(size).saturating_add(size & 1);
        }
        let (kind, channels, sample_rate, bits) =
            format.ok_or_else(|| error("the WAV file has no format chunk"))?;
        let data = data.ok_or_else(|| error("the WAV file has no sound data"))?;
        let width = usize::from(bits / 8);
        let frame = width.saturating_mul(usize::from(channels));
        if frame == 0 {
            return Err(error("the WAV file's format is damaged"));
        }
        let keep = (MAX_SECONDS * sample_rate as f32) as usize;
        let kept = usize::from(channels.min(2));
        let mut out: Vec<Vec<f32>> = vec![Vec::new(); kept];
        for frame in data.chunks_exact(frame).take(keep.saturating_add(1)) {
            for (channel, sample) in out.iter_mut().zip(frame.chunks_exact(width)) {
                channel.push(decode(sample, kind, bits));
            }
        }
        if out.first().is_none_or(Vec::is_empty) {
            return Err(error("the WAV file has no samples"));
        }
        ImpulseResponse::from_channels(out, sample_rate)
    }

    /// The response's own sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// How many channels it has (1 or 2).
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// How long it is, in seconds.
    pub fn seconds(&self) -> f32 {
        let length = self.channels.first().map_or(0, Vec::len);
        length as f32 / self.sample_rate.max(1) as f32
    }

    /// The response mixed to mono, at `rate`.
    pub fn mono_at_rate(&self, rate: u32) -> Vec<f32> {
        let count = self.channels.len().max(1) as f32;
        let mut mono = self.channel_at_rate(0, rate);
        for index in 1..self.channels.len() {
            for (sum, value) in mono.iter_mut().zip(self.channel_at_rate(index, rate)) {
                *sum += value;
            }
        }
        // Mixing correlated channels adds up; scale back toward the same energy.
        let scale = 1.0 / count.sqrt();
        for sample in &mut mono {
            *sample *= scale;
        }
        mono
    }

    /// Channel `index` (the last one past the end: a mono room plays the same on both
    /// sides) at `rate`, by linear interpolation.
    pub fn channel_at_rate(&self, index: usize, rate: u32) -> Vec<f32> {
        let Some(samples) = self.channels.get(index).or_else(|| self.channels.last()) else {
            return Vec::new();
        };
        if rate == self.sample_rate || rate == 0 {
            return samples.clone();
        }
        let step = self.sample_rate as f64 / rate as f64;
        let length = (samples.len() as f64 / step).floor() as usize;
        // Energy stays the same: more samples per second, each a little smaller.
        let scale = (step as f32).sqrt();
        (0..length)
            .map(|i| {
                let position = i as f64 * step;
                let at = position.floor() as usize;
                let fraction = (position - position.floor()) as f32;
                let a = samples.get(at).copied().unwrap_or(0.0);
                let b = samples.get(at.saturating_add(1)).copied().unwrap_or(0.0);
                (a + (b - a) * fraction) * scale
            })
            .collect()
    }
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let pair = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*pair.first()?, *pair.get(1)?]))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let four = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([
        *four.first()?,
        *four.get(1)?,
        *four.get(2)?,
        *four.get(3)?,
    ]))
}

/// The format chunk: sample format, channels, sample rate, bits per sample.
fn read_format(body: &[u8]) -> Result<(Format, u16, u32, u16), ImpulseError> {
    let damaged = || error("the WAV file's format is damaged");
    let tag = u16_at(body, 0).ok_or_else(damaged)?;
    let channels = u16_at(body, 2).ok_or_else(damaged)?;
    let sample_rate = u32_at(body, 4).ok_or_else(damaged)?;
    let bits = u16_at(body, 14).ok_or_else(damaged)?;
    // WAVE_FORMAT_EXTENSIBLE keeps the real format in the sub-format's first two bytes.
    let tag = if tag == 0xFFFE {
        u16_at(body, 24).ok_or_else(damaged)?
    } else {
        tag
    };
    let kind = match (tag, bits) {
        (1, 16 | 24 | 32) => Format::Pcm,
        (3, 32 | 64) => Format::Float,
        _ => {
            return Err(error(
                "the WAV file's sample format is not supported (use 16, 24 or 32-bit PCM or \
                 32-bit float)",
            ));
        }
    };
    if !(1..=8).contains(&channels) {
        return Err(error("the WAV file has an unusable number of channels"));
    }
    Ok((kind, channels, sample_rate, bits))
}

/// One sample, -1 to 1.
fn decode(bytes: &[u8], kind: Format, bits: u16) -> f32 {
    let byte = |i: usize| bytes.get(i).copied().unwrap_or(0);
    match (kind, bits) {
        (Format::Pcm, 16) => f32::from(i16::from_le_bytes([byte(0), byte(1)])) / 32_768.0,
        (Format::Pcm, 24) => {
            let value = i32::from_le_bytes([0, byte(0), byte(1), byte(2)]) >> 8;
            value as f32 / 8_388_608.0
        }
        (Format::Pcm, 32) => {
            i32::from_le_bytes([byte(0), byte(1), byte(2), byte(3)]) as f32 / 2_147_483_648.0
        }
        (Format::Float, 32) => f32::from_le_bytes([byte(0), byte(1), byte(2), byte(3)]),
        (Format::Float, 64) => f64::from_le_bytes([
            byte(0),
            byte(1),
            byte(2),
            byte(3),
            byte(4),
            byte(5),
            byte(6),
            byte(7),
        ]) as f32,
        _ => 0.0,
    }
}
