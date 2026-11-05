//! Playback in a browser: the synthesizer runs in an `AudioWorklet` on the Web Audio
//! thread. This is the `Player` the app talks to; it creates the worklet node and forwards
//! commands through its message port.
//!
//! The worklet is a separate wasm module (`crates/gscore-worklet`, built by `cargo xtask
//! web`) because an AudioWorklet cannot share the app's instance. The song crosses as the
//! wire format in `gscore_play::encode_sequence`.
//!
//! The browser may hold audio back until the user interacts with the page; Play is a key
//! press, so resuming the context there is allowed. The SoundFont is fetched from
//! `assets/soundfonts/default.sf2`; while it loads the player reports
//! [`WebPlayer::is_loading`] (the worklet keeps a Play pressed meanwhile), and a site
//! without one reports why playback is unavailable. Editing works either way.

use std::cell::Cell;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gscore_play::{MidiEvent, PlayError, PlayState, Player, Sequence, encode_sequence};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioContext, AudioWorkletNode, MessageEvent, MessagePort};

/// Where the site can put a SoundFont for the browser synth.
const SOUNDFONT_URL: &str = "assets/soundfonts/default.sf2";

/// What the worklet reports back.
#[derive(Debug)]
struct Shared {
    position: RefCell<u64>,
    state: RefCell<PlayState>,
    error: RefCell<Option<String>>,
    /// The SoundFont is still being fetched or loaded into the worklet.
    loading: Cell<bool>,
    /// The node must stay alive, or the browser disconnects it.
    node: RefCell<Option<AudioWorkletNode>>,
    port: RefCell<Option<MessagePort>>,
    /// Messages sent before the worklet was ready.
    queue: RefCell<Vec<JsValue>>,
}

impl Default for Shared {
    fn default() -> Self {
        Shared {
            position: RefCell::new(0),
            state: RefCell::new(PlayState::Stopped),
            error: RefCell::new(None),
            loading: Cell::new(true),
            node: RefCell::new(None),
            port: RefCell::new(None),
            queue: RefCell::new(Vec::new()),
        }
    }
}

impl Shared {
    fn report(&self, error: String) {
        self.loading.set(false);
        *self.state.borrow_mut() = PlayState::Stopped;
        *self.error.borrow_mut() = Some(error);
    }

    fn post(&self, message: JsValue) {
        match self.port.borrow().as_ref() {
            Some(port) => {
                let _ = port.post_message(&message);
            }
            None => self.queue.borrow_mut().push(message),
        }
    }
}

/// Plays sequences through the browser's AudioWorklet synthesizer.
#[derive(Debug)]
pub struct WebPlayer {
    shared: Rc<Shared>,
    context: AudioContext,
    sequence: Option<Arc<Sequence>>,
}

impl WebPlayer {
    /// Starts the browser audio output. The worklet module and an optional SoundFont load
    /// in the background; commands sent meanwhile wait in order.
    pub fn start() -> Result<Self, PlayError> {
        let context = AudioContext::new()
            .map_err(|_| PlayError::Device("the browser blocked audio output".to_owned()))?;
        let shared = Rc::new(Shared::default());
        let player = WebPlayer {
            shared: Rc::clone(&shared),
            context: context.clone(),
            sequence: None,
        };
        start_worklet(context, shared);
        Ok(player)
    }

    /// Why playback is unavailable, if it is.
    pub fn error(&self) -> Option<String> {
        self.shared.error.borrow().clone()
    }

    /// Whether the worklet's SoundFont is still loading (fetched, then parsed in the
    /// worklet). Commands sent meanwhile take effect once it is loaded.
    pub fn is_loading(&self) -> bool {
        self.shared.loading.get()
    }

    /// A short description of the audio output, for the toolbar.
    pub fn device(&self) -> Option<String> {
        Some("Browser audio (AudioWorklet)".to_owned())
    }

    fn send(&self, message: JsValue) {
        self.shared.post(message);
    }

    fn command(&self, name: &str, value: Option<f64>) -> Result<(), PlayError> {
        if let Some(error) = self.error() {
            return Err(PlayError::Device(error));
        }
        let message = object("command");
        set(&message, "name", &JsValue::from_str(name));
        if let Some(value) = value {
            set(&message, "value", &JsValue::from_f64(value));
        }
        self.send(message.into());
        Ok(())
    }

