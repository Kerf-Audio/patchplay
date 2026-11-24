//! What the bridge needs from the host: a way to play, behind a trait so the bridge and its tests
//! need no sound card.

use kerf_core::Patch;
use serde::Serialize;

/// Output level since the last read, per channel, `0.0..=1.0`, and the audio render load: the
/// time spent rendering over the length of the audio rendered (1.0 only just keeps up).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Meter {
    pub peak_l: f32,
    pub peak_r: f32,
    pub load: f32,
}

/// The latest output samples (the mono mix, oldest first) and the rate they were played at, for
/// the UI's oscilloscope and spectrum. Empty when nothing is playing or there is no audio.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Scope {
    pub rate: u32,
    pub samples: Vec<f32>,
}

pub trait Transport: Send {
    /// Starts playing the patch from the top.
    fn play(&mut self, patch: &Patch) -> Result<(), String>;
    /// Swaps in an edited patch while playing.
    fn update(&mut self, patch: &Patch);
    fn stop(&mut self);
    fn is_playing(&self) -> bool;
    fn meter(&self) -> Meter;
    fn scope(&self) -> Scope;
}

/// Used when there is no audio output (and in tests): playing is refused with
/// the reason the UI shows.
#[derive(Debug)]
pub struct NoAudio(pub String);

impl Transport for NoAudio {
    fn play(&mut self, _patch: &Patch) -> Result<(), String> {
        Err(self.0.clone())
    }

    fn update(&mut self, _patch: &Patch) {}

    fn stop(&mut self) {}

    fn is_playing(&self) -> bool {
        false
    }

    fn meter(&self) -> Meter {
        Meter::default()
    }

    fn scope(&self) -> Scope {
        Scope::default()
    }
}
