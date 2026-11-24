use std::f32::consts::TAU;

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{CV_MAX, Edge, Gate, Phase, lerp};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Spokeshave",
    band: Band::Motion,
    hp: 8,
    summary: "Four LFOs spaced like spokes",
    params: &[
        param(
            "rate",
            "Rate",
            0.3,
            "Speed of LFO 1. The others follow at related rates.",
        ),
        param(
            "spread",
            "Spread",
            0.5,
            "How far apart the four rates are. 0 = all the same.",
        ),
        param(
            "shape",
            "Shape",
            0.2,
            "Sine, triangle, ramp, square, and between.",
        ),
        param(
            "phase",
            "Phase",
            0.25,
            "Offsets the four LFOs from each other.",
        ),
        param(
            "depth",
            "Depth",
            1.0,
            "Shrinks the swing of all four LFOs towards 5 V. 0 holds them still there.",
        ),
    ],
    inputs: &[
        jack(
            "sync",
            "Sync",
            Clock,
            "Locks the rates to a clock. Rate then sets how many clock steps one cycle of LFO 1 takes.",
        ),
        jack("rate", "Rate", Cv, "Speeds up or slows down all four."),
    ],
    outputs: &[
        jack("lfo1", "1", Cv, "LFO 1."),
        jack("lfo2", "2", Cv, "LFO 2."),
        jack("lfo3", "3", Cv, "LFO 3."),
        jack("lfo4", "4", Cv, "LFO 4."),
    ],
};

/// Free-running rate of LFO 1 in hertz at the bottom and top of the knob.
const SLOWEST_HZ: f32 = 0.02;
const FASTEST_HZ: f32 = 20.0;
/// Octaves between neighbouring LFOs at full spread.
const SPREAD_OCTAVES: f32 = 0.75;
/// Clock steps per cycle of LFO 1 when synced: 2^7 at the bottom of the knob
/// down to 1 at the top.
const SYNC_POWERS: f32 = 7.0;

