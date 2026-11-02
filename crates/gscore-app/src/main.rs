//! `guitarscore [FILE]`: the desktop application.

// The lint checks each target separately; the dependencies are used by the library target.
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;
use std::process::ExitCode;

use gscore_app::{GuitarScoreApp, Settings};

fn main() -> ExitCode {
    if let Some(dir) = gscore_session::default_crash_dir() {
        gscore_session::install_panic_hook(dir);
    }
    let file = std::env::args_os().nth(1).map(PathBuf::from);
    // What the app remembers between runs: theme, zoom, panels, window, recent files,
    // keymap file and autosave interval.
    let settings_path = gscore_session::default_settings_path();
    let loaded_settings = match settings_path.as_deref() {
        Some(path) => gscore_session::load_json(path),
        None => gscore_session::LoadedJson {
            value: Settings::default(),
            path: None,
            problem: None,
        },
    };
    let mut settings = loaded_settings.value;
    settings.sanitize();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("GuitarScore")
        .with_inner_size([1100.0, 800.0])
        .with_min_inner_size([400.0, 300.0]);
    if let Some(window) = settings.window {
        viewport = viewport
            .with_inner_size(window.size())
            .with_position(window.position())
            .with_maximized(window.maximized);
    }
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport,
        ..eframe::NativeOptions::default()
    };
    let settings_problem = loaded_settings.problem;
    let settings_path_for_app = settings_path.clone();
    let started = eframe::run_native(
        "GuitarScore",
        options,
        Box::new(move |cc| {
            // A broken keymap file never stops the app: the built-in keymap is used and
            // the problem shown.
            let loaded = match settings.keymap_path.as_deref() {
                Some(path) => gscore_session::load_keymap(path),
                None => gscore_session::load_user_keymap(),
            };
            // Edits are journaled for crash recovery (AGENTS.md "Never lose work").
            let recovery = gscore_session::default_recovery_dir();
            let audio = (
                settings.prefer_synth,
                settings.soundfont_path.clone(),
                settings.midi_port.clone(),
            );
            let mut app = GuitarScoreApp::with_settings(
                &cc.egui_ctx,
                file,
                loaded.keymap,
                recovery,
                settings,
                settings_path_for_app,
            );
            if let Some(problem) = settings_problem.or(loaded.problem) {
                app.set_notice(problem);
            }
            // The built-in synthesizer plays with the chosen (or found) SoundFont; if the
            // user chose MIDI, that port is used instead.
            if audio.0 {
                if let Some(soundfont) = audio.1.or_else(gscore_audio::find_soundfont) {
                    app.playback_mut().use_soundfont(soundfont);
                }
            } else {
                app.playback_mut().choose_port(audio.2);
            }
            Ok(Box::new(app))
        }),
    );
    match started {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: cannot start GuitarScore: {error}");
            ExitCode::FAILURE
        }
    }
}
