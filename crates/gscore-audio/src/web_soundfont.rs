//! Finding and downloading the browser build's SoundFont, with progress.
//!
//! The site writes `assets/soundfonts/soundfont.json` (by `cargo xtask web`):
//! `{"url": "...", "name": "...", "bytes": N, "sha256": "..."}`. The URL is versioned, so a
//! new font is a new file. A site assembled by hand may still put `default.sf2` there;
//! without either, playback has no sound and says so.
//!
//! The body is read as a stream so the toolbar can show real progress: a 30 MB font over a
//! slow connection must not look stuck.

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{ReadableStreamDefaultReader, Response};

/// The manifest written by the build.
pub(crate) const MANIFEST_URL: &str = "assets/soundfonts/soundfont.json";
/// A SoundFont a hand-assembled site may provide instead.
pub(crate) const FALLBACK_URL: &str = "assets/soundfonts/default.sf2";
/// A damaged manifest cannot make the browser allocate without bound.
const MAX_SOUNDFONT_BYTES: u64 = 64 * 1024 * 1024;

/// A downloaded SoundFont and what the manifest called it.
pub(crate) struct Loaded {
    /// The file's bytes.
    pub(crate) bytes: Vec<u8>,
    /// The name to show ("GeneralUser GS 2.0.3"), or "SoundFont".
    pub(crate) name: String,
}

/// Downloads the site's SoundFont, calling `progress(done, total)` as bytes arrive.
pub(crate) async fn download(mut progress: impl FnMut(u64, u64)) -> Result<Loaded, String> {
    let (url, name, announced) = manifest()
        .await
        .unwrap_or_else(|| (FALLBACK_URL.to_owned(), "SoundFont".to_owned(), 0));
    // The manifest names a file next to itself. A hand-written site may give a full URL.
    let url = if url.contains('/') {
        url
    } else {
        format!("assets/soundfonts/{url}")
    };
    let response = fetch(&url).await?;
    let total = if announced > 0 {
        announced
    } else {
        content_length(&response).unwrap_or(0)
    };
    if total > MAX_SOUNDFONT_BYTES {
        return Err(format!("the SoundFont is too large ({total} bytes)"));
    }
    progress(0, total);
    let bytes = read_body(&response, total, &mut progress).await?;
    Ok(Loaded { bytes, name })
}

/// The manifest's URL, name and size, if the site has a valid one.
async fn manifest() -> Option<(String, String, u64)> {
    let response = fetch(MANIFEST_URL).await.ok()?;
    let value = JsFuture::from(response.json().ok()?).await.ok()?;
    let object = value.dyn_ref::<js_sys::Object>()?;
    let url = field(object, "url")?.as_string()?;
    let name = field(object, "name")
        .and_then(|value| value.as_string())
        .unwrap_or_else(|| "SoundFont".to_owned());
    let bytes = field(object, "bytes")
        .and_then(|value| value.as_f64())
        .filter(|bytes| bytes.is_finite() && *bytes > 0.0)
        .map(|bytes| u64::try_from(bytes as u128).unwrap_or(0))
        .unwrap_or(0);
    (!url.is_empty()).then_some((url, name, bytes))
}

async fn fetch(url: &str) -> Result<Response, String> {
    let window = web_sys::window().ok_or_else(|| "there is no browser window".to_owned())?;
    let fetched = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|_| format!("cannot fetch {url}"))?;
    let response: Response = fetched.unchecked_into();
    if !response.ok() {
        return Err(format!("the site has no SoundFont ({url})"));
    }
    Ok(response)
}

fn content_length(response: &Response) -> Option<u64> {
    response
        .headers()
        .get("content-length")
        .ok()
        .flatten()
        .and_then(|value| value.parse::<u64>().ok())
}

/// Reads the response body in chunks, reporting progress. A body that pauses (a slow
/// network) simply waits; there is no timer in a wasm task.
async fn read_body(
    response: &Response,
    total: u64,
    progress: &mut impl FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    let Some(body) = response.body() else {
        return Err("the SoundFont response has no body".to_owned());
    };
    let reader: ReadableStreamDefaultReader = body
        .get_reader()
        .dyn_into()
        .map_err(|_| "cannot read the SoundFont stream".to_owned())?;
    let mut bytes: Vec<u8> = Vec::new();
    loop {
        let chunk = JsFuture::from(reader.read())
            .await
            .map_err(|_| "the SoundFont download failed".to_owned())?;
        let object = chunk
            .dyn_ref::<js_sys::Object>()
            .ok_or_else(|| "the SoundFont download sent nonsense".to_owned())?;
        if field(object, "done")
            .and_then(|done| done.as_bool())
            .unwrap_or(false)
        {
            break;
        }
        let value = field(object, "value")
            .ok_or_else(|| "the SoundFont download sent no data".to_owned())?;
        let array: js_sys::Uint8Array = value
            .dyn_into()
            .map_err(|_| "the SoundFont download sent nonsense".to_owned())?;
        let length = usize::try_from(array.length()).unwrap_or(0);
        let start = bytes.len();
        let end = start.saturating_add(length);
        if u64::try_from(end).unwrap_or(u64::MAX) > MAX_SOUNDFONT_BYTES {
            return Err("the SoundFont is too large".to_owned());
        }
        bytes.resize(end, 0);
        if let Some(slice) = bytes.get_mut(start..end) {
            array.copy_to(slice);
        }
        progress(u64::try_from(end).unwrap_or(u64::MAX), total);
    }
    if bytes.is_empty() {
        return Err("the SoundFont is empty".to_owned());
    }
    Ok(bytes)
}

fn field(object: &js_sys::Object, key: &str) -> Option<JsValue> {
    js_sys::Reflect::get(object, &JsValue::from_str(key)).ok()
}
