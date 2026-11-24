//! Live audio output. With the `device` feature (default) a `Player` opens the default output
//! device (the cpal edge) and runs a `kerf-engine` `Engine` in the audio callback. Without it,
//! `Headless` is the same control side and voice for a host that owns the device itself (the
//! browser's AudioWorklet).
//!
//! Engines are compiled on the calling thread and handed to the audio thread
//! through a lock-free queue; retired engines come back the same way and are
//! dropped off the audio thread. Param edits travel through the same queue
//! without a rebuild.

mod control;
#[cfg(feature = "device")]
mod device;
mod headless;
mod load;
#[cfg(feature = "device")]
mod player;
mod tap;
mod voice;

#[cfg(test)]
mod tests;

use kerf_engine::EngineError;

pub use headless::Headless;
#[cfg(feature = "device")]
pub use player::Player;

/// Messages the audio thread can have waiting before the control side sees
/// `AudioError::Busy`.
const QUEUE_CAPACITY: usize = 1024;

/// Most samples `Player::scope` and `Headless::scope` return: about 43 ms at 48 kHz.
pub const SCOPE_SAMPLES: usize = tap::MAX_READ;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no audio output device found")]
    NoDevice,
    #[error("audio device: {0}")]
    Device(String),
    #[error("the audio device went away; make a new Player")]
    DeviceLost,
    #[error("the audio thread has not caught up yet; try again")]
    Busy,
    #[error(transparent)]
    Engine(#[from] EngineError),
}

/// Output peaks, `0.0..=1.0` per channel, and the render load (time spent rendering over the
/// length of the audio rendered; 1.0 only just keeps up), since the previous `Player::meter`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Meter {
    pub peak_l: f32,
    pub peak_r: f32,
    pub load: f32,
}
