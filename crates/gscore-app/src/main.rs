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
        Box::new(move |cc| Ok(Box::new(GuitarScoreApp::new(&cc.egui_ctx, file)))),
    );
    match started {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: cannot start GuitarScore: {error}");
            ExitCode::FAILURE
        }
    }
}
