//! The loading side, shared by the editor, the background tasks and `initialize`: owns the
//! session (through the bridge), builds engines at the host's sample rate and hands them to the
//! audio thread through a lock-free queue. Nothing here runs on the audio thread.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use kerf_bridge::{Bridge, NoAudio, PresetInfo, presets};
use kerf_core::Patch;
use kerf_engine::Engine;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::doc::{PatchCell, PatchDoc};
use crate::rack::{Built, Rack, Targets};

/// The factory preset a new instance starts from. If it is ever removed, the first preset
/// stands in.
pub const DEFAULT_PRESET: &str = "plain-sawn";

/// Engines in flight to the audio thread.
const INBOX: usize = 8;
const RETIRED: usize = 64;
const BUSY: &str = "the audio thread has not caught up yet; try again";

struct Queues {
    to_audio: Producer<Built>,
    retired: Consumer<Box<Engine>>,
}

/// Values the patch itself holds for the host-facing controls.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PatchControls {
    pub macros: Option<[f32; 4]>,
    pub level: Option<f32>,
}

pub struct Shared {
    bridge: Bridge,
    cell: Arc<PatchCell>,
    queues: Mutex<Queues>,
    sample_rate: AtomicU32,
    status: Mutex<String>,
    /// Patches loaded so far, and how many of them the editor has moved the host parameters to.
    loaded: AtomicU32,
    synced: AtomicU32,
    /// Published by the audio thread for the editor.
    pub running: AtomicBool,
    pub host_tempo: AtomicU32,
    pub peak: AtomicU32,
    /// Facts about the loaded patch, for the editor.
    pub has_jig: AtomicBool,
    pub has_finish: AtomicBool,
    pub patch_tempo: AtomicU32,
}

impl Shared {
    /// The loader and the rack it feeds.
    pub fn new(cell: Arc<PatchCell>) -> (Arc<Self>, Rack) {
        let (to_audio, inbox) = RingBuffer::new(INBOX);
        let (retire_tx, retired) = RingBuffer::new(RETIRED);
        let shared = Arc::new(Self {
            bridge: Bridge::new(Box::new(NoAudio("the host plays the patch".to_owned()))),
            cell,
            queues: Mutex::new(Queues { to_audio, retired }),
            sample_rate: AtomicU32::new(48_000f32.to_bits()),
            status: Mutex::new(String::new()),
            loaded: AtomicU32::new(0),
            synced: AtomicU32::new(0),
            running: AtomicBool::new(false),
            host_tempo: AtomicU32::new(0),
            peak: AtomicU32::new(0),
            has_jig: AtomicBool::new(false),
            has_finish: AtomicBool::new(false),
            patch_tempo: AtomicU32::new(0),
        });
        (shared, Rack::new(inbox, retire_tx))
    }

    pub fn set_sample_rate(&self, sample_rate: f32) {
        self.sample_rate
            .store(sample_rate.to_bits(), Ordering::Relaxed);
    }

    fn sample_rate(&self) -> f32 {
        f32::from_bits(self.sample_rate.load(Ordering::Relaxed))
    }

    pub fn cell(&self) -> &PatchCell {
        &self.cell
    }

