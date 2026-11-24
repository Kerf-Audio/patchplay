//! The same control side and voice as the `Player`, with no device: the host calls `render`
//! when it wants audio. The browser's AudioWorklet runs this, 128 frames at a time.
//!
//! Engines are built in `play` and `update` (in the host's message handler, never inside
//! `render`); `render` itself allocates nothing. Retired engines wait in a queue until
//! `drain_retired`, so the host can drop them between blocks.

use std::sync::Arc;

use kerf_core::Patch;
use kerf_engine::Engine;
use rtrb::{Consumer, RingBuffer};

use crate::control::Control;
use crate::voice::{Peaks, Voice};
use crate::{AudioError, Meter, QUEUE_CAPACITY};

const RETIRED_CAPACITY: usize = 64;

pub struct Headless {
    control: Control,
    voice: Voice,
    retired: Consumer<Box<Engine>>,
    peaks: Arc<Peaks>,
    sample_rate: u32,
}

impl Headless {
    pub fn new(sample_rate: u32) -> Self {
        let (messages_tx, messages_rx) = RingBuffer::new(QUEUE_CAPACITY);
        let (retired_tx, retired) = RingBuffer::new(RETIRED_CAPACITY);
        let peaks = Arc::new(Peaks::default());
        Self {
            control: Control::new(messages_tx, sample_rate),
            voice: Voice::new(messages_rx, retired_tx, Arc::clone(&peaks), sample_rate),
            retired,
            peaks,
            sample_rate,
        }
    }

    /// Starts `patch` from the top, fading out whatever was playing.
    pub fn play(&mut self, patch: &Patch) -> Result<(), AudioError> {
        self.control.play(patch)
    }

    /// Applies an edit to the playing patch (see `Player::update`). Does nothing when stopped.
    pub fn update(&mut self, patch: &Patch) -> Result<(), AudioError> {
        self.control.update(patch)
    }

    /// Fades out and stops.
    pub fn stop(&mut self) -> Result<(), AudioError> {
        self.control.stop()
    }

    pub fn is_playing(&self) -> bool {
        self.control.is_playing()
    }

    /// Fills interleaved stereo `out` (even length) with the next audio. No allocation.
    pub fn render(&mut self, out: &mut [f32]) {
        self.voice.render(out);
    }

    /// The host's timing of one `render` call: `busy_ms` spent on `frames` frames. The host owns
    /// the clock because `Instant` does not work in the browser.
    pub fn record_load(&self, busy_ms: f64, frames: usize) {
        let busy_ns = (busy_ms.max(0.0) * 1e6) as u64;
        self.peaks.load.record(busy_ns, frames, self.sample_rate);
    }

    /// Output peaks and render load since the last call.
    pub fn meter(&self) -> Meter {
        let (peak_l, peak_r) = self.peaks.take();
        Meter {
            peak_l,
            peak_r,
            load: self.peaks.load.take(),
        }
    }

    /// Fills `out` (up to `SCOPE_SAMPLES`) with the latest output samples (the mono mix),
    /// oldest first, and answers how many. No allocation.
    pub fn scope(&self, out: &mut [f32]) -> usize {
        self.peaks.tap.read_latest(out)
    }

    /// Drops engines the voice has finished with.
    pub fn drain_retired(&mut self) {
        while self.retired.pop().is_ok() {}
    }
}

#[cfg(test)]
mod tests {
    use kerf_core::text::parse;
    use kerf_modules::Builtin;
    use kerf_session::Session;

    use super::*;

    #[test]
    fn plays_a_patch_and_reports_levels() {
        let text = include_str!("../../../presets/first-cut.kerf");
        let patch = Session::open(Builtin, parse(text).unwrap())
            .unwrap()
            .patch()
            .clone();
        let mut h = Headless::new(48_000);
        let mut block = [0.0f32; 256];
        for _ in 0..750 {
            h.render(&mut block);
        }
        assert!(block.iter().all(|&x| x == 0.0), "silent until played");
        h.play(&patch).unwrap();
        let mut peak = 0.0f32;
        for _ in 0..750 {
            h.render(&mut block);
            peak = block.iter().fold(peak, |p, x| p.max(x.abs()));
        }
        assert!(peak > 0.05, "{peak}");
        assert!(h.is_playing());
        assert!(h.meter().peak_l > 0.0);
        h.stop().unwrap();
        for _ in 0..20 {
            h.render(&mut block);
        }
        h.drain_retired();
        assert!(!h.is_playing());
    }
}
