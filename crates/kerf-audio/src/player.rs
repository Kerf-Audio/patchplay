//! The `Player`: the default output device running one patch at a time.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use kerf_core::Patch;
use rtrb::RingBuffer;

use crate::control::Control;
use crate::device::Device;
use crate::voice::Peaks;
use crate::{AudioError, Meter, QUEUE_CAPACITY, SCOPE_SAMPLES};

/// The default output device, running one patch at a time. Methods take
/// `&self`, so a `Player` can sit in shared app state.
pub struct Player {
    control: Mutex<Control>,
    peaks: Arc<Peaks>,
    lost: Arc<AtomicBool>,
    sample_rate: u32,
    // Declared last: dropping the device stops the stream.
    _device: Device,
}

impl Player {
    /// Opens the default output device. Silent until `play`.
    pub fn new() -> Result<Self, AudioError> {
        let (messages_tx, messages_rx) = RingBuffer::new(QUEUE_CAPACITY);
        let peaks = Arc::new(Peaks::default());
        let lost = Arc::new(AtomicBool::new(false));
        let device = Device::open(messages_rx, Arc::clone(&peaks), Arc::clone(&lost))?;
        Ok(Self {
            control: Mutex::new(Control::new(messages_tx, device.sample_rate)),
            peaks,
            lost,
            sample_rate: device.sample_rate,
            _device: device,
        })
    }

    /// Starts `patch` from the top, fading out whatever was playing. The
    /// engine is built here, at the device's sample rate, not on the audio
    /// thread.
    pub fn play(&self, patch: &Patch) -> Result<(), AudioError> {
        self.checked()?.play(patch)
    }

    /// Applies an edit to the playing patch without restarting it. Param and
    /// tempo changes go straight to the running engine; any other edit swaps
    /// in a rebuilt engine that keeps the state (oscillator phase, envelopes,
    /// delay tails, sequencer position) of modules whose id and kind are
    /// unchanged. Does nothing when stopped.
    pub fn update(&self, patch: &Patch) -> Result<(), AudioError> {
        self.checked()?.update(patch)
    }

    /// Fades out and stops. The device stays open.
    pub fn stop(&self) {
        // A full queue means the audio thread is stuck; there is nothing to stop.
        let _ = self.lock().stop();
    }

    pub fn is_playing(&self) -> bool {
        self.lock().is_playing()
    }

    /// Output peaks and render load since the last call, so a UI polling at
    /// any rate sees every peak.
    pub fn meter(&self) -> Meter {
        let (peak_l, peak_r) = self.peaks.take();
        Meter {
            peak_l,
            peak_r,
            load: self.peaks.load.take(),
        }
    }

    /// The latest output samples (the mono mix), oldest first, up to `SCOPE_SAMPLES`; fewer
    /// only just after the device opened. Never blocks the audio thread.
    pub fn scope(&self) -> Vec<f32> {
        let mut out = vec![0.0; SCOPE_SAMPLES];
        let got = self.peaks.tap.read_latest(&mut out);
        out.truncate(got);
        out
    }

    /// The device's sample rate in Hz; engines are built at this rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn lock(&self) -> MutexGuard<'_, Control> {
        self.control.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn checked(&self) -> Result<MutexGuard<'_, Control>, AudioError> {
        if self.lost.load(Ordering::Relaxed) {
            return Err(AudioError::DeviceLost);
        }
        Ok(self.lock())
    }
}
