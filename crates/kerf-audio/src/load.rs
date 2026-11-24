//! How busy the audio callback is: the time spent rendering over the time the rendered audio
//! lasts, summed since the last `take`. 1.0 means the callback only just keeps up; above that,
//! the output drops out. The host owns the clock (`Instant` on the desktop, `performance.now`
//! in the browser), so this needs none.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub(crate) struct Load {
    busy_ns: AtomicU64,
    audio_ns: AtomicU64,
}

impl Load {
    /// Records one block: `busy_ns` spent rendering `frames` at `sample_rate`. No allocation.
    pub(crate) fn record(&self, busy_ns: u64, frames: usize, sample_rate: u32) {
        let audio_ns = frames as u64 * 1_000_000_000 / u64::from(sample_rate.max(1));
        self.busy_ns.fetch_add(busy_ns, Ordering::Relaxed);
        self.audio_ns.fetch_add(audio_ns, Ordering::Relaxed);
    }

    /// The load since the previous call, `0.0` when nothing was rendered. A block recorded
    /// between the two swaps lands half in this reading and half in the next, which is close
    /// enough for a meter.
    pub(crate) fn take(&self) -> f32 {
        let busy = self.busy_ns.swap(0, Ordering::Relaxed);
        let audio = self.audio_ns.swap(0, Ordering::Relaxed);
        if audio == 0 {
            0.0
        } else {
            (busy as f64 / audio as f64) as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Load;

    #[test]
    fn load_is_render_time_over_audio_time_and_resets_on_take() {
        let load = Load::default();
        // Two 480-frame blocks at 48 kHz last 20 ms; 5 ms of rendering is a quarter of that.
        load.record(2_000_000, 480, 48_000);
        load.record(3_000_000, 480, 48_000);
        assert!((load.take() - 0.25).abs() < 1e-6);
        assert_eq!(load.take(), 0.0);
    }

    #[test]
    fn an_overloaded_callback_reads_above_one() {
        let load = Load::default();
        load.record(15_000_000, 480, 48_000);
        assert!((load.take() - 1.5).abs() < 1e-6);
    }
}
