//! `cargo xtask web`: builds the browser build and assembles the static site.
//!
//! The output is exactly the files a static host serves:
//!
//! ```text
//! index.html          the page (canvas + loading screen)
//! guitarscore.js      wasm-bindgen's glue
//! guitarscore_bg.wasm the app
//! assets/             icon and optional runtime assets (SoundFonts)
//! manifest.json       installable app metadata
//! sw.js               service worker (offline app shell)
//! ```
//!
//! `--smoke` opens the built page in a headless Chromium, waits for the editor's ready
//! marker, and saves a screenshot.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod soundfont;

use crate::{TaskResult, cmd};

/// Where the build goes unless `--out` says otherwise.
const DEFAULT_OUT: &str = "dist/web";

pub(crate) fn run(root: &Path, args: &[String]) -> TaskResult {
    let options = Options::parse(args)?;
    let out = root.join(&options.out);
    println!(
        "web: building for wasm32-unknown-unknown ({})",
        options.profile()
    );
    let wasm = build(root, &options)?;
    // Start from an empty directory so files from an older layout cannot linger.
    if out.exists() {
        std::fs::remove_dir_all(&out)
            .map_err(|e| format!("cannot clear {}: {e}", out.display()))?;
    }
    bindgen(&wasm, &out, "guitarscore")?;
    let worklet = build_worklet(root, &options)?;
    bindgen(&worklet, &out, "worklet_synth")?;
    copy_shell(root, &out)?;
    soundfont::install(root, &out, &options.soundfont)?;
    stamp_service_worker(&out)?;
    if !options.no_wasm_opt {
        optimise(&out.join("guitarscore_bg.wasm"))?;
    }
    report_sizes(&out)?;
    soundfont::report_size(&out);
    if options.smoke {
        smoke(&out)?;
    }
    println!("web: wrote {}", out.display());
    Ok(())
}

/// What `cargo xtask web` was asked to do.
#[derive(Debug, Clone)]
struct Options {
    /// Where the static site is assembled (relative to the workspace root).
    out: PathBuf,
    /// Build without optimisations (faster, much larger).
    debug: bool,
    /// Skip binaryen's `wasm-opt` (it is optional, but the size matters for the web).
    no_wasm_opt: bool,
    /// Open the result in a headless browser and check that it starts.
    smoke: bool,
    /// Which SoundFont the site ships.
    soundfont: soundfont::Source,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Options {
            out: PathBuf::from(DEFAULT_OUT),
            debug: false,
            no_wasm_opt: false,
            smoke: false,
            soundfont: soundfont::Source::Bundled,
        };
        let mut rest = args.iter();
        while let Some(arg) = rest.next() {
            match arg.as_str() {
                "--out" => {
                    let Some(value) = rest.next() else {
                        return Err("--out needs a directory".to_owned());
                    };
                    options.out = PathBuf::from(value);
                }
                "--debug" => options.debug = true,
                "--no-wasm-opt" => options.no_wasm_opt = true,
                "--smoke" => options.smoke = true,
                "--soundfont" => {
                    let Some(value) = rest.next() else {
                        return Err("--soundfont needs a .sf2 file".to_owned());
                    };
                    if matches!(options.soundfont, soundfont::Source::None) {
                        return Err("--soundfont and --no-soundfont cannot be combined".to_owned());
                    }
                    options.soundfont = soundfont::Source::Local(PathBuf::from(value));
                }
                "--no-soundfont" => {
                    if matches!(options.soundfont, soundfont::Source::Local(_)) {
                        return Err("--soundfont and --no-soundfont cannot be combined".to_owned());
                    }
                    options.soundfont = soundfont::Source::None;
                }
                other => {
                    return Err(format!(
                        "unexpected argument `{other}`\n\nusage: cargo xtask web \
                         [--out DIR] [--debug] [--no-wasm-opt] [--smoke] \
                         [--soundfont PATH | --no-soundfont]"
                    ));
                }
            }
        }
        if options.out.as_os_str().is_empty() {
            return Err("--out needs a directory".to_owned());
        }
        Ok(options)
    }

    fn profile(&self) -> &'static str {
        if self.debug { "debug" } else { "release" }
    }
}

