//! The shapes the bridge answers with.

use std::path::Path;

use kerf_core::text::{Diagnostic, ParseError};
use kerf_core::{Band, ModuleDescriptor, Patch};
use kerf_engine::EngineError;
use kerf_session::{Lineage, PlannedCable, SessionError};
use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error("{path}: {source}")]
    Parse { path: String, source: ParseError },
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error("{path}: {message}")]
    File { path: String, message: String },
    #[error("{0}")]
    Audio(String),
    #[error("{0}")]
    Invalid(String),
}

impl Serialize for BridgeError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

/// A file error the host reports in the bridge's terms.
pub fn file_error(path: &Path, e: impl std::fmt::Display) -> BridgeError {
    BridgeError::File {
        path: path.display().to_string(),
        message: e.to_string(),
    }
}

/// Everything the UI draws the workbench from.
#[derive(Debug, Serialize)]
pub struct State {
    pub patch: Patch,
    /// The patch as `.kerf` text.
    pub text: String,
    pub can_undo: bool,
    pub can_redo: bool,
    /// The file the patch was opened from or last saved to.
    pub path: Option<String>,
    pub playing: bool,
    pub lineage: Lineage,
    /// The cables the command just applied added by auto-patching; empty from
    /// `get_state`.
    pub connected: Vec<PlannedCable>,
}

/// What `check_text` answers.
#[derive(Debug, Serialize)]
pub struct Checked {
    pub ok: bool,
    pub diagnostics: Vec<Diagnostic>,
}

/// What `suggest_cables` answers.
#[derive(Debug, Serialize)]
pub struct Suggestions {
    pub cables: Vec<PlannedCable>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BandInfo {
    pub id: Band,
    pub name: &'static str,
    pub summary: &'static str,
}

#[derive(Debug, Serialize)]
pub(crate) struct JackKindInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub tooltip: &'static str,
}

pub(crate) const JACK_KINDS: [JackKindInfo; 5] = [
    JackKindInfo {
        id: "audio",
        label: "audio",
        tooltip: "Audio: the sound itself, swinging around 0 V.",
    },
    JackKindInfo {
        id: "cv",
        label: "CV",
        tooltip: "Control voltage: moves a knob for you.",
    },
    JackKindInfo {
        id: "pitch",
        label: "pitch",
        tooltip: "Pitch: 1 V per octave.",
    },
    JackKindInfo {
        id: "gate",
        label: "gate",
        tooltip: "Gate or trigger: on above 1 V.",
    },
    JackKindInfo {
        id: "clock",
        label: "clock",
        tooltip: "Clock: steady pulses that keep time.",
    },
];

/// The module catalogue with every tooltip, as the panels need it.
#[derive(Debug, Serialize)]
pub struct Description {
    pub(crate) bands: Vec<BandInfo>,
    pub(crate) jack_kinds: &'static [JackKindInfo],
    pub(crate) modules: &'static [&'static ModuleDescriptor],
}

#[derive(Debug, Serialize)]
pub struct PresetInfo {
    pub name: String,
    /// `preset:<name>`; open it with `open_file`.
    pub path: String,
    pub tempo: f32,
    pub key: Option<String>,
    pub style: Option<String>,
    pub tags: Vec<String>,
    pub modules: usize,
}

#[derive(Debug, Serialize)]
pub struct RenderSummary {
    /// Where the host wrote the file; empty until it does.
    pub path: String,
    pub seconds: f32,
    pub peak_db: f32,
    pub rms_db: f32,
    /// Module kinds whose DSP is not written yet; they render as silence.
    pub silent_kinds: Vec<String>,
    /// Problems that did not stop the render, e.g. a sample file that could
    /// not be read (the sampler used its demo sample).
    pub warnings: Vec<String>,
}

/// What a sampler panel draws its waveform from.
#[derive(Debug, Serialize)]
pub struct SamplePeaks {
    /// Min and max of the audio the engine plays, about 256 pairs.
    pub peaks: Vec<[f32; 2]>,
    pub seconds: f32,
    pub slices: usize,
    /// The loaded file's name; `None` while the demo sample is in use.
    pub file: Option<String>,
    /// Why the demo sample is in use when a file was asked for.
    pub warning: Option<String>,
}
