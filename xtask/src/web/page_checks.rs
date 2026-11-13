//! The smoke test's checks of the running editor page (after it has started): a new
//! build takes over at the first visit after a deploy, an edit leaves a rescue copy of the
//! song with the page, and the crash screen a panic shows offers it for download. Driven
//! through the page's own events (a dropped file, typed keys), as a user would.

use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::cdp::Browser;

/// How long a new build may take to replace the old one in the page.
const TAKE_OVER: Duration = Duration::from_secs(30);

/// Plays a deploy: once the service worker controls the page, the built `sw.js` and
/// `index.html` change as a new build's would; one visit later the page must run the new
/// build (the old worker serves that visit, the page reloads itself when the new one takes
/// over). The built files are restored afterwards.
pub(super) fn new_build_takes_over(
    browser: &mut Browser,
    out: &Path,
    url: &str,
) -> Result<(), String> {
    let controlled = "new Promise((resolve) => { const started = Date.now(); \
                      const poll = () => { if (navigator.serviceWorker?.controller) return resolve('ok'); \
                      if (Date.now() - started > 15000) return resolve('no service worker took control'); \
                      setTimeout(poll, 100); }; poll(); })";
    let result = browser.eval(controlled)?;
    if result != "ok" {
        return Err(result);
    }
    let (worker, page) = (out.join("sw.js"), out.join("index.html"));
    let read = |path: &Path| {
        std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
    };
    let (old_worker, old_page) = (read(&worker)?, read(&page)?);
    let new_worker = old_worker.replacen(
        "const CACHE = \"guitarscore-",
        "const CACHE = \"guitarscore-next-",
        1,
    );
    let new_page = old_page.replacen(
        "</head>",
        "<meta name=\"gscore-build\" content=\"next\" />\n  </head>",
        1,
    );
    if new_worker == old_worker || new_page == old_page {
        return Err("cannot make a new build from sw.js and index.html".to_owned());
    }
    let write = |path: &Path, text: &str| {
        std::fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
    };
    write(&worker, &new_worker)?;
    write(&page, &new_page)?;
    let result = visit_until_new(browser, url);
    write(&worker, &old_worker)?;
    write(&page, &old_page)?;
    result
}

/// Visits `url` once and waits (through the page's own reload) for the new build's page
/// with the editor running.
fn visit_until_new(browser: &mut Browser, url: &str) -> Result<(), String> {
    let _ = browser.evaluate(url, "'visited'")?;
    let check = "(document.querySelector('meta[name=gscore-build]') ? 'new' : 'old') + \
                 (document.body?.dataset.guitarscoreReady === 'true' ? ' ready' : '')";
    let started = Instant::now();
    let mut last = String::from("nothing");
    while started.elapsed() < TAKE_OVER {
        // While the page reloads, its context is gone and the evaluation fails.
        if let Ok(state) = browser.eval(check) {
            if state == "new ready" {
                return Ok(());
            }
            last = state;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!(
        "the page still runs the previous build after one visit ({last})"
    ))
}

/// A small test song, dropped on the page.
const SONG: &[u8] = include_bytes!("../../../testdata/basic/steel_guitar_dadgad.gp");
const SONG_NAME: &str = "steel_guitar_dadgad.gp";

/// Drops [`SONG`] on the editor, types a fret, and waits for the rescue copy of the edited
/// song (`app/web_rescue.rs`): a Guitar Pro 7 file (a zip) under the song's name.
pub(super) fn edit_leaves_a_rescue_copy(browser: &mut Browser) -> Result<String, String> {
    let mut bytes = String::with_capacity(SONG.len() * 4);
    for (index, byte) in SONG.iter().enumerate() {
        if index > 0 {
            bytes.push(',');
        }
        let _ = write!(bytes, "{byte}");
    }
    let script = format!(
        "new Promise((resolve) => {{ \
           const canvas = document.getElementById('guitarscore'); \
           const file = new File([new Uint8Array([{bytes}])], '{SONG_NAME}'); \
           const transfer = new DataTransfer(); transfer.items.add(file); \
           for (const type of ['dragenter', 'dragover', 'drop']) \
             canvas.dispatchEvent(new DragEvent(type, {{ dataTransfer: transfer, \
               bubbles: true, cancelable: true }})); \
           const started = Date.now(); \
           const key = (type, k, code) => canvas.dispatchEvent(new KeyboardEvent(type, \
             {{ key: k, code, bubbles: true, cancelable: true }})); \
           let typed = false; \
           const poll = () => {{ \
             const copies = Object.values(globalThis.guitarscoreRescue ?? {{}}); \
             const copy = copies.find((c) => c && c.name === '{SONG_NAME}'); \
             if (copy) {{ const b = copy.bytes; \
               return resolve(b[0] === 80 && b[1] === 75 ? 'ok, ' + b.length + ' bytes' \
                 : 'the copy is not a .gp file'); }} \
             if (!typed && Date.now() - started > 1500) {{ \
               typed = true; canvas.focus(); \
               key('keydown', '5', 'Digit5'); key('keyup', '5', 'Digit5'); }} \
             if (Date.now() - started > 15000) \
               return resolve('no rescue copy after an edit (opened and typed: ' + typed + ')'); \
             setTimeout(poll, 100); }}; \
           poll(); }})"
    );
    let result = browser.eval(&script)?;
    if result.starts_with("ok") {
        Ok(result)
    } else {
        Err(result)
    }
}

/// Shows the crash screen (the function the panic hook calls) and checks that it offers
/// the rescue copies for download. Run last on the page: the editor is hidden after it.
pub(super) fn crash_screen_offers_the_copies(browser: &mut Browser) -> Result<(), String> {
    let script = "(() => { const copies = Object.values(globalThis.guitarscoreRescue ?? {}); \
                  globalThis.guitarscoreCrashed('smoke test: a simulated panic'); \
                  const screen = document.getElementById('crashed'); \
                  if (!screen || screen.hidden) return 'the crash screen is not shown'; \
                  const buttons = [...screen.querySelectorAll('#crash-rescue button')]; \
                  if (copies.length === 0) return 'no copies to offer'; \
                  if (buttons.length !== copies.length || \
                      !buttons.every((b, i) => b.textContent.includes(copies[i].name))) \
                    return 'the copies are not offered: ' + screen.textContent; \
                  return 'ok'; })()";
    let result = browser.eval(script)?;
    if result == "ok" { Ok(()) } else { Err(result) }
}
