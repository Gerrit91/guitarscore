//! Renders the app offscreen to a PNG, with no window and no focus stealing (AGENTS.md
//! "Verify UI changes visually"):
//!
//! ```text
//! cargo run -p gscore-app --example snapshot -- <file> <out.png> [--dark] [--track N] [--zoom Z] [--size WxH] [--settings] [--history] [--mixer] [--new-track] [--scale S] [--click LABEL]...
//! ```

// An example is a small tool: failing loudly with a message is fine here.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    unused_crate_dependencies
)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gscore_app::{GuitarScoreApp, ThemeChoice};

/// How long to wait for the file to open.
const OPEN_TIMEOUT: Duration = Duration::from_secs(30);

fn main() {
    let mut args = std::env::args().skip(1);
    let usage = "usage: snapshot <file> <out.png> [--dark] [--track N] [--zoom Z] [--size WxH] [--settings] [--history] [--mixer] [--new-track] [--scale S] [--click LABEL]...";
    let file = PathBuf::from(args.next().expect(usage));
    let out = PathBuf::from(args.next().expect(usage));
    let (mut dark, mut track, mut zoom, mut size) = (false, 1_usize, 1.0_f32, (1100.0, 800.0));
    let mut settings = false;
    let mut history = false;
    let mut mixer = false;
    let mut new_track = false;
    let mut scale = 1.0_f32;
    let mut clicks = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dark" => dark = true,
            "--settings" => settings = true,
            "--history" => history = true,
            "--mixer" => mixer = true,
            "--new-track" => new_track = true,
            "--track" => track = args.next().and_then(|t| t.parse().ok()).expect(usage),
            "--scale" => scale = args.next().and_then(|s| s.parse().ok()).expect(usage),
            "--click" => clicks.push(args.next().expect(usage)),
            "--zoom" => zoom = args.next().and_then(|z| z.parse().ok()).expect(usage),
            "--size" => {
                let text = args.next().expect(usage);
                let (w, h) = text.split_once('x').expect(usage);
                size = (w.parse().expect(usage), h.parse().expect(usage));
            }
            other => panic!("unknown argument {other}; {usage}"),
        }
    }

    let mut harness = Harness::builder()
        .with_size(egui::vec2(size.0, size.1))
        .with_pixels_per_point(scale)
        .wgpu()
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(file.clone())));
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(started.elapsed() < OPEN_TIMEOUT, "opening timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    if history {
        // A few edits, the last one undone, so the history shows every kind of row.
        for label in ["1/8", "Rest", "1/4"] {
            harness.get_by_label(label).click();
            harness.run();
        }
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        harness.run();
    }
    {
        let app = harness.state_mut();
        let view = app.view_mut();
        view.theme = if dark {
            ThemeChoice::Dark
        } else {
            ThemeChoice::Light
        };
        view.track = track.saturating_sub(1);
        view.set_zoom(zoom);
        view.grid_mixer = mixer;
        if settings {
            app.open_settings();
        }
        if history {
            app.open_history();
        }
        if new_track {
            app.open_new_track();
        }
    }
    harness.run();
    // Widgets to click before the picture, by accessibility label (opens a dialog).
    for label in &clicks {
        harness.get_by_label(label).click();
        harness.run();
    }
    if let Some(notice) = harness.state().notice() {
        eprintln!("notice: {notice}");
    }
    let image = harness
        .render()
        .unwrap_or_else(|e| panic!("cannot render: {e}"));
    image
        .save(&out)
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", out.display()));
    println!("wrote {}", out.display());
}
