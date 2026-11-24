//! The command bridge: the one implementation of everything the UI can ask for, with no windows,
//! sound card, sockets or files. The desktop app (`kerf-app`: Tauri commands, `--serve` HTTP)
//! and the browser build (`kerf-web`: WebAssembly) both wrap it, so the UI behaves the same
//! everywhere. The host supplies a [`Transport`] for sound.
//!
//! Factory presets are embedded at build time (see `build.rs`); files are the host's business:
//! it reads text and calls [`Bridge::open_text`], or takes [`Bridge::text`] and saves it.

mod dispatch;
pub mod presets;
mod render;
mod transport;
mod types;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use kerf_core::text::{Diagnostic, Severity, diagnose, parse, print};
use kerf_core::{Band, Catalogue, Command, Patch};
use kerf_engine::samples;
use kerf_gen::StyleInfo;
use kerf_modules::Builtin;
use kerf_session::{Completions, LanguageDocs, Session, Suggest};

pub use dispatch::{args, dispatch, json_of};
pub use transport::{Meter, NoAudio, Scope, Transport};
pub use types::{
    BridgeError, Checked, Description, PresetInfo, RenderSummary, SamplePeaks, State, Suggestions,
    file_error,
};

use types::{BandInfo, JACK_KINDS};

struct Inner {
    session: Session<Builtin>,
    transport: Box<dyn Transport>,
    path: Option<String>,
}

impl Inner {
    fn state(&self) -> State {
        let patch = self.session.patch();
        State {
            patch: patch.clone(),
            text: print(patch),
            can_undo: self.session.can_undo(),
            can_redo: self.session.can_redo(),
            path: self.path.clone(),
            playing: self.transport.is_playing(),
            lineage: self.session.lineage().clone(),
            connected: Vec::new(),
        }
    }

    /// The patch with relative sample paths made absolute against the folder
    /// of the open `.kerf` file, ready for an engine.
    fn engine_patch(&self) -> Patch {
        let mut patch = self.session.patch().clone();
        let dir = self
            .path
            .as_deref()
            .and_then(|p| Path::new(p).parent())
            .filter(|d| !d.as_os_str().is_empty());
        if let Some(dir) = dir {
            samples::resolve_paths(&mut patch, dir);
        }
        patch
    }

    /// Lets the audio side hear an edit that has just been applied.
    fn sync_transport(&mut self) {
        if self.transport.is_playing() {
            let patch = self.engine_patch();
            self.transport.update(&patch);
        }
    }
}

pub struct Bridge {
    inner: Mutex<Inner>,
}

impl Bridge {
    /// An empty patch.
    pub fn new(transport: Box<dyn Transport>) -> Self {
        Self {
            inner: Mutex::new(Inner {
                session: Session::new(Builtin),
                transport,
                path: None,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panic mid-command cannot leave the session half-edited: `apply`
        // swaps the patch in whole.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn get_state(&self) -> State {
        self.lock().state()
    }

    /// What an editor needs to complete `.kerf` text.
    pub fn completions(&self) -> Completions {
        kerf_session::completions(&Builtin)
    }

    /// The language guide: hand-written sections, then the module reference.
    pub fn language_docs(&self) -> LanguageDocs {
        kerf_session::language_docs(&Builtin)
    }

    /// Checks `.kerf` text without applying it.
    pub fn check_text(&self, text: &str) -> Checked {
        let mut diagnostics = diagnose(&Builtin, text);
        if diagnostics.iter().all(|d| d.severity != Severity::Error) {
            // The editor's checks stop short of the session's; anything the
            // session would still refuse is reported at the top of the text.
            let refused = parse(text).map_err(|e| e.to_string()).and_then(|p| {
                Session::open(Builtin, p)
                    .map(drop)
                    .map_err(|e| e.to_string())
            });
            if let Err(message) = refused {
                diagnostics.insert(
                    0,
                    Diagnostic {
                        line: 1,
                        col: 1,
                        end_col: 1,
                        severity: Severity::Error,
                        message,
                    },
                );
            }
        }
        Checked {
            ok: diagnostics.iter().all(|d| d.severity != Severity::Error),
            diagnostics,
        }
    }

    /// The cables auto-patching would add; nothing is changed.
    pub fn suggest_cables(&self, q: &Suggest) -> Result<Suggestions, BridgeError> {
        let cables = self.lock().session.suggest_cables(q)?;
        Ok(Suggestions { cables })
    }

    /// The generator's styles, for the style picker.
    pub fn list_styles(&self) -> Vec<StyleInfo> {
        kerf_gen::styles()
    }

    pub fn describe(&self) -> Description {
        Description {
            bands: Band::ALL
                .iter()
                .map(|&id| BandInfo {
                    id,
                    name: id.name(),
                    summary: id.summary(),
                })
                .collect(),
            jack_kinds: &JACK_KINDS,
            modules: Builtin.all(),
        }
    }

    /// Applies one command; on error nothing changes.
    pub fn apply(&self, cmd: Command) -> Result<State, BridgeError> {
        let mut g = self.lock();
        g.session.apply(cmd)?;
        g.sync_transport();
        let mut state = g.state();
        state.connected = g.session.last_plan().to_vec();
        Ok(state)
    }

    /// Applies `cmd` to a copy of the patch and returns the result, letting a
    /// knob drag or the morph fader be seen (and, while playing, heard) as it
    /// moves. The session and its undo history are untouched; the gesture ends
    /// with one real command through `apply`.
    pub fn preview(&self, cmd: Command) -> Result<Patch, BridgeError> {
        let mut g = self.lock();
        let mut scratch = Session::open(Builtin, g.session.patch().clone())?;
        scratch.apply(cmd)?;
        if g.transport.is_playing() {
            g.transport.update(scratch.patch());
        }
        Ok(scratch.patch().clone())
    }

    /// Starts a session from `.kerf` text. `source` names it in error messages; `path` is
    /// the file it came from (`None` for an unsaved patch, so Save asks where to put it).
    pub fn open_text(
        &self,
        source: &str,
        text: &str,
        path: Option<String>,
    ) -> Result<State, BridgeError> {
        let patch = parse(text).map_err(|e| BridgeError::Parse {
            path: source.to_owned(),
            source: e,
        })?;
        let session = Session::open(Builtin, patch)?;
        let mut g = self.lock();
        g.session = session;
        g.path = path;
        g.sync_transport();
        Ok(g.state())
    }

    /// The patch as `.kerf` text, for the host to save.
    pub fn text(&self) -> String {
        print(self.lock().session.patch())
    }

    /// The file the patch was opened from or last saved to.
    pub fn path(&self) -> Option<String> {
        self.lock().path.clone()
    }

    /// Records where the host saved the patch.
    pub fn set_path(&self, path: Option<String>) -> State {
        let mut g = self.lock();
        g.path = path;
        g.state()
    }

    pub fn transport_play(&self) -> Result<State, BridgeError> {
        let mut g = self.lock();
        let patch = g.engine_patch();
        g.transport.play(&patch).map_err(BridgeError::Audio)?;
        Ok(g.state())
    }

    pub fn transport_stop(&self) -> State {
        let mut g = self.lock();
        g.transport.stop();
        g.state()
    }

    pub fn meter(&self) -> Meter {
        self.lock().transport.meter()
    }

    /// The latest output samples for the oscilloscope and spectrum.
    pub fn scope(&self) -> Scope {
        self.lock().transport.scope()
    }
}
