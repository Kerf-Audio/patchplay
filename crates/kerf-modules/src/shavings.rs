use std::f32::consts::{FRAC_PI_2, PI};

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, Gate, Rng};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Shavings",
    band: Band::Space,
    hp: 8,
    summary: "Granular cloud",
    params: &[
        param("size", "Size", 0.4, "Length of each grain."),
        param("density", "Density", 0.5, "Grains per second."),
        param(
            "spray",
            "Spray",
            0.3,
            "Randomises where each grain is taken from.",
        ),
        param("pitch", "Pitch", 0.5, "Transposes the grains."),
    ],
    inputs: &[
        jack("in", "In", Audio, "The sound to scatter."),
        jack("freeze", "Freeze", Gate, "Holds the current buffer."),
    ],
    outputs: &[
        jack("l", "L", Audio, "Left."),
        jack("r", "R", Audio, "Right."),
    ],
};

const BUFFER_SECONDS: f32 = 3.0;
const MAX_GRAINS: usize = 16;
const MIN_GRAIN_SECONDS: f32 = 0.02;
const MAX_GRAIN_SECONDS: f32 = 0.4;
const MIN_DENSITY: f32 = 2.0;
const MAX_DENSITY: f32 = 80.0;
/// Pitch knob spans this many semitones either side of centre.
const PITCH_RANGE: f32 = 24.0;
/// Random pitch scatter per grain at full Spray, in semitones.
const PITCH_SPRAY: f32 = 0.25;

#[derive(Debug, Clone, Copy, Default)]
struct Grain {
    /// Read position in the buffer, in samples. Double precision: an `f32`
    /// cannot hold a fractional step this late in a three-second buffer.
    pos: f64,
    /// Samples advanced per output sample.
    step: f32,
    age: u32,
    length: u32,
    left: f32,
    right: f32,
}

impl Grain {
    fn live(&self) -> bool {
        self.age < self.length
    }

    /// Moves on one output sample, wrapping round a buffer of `len` samples.
    fn advance(&mut self, len: usize) {
        self.age += 1;
        self.pos += f64::from(self.step);
        if self.pos >= len as f64 {
            self.pos -= len as f64;
        }
    }
}

#[derive(Debug, Clone)]
pub struct State {
    buffer: Vec<f32>,
    write: usize,
    grains: [Grain; MAX_GRAINS],
    freeze: Gate,
    rng: Rng,
    /// Samples until the next grain starts.
    until_next: f32,
}

impl State {
    pub fn new(seed: u64, sample_rate: f32) -> Self {
        Self {
            buffer: vec![0.0; (BUFFER_SECONDS * sample_rate) as usize],
            write: 0,
            grains: [Grain::default(); MAX_GRAINS],
            freeze: Gate::default(),
            rng: Rng::new(seed),
            until_next: 0.0,
        }
    }

    /// Starts a grain in a free slot; with none free the grain is skipped.
    fn spawn(&mut self, size: f32, spray: f32, pitch: f32, sr: f32) {
        let Some(slot) = self.grains.iter().position(|g| !g.live()) else {
            return;
        };
        let length = (size * sr).max(2.0);
        let semitones =
            (pitch - 0.5) * 2.0 * PITCH_RANGE + (self.rng.unit() * 2.0 - 1.0) * spray * PITCH_SPRAY;
        let step = (semitones / 12.0).exp2();
        // Start far enough back that the grain never reads past the write head.
        let base = length * step.max(1.0);
        let room = (self.buffer.len() as f32 - 4.0 - base).max(0.0);
        let back = base + spray * self.rng.unit() * room;
        let pan = self.rng.unit() * FRAC_PI_2;
        self.grains[slot] = Grain {
            pos: (self.write as f64 - f64::from(back)).rem_euclid(self.buffer.len() as f64),
            step,
            age: 0,
            length: length as u32,
            left: pan.cos(),
            right: pan.sin(),
        };
    }
}

