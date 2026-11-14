//! Writing a sequence as a WAV file: the song rendered offline with a SoundFont, without an
//! audio device. Samples are written as 16-bit stereo PCM at 44.1 kHz.

use std::io::Write;

use gscore_guitar::{ChainSpec, StringModel};

use crate::chains::{ChainKnobs, prepare_chain};
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
    write_wav_with(
        writer,
        sequence,
        bank,
        sample_rate,
        &[],
        &[],
        &[],
        &[],
        &[],
        &BusChoices::default(),
    )
}

/// An effect chain to render with: the track (its index in the song), its blocks and their
/// knobs.
pub type ChainChoice = (usize, ChainSpec, ChainKnobs);

/// One part of a drum track's chain to render with: the track, the part, its mixing level
/// (0-127, 127 unity), its pan (0 left, 64 centre, 127 right), its room send (0-127), its
/// blocks and their knobs.
pub type PartChoice = (usize, usize, u8, u8, u8, ChainSpec, ChainKnobs);

/// A mixer bus to render with: its chain, its knob values and its level.
#[derive(Debug, Clone)]
pub struct BusChoice {
    pub spec: ChainSpec,
    pub knobs: ChainKnobs,
    /// 0-127, 127 unity.
    pub level: u8,
    pub muted: bool,
}

/// The mixer's buses, and each track's send into the compressor bus, to render with.
#[derive(Debug, Clone, Default)]
pub struct BusChoices {
    /// The reverb bus (an all-wet chain; the tracks' Reverb knobs are its sends).
    pub reverb: Option<BusChoice>,
    /// The compressor bus (usually a compressor driven hard, blended in under the dry
    /// tracks); each track's Comp send feeds it.
    pub compressor: Option<BusChoice>,
    /// The chorus bus: the tracks' Chorus values are its sends (they come with the
    /// sequence, so no list of sends is needed).
    pub chorus: Option<BusChoice>,
    /// The master's chain, after the tracks and buses.
    pub master: Option<BusChoice>,
    /// Each track's Comp send (its song index, 0-127).
    pub comp_sends: Vec<(usize, u8)>,
}

/// What a stem leaves in its file: the tracks' own sound, or only the mixer's buses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StemPart {
    /// The tracks' own sound (their chains, faders and pans); the caller passes the buses
    /// it wants with it (none for a plain track stem, all for the whole mix).
    Tracks,
    /// Only the buses' returns (the tracks' sends through them), without the dry tracks: a
    /// bus stem. The caller passes one bus's chain (and no master chain).
    Buses,
}

/// Like [`write_wav_with`], with the modelled strings of each listed kind sounding as its
/// model (the others keep their presets), each listed track with its own model, each listed
/// track through its effect chain (the others have none), each listed track's input trim
/// (in dB, before its chain), and the mixer's buses.
#[allow(clippy::too_many_arguments)]
pub fn write_wav_with(
    writer: impl Write,
    sequence: &Sequence,
    bank: &SoundBank,
    sample_rate: u32,
    models: &[(StringKind, StringModel)],
    track_models: &[(usize, StringModel)],
    chains: &[ChainChoice],
    trims: &[(usize, f32)],
    parts: &[PartChoice],
    buses: &BusChoices,
) -> Result<WavInfo, PlayError> {
    write_wav_part_with(
        writer,
        sequence,
        bank,
        sample_rate,
        models,
        track_models,
        chains,
        trims,
        parts,
        buses,
        StemPart::Tracks,
    )
}

