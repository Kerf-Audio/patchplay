use std::f32::consts::TAU;

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{CV_MAX, Edge, GATE_HIGH, Gate, OnePole, Phase, Rng, lerp, smoothing};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Rocker",
    band: Band::Motion,
    hp: 6,
    summary: "A rocking LFO",
    params: &[
        param(
            "rate",
            "Rate",
            0.4,
            "How fast it rocks, from one cycle every 50 seconds to 40 times a second. Ignored while Sync is on.",
        ),
        param(
            "sync",
            "Sync",
            0.0,
            "Off at the bottom. Turn up to lock to the patch tempo: one cycle takes 4 bars, 2 bars, 1 bar, 2 beats, 1 beat, a half beat or a quarter beat.",
        ),
        param(
            "shape",
            "Shape",
            0.0,
            "Slides from sine through triangle, ramp and square to stepped random.",
        ),
        param(
            "depth",
            "Depth",
            1.0,
            "How far the wave swings. 0 holds it still at Offset.",
        ),
        param(
            "offset",
            "Offset",
            0.5,
            "Shifts the whole wave up or down. Centre sits it around 0 V.",
        ),
        param(
            "phase",
            "Phase",
            0.0,
            "Where in the cycle the wave starts and restarts.",
        ),
    ],
    inputs: &[
        jack(
            "rate",
            "Rate",
            Cv,
            "Speeds it up or slows it down. While Sync is on it picks a shorter or longer note length instead.",
        ),
        jack("reset", "Reset", Gate, "Restarts the wave at Phase."),
    ],
    outputs: &[
        jack("out", "Out", Cv, "The wave, swinging above and below 0 V."),
        jack("inverted", "Inverted", Cv, "The wave upside down."),
        jack(
            "pulse",
            "Pulse",
            Gate,
            "High for the first half of every cycle: a clock at the speed of the wave.",
        ),
    ],
};

/// Free-running cycles per second at the bottom and top of the Rate knob.
const SLOWEST_HZ: f32 = 0.02;
const FASTEST_HZ: f32 = 40.0;
/// Beats in one cycle for each Sync position above Off, slowest first.
const SYNC_BEATS: [f32; 7] = [16.0, 8.0, 4.0, 2.0, 1.0, 0.5, 0.25];
/// Time the Shape knob takes to settle, so turning it never clicks.
const SHAPE_SECONDS: f32 = 0.005;

pub struct State {
    reset: Gate,
    phase: Phase,
    rng: Rng,
    /// The stepped-random value for this cycle, `-1..1`.
    held: f32,
    shape: OnePole,
    shape_coeff: f32,
    /// False until the first sample, which takes the Shape knob as it is.
    settled: bool,
}

impl State {
    pub fn new(seed: u64, sample_rate: f32) -> Self {
        let mut rng = Rng::new(seed);
        let held = rng.unit() * 2.0 - 1.0;
        Self {
            reset: Gate::default(),
            phase: Phase::default(),
            rng,
            held,
            shape: OnePole::default(),
            shape_coeff: smoothing(SHAPE_SECONDS, sample_rate),
            settled: false,
        }
    }
}

