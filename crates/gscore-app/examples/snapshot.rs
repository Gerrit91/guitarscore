//! Renders the app offscreen to a PNG, with no window and no focus stealing (AGENTS.md
//! "Verify UI changes visually"):
//!
//! ```text
//! cargo run -p gscore-app --example snapshot -- <file> <out.png> [--dark] [--track N] [--zoom Z] [--size WxH] [--settings] [--history] [--mixer] [--new-track]
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
use gscore_app::{GuitarScoreApp, ThemeChoice};

/// How long to wait for the file to open.
const OPEN_TIMEOUT: Duration = Duration::from_secs(30);

fn main() {
    let mut args = std::env::args().skip(1);
    let usage = "usage: snapshot <file> <out.png> [--dark] [--track N] [--zoom Z] [--size WxH] [--settings] [--history] [--mixer] [--new-track]";
    let file = PathBuf::from(args.next().expect(usage));
    let out = PathBuf::from(args.next().expect(usage));
    let (mut dark, mut track, mut zoom, mut size) = (false, 1_usize, 1.0_f32, (1100.0, 800.0));
    let mut settings = false;
    let mut history = false;
    let mut mixer = false;
    let mut new_track = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dark" => dark = true,
            "--settings" => settings = true,
            "--history" => history = true,
            "--mixer" => mixer = true,
            "--new-track" => new_track = true,
            "--track" => track = args.next().and_then(|t| t.parse().ok()).expect(usage),
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
        .wgpu()
        .build_eframe(|cc| GuitarScoreApp::new(&cc.egui_ctx, Some(file.clone())));
    let started = Instant::now();
    while harness.state().is_busy() {
        assert!(started.elapsed() < OPEN_TIMEOUT, "opening timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
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