    fn load_sequence(&mut self, sequence: Arc<Sequence>, replace: bool) -> Result<(), PlayError> {
        if let Some(error) = self.error() {
            return Err(PlayError::Device(error));
        }
        let bytes = encode_sequence(&sequence);
        let array = js_sys::Uint8Array::from(&bytes[..]);
        let message = object(if replace { "replace" } else { "sequence" });
        set(&message, "bytes", &array.buffer().into());
        self.send(message.into());
        self.sequence = Some(sequence);
        Ok(())
    }
}

/// Creates the audio graph off the UI thread's stack and flushes what waited for it.
fn start_worklet(context: AudioContext, shared: Rc<Shared>) {
    wasm_bindgen_futures::spawn_local(async move {
        let result = async {
            let worklet = context
                .audio_worklet()
                .map_err(|_| "the browser has no AudioWorklet".to_owned())?;
            let added = worklet
                .add_module("worklet.js")
                .map_err(|_| "cannot start the AudioWorklet".to_owned())?;
            JsFuture::from(added)
                .await
                .map_err(|_| "the AudioWorklet module did not load".to_owned())?;
            let node = AudioWorkletNode::new(&context, "gscore-synth")
                .map_err(|_| "cannot create the synthesizer node".to_owned())?;
            let port = node
                .port()
                .map_err(|_| "the synthesizer node has no port".to_owned())?;
            let handler = {
                let shared = Rc::clone(&shared);
                Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
                    handle_message(&shared, event.data());
                })
            };
            port.set_onmessage(Some(handler.as_ref().unchecked_ref()));
            // The handler lives as long as the node; the browser keeps it.
            handler.forget();
            let destination = context.destination();
            node.connect_with_audio_node(&destination)
                .map_err(|_| "cannot connect the synthesizer to the speakers".to_owned())?;
            *shared.port.borrow_mut() = Some(port.clone());
            *shared.node.borrow_mut() = Some(node);
            // An AudioWorklet cannot fetch: compile the module here and send it first, so
            // the worklet can create the synthesizer before any queued command arrives.
            match load_worklet_module().await {
                Ok(module) => shared.post(module),
                Err(error) => {
                    shared.report(error);
                    return Ok(());
                }
            }
            for message in shared.queue.borrow_mut().drain(..) {
                let _ = port.post_message(&message);
            }
            Ok::<(), String>(())
        }
        .await;
        if let Err(error) = result {
            shared.report(error);
            return;
        }
        // Without a SoundFont there is no sound: the reason is reported on Play.
        if let Err(error) = fetch_soundfont(&shared).await {
            shared.report(error);
        }
    });
}

/// Fetches the worklet's wasm module and compiles it on this thread; the worklet gets the
/// compiled `WebAssembly.Module` (an AudioWorklet cannot fetch).
async fn load_worklet_module() -> Result<JsValue, String> {
    let window = web_sys::window().ok_or_else(|| "there is no browser window".to_owned())?;
    let fetched = JsFuture::from(window.fetch_with_str("worklet_synth_bg.wasm"))
        .await
        .map_err(|_| "cannot fetch the browser synthesizer".to_owned())?;
    let response: web_sys::Response = fetched.unchecked_into();
    if !response.ok() {
        return Err("the browser synthesizer module is missing from the site".to_owned());
    }
    let buffer = response
        .array_buffer()
        .map_err(|_| "cannot read the browser synthesizer".to_owned())?;
    let buffer = JsFuture::from(buffer)
        .await
        .map_err(|_| "cannot read the browser synthesizer".to_owned())?;
    let compiled = js_sys::WebAssembly::compile(&buffer);
    let module = JsFuture::from(compiled)
        .await
        .map_err(|_| "the browser synthesizer failed to compile".to_owned())?;
    let message = object("module");
    set(&message, "module", &module);
    Ok(message.into())
}

/// Fetches the site's SoundFont and hands it to the worklet, which answers `loaded`.
async fn fetch_soundfont(shared: &Shared) -> Result<(), String> {
    let missing = || {
        format!(
            "no SoundFont on this site ({SOUNDFONT_URL}): the browser build cannot play \
             sound yet; editing works"
        )
    };
    let window = web_sys::window().ok_or_else(|| "there is no browser window".to_owned())?;
    let response = JsFuture::from(window.fetch_with_str(SOUNDFONT_URL))
        .await
        .map_err(|_| missing())?;
    let response: web_sys::Response = response.unchecked_into();
    if !response.ok() {
        return Err(missing());
    }
    let buffer = response
        .array_buffer()
        .map_err(|_| "cannot read the SoundFont".to_owned())?;
    let buffer = JsFuture::from(buffer)
        .await
        .map_err(|_| "cannot read the SoundFont".to_owned())?;
    let message = object("soundfont");
    set(&message, "bytes", &buffer);
    shared.post(message.into());
    Ok(())
}

