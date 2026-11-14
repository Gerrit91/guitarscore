//! Measuring a tone's loudness for `cargo xtask tones normalize --measure`: one fixed note
//! on the tone's own strings and chain, the same input for every tone, rendered offline.
//!
//! This runs only in the tool (the `tones` example), never in the application.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gscore_guitar::{
    Attack, BLOCKS, BlockKind, BlockSettings, ChainSpec, ImpulseResponse, Pluck, StringVoice,
};

use crate::chain_settings::{BlockChoice, ChainSetting, Tone};

/// The rate everything is measured at.
pub(super) const RATE: u32 = 48_000;
/// The reference note: A2, plucked at a fixed strength.
const FREQUENCY: f32 = 110.0;
const VELOCITY: f32 = 0.8;
/// How long the note sounds, and the stretch measured (after the attack, before the tail
/// has died away).
const SECONDS: f32 = 2.0;
const FROM: f32 = 0.2;
const TO: f32 = 1.5;
/// Below this level a stretch counts as silent, dBFS.
pub(super) const SILENT_DB: f32 = -100.0;

/// A tone's level measured over the reference note.
pub(super) struct Measurement {
    /// RMS of the chain's output, dBFS.
    pub(super) rms_db: f32,
    /// The loudest sample of the measured stretch, dBFS: how close the note comes to the
    /// meter's red.
    pub(super) peak_db: f32,
    /// The loudest sample of the strings alone (before the chain), dBFS: what the mixer's
    /// input meter sees.
    pub(super) input_db: f32,
    /// The note had died away before the usual stretch (a muted, short string), so its
    /// first [`FROM`] seconds were measured instead.
    pub(super) short: bool,
    /// Impulse responses the chain could not load (their place was left empty, as playback
    /// does, so the measurement is of what would be heard without them).
    pub(super) skipped: Vec<String>,
}

/// Renders `tone`'s reference note through its chain (at its current output level) and
/// measures it.
pub(super) fn measure(
    tone: &Tone,
    impulses: &mut HashMap<PathBuf, Option<Arc<ImpulseResponse>>>,
) -> Result<Measurement, String> {
    let mut voice = StringVoice::new(RATE as f32, 60.0, tone.sound.model());
    voice.pluck(Pluck {
        frequency: FREQUENCY,
        velocity: VELOCITY,
        attack: Attack::Pick,
        palm_mute: false,
        dead: false,
        harmonic: false,
    });
    let frames = (RATE as f32 * SECONDS) as usize;
    let mut mono = vec![0.0_f32; frames];
    voice.render(&mut mono);

    let loudest = |window: &[f32]| {
        let loudest = window
            .iter()
            .fold(0.0_f32, |loudest, sample| loudest.max(sample.abs()));
        20.0 * loudest.max(1e-9).log10()
    };
    let input_db = loudest(&mono);
    let (spec, settings, skipped) = chain_spec(&tone.chain, impulses);
    let mut chain = spec.prepare(RATE).instance(&settings, tone.chain.output_db);
    let mut left = mono.clone();
    let mut right = mono;
    chain.process_in_place(&mut left, &mut right, false);

    let from = (RATE as f32 * FROM) as usize;
    let to = ((RATE as f32 * TO) as usize).min(left.len());
    let window = left.get(from..to).unwrap_or_default();
    if window.is_empty() {
        return Err("the reference note is too short to measure".to_owned());
    }
    let level = |window: &[f32]| {
        let sum: f32 = window.iter().map(|sample| sample * sample).sum();
        let rms = (sum / window.len().max(1) as f32).sqrt();
        20.0 * rms.max(1e-9).log10()
    };
    let mut rms_db = level(window);
    let mut peak_db = loudest(window);
    // A string that is over before the stretch begins (a muted tone) is measured where it
    // sounds: its first moments. A tone silent there too has no level to match; the
    // caller decides what to do with it.
    let short = rms_db < SILENT_DB;
    if short {
        let first = left.get(..from).unwrap_or_default();
        rms_db = level(first);
        peak_db = loudest(first);
    }
    Ok(Measurement {
        rms_db,
        peak_db,
        input_db,
        short,
        skipped,
    })
}

/// The engine's chain for `setting`, with impulse responses loaded from disk (a missing or
/// damaged one leaves its place empty and is reported, as playback does).
fn chain_spec(
    setting: &ChainSetting,
    impulses: &mut HashMap<PathBuf, Option<Arc<ImpulseResponse>>>,
) -> (ChainSpec, [BlockSettings; BLOCKS], Vec<String>) {
    let mut spec = ChainSpec::default();
    let mut skipped = Vec::new();
    for (place, block) in spec.blocks.iter_mut().zip(&setting.blocks) {
        *place = match (block.choice, &block.file) {
            (BlockChoice::Empty, _) => None,
            (BlockChoice::Reverb, _) => Some(BlockKind::Reverb),
            (BlockChoice::Amp, _) => Some(BlockKind::Amp),
            (BlockChoice::Drive, _) => Some(BlockKind::Drive),
            (BlockChoice::Compressor, _) => Some(BlockKind::Compressor),
            (BlockChoice::Eq, _) => Some(BlockKind::Eq),
            (BlockChoice::Chorus, _) => Some(BlockKind::Chorus),
            (BlockChoice::Delay, _) => Some(BlockKind::Delay),
            // A cabinet without a file of its own plays the built-in one.
            (BlockChoice::Cabinet, None) => block.built_in_cabinet().map(BlockKind::Impulse),
            (BlockChoice::Room, None) => None,
            (choice, Some(file)) => match impulse(file, impulses) {
                Ok(response) if choice == BlockChoice::Room => {
                    Some(BlockKind::room(response, block.room_length()))
                }
                Ok(response) => Some(BlockKind::Impulse(response)),
                Err(error) => {
                    skipped.push(error);
                    None
                }
            },
        };
    }
    (spec, setting.knobs(), skipped)
}

fn impulse(
    path: &Path,
    cache: &mut HashMap<PathBuf, Option<Arc<ImpulseResponse>>>,
) -> Result<Arc<ImpulseResponse>, String> {
    if let Some(cached) = cache.get(path) {
        return cached
            .clone()
            .ok_or_else(|| format!("cannot load {} (it failed before)", path.display()));
    }
    let bytes =
        std::fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    match ImpulseResponse::from_wav(&bytes) {
        Ok(response) => {
            let response = Arc::new(response);
            cache.insert(path.to_path_buf(), Some(Arc::clone(&response)));
            Ok(response)
        }
        Err(error) => {
            cache.insert(path.to_path_buf(), None);
            Err(format!("{}: {error}", path.display()))
        }
    }
}
