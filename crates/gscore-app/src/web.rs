//! The browser application entry point: `wasm-bindgen` calls [`start`] when the module
//! loads. It finds the canvas, reads the remembered settings from browser storage, and
//! hands the same [`GuitarScoreApp`] the desktop uses to eframe's web runner (WebGL2).
//!
//! Everything native-specific (file dialogs, window state, MIDI ports) is behind
//! `cfg(target_arch = "wasm32")` in the rest of the crate: opening picks a file in the
//! browser, saving downloads it, and the settings live in `localStorage`.

use std::sync::atomic::{AtomicBool, Ordering};

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::GuitarScoreApp;
use crate::Settings;

/// Whether a song has unsaved edits, as of the last frame.
static UNSAVED: AtomicBool = AtomicBool::new(false);

/// Tells the page whether a song has unsaved edits (the app calls it every frame).
pub(crate) fn set_unsaved(unsaved: bool) {
    UNSAVED.store(unsaved, Ordering::Relaxed);
}

/// Makes the browser ask before the page is closed or reloaded while a song has unsaved
/// edits. There is no recovery journal in the browser yet, so this is the only guard.
fn warn_before_leaving(window: &web_sys::Window) -> Result<(), JsValue> {
    let handler = Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(
        |event: web_sys::BeforeUnloadEvent| {
            if UNSAVED.load(Ordering::Relaxed) {
                event.prevent_default();
                // Older browsers show the prompt only when a return value is set.
                event.set_return_value("unsaved");
            }
        },
    );
    window.add_event_listener_with_callback("beforeunload", handler.as_ref().unchecked_ref())?;
    // The listener lives as long as the page.
    handler.forget();
    Ok(())
}

/// Logs a panic to the browser console instead of losing it silently. Panics abort the
/// wasm instance, so this is a diagnostic aid, not containment (AGENTS.md "Portability").
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&JsValue::from_str(&format!("GuitarScore panicked: {info}")));
    }));
}

/// Starts the app in the page's `#guitarscore` canvas. The returned promise rejects if the
/// canvas or the graphics context is missing.
#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    install_panic_hook();
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no browser window"))?;
    warn_before_leaving(&window)?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let canvas = document
        .get_element_by_id("guitarscore")
        .ok_or_else(|| JsValue::from_str("the page has no #guitarscore canvas"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| JsValue::from_str("#guitarscore is not a canvas"))?;

    // A broken or missing entry falls back to the defaults, and the problem is shown.
    let loaded = gscore_session::load_web_json::<Settings>(gscore_session::WebKey::SETTINGS);
    let mut settings = loaded.value;
    settings.sanitize();
    let mut problem = loaded.problem;
    let mut keymap = gscore_session::load_web_keymap().keymap;
    if let Err(error) = settings.apply_shortcuts(&mut keymap) {
        problem = Some(match problem {
            Some(earlier) => format!("{earlier}\n{error}"),
            None => error,
        });
    }

    let creator: eframe::AppCreator<'static> = Box::new(move |cc| {
        let mut app = GuitarScoreApp::with_settings(
            &cc.egui_ctx,
            None,
            keymap,
            // The recovery journal is a file journal; the browser build does not write
            // one yet (docs/web.md).
            None,
            settings,
            None,
        );
        // The notice bar is for problems (it is drawn as an error); the empty page
        // already says how to open a song.
        if let Some(problem) = problem {
            app.set_notice(problem);
        }
        Ok(Box::new(app))
    });

    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions {
                renderer: eframe::Renderer::Glow,
                ..eframe::WebOptions::default()
            },
            creator,
        )
        .await?;
    // The page hides its loading screen once the editor runs; `cargo xtask web --smoke`
    // (a headless browser) waits for the same marker.
    if let Some(body) = document.body() {
        body.set_attribute("data-guitarscore-ready", "true")
            .map_err(|_| JsValue::from_str("cannot mark the page as ready"))?;
    }
    Ok(())
}
