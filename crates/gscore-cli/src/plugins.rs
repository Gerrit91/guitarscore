//! `plugins`: the CLAP plugins installed (or in the given libraries and folders), and with
//! `--check` whether each effect starts, processes sound and keeps its settings.

use std::fmt::Write as _;
use std::path::PathBuf;

use gscore_plugin::{AudioConfig, Effect, Library, PluginInfo};

use crate::CliError;

/// The sound `--check` runs effects at.
const CONFIG: AudioConfig = AudioConfig {
    sample_rate: 48_000,
    max_frames: 512,
};

/// Lists the libraries found in `paths` (the system's plugin folders if empty) and their
/// plugins; with `check`, runs every effect. Fails if a library cannot be opened or a
/// checked effect fails, after listing everything else.
pub(crate) fn run(paths: &[PathBuf], check: bool) -> Result<String, CliError> {
    let dirs = if paths.is_empty() {
        gscore_plugin::default_dirs()
    } else {
        paths.to_vec()
    };
    let libraries = gscore_plugin::bundles_in(&dirs);
    let mut out = String::new();
    let mut problems = Vec::new();
    if libraries.is_empty() {
        let shown: Vec<String> = dirs.iter().map(|d| d.display().to_string()).collect();
        let _ = writeln!(out, "No CLAP plugins in {}.", shown.join(", "));
        return Ok(out);
    }
    let mut count = 0_usize;
    for path in &libraries {
        let library = match Library::open(path) {
            Ok(library) => library,
            Err(e) => {
                problems.push(e.to_string());
                continue;
            }
        };
        let plugins = library.plugins();
        let _ = writeln!(out, "{} ({} plugins)", path.display(), plugins.len());
        for info in &plugins {
            count = count.saturating_add(1);
            let _ = writeln!(out, "  {}", describe(info));
            if check && info.is_effect() {
                match check_effect(&library, &info.id) {
                    Ok(summary) => {
                        let _ = writeln!(out, "    ok: {summary}");
                    }
                    Err(e) => {
                        let _ = writeln!(out, "    FAILED: {e}");
                        problems.push(format!("{}: {e}", info.id));
                    }
                }
            }
        }
    }
    let _ = writeln!(out, "{count} plugins in {} libraries.", libraries.len());
    if problems.is_empty() {
        Ok(out)
    } else {
        Err(CliError::Failed(format!(
            "{out}{} problem(s):\n  {}",
            problems.len(),
            problems.join("\n  ")
        )))
    }
}

/// "Name (vendor 1.0) id [features]", and what GuitarScore can use it as.
fn describe(info: &PluginInfo) -> String {
    let kind = match (info.is_effect(), info.is_instrument()) {
        (true, _) => "effect",
        (false, true) => "instrument (not usable yet)",
        (false, false) => "other",
    };
    format!(
        "{} ({} {}) {} [{}] - {kind}",
        info.name,
        info.vendor,
        info.version,
        info.id,
        info.features.join(", ")
    )
}

/// Starts the effect, runs one second of a decaying tone through it, saves and loads its
/// settings. Returns a short summary.
fn check_effect(library: &Library, id: &str) -> Result<String, String> {
    let mut effect = Effect::new(library, id).map_err(|e| e.to_string())?;
    let params = effect.params().len();
    let latency = effect.latency();
    let mut processor = effect.activate(CONFIG).map_err(|e| e.to_string())?;
    let frames = CONFIG.sample_rate as usize;
    let mut left: Vec<f32> = (0..frames)
        .map(|i| {
            let t = i as f32 / CONFIG.sample_rate as f32;
            (t * 220.0 * std::f32::consts::TAU).sin() * (-4.0 * t).exp() * 0.5
        })
        .collect();
    let mut right = left.clone();
    let result = processor.process(&mut left, &mut right);
    effect.deactivate(processor);
    result.map_err(|e| e.to_string())?;
    if !left.iter().chain(&right).all(|s| s.is_finite()) {
        return Err("its sound is not a number (NaN or infinite)".into());
    }
    let peak = left
        .iter()
        .chain(&right)
        .fold(0.0_f32, |peak, s| peak.max(s.abs()));
    let state = match effect.save_state() {
        Ok(blob) => {
            effect.load_state(&blob).map_err(|e| e.to_string())?;
            format!("{} bytes of settings", blob.len())
        }
        Err(_) => "no settings".to_owned(),
    };
    Ok(format!(
        "{params} parameters, latency {latency} frames, peak {peak:.2}, {state}"
    ))
}
