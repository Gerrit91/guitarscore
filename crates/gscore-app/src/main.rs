//! `guitarscore [FILE]`: the desktop application.

// The lint checks each target separately; the dependencies are used by the library target.
#![allow(unused_crate_dependencies)]

use std::path::PathBuf;
use std::process::ExitCode;

use gscore_app::GuitarScoreApp;

fn main() -> ExitCode {
    if let Some(dir) = gscore_session::default_crash_dir() {
        gscore_session::install_panic_hook(dir);
    }
    let file = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("GuitarScore")
            .with_inner_size([1100.0, 800.0])
            .with_min_inner_size([400.0, 300.0]),
        ..eframe::NativeOptions::default()
    };
    let started = eframe::run_native(
        "GuitarScore",
        options,
        Box::new(move |cc| {
            // A broken keymap file never stops the app: the built-in keymap is used and
            // the problem shown.
            let loaded = gscore_session::load_user_keymap();
            // Edits are journaled for crash recovery (AGENTS.md "Never lose work").
            let recovery = gscore_session::default_recovery_dir();
            let mut app = GuitarScoreApp::with_options(&cc.egui_ctx, file, loaded.keymap, recovery);
            if let Some(problem) = loaded.problem {
                app.set_notice(problem);
            }
            // The built-in synthesizer plays when a SoundFont is installed; otherwise MIDI.
            if let Some(soundfont) = gscore_audio::find_soundfont() {
                app.playback_mut().use_soundfont(soundfont);
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