/// Compiles the app for wasm and returns the module's path.
fn build(root: &Path, options: &Options) -> Result<PathBuf, String> {
    let mut args = vec![
        "build",
        "--package",
        "gscore-app",
        "--lib",
        "--target",
        "wasm32-unknown-unknown",
        "--no-default-features",
        "--features",
        "web",
    ];
    if !options.debug {
        args.push("--release");
    }
    cmd::run_status(&cmd::cargo(), &args, root)?;
    let profile = if options.debug { "debug" } else { "release" };
    let wasm = root
        .join("target")
        .join("wasm32-unknown-unknown")
        .join(profile)
        .join("gscore_app.wasm");
    if !wasm.is_file() {
        return Err(format!(
            "the build did not produce {} (did the crate name or lib target change?)",
            wasm.display()
        ));
    }
    Ok(wasm)
}

/// Compiles the AudioWorklet synthesizer (a second wasm module for the audio thread).
fn build_worklet(root: &Path, options: &Options) -> Result<PathBuf, String> {
    let mut args = vec![
        "build",
        "--package",
        "gscore-worklet",
        "--lib",
        "--target",
        "wasm32-unknown-unknown",
    ];
    if !options.debug {
        args.push("--release");
    }
    cmd::run_status(&cmd::cargo(), &args, root)?;
    let profile = if options.debug { "debug" } else { "release" };
    let wasm = root
        .join("target")
        .join("wasm32-unknown-unknown")
        .join(profile)
        .join("gscore_worklet.wasm");
    if !wasm.is_file() {
        return Err(format!(
            "the build did not produce {} (did the worklet crate change?)",
            wasm.display()
        ));
    }
    Ok(wasm)
}

/// Writes the JavaScript glue and the wasm module into `out`.
fn bindgen(wasm: &Path, out: &Path, name: &str) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    let Some(bindgen) = which(&["wasm-bindgen"]) else {
        return Err("`wasm-bindgen` was not found.\n\
             Install it with `cargo install wasm-bindgen-cli --version 0.2.130` (the version \
             must match the `wasm-bindgen` crate), or download the release from \
             https://github.com/wasm-bindgen/wasm-bindgen/releases and put it on PATH."
            .to_owned());
    };
    let out_dir = out.to_string_lossy().into_owned();
    let wasm_name = wasm.to_string_lossy().into_owned();
    cmd::run_status(
        &bindgen,
        &[
            "--target",
            "web",
            "--out-dir",
            &out_dir,
            "--out-name",
            name,
            "--no-typescript",
            &wasm_name,
        ],
        Path::new("."),
    )?;
    let glue = format!("{name}.js");
    let module = format!("{name}_bg.wasm");
    for file in [glue, module] {
        if !out.join(&file).is_file() {
            return Err(format!("wasm-bindgen did not write {file}"));
        }
    }
    Ok(())
}

/// Copies the page, manifest, service worker and runtime assets.
fn copy_shell(root: &Path, out: &Path) -> Result<(), String> {
    let source = root.join("web");
    for name in [
        "index.html",
        "manifest.json",
        "sw.js",
        "worklet.js",
        "worklet-polyfills.js",
        "worklet-check.html",
    ] {
        let from = source.join(name);
        let to = out.join(name);
        std::fs::copy(&from, &to)
            .map_err(|e| format!("cannot copy {} to {}: {e}", from.display(), to.display()))?;
    }
    // Optional runtime assets (the icon, SoundFonts); missing ones are fine.
    copy_tree(&source.join("assets"), &out.join("assets"))
}

/// Copies every file below `from` into `to`, creating directories. A missing source is not
/// an error (a source build without optional assets).
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    if !from.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(to).map_err(|e| format!("cannot create {}: {e}", to.display()))?;
    let entries =
        std::fs::read_dir(from).map_err(|e| format!("cannot read {}: {e}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", from.display()))?;
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &target)?;
        } else {
            std::fs::copy(&path, &target).map_err(|e| {
                format!(
                    "cannot copy {} to {}: {e}",
                    path.display(),
                    target.display()
                )
            })?;
        }
    }
    Ok(())
}

