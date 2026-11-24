//! The host-visible parameters and the state saved with them.

use std::sync::Arc;

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;

use crate::doc::PatchCell;
use crate::rack::Controls;

#[derive(Params)]
pub struct PatchplayParams {
    /// The editor's size, kept with the project.
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    /// The patch as `.kerf` text.
    #[persist = "kerf-patch"]
    pub patch: Arc<PatchCell>,

    /// The first Jig module's four macros.
    #[id = "macro1"]
    pub macro1: FloatParam,
    #[id = "macro2"]
    pub macro2: FloatParam,
    #[id = "macro3"]
    pub macro3: FloatParam,
    #[id = "macro4"]
    pub macro4: FloatParam,

    /// The first Finish module's master level.
    #[id = "level"]
    pub level: FloatParam,

    /// Run the patch only while the host plays, and from the top each time it starts.
    #[id = "follow-transport"]
    pub follow_transport: BoolParam,

    /// Run the patch at the host's tempo.
    #[id = "follow-tempo"]
    pub follow_tempo: BoolParam,
}

fn unit(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_smoother(SmoothingStyle::Linear(20.0))
        .with_value_to_string(formatters::v2s_f32_percentage(0))
        .with_string_to_value(formatters::s2v_f32_percentage())
}

impl PatchplayParams {
    pub fn new(patch: Arc<PatchCell>) -> Self {
        Self {
            editor_state: EguiState::from_size(520, 560),
            patch,
            macro1: unit("Macro 1", 0.5),
            macro2: unit("Macro 2", 0.5),
            macro3: unit("Macro 3", 0.5),
            macro4: unit("Macro 4", 0.5),
            // The Finish module's own default.
            level: unit("Level", 0.7),
            follow_transport: BoolParam::new("Follow transport", true),
            follow_tempo: BoolParam::new("Follow host tempo", true),
        }
    }

    pub fn macros(&self) -> [&FloatParam; 4] {
        [&self.macro1, &self.macro2, &self.macro3, &self.macro4]
    }

    /// The values for the next `frames` frames, advancing the smoothers.
    pub fn controls(&self, frames: usize) -> Controls {
        let frames = frames as u32;
        Controls {
            macros: self.macros().map(|p| p.smoothed.next_step(frames)),
            level: self.level.smoothed.next_step(frames),
            follow_tempo: self.follow_tempo.value(),
            follow_transport: self.follow_transport.value(),
        }
    }
}
