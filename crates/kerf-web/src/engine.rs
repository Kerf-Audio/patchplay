//! The worklet's half: the engine running headless. `process` renders into a buffer allocated
//! once; the worklet reads it through a view on the module's memory (`output_ptr`), so a
//! 128-frame block costs no allocation and no copy across the wasm boundary.

use kerf_audio::{Headless, SCOPE_SAMPLES};
use wasm_bindgen::prelude::*;

/// Frames the output buffer holds; Web Audio blocks are 128, and this leaves room for larger ones.
const MAX_FRAMES: usize = 1024;

#[wasm_bindgen]
pub struct EngineHandle {
    audio: Headless,
    out: Vec<f32>,
}

#[wasm_bindgen]
impl EngineHandle {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: u32) -> Self {
        Self {
            audio: Headless::new(sample_rate),
            out: vec![0.0; 2 * MAX_FRAMES],
        }
    }

    /// Applies a message from the page (see `take_audio_update`): `play`, `update` or `stop`.
    /// Builds engines here, outside `process`.
    pub fn apply(&mut self, kind: &str, kerf_text: &str) -> Result<(), String> {
        let result = match kind {
            "stop" => self.audio.stop(),
            "play" => self.audio.play(&crate::parse_patch(kerf_text)?),
            "update" => self.audio.update(&crate::parse_patch(kerf_text)?),
            other => return Err(format!("unknown audio message `{other}`")),
        };
        self.audio.drain_retired();
        result.map_err(|e| e.to_string())
    }

    /// Where `process` leaves interleaved stereo `f32`s in the module's memory.
    pub fn output_ptr(&self) -> usize {
        self.out.as_ptr() as usize
    }

    /// Renders the next `frames.min(1024)` frames into the output buffer and answers how many
    /// that was; a caller with more to render calls again for the rest. No allocation.
    pub fn process(&mut self, frames: usize) -> usize {
        let frames = frames.min(MAX_FRAMES);
        self.audio.render(&mut self.out[..2 * frames]);
        frames
    }

    /// The worklet's timing of one `process` call, for the load meter; the worklet owns the clock.
    pub fn record_load(&self, busy_ms: f64, frames: usize) {
        self.audio.record_load(busy_ms, frames);
    }

    /// `[peak_l, peak_r, load]`, for the 30 Hz report.
    pub fn levels(&mut self) -> Vec<f64> {
        let m = self.audio.meter();
        self.audio.drain_retired();
        vec![f64::from(m.peak_l), f64::from(m.peak_r), f64::from(m.load)]
    }

    /// The latest output samples (the mono mix, oldest first), for the page's scope. Allocates,
    /// so the worklet calls it from its report, never from `process`.
    pub fn scope(&self) -> Vec<f32> {
        let mut out = vec![0.0; SCOPE_SAMPLES];
        let got = self.audio.scope(&mut out);
        out.truncate(got);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_block_is_rendered_in_chunks_the_caller_can_follow() {
        let mut engine = EngineHandle::new(48_000);
        let (mut done, mut calls) = (0, 0);
        while done < 3000 {
            let n = engine.process(3000 - done);
            assert!(n > 0 && n <= MAX_FRAMES);
            done += n;
            calls += 1;
        }
        assert_eq!((done, calls), (3000, 3));
        assert_eq!(engine.process(0), 0);
    }
}
