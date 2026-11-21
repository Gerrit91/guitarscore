//! Keeps the screen on while a song plays in the browser (M25), through the Screen Wake
//! Lock API: a phone on a music stand must not dim and lock in the middle of a loop.
//!
//! The browser gives the lock up by itself when the page is hidden (another app, the
//! screen switched off by hand), so it is asked for again once the page is visible and
//! still playing. Browsers without the API (or refusing it: no HTTPS, low battery) are
//! left alone; the screen then follows the device's own settings, as before.

use std::cell::RefCell;

use wasm_bindgen::{JsCast, JsValue};

/// Seconds between two requests, so a browser that refuses is not asked every frame.
const RETRY: f64 = 5.0;

/// The lock the page holds, if any.
#[derive(Default)]
struct Lock {
    /// The browser's `WakeLockSentinel`.
    sentinel: Option<JsValue>,
    /// A request is on its way.
    requesting: bool,
    /// When the last request was made (egui time, seconds).
    asked_at: Option<f64>,
}

thread_local! {
    static LOCK: RefCell<Lock> = RefCell::new(Lock::default());
}

/// Holds the screen on while `wanted`, and lets it go otherwise. Called every frame;
/// `now` is egui's time in seconds.
pub(super) fn keep_screen_on(wanted: bool, now: f64) {
    LOCK.with(|lock| {
        let mut lock = lock.borrow_mut();
        // A lock the browser released (the page was hidden) is gone.
        if lock.sentinel.as_ref().is_some_and(released) {
            lock.sentinel = None;
        }
        if !wanted {
            if let Some(sentinel) = lock.sentinel.take() {
                release(&sentinel);
            }
            return;
        }
        if lock.sentinel.is_some() || lock.requesting || !page_visible() {
            return;
        }
        if lock.asked_at.is_some_and(|at| now - at < RETRY) {
            return;
        }
        lock.asked_at = Some(now);
        let Some(promise) = request() else {
            return;
        };
        lock.requesting = true;
        wasm_bindgen_futures::spawn_local(async move {
            let result = wasm_bindgen_futures::JsFuture::from(promise).await;
            LOCK.with(|lock| {
                let mut lock = lock.borrow_mut();
                lock.requesting = false;
                if let Ok(sentinel) = result {
                    lock.sentinel = Some(sentinel);
                }
            });
        });
    });
}

/// `navigator.wakeLock.request("screen")`, if the browser has it.
fn request() -> Option<js_sys::Promise> {
    let navigator = js_sys::Reflect::get(&js_sys::global(), &"navigator".into()).ok()?;
    let wake_lock = js_sys::Reflect::get(&navigator, &"wakeLock".into()).ok()?;
    if !wake_lock.is_object() {
        return None;
    }
    let request = js_sys::Reflect::get(&wake_lock, &"request".into())
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;
    request
        .call1(&wake_lock, &JsValue::from_str("screen"))
        .ok()?
        .dyn_into::<js_sys::Promise>()
        .ok()
}

/// Whether the browser released `sentinel` (its `released` flag).
fn released(sentinel: &JsValue) -> bool {
    js_sys::Reflect::get(sentinel, &"released".into())
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(true)
}

/// Gives the lock back (`sentinel.release()`); its promise is not waited for.
fn release(sentinel: &JsValue) {
    if let Ok(function) = js_sys::Reflect::get(sentinel, &"release".into())
        .and_then(|value| value.dyn_into::<js_sys::Function>().map_err(JsValue::from))
    {
        let _ = function.call0(sentinel);
    }
}

/// Whether the page is shown (`document.visibilityState`): a hidden page cannot hold
/// the lock.
fn page_visible() -> bool {
    js_sys::Reflect::get(&js_sys::global(), &"document".into())
        .and_then(|document| js_sys::Reflect::get(&document, &"visibilityState".into()))
        .ok()
        .and_then(|state| state.as_string())
        .is_none_or(|state| state == "visible")
}
