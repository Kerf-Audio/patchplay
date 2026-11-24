//! The commands by name, with their arguments as a JSON object: what the `--serve` endpoints and
//! the browser build's `call(name, json)` both run. Commands that need the host (open or save a
//! file, render to a file) are the host's own.

use kerf_core::Command;
use kerf_session::Suggest;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{Bridge, BridgeError};

#[derive(Deserialize)]
struct TextArgs {
    text: String,
}

#[derive(Deserialize)]
struct ApplyArgs {
    cmd: Command,
}

#[derive(Deserialize)]
struct ModuleArgs {
    module: String,
}

/// Reads a command's arguments.
pub fn args<T: DeserializeOwned>(v: Value) -> Result<T, BridgeError> {
    serde_json::from_value(v).map_err(|e| BridgeError::Invalid(format!("bad arguments: {e}")))
}

/// A bridge result as JSON.
pub fn json_of(v: impl serde::Serialize) -> Value {
    serde_json::to_value(v).expect("bridge results serialise")
}

/// Runs one command by name. `None` when no such command exists here.
pub fn dispatch(bridge: &Bridge, name: &str, a: Value) -> Option<Result<Value, BridgeError>> {
    Some(match name {
        "get_state" => Ok(json_of(bridge.get_state())),
        "describe" => Ok(json_of(bridge.describe())),
        "list_presets" => Ok(json_of(bridge.list_presets())),
        "list_styles" => Ok(json_of(bridge.list_styles())),
        "completions" => Ok(json_of(bridge.completions())),
        "language_docs" => Ok(json_of(bridge.language_docs())),
        "check_text" => args(a).map(|t: TextArgs| json_of(bridge.check_text(&t.text))),
        "suggest_cables" => args(a).and_then(|q: Suggest| bridge.suggest_cables(&q).map(json_of)),
        "meter" => Ok(json_of(bridge.meter())),
        "scope" => Ok(json_of(bridge.scope())),
        "transport_play" => bridge.transport_play().map(json_of),
        "transport_stop" => Ok(json_of(bridge.transport_stop())),
        "apply" => args(a).and_then(|ApplyArgs { cmd }| bridge.apply(cmd).map(json_of)),
        "preview" => args(a).and_then(|ApplyArgs { cmd }| bridge.preview(cmd).map(json_of)),
        "sample_peaks" => {
            args(a).and_then(|p: ModuleArgs| bridge.sample_peaks(&p.module).map(json_of))
        }
        "demo_sample_peaks" => Ok(json_of(Bridge::demo_sample_peaks())),
        _ => return None,
    })
}
