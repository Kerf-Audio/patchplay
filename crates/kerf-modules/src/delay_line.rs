use kerf_dsp::lerp;

/// A circular buffer read at fractional delays with linear interpolation.
#[derive(Debug, Clone)]
pub struct DelayLine {
    buffer: Vec<f32>,
    write: usize,
}

impl DelayLine {
    /// A line that can delay by up to `max_samples`.
    pub fn new(max_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; max_samples + 3],
            write: 0,
        }
    }

    /// The sample pushed `samples` pushes ago (1 is the latest), clamped to
    /// what the line holds. Read before the next [`push`](Self::push).
    pub fn tap(&self, samples: f32) -> f32 {
        let len = self.buffer.len();
        let samples = samples.clamp(1.0, (len - 2) as f32);
        let whole = samples as usize;
        let newer = (self.write + len - whole) % len;
        let older = (newer + len - 1) % len;
        lerp(
            self.buffer[newer],
            self.buffer[older],
            samples - whole as f32,
        )
    }

    pub fn push(&mut self, x: f32) {
        self.buffer[self.write] = x;
        self.write = (self.write + 1) % self.buffer.len();
    }
}

#[cfg(test)]
mod tests {
    use super::DelayLine;

    #[test]
    fn taps_return_what_was_pushed_that_many_samples_ago() {
        let mut line = DelayLine::new(10);
        for i in 1..=20 {
            let before = line.tap(3.0);
            line.push(i as f32);
            if i > 3 {
                assert_eq!(before, (i - 3) as f32);
            }
        }
        assert_eq!(line.tap(1.0), 20.0);
        assert_eq!(line.tap(2.5), 18.5);
    }
}
