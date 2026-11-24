//! patchplay in the browser. One WebAssembly module, two roles:
//!
//! * On the page, `call(name, json)` runs the shared bridge (`kerf-bridge`): the same commands
//!   the desktop app has, in one session. Playing queues the patch as `.kerf` text for the
//!   audio worklet (`take_audio_update`); the worklet reports levels back (`set_levels`).
//! * In the AudioWorklet, `EngineHandle` runs the engine headless, 128 frames at a time.
//!
//! Files, dialogs and the audio device belong to the page; see `ui/src/web/`.

use std::sync::{Arc, Mutex, PoisonError};

use kerf_bridge::{Bridge, Meter, RenderSummary, Scope, Transport};
use kerf_core::Patch;
use kerf_core::text::{parse, print};
use wasm_bindgen::prelude::*;

mod engine;
pub use engine::EngineHandle;

#[derive(Default)]
struct Shared {
    playing: bool,
    /// Messages for the worklet, oldest first: `play`, `update` or `stop`, with `.kerf` text.
    outbox: Vec<(&'static str, String)>,
    meter: Meter,
    scope: Scope,
}

struct WebTransport(Arc<Mutex<Shared>>);

impl WebTransport {
    fn shared(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Transport for WebTransport {
    fn play(&mut self, patch: &Patch) -> Result<(), String> {
        let mut s = self.shared();
        s.playing = true;
        s.outbox.clear();
        s.outbox.push(("play", print(patch)));
        Ok(())
    }

    fn update(&mut self, patch: &Patch) {
        let mut s = self.shared();
        let text = print(patch);
        match s.outbox.last_mut() {
            Some((kind, queued)) if *kind == "update" => *queued = text,
            _ => s.outbox.push(("update", text)),
        }
    }

    fn stop(&mut self) {
        let mut s = self.shared();
        s.playing = false;
        s.outbox.clear();
        s.outbox.push(("stop", String::new()));
    }

    fn is_playing(&self) -> bool {
        self.shared().playing
    }

    fn meter(&self) -> Meter {
        self.shared().meter
    }

    fn scope(&self) -> Scope {
        self.shared().scope.clone()
    }
}

struct App {
    bridge: Bridge,
    shared: Arc<Mutex<Shared>>,
    last_render: Mutex<Option<RenderSummary>>,
}

thread_local! {
    static APP: App = {
        let shared = Arc::new(Mutex::new(Shared::default()));
        App {
            bridge: Bridge::new(Box::new(WebTransport(Arc::clone(&shared)))),
            shared,
            last_render: Mutex::new(None),
        }
    };
}

fn text_arg(v: &serde_json::Value, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::to_owned)
        .ok_or_else(|| format!("bad arguments: `{key}` is missing"))
}

/// Runs a bridge command by name with its arguments as JSON; answers JSON. A rejected command
/// throws its message. Besides the shared commands: `open_text {name, text}`, `open_preset
/// {name}` and `set_path {path}`, which the page uses for files.
#[wasm_bindgen]
pub fn call(name: &str, args: &str) -> Result<String, String> {
    let a: serde_json::Value = if args.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(args).map_err(|e| format!("body is not JSON: {e}"))?
    };
    APP.with(|app| {
        let b = &app.bridge;
        let result = match name {
            "open_text" => {
                let (n, text) = (text_arg(&a, "name")?, text_arg(&a, "text")?);
                b.open_text(&n, &text, Some(n.clone()))
                    .map(kerf_bridge::json_of)
            }
            "open_preset" => b
                .open_preset(&text_arg(&a, "name")?)
                .map(kerf_bridge::json_of),
            "set_path" => Ok(kerf_bridge::json_of(
                b.set_path(a.get("path").and_then(|p| p.as_str()).map(str::to_owned)),
            )),
            _ => kerf_bridge::dispatch(b, name, a)
                .ok_or_else(|| format!("no such command {name}"))?,
        };
        result.map(|v| v.to_string()).map_err(|e| e.to_string())
    })
}

/// Renders the patch offline to WAV bytes; `render_summary` then answers the summary as JSON.
#[wasm_bindgen]
pub fn render_wav(seconds: f32) -> Result<Vec<u8>, String> {
    APP.with(|app| {
        let (bytes, summary) = app
            .bridge
            .render_wav_bytes(seconds)
            .map_err(|e| e.to_string())?;
        *app.last_render
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(summary);
        Ok(bytes)
    })
}

#[wasm_bindgen]
pub fn render_summary() -> String {
    APP.with(|app| {
        let last = app
            .last_render
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        last.as_ref()
            .map_or_else(String::new, |s| kerf_bridge::json_of(s).to_string())
    })
}

/// Hands over decoded mono audio under a pseudo path such as `web:kick.wav`.
#[wasm_bindgen]
pub fn provide_sample(name: &str, mono: &[f32], rate: u32) -> Result<(), String> {
    kerf_engine::samples::provide(name, mono, rate).map_err(|e| e.to_string())
}

#[wasm_bindgen]
pub fn forget_sample(name: &str) {
    kerf_engine::samples::forget(name);
}

/// The worklet's report, about 30 times a second.
#[wasm_bindgen]
pub fn set_levels(peak_l: f32, peak_r: f32, load: f32) {
    APP.with(|app| {
        let mut s = app.shared.lock().unwrap_or_else(PoisonError::into_inner);
        s.meter = Meter {
            peak_l,
            peak_r,
            load,
        };
    });
}

/// The worklet's latest output samples (it sends them only while the page asks for the scope).
#[wasm_bindgen]
pub fn set_scope(samples: &[f32], rate: u32) {
    APP.with(|app| {
        let mut s = app.shared.lock().unwrap_or_else(PoisonError::into_inner);
        s.scope.rate = rate;
        s.scope.samples.clear();
        s.scope.samples.extend_from_slice(samples);
    });
}

/// The next message for the worklet as `kind\n.kerf text` (`play`, `update` or `stop`); empty
/// when there is none.
#[wasm_bindgen]
pub fn take_audio_update() -> String {
    APP.with(|app| {
        let mut s = app.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if s.outbox.is_empty() {
            return String::new();
        }
        let (kind, text) = s.outbox.remove(0);
        format!("{kind}\n{text}")
    })
}

pub(crate) fn parse_patch(text: &str) -> Result<Patch, String> {
    parse(text).map_err(|e| e.to_string())
}
