//! Part levels: a smoothed gain on audio cables that leave a part.

/// How fast a level change settles (time constant, seconds): long enough that
/// a fader drag cannot click, short enough to feel immediate.
const SETTLE_SECONDS: f32 = 0.005;

/// A cable's source: the output it reads, and the part whose level scales it
/// (an audio cable from inside a part to a module outside it).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Source {
    pub value: usize,
    pub part: Option<usize>,
}

/// One part's gain: the level asked for, and the smoothed gain in use.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PartGain {
    target: f32,
    pub current: f32,
}

impl PartGain {
    /// Starts settled, so a patch opens at its levels without a fade-in.
    pub fn new(level: f32) -> Self {
        Self {
            target: level,
            current: level,
        }
    }

    /// The one-pole coefficient per sample at `sample_rate`.
    pub fn coefficient(sample_rate: f32) -> f32 {
        1.0 - (-1.0 / (SETTLE_SECONDS * sample_rate)).exp()
    }

    pub fn set(&mut self, level: f32) {
        self.target = level.clamp(0.0, 1.0);
    }

    /// Moves one sample towards the target; lands on it exactly once close,
    /// so a level of 1.0 is exactly unity.
    pub fn tick(&mut self, coefficient: f32) {
        if self.current != self.target {
            self.current += (self.target - self.current) * coefficient;
            if (self.target - self.current).abs() < 1e-5 {
                self.current = self.target;
            }
        }
    }
}
