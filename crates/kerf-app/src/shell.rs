//! The Tauri shell: one thin command per bridge call, with the same names and
//! arguments as the `--serve` endpoints.

use std::path::Path;
use std::sync::Arc;

use kerf_core::{Command, ModuleId, PartId, Patch};
use kerf_gen::StyleInfo;
use kerf_session::{Completions, LanguageDocs, Suggest};
use tauri::Manager;

use crate::audio;
use crate::bridge::{
    Bridge, BridgeError, Checked, Description, PresetInfo, RenderSummary, SamplePeaks, State,
    Suggestions,
};
use crate::transport::{Meter, Scope};

type Shared<'a> = tauri::State<'a, Arc<Bridge>>;

// `async` makes Tauri run a command on its thread pool instead of the main thread. The ones
// marked so can take a while (Random and Evolve render audio to check what they made), and
// would freeze the window and every poll queued behind them.

#[tauri::command]
fn get_state(bridge: Shared) -> State {
    bridge.get_state()
}

#[tauri::command]
fn describe(bridge: Shared) -> Description {
    bridge.describe()
}

#[tauri::command]
fn list_presets(bridge: Shared) -> Vec<PresetInfo> {
    bridge.list_presets()
}

#[tauri::command]
fn list_styles(bridge: Shared) -> Vec<StyleInfo> {
    bridge.list_styles()
}

#[tauri::command]
fn completions(bridge: Shared) -> Completions {
    bridge.completions()
}

#[tauri::command]
fn language_docs(bridge: Shared) -> LanguageDocs {
    bridge.language_docs()
}

#[tauri::command]
fn check_text(bridge: Shared, text: String) -> Checked {
    bridge.check_text(&text)
}

#[tauri::command]
fn suggest_cables(
    bridge: Shared,
    kind: Option<String>,
    module: Option<ModuleId>,
    near: Option<ModuleId>,
    part: Option<PartId>,
) -> Result<Suggestions, BridgeError> {
    bridge.suggest_cables(&Suggest {
        kind,
        module,
        near,
        part,
    })
}

#[tauri::command(async)]
fn apply(bridge: Shared, cmd: Command) -> Result<State, BridgeError> {
    bridge.apply(cmd)
}

#[tauri::command(async)]
fn preview(bridge: Shared, cmd: Command) -> Result<Patch, BridgeError> {
    bridge.preview(cmd)
}

#[tauri::command(async)]
fn open_file(bridge: Shared, path: String) -> Result<State, BridgeError> {
    bridge.open_file(Path::new(&path))
}

#[tauri::command(async)]
fn save_file(bridge: Shared, path: Option<String>) -> Result<State, BridgeError> {
    bridge.save_file(path.as_deref().map(Path::new))
}

#[tauri::command(async)]
fn transport_play(bridge: Shared) -> Result<State, BridgeError> {
    bridge.transport_play()
}

#[tauri::command]
fn transport_stop(bridge: Shared) -> State {
    bridge.transport_stop()
}

#[tauri::command]
fn meter(bridge: Shared) -> Meter {
    bridge.meter()
}

#[tauri::command]
fn scope(bridge: Shared) -> Scope {
    bridge.scope()
}

#[tauri::command(async)]
fn render_wav(bridge: Shared, path: String, seconds: f32) -> Result<RenderSummary, BridgeError> {
    bridge.render_wav(Path::new(&path), seconds)
}

#[tauri::command]
fn sample_peaks(bridge: Shared, module: String) -> Result<SamplePeaks, BridgeError> {
    bridge.sample_peaks(&module)
}

#[tauri::command]
fn demo_sample_peaks() -> SamplePeaks {
    kerf_bridge::Bridge::demo_sample_peaks()
}

pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.manage(Arc::new(Bridge::new(audio::transport())));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            describe,
            list_presets,
            list_styles,
            completions,
            language_docs,
            check_text,
            suggest_cables,
            apply,
            preview,
            open_file,
            save_file,
            transport_play,
            transport_stop,
            meter,
            scope,
            render_wav,
            sample_peaks,
            demo_sample_peaks,
        ])
        .run(tauri::generate_context!())
}