    pub fn status(&self) -> String {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Shows `message` in the editor.
    pub fn report(&self, message: String) {
        self.set_status(message);
    }

    fn set_status(&self, message: String) {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = message;
    }

    /// If a patch has been loaded since the host parameters last took its values, counts them as
    /// taken and says so: the caller then sets the parameters from `patch_controls`.
    pub fn take_unsynced(&self) -> bool {
        let loaded = self.loaded.load(Ordering::Acquire);
        self.synced.swap(loaded, Ordering::Relaxed) != loaded
    }

    pub fn presets(&self) -> Vec<PresetInfo> {
        self.bridge.list_presets()
    }

    /// Opens `.kerf` text as the patch, saves it in the plugin state and sends an engine for it
    /// to the audio thread. On error nothing changes: the engine is built and queued first, and
    /// the session, the saved state and the editor's facts follow only once it is on its way.
    pub fn load_text(&self, name: &str, text: &str) -> Result<(), String> {
        let mut queues = self.queues.lock().unwrap_or_else(|e| e.into_inner());
        if queues.to_audio.is_full() {
            return Err(BUSY.to_owned());
        }
        let previous = self.bridge.text();
        let state = self
            .bridge
            .open_text(name, text, None)
            .map_err(|e| e.to_string())?;
        if let Err(e) = self.send(&mut queues, &state.patch) {
            // The session cannot be left on a patch that has no engine.
            let _ = self.bridge.open_text(name, &previous, None);
            return Err(e);
        }
        self.cell.store(PatchDoc {
            name: name.to_owned(),
            text: state.text,
        });
        self.loaded.fetch_add(1, Ordering::Release);
        Ok(())
    }

    /// Loads the factory preset `name`.
    pub fn load_preset(&self, name: &str) -> Result<(), String> {
        let text = presets::text(name).ok_or_else(|| format!("no factory preset `{name}`"))?;
        self.load_text(name, text)
    }

    /// Loads what the plugin state holds, falling back to the default preset when it holds
    /// nothing or something that no longer opens.
    pub fn reload(&self) {
        let doc = self.cell.get();
        let result = if doc.text.trim().is_empty() {
            Err(String::new())
        } else {
            self.load_text(&doc.name, &doc.text)
        };
        if let Err(message) = result {
            let fallback = self.load_preset(DEFAULT_PRESET).or_else(|_| {
                let first = self.presets().into_iter().next();
                first.map_or(Err("no factory presets".to_owned()), |p| {
                    self.load_preset(&p.name)
                })
            });
            match (message.is_empty(), fallback) {
                (_, Err(e)) => self.set_status(e),
                (false, Ok(())) => self.set_status(format!("saved patch did not open: {message}")),
                (true, Ok(())) => self.set_status(String::new()),
            }
        } else {
            self.set_status(String::new());
        }
    }

    /// Builds a fresh engine from the current patch at the current sample rate and queues it.
    /// Also drops engines the audio thread has finished with. Builds are serialised, so engines
    /// arrive in the order the patches were loaded.
    pub fn publish(&self) -> Result<(), String> {
        let mut queues = self.queues.lock().unwrap_or_else(|e| e.into_inner());
        let patch = self.bridge.get_state().patch;
        self.send(&mut queues, &patch)
    }

    /// Builds an engine of `patch` at the current sample rate and queues it; the editor's facts
    /// about the patch change only when it is queued.
    fn send(&self, queues: &mut Queues, patch: &Patch) -> Result<(), String> {
        while queues.retired.pop().is_ok() {}
        let engine = Engine::new(patch, self.sample_rate()).map_err(|e| e.to_string())?;
        let targets = Targets::find(&engine, patch);
        let built = Built {
            engine: Box::new(engine),
            targets,
            sample_rate: self.sample_rate(),
        };
        queues.to_audio.push(built).map_err(|_| BUSY.to_owned())?;
        let has = |kind: &str| patch.modules.iter().any(|m| m.kind == kind);
        self.has_jig.store(has("Jig"), Ordering::Relaxed);
        self.has_finish.store(has("Finish"), Ordering::Relaxed);
        self.patch_tempo
            .store(patch.tempo.to_bits(), Ordering::Relaxed);
        Ok(())
    }

    /// Drops engines the audio thread has finished with.
    pub fn drain_retired(&self) {
        let mut queues = self.queues.lock().unwrap_or_else(|e| e.into_inner());
        while queues.retired.pop().is_ok() {}
    }

    pub fn patch(&self) -> Patch {
        self.bridge.get_state().patch
    }

    /// What the loaded patch says the macros and the master level are.
    pub fn patch_controls(&self) -> PatchControls {
        controls_of(&self.patch())
    }
}

/// The first `Jig`'s macros and the first `Finish`'s volume, as the patch holds them.
pub fn controls_of(patch: &Patch) -> PatchControls {
    let value = |kind: &str, key: &str, default: f32| {
        patch.modules.iter().find(|m| m.kind == kind).map(|m| {
            m.params
                .iter()
                .find(|p| p.key.as_str() == key)
                .map_or(default, |p| p.value)
        })
    };
    let macros = ["m1", "m2", "m3", "m4"].map(|k| value("Jig", k, 0.5));
    PatchControls {
        macros: macros[0].map(|_| macros.map(|v| v.unwrap_or(0.5))),
        level: value("Finish", "volume", 0.7),
    }
}
