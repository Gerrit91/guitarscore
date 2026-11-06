//! Writing a sequence as a WAV file: the song rendered offline with a SoundFont, without an
//! audio device. Samples are written as 16-bit stereo PCM at 44.1 kHz.

use std::io::Write;

use gscore_guitar::{BLOCKS, BlockSettings, ChainSpec, StringModel};

use crate::chains::ChainSet;
use gscore_play::{PlayError, Sequence, StringKind};

use crate::synth::{BLOCK, SoundBank, SynthEngine};

/// The sample rate WAV export uses.
pub const WAV_SAMPLE_RATE: u32 = 44_100;
/// How long notes may ring out after the sequence's end.
const TAIL_SECONDS: u64 = 2;

/// What `write_wav` wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavInfo {
    /// The sample rate.
    pub sample_rate: u32,
    /// Frames (samples per channel).
    pub frames: u64,
}

/// Renders `sequence` with `bank` and writes it as a WAV file to `writer`. The file ends a
/// little after the last note, so releases are not cut off.
pub fn write_wav(
    writer: impl Write,
    sequence: &Sequence,
    bank: &SoundBank,
) -> Result<WavInfo, PlayError> {
    write_wav_at(writer, sequence, bank, WAV_SAMPLE_RATE)
}

/// Like [`write_wav`] at a chosen sample rate.
pub fn write_wav_at(
    writer: impl Write,
    sequence: &Sequence,
    bank: &SoundBank,
    sample_rate: u32,
) -> Result<WavInfo, PlayError> {
    write_wav_with(writer, sequence, bank, sample_rate, &[], &[], None)
}

/// An effect chain to render with: the kind of string, its blocks and their knobs.
pub type ChainChoice = (StringKind, ChainSpec, [BlockSettings; BLOCKS]);

/// The mixer's reverb bus to render with: an all-wet chain, its knobs and its level.
#[derive(Debug, Clone)]
pub struct ReverbBusChoice {
    pub spec: ChainSpec,
    pub settings: [BlockSettings; BLOCKS],
    /// 0-127, 127 unity.
    pub level: u8,
    pub muted: bool,
}

/// Like [`write_wav_at`], with the modelled strings of each listed kind sounding as its
/// model (the others keep their presets) through its effect chain (the others have none).
#[allow(clippy::too_many_arguments)]
pub fn write_wav_with(
    mut writer: impl Write,
    sequence: &Sequence,
    bank: &SoundBank,
    sample_rate: u32,
    models: &[(StringKind, StringModel)],
    chains: &[ChainChoice],
    bus: Option<&ReverbBusChoice>,
) -> Result<WavInfo, PlayError> {
    let frames = frames_for(sequence, sample_rate)?;
    let data_bytes = frames
        .checked_mul(4)
        .and_then(|bytes| u32::try_from(bytes).ok())
        .ok_or_else(|| PlayError::Device("the song is too long for a WAV file".to_owned()))?;
    write_header(&mut writer, sample_rate, data_bytes)?;

    let mut engine = SynthEngine::new(bank, sample_rate)?;
    for &(kind, model) in models {
        engine.set_string_model(kind, model);
    }
    for (kind, spec, settings) in chains {
        let tracks = crate::chains::tracks_of_kind(sequence, *kind);
        if tracks > 0 && !ChainSet::is_empty(spec) {
            let chain = ChainSet::new(spec, settings, sample_rate, tracks);
            let _ = engine.set_chain(*kind, Some(Box::new(chain)));
        }
    }
    if let Some(bus) = bus
        && !ChainSet::is_empty(&bus.spec)
    {
        let chain = bus.spec.prepare(sample_rate).instance(&bus.settings);
        engine.set_reverb_bus(Some(Box::new(chain)));
        engine.set_reverb_bus_settings(bus.settings, bus.level, bus.muted);
    }
    engine.load(std::sync::Arc::new(sequence.clone()));
    engine.play();
    let mut left = [0.0_f32; BLOCK];
    let mut right = [0.0_f32; BLOCK];
    let mut written = 0_u64;
    while written < frames {
        let count = usize::try_from((frames - written).min(BLOCK as u64)).unwrap_or(BLOCK);
        engine.render(
            left.get_mut(..count).unwrap_or_default(),
            right.get_mut(..count).unwrap_or_default(),
        );
        for index in 0..count {
            let l = left.get(index).copied().unwrap_or(0.0);
            let r = right.get(index).copied().unwrap_or(0.0);
            writer
                .write_all(&sample(l).to_le_bytes())
                .and_then(|()| writer.write_all(&sample(r).to_le_bytes()))
                .map_err(|e| PlayError::Device(format!("cannot write the WAV file: {e}")))?;
        }
        written = written.saturating_add(count as u64);
    }
    writer
        .flush()
        .map_err(|e| PlayError::Device(format!("cannot write the WAV file: {e}")))?;
    Ok(WavInfo {
        sample_rate,
        frames: written,
    })
}