/// [`write_wav_with`] for a stem: `part` says whether the tracks' own sound is written
/// ([`StemPart::Tracks`], as the whole mix) or only the buses' returns
/// ([`StemPart::Buses`], a bus stem). Everything else is as there.
#[allow(clippy::too_many_arguments)]
pub fn write_wav_part_with(
    mut writer: impl Write,
    sequence: &Sequence,
    bank: &SoundBank,
    sample_rate: u32,
    models: &[(StringKind, StringModel)],
    track_models: &[(usize, StringModel)],
    chains: &[ChainChoice],
    trims: &[(usize, f32)],
    parts: &[PartChoice],
    buses: &BusChoices,
    part: StemPart,
) -> Result<WavInfo, PlayError> {
    let frames = frames_for(sequence, sample_rate)?;
    let data_bytes = frames
        .checked_mul(4)
        .and_then(|bytes| u32::try_from(bytes).ok())
        .ok_or_else(|| PlayError::Device("the song is too long for a WAV file".to_owned()))?;
    write_header(&mut writer, sample_rate, data_bytes)?;

    let mut engine = SynthEngine::new(bank, sample_rate)?;
    engine.set_dry(part == StemPart::Tracks);
    for &(kind, model) in models {
        engine.set_string_model(kind, model);
    }
    for &(track, model) in track_models {
        engine.set_track_model(track, Some(model));
    }
    for (track, spec, knobs) in chains {
        if let Some(chain) = prepare_chain(spec, knobs, sample_rate) {
            let _ = engine.set_chain(*track, Some(chain), *knobs);
        }
    }
    for &(track, db) in trims {
        engine.set_trim(track, db);
    }
    // Each kit's one short shared room, behind its parts' Room sends.
    let mut room_tracks: Vec<usize> = Vec::new();
    for (track, _, _, _, room, _, _) in parts {
        if *room > 0 && !room_tracks.contains(track) {
            room_tracks.push(*track);
        }
    }
    for track in room_tracks {
        let _ = engine.set_track_room(track, crate::chains::prepare_drum_room(sample_rate));
    }
    for (track, part, level, pan, room, spec, knobs) in parts {
        if let Some(chain) = prepare_chain(spec, knobs, sample_rate) {
            let _ = engine.set_part_chain(*track, *part, Some(chain), *knobs);
        }
        engine.set_part_level(*track, *part, *level);
        engine.set_part_pan(*track, *part, *pan);
        engine.set_part_room(*track, *part, *room);
    }
    for (bus, choice) in [
        (crate::Bus::Reverb, &buses.reverb),
        (crate::Bus::Compressor, &buses.compressor),
        (crate::Bus::Chorus, &buses.chorus),
    ] {
        let Some(choice) = choice else {
            continue;
        };
        if let Some(chain) = prepare_chain(&choice.spec, &choice.knobs, sample_rate) {
            engine.set_bus(bus, Some(chain));
        }
        engine.set_bus_settings(bus, choice.knobs, choice.level, choice.muted);
    }
    if let Some(choice) = &buses.master
        && let Some(chain) = prepare_chain(&choice.spec, &choice.knobs, sample_rate)
    {
        let _ = engine.set_master_chain(Some(chain), choice.knobs);
    }
    for &(track, value) in &buses.comp_sends {
        engine.set_comp_send(track, value);
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
    fn a_bus_stem_carries_only_what_the_bus_returns() {
        use gscore_guitar::{BlockKind, ChainSpec};
        use gscore_model::TrackIndex;
        use gscore_model::testing::{self, TestNote};
        use gscore_play::Sequence;

        let bytes = sine_soundfont("Sine");
        let bank = SoundBank::from_bytes(&bytes).unwrap();
        let mut song = testing::song_with_notes(&[(1, &[TestNote::new(1, 5)]), (4, &[])]).unwrap();
        // The track sends into the reverb bus.
        let _ = song.set_track_reverb(TrackIndex::new(0), 100).unwrap();
        let sequence = Sequence::new(&song, None).unwrap();
        let reverb = BusChoices {
            reverb: Some(BusChoice {
                spec: ChainSpec {
                    blocks: [Some(BlockKind::Reverb), None, None, None, None],
                },
                knobs: ChainKnobs::default(),
                level: 127,
                muted: false,
            }),
            ..BusChoices::default()
        };
        let render = |buses: &BusChoices, part| {
            let mut out = Vec::new();
            write_wav_part_with(
                &mut out,
                &sequence,
                &bank,
                16_000,
                &[],
                &[],
                &[],
                &[],
                &[],
                buses,
                part,
            )
            .unwrap();
            out[44..].iter().any(|&byte| byte != 0)
        };
        // The track's stem has the note; without a bus the bus stem is silence (the dry
        // signal is left out), and with one it carries the reverb's return.
        assert!(render(&BusChoices::default(), StemPart::Tracks));
        assert!(!render(&BusChoices::default(), StemPart::Buses));
        assert!(render(&reverb, StemPart::Buses));
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
