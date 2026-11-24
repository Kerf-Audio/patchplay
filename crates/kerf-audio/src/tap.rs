//! The scope tap: the last few thousand output samples (the mono mix, left plus right over two)
//! in a ring of atomics, for the UI's oscilloscope and spectrum.
//!
//! The audio thread writes with relaxed stores and one release store per run of at most
//! `PUBLISH` frames; it never waits for a reader and the reader never blocks it. When nobody
//! reads, the cost is one store per frame. A reader that is overtaken while it copies sees that
//! and tries again: a seqlock, with fences on both sides because the sample stores and loads
//! are relaxed.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering, fence};

/// Samples the ring holds. A power of two.
const CAPACITY: usize = 4096;
const MASK: usize = CAPACITY - 1;
/// The most a read returns: half the ring, so the writer needs more than two thousand samples
/// to overtake a reader that is halfway through a copy.
pub(crate) const MAX_READ: usize = CAPACITY / 2;
const ATTEMPTS: usize = 4;
/// The most frames written before `written` is published. Bounds how far the writer can be ahead
/// of the count a reader sees, which is what the reader's overtaken check relies on.
const PUBLISH: usize = 256;

pub(crate) struct Tap {
    samples: Box<[AtomicU32]>,
    /// Samples written since the start; the next write goes to `written & MASK`.
    written: AtomicU64,
}

impl Default for Tap {
    fn default() -> Self {
        Self {
            samples: (0..CAPACITY).map(|_| AtomicU32::new(0)).collect(),
            written: AtomicU64::new(0),
        }
    }
}

impl Tap {
    /// Records the mono mix of interleaved stereo `block`. Audio thread: no allocation, no lock.
    pub(crate) fn push_stereo(&self, block: &[f32]) {
        let mut at = self.written.load(Ordering::Relaxed);
        for run in block.chunks(2 * PUBLISH) {
            // Pairs with the reader's acquire fence: a reader that sees any sample stored below
            // also sees the count published before this fence.
            fence(Ordering::Release);
            for frame in run.chunks_exact(2) {
                let mono = 0.5 * (frame[0] + frame[1]);
                self.samples[at as usize & MASK].store(mono.to_bits(), Ordering::Relaxed);
                at += 1;
            }
            self.written.store(at, Ordering::Release);
        }
    }

    /// Fills `out` (at most `MAX_READ` long; the rest is ignored) with the latest samples,
    /// oldest first, and answers how many there were: fewer than asked for only before that
    /// many have been written.
    pub(crate) fn read_latest(&self, out: &mut [f32]) -> usize {
        let wanted = out.len().min(MAX_READ);
        let mut count = 0;
        for _ in 0..ATTEMPTS {
            let end = self.written.load(Ordering::Acquire);
            count = wanted.min(end as usize);
            let start = end - count as u64;
            for (i, slot) in out[..count].iter_mut().enumerate() {
                let bits = self.samples[(start as usize + i) & MASK].load(Ordering::Relaxed);
                *slot = f32::from_bits(bits);
            }
            // The sample loads above must finish before `written` is read again.
            fence(Ordering::Acquire);
            // Nothing in `start..end` has been overwritten unless the writer got a whole ring
            // ahead, counting the run it may be writing but has not yet published.
            let ahead = self.written.load(Ordering::Relaxed) - start;
            if ahead + PUBLISH as u64 <= CAPACITY as u64 {
                break;
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stereo frames whose mono mix is `first, first + 1, ...` (left is that plus 0.25, right minus).
    fn ramp(first: usize, n: usize) -> Vec<f32> {
        (first..first + n)
            .flat_map(|i| [i as f32 + 0.25, i as f32 - 0.25])
            .collect()
    }

    fn latest(tap: &Tap, n: usize) -> Vec<f32> {
        let mut out = vec![-1.0; n];
        let got = tap.read_latest(&mut out);
        out.truncate(got);
        out
    }

    fn expect(first: usize, n: usize) -> Vec<f32> {
        (first..first + n).map(|i| i as f32).collect()
    }

    #[test]
    fn an_empty_tap_reads_nothing() {
        assert!(latest(&Tap::default(), 100).is_empty());
    }

    #[test]
    fn a_short_history_reads_what_there_is_in_order() {
        let tap = Tap::default();
        tap.push_stereo(&ramp(0, 10));
        assert_eq!(latest(&tap, 100), expect(0, 10));
        assert_eq!(latest(&tap, 4), expect(6, 4));
    }

    #[test]
    fn the_latest_samples_come_back_in_order_across_blocks() {
        let tap = Tap::default();
        for block in 0..10 {
            tap.push_stereo(&ramp(block * 100, 100));
        }
        assert_eq!(latest(&tap, 250), expect(750, 250));
    }

    #[test]
    fn reads_stay_correct_after_the_ring_wraps_many_times() {
        let tap = Tap::default();
        // 130 blocks of 128 is 16640 samples, four wraps and a bit, at a size that never lines up.
        for block in 0..130 {
            tap.push_stereo(&ramp(block * 128, 128));
            let total = (block + 1) * 128;
            let n = MAX_READ.min(total);
            assert_eq!(
                latest(&tap, MAX_READ),
                expect(total - n, n),
                "block {block}"
            );
        }
    }

    #[test]
    fn a_block_longer_than_the_ring_leaves_its_tail() {
        let tap = Tap::default();
        tap.push_stereo(&ramp(0, 3 * CAPACITY + 7));
        assert_eq!(
            latest(&tap, MAX_READ),
            expect(3 * CAPACITY + 7 - MAX_READ, MAX_READ)
        );
    }

    #[test]
    fn a_reader_never_sees_a_torn_run_while_a_long_block_is_written() {
        use std::sync::atomic::AtomicBool;
        // Blocks of 3000 frames are longer than half the ring: unpublished writes would reach
        // the samples a reader is copying.
        const BLOCK: usize = 3000;
        const BLOCKS: usize = 1500;
        let tap = Tap::default();
        let done = AtomicBool::new(false);
        std::thread::scope(|s| {
            s.spawn(|| {
                for block in 0..BLOCKS {
                    tap.push_stereo(&ramp(block * BLOCK, BLOCK));
                }
                done.store(true, Ordering::Release);
            });
            let mut out = vec![0.0; MAX_READ];
            while !done.load(Ordering::Acquire) {
                let got = tap.read_latest(&mut out);
                for pair in out[..got].windows(2) {
                    assert_eq!(pair[1], pair[0] + 1.0, "a torn read: {pair:?}");
                }
            }
        });
    }

    #[test]
    fn a_read_never_asks_for_more_than_half_the_ring() {
        let tap = Tap::default();
        tap.push_stereo(&ramp(0, CAPACITY));
        assert_eq!(latest(&tap, 3 * CAPACITY).len(), MAX_READ);
    }
}