fn handle_message(shared: &Shared, data: JsValue) {
    let Some(object) = data.dyn_ref::<js_sys::Object>() else {
        return;
    };
    let kind = get(object, "type").and_then(|value| value.as_string());
    match kind.as_deref() {
        Some("status") => {
            if let Some(position) = get(object, "position").and_then(|value| value.as_f64()) {
                let position = if position.is_finite() && position > 0.0 {
                    u64::try_from(position as u128).unwrap_or(u64::MAX)
                } else {
                    0
                };
                *shared.position.borrow_mut() = position;
            }
            let state = get(object, "state").and_then(|value| value.as_f64());
            *shared.state.borrow_mut() = match state {
                Some(1.0) => PlayState::Playing,
                Some(2.0) => PlayState::Paused,
                _ => PlayState::Stopped,
            };
        }
        Some("loaded") => shared.loading.set(false),
        Some("error") => {
            let message = get(object, "message")
                .and_then(|value| value.as_string())
                .unwrap_or_else(|| "the browser synthesizer failed".to_owned());
            shared.report(message);
        }
        _ => {}
    }
}

fn object(kind: &str) -> js_sys::Object {
    let object = js_sys::Object::new();
    set(&object, "type", &JsValue::from_str(kind));
    object
}

fn set(object: &js_sys::Object, key: &str, value: &JsValue) {
    let _ = js_sys::Reflect::set(object, &JsValue::from_str(key), value);
}

fn get(object: &js_sys::Object, key: &str) -> Option<JsValue> {
    js_sys::Reflect::get(object, &JsValue::from_str(key)).ok()
}

impl Drop for WebPlayer {
    /// A dropped player (a failed one is started afresh) closes its audio context:
    /// browsers allow only a few at a time.
    fn drop(&mut self) {
        if let Some(port) = self.shared.port.borrow_mut().take() {
            port.set_onmessage(None);
        }
        if let Some(node) = self.shared.node.borrow_mut().take() {
            let _ = node.disconnect();
        }
        let _ = self.context.close();
    }
}

impl Player for WebPlayer {
    fn load(&mut self, sequence: Arc<Sequence>) -> Result<(), PlayError> {
        self.load_sequence(sequence, false)
    }

    fn play(&mut self) -> Result<(), PlayError> {
        // Allowed here: Play is a key press, the browser's gesture requirement.
        let context = self.context.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(promise) = context.resume() {
                let _ = JsFuture::from(promise).await;
            }
        });
        self.command("play", None)
    }

    fn pause(&mut self) -> Result<(), PlayError> {
        self.command("pause", None)
    }

    fn stop(&mut self) -> Result<(), PlayError> {
        self.command("stop", None)
    }

    fn seek(&mut self, tick: u64) -> Result<(), PlayError> {
        self.command("seek", Some(tick as f64))
    }

    fn set_tempo_scale(&mut self, scale: f32) -> Result<(), PlayError> {
        self.command("tempo", Some(f64::from(scale)))
    }

    fn send_event(&mut self, event: MidiEvent) -> Result<(), PlayError> {
        let (status, data1, data2) = match event {
            MidiEvent::NoteOn {
                channel,
                key,
                velocity,
            } => (0x90 | channel.get(), key, velocity),
            MidiEvent::NoteOff { channel, key } => (0x80 | channel.get(), key, 0),
            MidiEvent::ProgramChange { channel, program } => (0xC0 | channel.get(), program, 0),
            MidiEvent::Control {
                channel,
                controller,
                value,
            } => (0xB0 | channel.get(), controller, value),
            MidiEvent::PitchBend { channel, value } => {
                let value = value.min(0x3FFF);
                let lsb = u8::try_from(value & 0x7F).unwrap_or(0);
                let msb = u8::try_from(value >> 7).unwrap_or(0x40);
                (0xE0 | channel.get(), lsb, msb)
            }
        };
        let message = object("midi");
        set(&message, "status", &JsValue::from_f64(f64::from(status)));
        set(&message, "data1", &JsValue::from_f64(f64::from(data1)));
        set(&message, "data2", &JsValue::from_f64(f64::from(data2)));
        self.send(message.into());
        Ok(())
    }

    fn replace(&mut self, sequence: Arc<Sequence>) -> Result<(), PlayError> {
        self.load_sequence(sequence, true)
    }

    fn position(&self) -> u64 {
        *self.shared.position.borrow()
    }

    fn state(&self) -> PlayState {
        *self.shared.state.borrow()
    }
}