/// How many frames the rendered song takes, tail included.
fn frames_for(sequence: &Sequence, sample_rate: u32) -> Result<u64, PlayError> {
    if !(16_000..=192_000).contains(&sample_rate) {
        return Err(PlayError::Device(format!(
            "sample rate {sample_rate} is not usable"
        )));
    }
    let micros = u128::from(sequence.micros());
    let body = micros
        .saturating_mul(u128::from(sample_rate))
        .checked_div(1_000_000)
        .unwrap_or(0);
    let tail = u128::from(sample_rate).saturating_mul(u128::from(TAIL_SECONDS));
    u64::try_from(body.saturating_add(tail))
        .map_err(|_| PlayError::Device("the song is too long for a WAV file".to_owned()))
}

/// The WAV (RIFF) header of 44 bytes.
fn write_header(
    writer: &mut impl Write,
    sample_rate: u32,
    data_bytes: u32,
) -> Result<(), PlayError> {
    let bad = |e: std::io::Error| PlayError::Device(format!("cannot write the WAV file: {e}"));
    writer.write_all(b"RIFF").map_err(bad)?;
    writer
        .write_all(&(36_u32.saturating_add(data_bytes)).to_le_bytes())
        .map_err(bad)?;
    writer.write_all(b"WAVEfmt ").map_err(bad)?;
    writer.write_all(&16_u32.to_le_bytes()).map_err(bad)?;
    writer.write_all(&1_u16.to_le_bytes()).map_err(bad)?; // PCM
    writer.write_all(&2_u16.to_le_bytes()).map_err(bad)?; // stereo
    writer.write_all(&sample_rate.to_le_bytes()).map_err(bad)?;
    writer
        .write_all(&sample_rate.saturating_mul(4).to_le_bytes())
        .map_err(bad)?;
    writer.write_all(&4_u16.to_le_bytes()).map_err(bad)?; // bytes per frame
    writer.write_all(&16_u16.to_le_bytes()).map_err(bad)?; // bits per sample
    writer.write_all(b"data").map_err(bad)?;
    writer.write_all(&data_bytes.to_le_bytes()).map_err(bad)
}

/// A clamped sample as 16-bit PCM; NaN and infinities are silence.
fn sample(value: f32) -> i16 {
    if value.is_finite() {
        (value.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::testing::sine_soundfont;

    #[test]
    fn a_rendered_song_is_a_readable_wav_with_the_expected_sizes() {
        use gscore_model::testing::{self, TestNote};
        use gscore_play::Sequence;

        let bytes = sine_soundfont("Sine");
        let bank = SoundBank::from_bytes(&bytes).unwrap();
        let song =
            testing::song_with_notes(&[(4, &[TestNote::new(1, 5)]), (4, &[]), (4, &[]), (4, &[])])
                .unwrap();
        let sequence = Sequence::new(&song, None).unwrap();
        let mut out = Vec::new();
        let info = write_wav_at(&mut out, &sequence, &bank, 16_000).unwrap();
        assert_eq!(info.sample_rate, 16_000);
        assert_eq!(&out[0..4], b"RIFF");
        assert_eq!(&out[8..12], b"WAVE");
        assert_eq!(&out[12..16], b"fmt ");
        assert_eq!(out.len(), 44 + usize::try_from(info.frames).unwrap() * 4);
        let chunk = u32::from_le_bytes(out[4..8].try_into().unwrap());
        assert_eq!(u64::from(chunk), out.len() as u64 - 8);
        let data = u32::from_le_bytes(out[40..44].try_into().unwrap());
        assert_eq!(u64::from(data), info.frames * 4);
        // Two seconds of a bar at 120 BPM, plus the two-second tail.
        assert_eq!(info.frames, 4 * 16_000);
        // Something was rendered (a sine SoundFont plays it).
        assert!(out[44..].iter().any(|&byte| byte != 0));
    }

    #[test]
    fn a_bad_sample_rate_is_an_error() {
        use gscore_model::testing::{self, GUITAR_6};
        use gscore_play::Sequence;

        let bytes = sine_soundfont("Sine");
        let bank = SoundBank::from_bytes(&bytes).unwrap();
        let song = testing::song(&[GUITAR_6], 1).unwrap();
        let sequence = Sequence::new(&song, None).unwrap();
        assert!(write_wav_at(Vec::new(), &sequence, &bank, 0).is_err());
    }
}