/// The buffer at a fractional position, linearly interpolated.
fn read(buffer: &[f32], pos: f64) -> f32 {
    let whole = pos as usize;
    let frac = (pos - whole as f64) as f32;
    let a = buffer[whole % buffer.len()];
    let b = buffer[(whole + 1) % buffer.len()];
    a + (b - a) * frac
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[size, density, spray, pitch] = io.params else {
            return;
        };
        let sr = io.sample_rate;
        self.freeze.update(io.inputs[1]);
        if !self.freeze.is_high() {
            self.buffer[self.write] = io.inputs[0] / AUDIO_VOLTS;
            self.write = (self.write + 1) % self.buffer.len();
        }

        let seconds = MIN_GRAIN_SECONDS * (MAX_GRAIN_SECONDS / MIN_GRAIN_SECONDS).powf(size);
        let rate = MIN_DENSITY * (MAX_DENSITY / MIN_DENSITY).powf(density);
        self.until_next -= 1.0;
        if self.until_next <= 0.0 {
            self.spawn(seconds, spray, pitch, sr);
            self.until_next += sr / rate * (0.5 + self.rng.unit());
        }

        // Overlapping grains add; scale so a coherent pile-up stays near unity.
        let gain = 1.0 / (rate * seconds * 0.5).max(1.0);
        let (mut left, mut right) = (0.0, 0.0);
        for g in self.grains.iter_mut().filter(|g| g.live()) {
            let window = (PI * g.age as f32 / g.length as f32).sin().powi(2);
            let s = read(&self.buffer, g.pos) * window;
            left += s * g.left;
            right += s * g.right;
            g.advance(self.buffer.len());
        }
        io.outputs[0] = (left * gain).tanh() * AUDIO_VOLTS;
        io.outputs[1] = (right * gain).tanh() * AUDIO_VOLTS;
    }
}

#[cfg(test)]
mod tests {
    use super::{Grain, State};
    use crate::rig::{SR, peak, render, rms, sine};

    const CONNECTED: [bool; 2] = [true, false];

    fn cloud(
        params: [f32; 4],
        seconds: f32,
        input: impl FnMut(usize, &mut [f32]),
    ) -> Vec<Vec<f32>> {
        render(
            &mut State::new(7, SR),
            &params,
            &CONNECTED,
            2,
            (seconds * SR) as usize,
            input,
        )
    }

    #[test]
    fn a_grain_keeps_its_pitch_late_in_the_buffer() {
        let step = 2f32.powf(1.0 / 12.0);
        let mut grain = Grain {
            pos: 100_000.0,
            step,
            length: 19_200,
            ..Grain::default()
        };
        for _ in 0..grain.length {
            grain.advance(144_000);
        }
        let expected = 100_000.0 + f64::from(grain.length) * f64::from(step);
        assert!(
            (grain.pos - expected).abs() < 0.01,
            "off by {} samples",
            grain.pos - expected
        );
    }

    #[test]
    fn a_grain_wraps_round_the_buffer() {
        let mut grain = Grain {
            pos: 143_999.5,
            step: 1.0,
            length: 10,
            ..Grain::default()
        };
        grain.advance(144_000);
        assert_eq!(grain.pos, 0.5);
    }

    #[test]
    fn a_cloud_follows_its_input_and_stays_in_range() {
        for params in [
            [0.4, 0.5, 0.3, 0.5],
            [1.0, 1.0, 1.0, 1.0],
            [0.0, 1.0, 0.0, 0.0],
        ] {
            let out = cloud(params, 2.0, |i, inp| inp[0] = sine(i, 220.0, 5.0));
            for channel in &out {
                assert!(rms(&channel[SR as usize..]) > 0.05, "{params:?} is silent");
                assert!(
                    peak(channel) <= 5.0,
                    "{params:?} peaks at {}",
                    peak(channel)
                );
            }
        }
    }

    #[test]
    fn silent_input_gives_silence() {
        let out = cloud([0.4, 0.8, 0.5, 0.5], 2.0, |_, _| {});
        assert_eq!(peak(&out[0]).max(peak(&out[1])), 0.0);
    }

    #[test]
    fn the_cloud_dies_once_the_buffer_is_overwritten() {
        let out = cloud([0.4, 0.8, 0.5, 0.5], 6.0, |i, inp| {
            inp[0] = if i < SR as usize {
                sine(i, 220.0, 5.0)
            } else {
                0.0
            };
        });
        assert!(peak(&out[0][(4.5 * SR) as usize..]) == 0.0);
    }

    #[test]
    fn freeze_holds_the_sound_after_the_input_stops() {
        let tone_then_freeze = |i: usize, inp: &mut [f32]| {
            let n = SR as usize;
            inp[0] = if i < n { sine(i, 220.0, 5.0) } else { 0.0 };
            inp[1] = if i >= n { 10.0 } else { 0.0 };
        };
        let out = render(
            &mut State::new(7, SR),
            &[0.4, 0.8, 0.1, 0.5],
            &[true, true],
            2,
            (6.0 * SR) as usize,
            tone_then_freeze,
        );
        assert!(rms(&out[0][(5.0 * SR) as usize..]) > 0.05);
    }

    #[test]
    fn the_same_seed_makes_the_same_cloud() {
        let tone = |i: usize, inp: &mut [f32]| inp[0] = sine(i, 330.0, 4.0);
        let a = cloud([0.4, 0.5, 0.5, 0.6], 0.5, tone);
        let b = cloud([0.4, 0.5, 0.5, 0.6], 0.5, tone);
        assert_eq!(a, b);
    }
}