/// Sine, triangle, ramp or square at `phase`, all rising through zero at 0.
fn waveform(index: usize, phase: f32) -> f32 {
    match index {
        0 => (TAU * phase).sin(),
        1 if phase < 0.25 => 4.0 * phase,
        1 if phase < 0.75 => 2.0 - 4.0 * phase,
        1 => 4.0 * phase - 4.0,
        2 => 2.0 * (phase + 0.5).fract() - 1.0,
        _ => {
            if phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
    }
}

/// Crossfades between neighbouring waveforms as `shape` runs 0..1.
fn blend(shape: f32, phase: f32) -> f32 {
    let at = shape * 3.0;
    let index = (at as usize).min(2);
    lerp(
        waveform(index, phase),
        waveform(index + 1, phase),
        at - index as f32,
    )
}

#[derive(Default)]
pub struct State {
    clock: Gate,
    /// Clock steps seen since the first one.
    step: Option<u32>,
    phases: [Phase; 4],
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[rate, spread, shape, offset, depth] = io.params else {
            return;
        };
        let rate = (rate + io.inputs[1] / CV_MAX).clamp(0.0, 1.0);
        let sync = io.connected[0];
        let hz = if sync {
            let per_cycle = (SYNC_POWERS * (1.0 - rate)).round().exp2() as u32;
            if self.clock.update(io.inputs[0]) == Edge::Rose {
                let step = self.step.map_or(0, |s| s.wrapping_add(1));
                self.step = Some(step);
                if step % per_cycle == 0 {
                    self.phases = [Phase::default(); 4];
                }
            }
            // The clock runs in sixteenths.
            io.tempo * 4.0 / 60.0 / per_cycle as f32
        } else {
            SLOWEST_HZ * (FASTEST_HZ / SLOWEST_HZ).powf(rate)
        };
        let ratio = (spread * SPREAD_OCTAVES).exp2();
        let mut speed = hz / io.sample_rate;
        for (k, (out, phase)) in io.outputs.iter_mut().zip(&mut self.phases).enumerate() {
            let at = phase.advance(speed) + k as f32 * offset * 0.25;
            *out = CV_MAX / 2.0 * (1.0 + blend(shape, at.fract()) * depth);
            speed *= ratio;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::{Dsp, Io};

    const SR: f32 = 48_000.0;

    /// Renders `samples` frames with no clock; returns the four outputs.
    fn run(params: [f32; 4], samples: usize) -> Vec<[f32; 4]> {
        render(params, 120.0, samples, |_| None)
    }

    /// `clock` gives the sync voltage per sample; `None` leaves Sync unpatched.
    fn render(
        params: [f32; 4],
        tempo: f32,
        samples: usize,
        clock: impl Fn(usize) -> Option<f32>,
    ) -> Vec<[f32; 4]> {
        let mut lfo = State::default();
        (0..samples)
            .map(|i| {
                let mut out = [0.0; 4];
                lfo.process(&mut Io {
                    sample_rate: SR,
                    tempo,
                    params: &[params[0], params[1], params[2], params[3], 1.0],
                    inputs: &[clock(i).unwrap_or(0.0), 0.0],
                    connected: &[clock(i).is_some(), false],
                    outputs: &mut out,
                });
                out
            })
            .collect()
    }

    /// Sample indices where `lane` rises through the 5 V centre.
    fn rises(frames: &[[f32; 4]], lane: usize) -> Vec<usize> {
        (1..frames.len())
            .filter(|&i| frames[i - 1][lane] < 5.0 && frames[i][lane] >= 5.0)
            .collect()
    }

    /// Rate knob position for 2 Hz: 0.02 * 1000^(2/3) = 2.
    const TWO_HZ: f32 = 2.0 / 3.0;

    #[test]
    fn period_matches_the_rate() {
        let frames = run([TWO_HZ, 0.0, 0.0, 0.0], 10 * SR as usize);
        let rises = rises(&frames, 0);
        let period = (rises[rises.len() - 1] - rises[0]) as f32 / (rises.len() - 1) as f32;
        assert!((period - SR / 2.0).abs() < 120.0, "period {period} samples");
        let (lo, hi) = frames
            .iter()
            .fold((10f32, 0f32), |(lo, hi), f| (lo.min(f[0]), hi.max(f[0])));
        assert!(lo < 0.01 && hi > 9.99, "{lo}..{hi}");
    }

    #[test]
    fn spread_separates_the_rates_and_zero_keeps_them_together() {
        let together = run([TWO_HZ, 0.0, 0.0, 0.0], 4 * SR as usize);
        assert_eq!(rises(&together, 0), rises(&together, 3));
        let apart = run([TWO_HZ, 1.0, 0.0, 0.0], 4 * SR as usize);
        let slow = rises(&apart, 0).len();
        let fast = rises(&apart, 3).len();
        // Three steps of 0.75 octave: about 4.76 times faster.
        assert!(fast > 4 * slow && fast < 6 * slow, "{slow} vs {fast}");
    }

    #[test]
    fn phase_fans_the_four_lfos_out_like_spokes() {
        let first = run([TWO_HZ, 0.0, 0.0, 1.0], 1)[0];
        for (out, want) in first.iter().zip([5.0, 10.0, 5.0, 0.0]) {
            assert!((out - want).abs() < 0.1, "{first:?}");
        }
    }

    #[test]
    fn shape_runs_from_sine_to_square() {
        let square = run([TWO_HZ, 0.0, 1.0, 0.0], SR as usize);
        assert!(square.iter().all(|f| f[0] < 0.01 || f[0] > 9.99));
        let sine = run([TWO_HZ, 0.0, 0.0, 0.0], SR as usize);
        assert!(sine.iter().any(|f| f[0] > 1.0 && f[0] < 9.0));
    }

    #[test]
    fn sync_locks_cycles_to_the_clock_not_the_tempo() {
        // 16ths at 120 BPM: a pulse every 6000 samples. Rate 5/7 is 4 steps
        // per cycle; the patch tempo is wrong on purpose.
        let frames = render([5.0 / 7.0, 0.0, 0.0, 0.0], 110.0, 8 * 24_000, |i| {
            Some(if i % 6000 < 240 { 10.0 } else { 0.0 })
        });
        let rises = rises(&frames, 0);
        assert!(rises.len() >= 7, "{rises:?}");
        assert!(rises.iter().all(|r| r % 24_000 == 0), "{rises:?}");
    }
}