/// Gives the service worker a cache name unique to this build, so returning visitors get
/// the new files instead of the previous cache.
fn stamp_service_worker(out: &Path) -> Result<(), String> {
    let path = out.join("sw.js");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let version = format!("{}-{seconds}", env!("CARGO_PKG_VERSION"));
    let text = text.replace("__VERSION__", &version);
    std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Runs `wasm-opt -Oz` when it is installed; without it the build still works, just larger.
fn optimise(wasm: &Path) -> Result<(), String> {
    let Some(wasm_opt) = which(&["wasm-opt"]) else {
        eprintln!(
            "web: wasm-opt not found; the module is not size-optimised (install binaryen, \
             e.g. `pacman -S binaryen`)"
        );
        return Ok(());
    };
    let temporary = wasm.with_extension("opt.wasm");
    let input = wasm.to_string_lossy().into_owned();
    let output = temporary.to_string_lossy().into_owned();
    cmd::run_status(
        &wasm_opt,
        &["-Oz", "--strip-debug", "-o", &output, &input],
        Path::new("."),
    )?;
    std::fs::rename(&temporary, wasm).map_err(|e| format!("cannot replace {}: {e}", wasm.display()))
}

/// Prints the sizes that matter for a static host: the wasm module raw, gzipped and
/// brotli-compressed, plus the JavaScript glue.
fn report_sizes(out: &Path) -> Result<(), String> {
    for name in ["guitarscore_bg.wasm", "guitarscore.js"] {
        let path = out.join(name);
        let bytes =
            std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if name.ends_with(".wasm") {
            let gzip = compressed_size("gzip", &["-9", "-c"], &path)?;
            let brotli = compressed_size("brotli", &["-q", "11", "-c"], &path)?;
            println!(
                "web: {name}: {} raw, {} gzip, {} brotli",
                human(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
                human(gzip),
                human(brotli)
            );
        } else {
            println!(
                "web: {name}: {}",
                human(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
            );
        }
    }
    Ok(())
}

/// The size of `path` after `program` compresses it to stdout, or 0 if the program is
/// missing. The file is the compressor's argument, so a full stdout pipe cannot deadlock
/// the writer.
pub(crate) fn compressed_size(program: &str, args: &[&str], path: &Path) -> Result<u64, String> {
    let Some(program) = which(&[program]) else {
        return Ok(0);
    };
    let file = path.to_string_lossy().into_owned();
    let output = Command::new(program)
        .args(args)
        .arg(file)
        .output()
        .map_err(|e| format!("cannot run the compressor: {e}"))?;
    Ok(u64::try_from(output.stdout.len()).unwrap_or(0))
}

fn human(bytes: u64) -> String {
    let bytes = u32::try_from(bytes).map(f64::from).unwrap_or(f64::MAX);
    if bytes >= 1024.0 * 1024.0 {
        format!("{:.2} MiB", bytes / (1024.0 * 1024.0))
    } else if bytes >= 1024.0 {
        format!("{:.0} KiB", bytes / 1024.0)
    } else {
        format!("{bytes:.0} B")
    }
}

/// Opens the built page in a headless Chromium and checks that the app starts (the ready
/// marker appears). Saves a screenshot next to the build.
fn smoke(out: &Path) -> TaskResult {
    let browser =
        which(&["chromium", "chromium-browser", "google-chrome", "chrome"]).ok_or_else(|| {
            "no Chromium was found for the smoke test; install chromium (or run without \
             --smoke)"
                .to_owned()
        })?;
    let port = free_port()?;
    let out_text = out.to_string_lossy().into_owned();
    let mut server = Command::new("python3")
        .args([
            "-m",
            "http.server",
            &port.to_string(),
            "--bind",
            "127.0.0.1",
            "--directory",
            &out_text,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start the static server: {e}"))?;
    let result = run_headless(&browser, out, port);
    let _ = server.kill();
    let _ = server.wait();
    result
}

fn run_headless(browser: &std::ffi::OsString, out: &Path, port: u16) -> TaskResult {
    wait_for_server(port, Duration::from_secs(10))?;
    let debug_port = free_port()?;
    let mut browser = crate::cdp::Browser::start(browser, debug_port)?;
    // The app's page: wait for eframe's runner, and report what the page shows if it
    // fails (the page writes errors into #status).
    let app = "new Promise((resolve) => { const started = Date.now(); \
         const poll = () => { const body = document.body; \
         if (body && body.dataset.guitarscoreReady === 'true') return resolve('ok'); \
         const status = document.getElementById('status'); \
         const text = status ? status.textContent.trim() : ''; \
         if (text) return resolve('failed: ' + text); \
         if (Date.now() - started > 30000) return resolve('timeout: the editor did not start'); \
         setTimeout(poll, 100); }; poll(); })";
    let url = format!("http://127.0.0.1:{port}/");
    let result = browser.evaluate(&url, app)?;
    if result != "ok" {
        return Err(format!(
            "the editor did not start in the browser: {result}{}",
            console_log(browser.events())
        ));
    }
    browser.screenshot(&out.join("smoke.png"))?;

    // The bundled SoundFont must download and be accepted by the worklet, or the page
    // says why. This is where a wrong manifest, a failed download or a broken RIFF file
    // shows up in CI.
    let sound = "new Promise((resolve) => { const started = Date.now(); \
                  const poll = () => { const value = document.body.dataset.guitarscoreSound; \
                  if (value === 'loaded') return resolve('ok'); \
                  if (value) return resolve(value); \
                  if (Date.now() - started > 45000) return resolve('timeout; progress=' + \
                    (document.body.dataset.guitarscoreSoundProgress || 'none')); \
                  setTimeout(poll, 100); }; poll(); })";
    let started = Instant::now();
    let result = browser.eval(sound)?;
    if result != "ok" {
        return Err(format!(
            "the browser's SoundFont did not load: {result}{}",
            console_log(browser.events())
        ));
    }
    let millis = browser
        .eval("document.body.dataset.guitarscoreSoundMs || 'unknown'")
        .unwrap_or_else(|_| "unknown".to_owned());
    println!(
        "web: SoundFont loaded in {millis} ms from page start ({:.1} s after the editor marker)",
        started.elapsed().as_secs_f32()
    );

    // The audio worklet page: its module and second wasm instance must load, the
    // SoundFont must play a note, and the analyser must see samples (not silence).
    let worklet = "new Promise((resolve) => { const started = Date.now(); \
                   const poll = () => { const body = document.body; \
                   const sound = body.dataset.workletSound; \
                   const status = document.getElementById('status'); \
                   if (sound === 'ok') return resolve('ok, rms ' + body.dataset.workletRms); \
                   if (sound === 'silent') return resolve('silent (rms ' + body.dataset.workletRms + ')'); \
                   if (body.dataset.workletCheck === 'failed') \
                     return resolve('failed: ' + (status ? status.textContent : 'unknown')); \
                   if (Date.now() - started > 60000) return resolve('timeout: step=' + \
                     (body.dataset.step || 'none') + ' status=' + (status ? status.textContent : 'none')); \
                   setTimeout(poll, 100); }; poll(); })";
    let url = format!("http://127.0.0.1:{port}/worklet-check.html");
    let result = browser.evaluate(&url, worklet)?;
    if !result.starts_with("ok") {
        return Err(format!(
            "the AudioWorklet synthesizer did not play in the browser: {result}{}",
            console_log(browser.events())
        ));
    }
    println!("web: worklet sound OK ({result})");
    browser.screenshot(&out.join("smoke-worklet.png"))?;
    println!(
        "web: smoke tests OK; screenshots at {} and {}",
        out.join("smoke.png").display(),
        out.join("smoke-worklet.png").display()
    );
    Ok(())
}

/// The browser's console messages, as a failure message's tail.
fn console_log(events: &[String]) -> String {
    if events.is_empty() {
        return String::new();
    }
    let mut message = String::from("\nThe browser's console said:");
    for event in events.iter().rev().take(8).rev() {
        message.push_str("\n  ");
        message.push_str(event);
    }
    message
}

fn wait_for_server(port: u16, timeout: Duration) -> Result<(), String> {
    let started = Instant::now();
    let address = format!("127.0.0.1:{port}");
    while started.elapsed() < timeout {
        if std::net::TcpStream::connect(&address).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!("the static server on {address} did not start"))
}

fn free_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("cannot find a free port: {e}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|e| format!("cannot read the port: {e}"))
}

/// The first program found on `PATH`.
fn which(names: &[&str]) -> Option<std::ffi::OsString> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate.into_os_string());
            }
        }
    }
    None
}
