//! `guitarscore [FILE]`: the desktop application. The browser build starts through
//! [`gscore_app::web`] instead (an empty `main` keeps the binary target valid on wasm).

// The lint checks each target separately; the dependencies are used by the library target.
#![allow(unused_crate_dependencies)]

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::process::ExitCode;

#[cfg(not(target_arch = "wasm32"))]
use gscore_app::{APP_ICON, GuitarScoreApp, Settings};

#[cfg(not(target_arch = "wasm32"))]
fn main() -> ExitCode {
    // Started by GuitarScore itself to run one plugin apart from the app (M24): no window,
    // no settings, just the requests on stdin.
    if std::env::args_os().nth(1).as_deref() == Some(gscore_plugin::remote::HOST_FLAG.as_ref()) {
        let input = std::io::BufReader::new(std::io::stdin());
        let output = std::io::BufWriter::new(std::io::stdout());
        let code = gscore_plugin::serve(input, output);
        return ExitCode::from(u8::try_from(code).unwrap_or(1));
    }
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
        // Matches `packaging/guitarscore.desktop`, so Wayland docks show its icon.
        .with_app_id("guitarscore")
        .with_inner_size([1100.0, 800.0])
        .with_min_inner_size([400.0, 300.0])
        // The app draws its own title bar (song title, window buttons) and no frame.
        .with_decorations(false)
        // It always opens maximized (or full screen, if it was closed that way); the
        // remembered size is for when it is restored down.
        .with_maximized(true);
    // The window and taskbar icon (X11, Windows); a broken image only loses the icon.
    match eframe::icon_data::from_png_bytes(APP_ICON) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => eprintln!("guitarscore: cannot read the app icon: {e}"),
    }
    if let Some(window) = settings.window {
        viewport = viewport
            .with_inner_size(window.size())
            .with_fullscreen(window.fullscreen);
        if let Some(position) = window.position() {
            viewport = viewport.with_position(position);
        }
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
            let mut loaded = match settings.keymap_path.as_deref() {
                Some(path) => gscore_session::load_keymap(path),
                None => gscore_session::load_user_keymap(),
            };
            // Shortcuts changed in the app live in the settings and are applied over the
            // built-in keymap or the keymap file.
            if let Err(problem) = settings.apply_shortcuts(&mut loaded.keymap) {
                loaded.problem = Some(match loaded.problem.take() {
                    Some(earlier) => format!("{earlier}\n{problem}"),
                    None => problem,
                });
            }
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
            // Asks once whether to look for updates daily (nothing is sent before a yes).
            app.enable_automatic_update_check();
            // A release build or AppImage on Linux offers (once) to add itself to the
            // applications menu, and keeps an entry it added pointing at itself. A build
            // from a source checkout leaves the menu alone.
            #[cfg(target_os = "linux")]
            if gscore_version::RELEASE || std::env::var_os("APPIMAGE").is_some() {
                app.enable_desktop_entry();
            }
            // A release build on Linux or Windows offers, in Settings › Editor, to open the
            // Guitar Pro files (and keeps an association it wrote pointing at itself).
            #[cfg(any(target_os = "linux", windows))]
            if gscore_version::RELEASE || std::env::var_os("APPIMAGE").is_some() {
                app.enable_file_association();
            }
            if let Some(problem) = settings_problem.or(loaded.problem) {
                app.set_notice(problem);
            }
            // The built-in synthesizer plays with the chosen (or found) SoundFont; if the
            // user chose MIDI, that port is used instead.
            // A saved SoundFont that moved with the app (an update, a moved folder) is
            // found again without a word; only one that is really gone is reported.
            if audio.0 {
                let soundfont = match audio.1 {
                    Some(saved) => {
                        let resolved = gscore_audio::resolve_saved_soundfont(&saved);
                        if let Some(problem) = resolved.problem {
                            app.set_notice(problem);
                        }
                        resolved.path
                    }
                    None => gscore_audio::find_soundfont(),
                };
                if let Some(soundfont) = soundfont {
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