/// Sine, triangle, ramp, square or the held random value at `phase`. All
/// but the random one rise through zero at phase 0.
fn waveform(index: usize, phase: f32, held: f32) -> f32 {
    match index {
        0 => (TAU * phase).sin(),
        1 if phase < 0.25 => 4.0 * phase,
        1 if phase < 0.75 => 2.0 - 4.0 * phase,
        1 => 4.0 * phase - 4.0,
        2 => 2.0 * (phase + 0.5).fract() - 1.0,
        3 => {
            if phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        _ => held,
    }
}

/// Crossfades between neighbouring waveforms as `shape` runs 0..1.
fn blend(shape: f32, phase: f32, held: f32) -> f32 {
    let at = shape.clamp(0.0, 1.0) * 4.0;
    let index = (at as usize).min(3);
    lerp(
        waveform(index, phase, held),
        waveform(index + 1, phase, held),
        at - index as f32,
    )
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[rate, sync, shape, depth, offset, phase_offset] = io.params else {
            return;
        };
        let cv = io.inputs[0] / CV_MAX;
        let synced = (sync * SYNC_BEATS.len() as f32).round() as i32;
        let hz = if synced > 0 {
            // The CV walks the note length up and down the Sync positions.
            let at = (synced as f32 + cv * SYNC_BEATS.len() as f32).round();
            let beats = SYNC_BEATS[(at.clamp(1.0, 7.0) as usize) - 1];
            io.tempo / 60.0 / beats
        } else {
            SLOWEST_HZ * (FASTEST_HZ / SLOWEST_HZ).powf((rate + cv).clamp(0.0, 1.0))
        };
        let dt = hz / io.sample_rate;
        if self.reset.update(io.inputs[1]) == Edge::Rose {
            self.phase = Phase::default();
            self.held = self.rng.unit() * 2.0 - 1.0;
        }
        let phase = self.phase.advance(dt);
        if phase < dt {
            self.held = self.rng.unit() * 2.0 - 1.0;
        }
        let coeff = if self.settled { self.shape_coeff } else { 1.0 };
        self.settled = true;
        let shape = self.shape.process(shape, coeff);
        let at = (phase + phase_offset).fract();
        let wave = blend(shape, at, self.held);
        let half = CV_MAX / 2.0;
        let out = (wave * depth + (offset * 2.0 - 1.0)) * half;
        io.outputs[0] = out;
        io.outputs[1] = -out;
        io.outputs[2] = if at < 0.5 { GATE_HIGH } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::{Dsp, Io};

    const SR: f32 = 48_000.0;

    /// Renders `samples` frames at 120 BPM. `reset` gives the Reset voltage
    /// per sample and `cv` the Rate voltage. Frames are `[out, inverted, pulse]`.
    fn render(
        seed: u64,
        params: [f32; 6],
        samples: usize,
        cv: f32,
        reset: &dyn Fn(usize) -> f32,
    ) -> Vec<[f32; 3]> {
        let mut rocker = State::new(seed, SR);
        (0..samples)
            .map(|i| {
                let mut out = [0.0; 3];
                rocker.process(&mut Io {
                    sample_rate: SR,
                    tempo: 120.0,
                    params: &params,
                    inputs: &[cv, reset(i)],
                    connected: &[true, true],
                    outputs: &mut out,
                });
                out
            })
            .collect()
    }

    fn free(params: [f32; 6], samples: usize) -> Vec<[f32; 3]> {
        render(1, params, samples, 0.0, &|_| 0.0)
    }

    /// Rising zero crossings of the first output.
    fn rises(frames: &[[f32; 3]]) -> usize {
        frames
            .windows(2)
            .filter(|w| w[0][0] < 0.0 && w[1][0] >= 0.0)
            .count()
    }

    #[test]
    fn free_runs_at_the_rate_knob() {
        for rate in [0.3, 0.5, 0.7] {
            // 10 seconds of a sine, starting just after a rise.
            let frames = free([rate, 0.0, 0.0, 1.0, 0.5, 0.0], 10 * SR as usize);
            let expected = 0.02 * 2000f32.powf(rate) * 10.0;
            let got = rises(&frames) as f32;
            assert!(
                (got - expected).abs() <= 1.01,
                "{rate}: {got} vs {expected}"
            );
        }
    }

    #[test]
    fn sync_locks_to_the_patch_tempo() {
        // Position 5 of 7 is one beat a cycle: 2 Hz at 120 BPM. Rate is ignored.
        for rate in [0.0, 0.9] {
            let frames = free([rate, 5.0 / 7.0, 0.0, 1.0, 0.5, 0.0], 4 * SR as usize);
            assert!((rises(&frames) as i32 - 8).abs() <= 1, "{}", rises(&frames));
        }
        // A quarter beat a cycle: 8 Hz.
        let frames = free([0.4, 1.0, 0.0, 1.0, 0.5, 0.0], 2 * SR as usize);
        assert!(
            (rises(&frames) as i32 - 16).abs() <= 1,
            "{}",
            rises(&frames)
        );
    }

    #[test]
    fn rate_cv_changes_the_speed() {
        let slow = render(
            1,
            [0.4, 0.0, 0.0, 1.0, 0.5, 0.0],
            4 * SR as usize,
            0.0,
            &|_| 0.0,
        );
        let fast = render(
            1,
            [0.4, 0.0, 0.0, 1.0, 0.5, 0.0],
            4 * SR as usize,
            3.0,
            &|_| 0.0,
        );
        assert!(rises(&fast) > 2 * rises(&slow));
    }

    #[test]
    fn swings_five_volts_either_way_and_depth_scales_it() {
        let peak = |depth| {
            free([0.6, 0.0, 0.0, depth, 0.5, 0.0], 2 * SR as usize)
                .iter()
                .fold(0f32, |p, f| p.max(f[0].abs()))
        };
        assert!((peak(1.0) - 5.0).abs() < 0.01, "{}", peak(1.0));
        assert!((peak(0.5) - 2.5).abs() < 0.01, "{}", peak(0.5));
        assert_eq!(peak(0.0), 0.0);
    }

    #[test]
    fn offset_shifts_the_wave() {
        let mean = |offset| {
            let frames = free([0.6, 0.0, 0.0, 0.0, offset, 0.0], 4800);
            frames.iter().map(|f| f[0]).sum::<f32>() / frames.len() as f32
        };
        assert!(mean(0.5).abs() < 1e-5);
        assert!((mean(1.0) - 5.0).abs() < 1e-4);
        assert!((mean(0.0) + 5.0).abs() < 1e-4);
    }

    #[test]
    fn inverted_is_the_wave_upside_down_and_pulse_is_the_first_half() {
        // 1 Hz square, so each half lasts half a second.
        let rate = (1.0f32 / 0.02).ln() / 2000f32.ln();
        let frames = free([rate, 0.0, 0.75, 1.0, 0.5, 0.0], 2 * SR as usize);
        assert!(frames.iter().all(|f| f[1] == -f[0]));
        let high = frames.iter().filter(|f| f[2] == 10.0).count() as f32 / frames.len() as f32;
        assert!((high - 0.5).abs() < 0.01, "{high}");
        assert!(frames.iter().all(|f| f[2] == 0.0 || f[2] == 10.0));
    }

    #[test]
    fn reset_restarts_the_wave_at_phase() {
        let reset_at = 20_000;
        let reset = |i: usize| {
            if (reset_at..reset_at + 100).contains(&i) {
                10.0
            } else {
                0.0
            }
        };
        // A quarter turn of phase: a sine then starts at its peak.
        let frames = render(1, [0.5, 0.0, 0.0, 1.0, 0.5, 0.25], 30_000, 0.0, &reset);
        assert!(frames[reset_at - 1][0] < 4.9);
        assert!(frames[reset_at][0] > 4.99, "{}", frames[reset_at][0]);
        // With phase 0 it restarts rising through zero.
        let frames = render(1, [0.5, 0.0, 0.0, 1.0, 0.5, 0.0], 30_000, 0.0, &reset);
        assert!(frames[reset_at][0].abs() < 0.05, "{}", frames[reset_at][0]);
        assert!(frames[reset_at + 10][0] > frames[reset_at][0]);
    }

    #[test]
    fn turning_shape_does_not_click() {
        // A very slow wave, so any jump comes from the knob. Shape leaps from sine to
        // triangle at the same place in the cycle (0.3 of the way through).
        let mut rocker = State::new(1, SR);
        let mut last: Option<f32> = None;
        let mut worst = 0f32;
        for i in 0..4800 {
            let shape = if i < 2400 { 0.0 } else { 0.25 };
            let mut out = [0.0f32; 3];
            rocker.process(&mut Io {
                sample_rate: SR,
                tempo: 120.0,
                params: &[0.0, 0.0, shape, 1.0, 0.5, 0.3],
                inputs: &[0.0, 0.0],
                connected: &[false, false],
                outputs: &mut out,
            });
            if let Some(prev) = last {
                worst = worst.max((out[0] - prev).abs());
            }
            last = Some(out[0]);
        }
        assert!(worst < 0.05, "{worst}");
    }

    #[test]
    fn stepped_random_holds_for_a_cycle_and_follows_the_seed() {
        let params = [0.6, 0.0, 1.0, 1.0, 0.5, 0.0];
        let a = render(3, params, 24_000, 0.0, &|_| 0.0);
        let changes = (1..a.len()).filter(|&i| a[i][0] != a[i - 1][0]).count();
        // 0.02 * 2000^0.6 is about 1.9 Hz: a few picks in half a second, never a glide.
        assert!((0..4).contains(&changes), "{changes}");
        assert_eq!(a, render(3, params, 24_000, 0.0, &|_| 0.0));
        assert_ne!(a, render(4, params, 24_000, 0.0, &|_| 0.0));
    }

    #[test]
    fn stays_finite_and_bounded_at_the_extremes() {
        for bits in 0..64u32 {
            let params: [f32; 6] = std::array::from_fn(|k| (bits >> k & 1) as f32);
            for cv in [-10.0, 0.0, 10.0] {
                let frames = render(1, params, 4800, cv, &|i| {
                    if i % 700 < 20 { 10.0 } else { 0.0 }
                });
                assert!(
                    frames
                        .iter()
                        .all(|f| f.iter().all(|v| v.is_finite()) && f[0].abs() <= 10.001),
                    "{params:?} {cv}"
                );
            }
        }
    }
}
