//! The audio-thread side: takes messages from the control side, runs the
//! engine and cross-fades between engines. No allocation, locks or I/O in
//! `render`; retired engines go back through a queue to be dropped elsewhere.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use kerf_engine::{Change, Engine};
use rtrb::{Consumer, Producer};

use crate::load::Load;
use crate::tap::Tap;

/// Length of the fade in, out and between engines.
const FADE_SECONDS: f32 = 0.025;

/// What the control side sends to the audio thread.
pub(crate) enum Msg {
    /// Replace the engine. With `carry`, module state moves from the old
    /// engine to the new one and the swap is instant; without, the old engine
    /// fades out as the new one fades in.
    Swap {
        engine: Box<Engine>,
        carry: bool,
    },
    Change(Change),
    /// Fade out and drop the engine.
    Stop,
}

/// Output peaks since the last `take`, as `f32` bits, and the scope tap. Peaks are never
/// negative, so the bits order like the values and `fetch_max` works.
#[derive(Default)]
pub(crate) struct Peaks {
    left: AtomicU32,
    right: AtomicU32,
    pub(crate) load: Load,
    pub(crate) tap: Tap,
}

impl Peaks {
    fn record(&self, left: f32, right: f32) {
        self.left.fetch_max(left.to_bits(), Ordering::Relaxed);
        self.right.fetch_max(right.to_bits(), Ordering::Relaxed);
    }

    pub(crate) fn take(&self) -> (f32, f32) {
        (
            f32::from_bits(self.left.swap(0, Ordering::Relaxed)),
            f32::from_bits(self.right.swap(0, Ordering::Relaxed)),
        )
    }
}

pub(crate) struct Voice {
    messages: Consumer<Msg>,
    retired: Producer<Box<Engine>>,
    peaks: Arc<Peaks>,
    current: Option<Box<Engine>>,
    /// The engine fading out while `fade_pos < fade_len`.
    fading: Option<Box<Engine>>,
    fade_len: usize,
    fade_pos: usize,
}

impl Voice {
    pub(crate) fn new(
        messages: Consumer<Msg>,
        retired: Producer<Box<Engine>>,
        peaks: Arc<Peaks>,
        sample_rate: u32,
    ) -> Self {
        let fade_len = ((sample_rate as f32 * FADE_SECONDS) as usize).max(1);
        Self {
            messages,
            retired,
            peaks,
            current: None,
            fading: None,
            fade_len,
            fade_pos: fade_len,
        }
    }

    /// Fills interleaved stereo `out` (even length) with the next audio.
    pub(crate) fn render(&mut self, out: &mut [f32]) {
        while let Ok(msg) = self.messages.pop() {
            self.receive(msg);
        }

        let mut done = 0;
        while self.fade_pos < self.fade_len && done < out.len() {
            let t = self.fade_pos as f32 / self.fade_len as f32;
            let new = self.current.as_mut().map_or([0.0; 2], |e| e.next_frame());
            let old = self.fading.as_mut().map_or([0.0; 2], |e| e.next_frame());
            out[done] = new[0] * t + old[0] * (1.0 - t);
            out[done + 1] = new[1] * t + old[1] * (1.0 - t);
            done += 2;
            self.fade_pos += 1;
            if self.fade_pos == self.fade_len {
                let faded = self.fading.take();
                self.retire(faded);
            }
        }
        match self.current.as_mut() {
            Some(engine) => engine.process_block(&mut out[done..]),
            None => out[done..].fill(0.0),
        }

        let (mut left, mut right) = (0.0f32, 0.0f32);
        for frame in out.chunks_exact_mut(2) {
            // Never let a runaway patch reach the speakers.
            for x in frame.iter_mut() {
                *x = if x.is_finite() {
                    x.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
            }
            left = left.max(frame[0].abs());
            right = right.max(frame[1].abs());
        }
        self.peaks.record(left, right);
        self.peaks.tap.push_stereo(out);
    }

    fn receive(&mut self, msg: Msg) {
        match msg {
            Msg::Change(change) => {
                if let Some(engine) = self.current.as_mut() {
                    engine.apply(change);
                }
            }
            Msg::Swap { mut engine, carry } => match self.current.take() {
                Some(mut old) if carry => {
                    engine.take_state(&mut old);
                    self.current = Some(engine);
                    self.retire(Some(old));
                }
                old => {
                    self.current = Some(engine);
                    self.start_fade(old);
                }
            },
            Msg::Stop => {
                let old = self.current.take();
                self.start_fade(old);
            }
        }
    }

    /// Fades `old` out (if there is one) while `current` fades in.
    fn start_fade(&mut self, old: Option<Box<Engine>>) {
        let superseded = self.fading.take();
        self.retire(superseded);
        self.fading = old;
        self.fade_pos = 0;
    }

    fn retire(&mut self, engine: Option<Box<Engine>>) {
        if let Some(engine) = engine {
            // A full queue (nothing drains it) drops the engine here instead.
            let _ = self.retired.push(engine);
        }
    }
}
