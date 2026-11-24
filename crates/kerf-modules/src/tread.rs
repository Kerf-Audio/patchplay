use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{CV_MAX, Edge, GATE_HIGH, Gate, OnePole, PULSE_SECONDS, Rng, smoothing};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Tread",
    band: Band::Motion,
    hp: 6,
    summary: "Eight-step modulator",
    params: &[
        param(
            "step1",
            "1",
            0.5,
            "Value of step 1. Centre is 0 V; the ends are -5 V and +5 V.",
        ),
        param("step2", "2", 0.75, "Value of step 2."),
        param("step3", "3", 0.35, "Value of step 3."),
        param("step4", "4", 0.9, "Value of step 4."),
        param("step5", "5", 0.5, "Value of step 5."),
        param("step6", "6", 0.2, "Value of step 6."),
        param("step7", "7", 0.65, "Value of step 7."),
        param("step8", "8", 0.8, "Value of step 8."),
        param("length", "Length", 1.0, "How many steps play, 1 to 8."),
        param(
            "direction",
            "Direction",
            0.0,
            "Which way it walks the steps: forward, back, pendulum (there and back) or random.",
        ),
        param(
            "glide",
            "Glide",
            0.0,
            "Slides from one step value to the next. At 0 the steps jump.",
        ),
        param(
            "rate",
            "Rate",
            0.5,
            "How fast it steps on its own, when nothing is patched into Clock.",
        ),
        param(
            "depth",
            "Depth",
            1.0,
            "Scales the CV out. 0 holds it at 0 V.",
        ),
    ],
    inputs: &[
        jack("clock", "Clock", Clock, "Moves on one step per pulse."),
        jack("reset", "Reset", Gate, "Back to the start of the walk."),
    ],
    outputs: &[
        jack(
            "cv",
            "CV",
            Cv,
            "The value of the current step, above and below 0 V.",
        ),
        jack("step", "Step", Gate, "Pulses on every step."),
        jack(
            "end",
            "End",
            Gate,
            "Pulses when a walk through the steps finishes.",
        ),
    ],
};

/// Free-running steps per second at the bottom and top of the Rate knob.
const SLOWEST_HZ: f32 = 0.05;
const FASTEST_HZ: f32 = 20.0;
/// Longest glide, at the top of the knob.
const LONGEST_GLIDE: f32 = 0.25;
const STEPS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Forward,
    Back,
    Pendulum,
    Random,
}

impl Direction {
    fn from_param(p: f32) -> Self {
        match (p * 4.0) as usize {
            0 => Self::Forward,
            1 => Self::Back,
            2 => Self::Pendulum,
            _ => Self::Random,
        }
    }
}

pub struct State {
    clock: Gate,
    reset: Gate,
    rng: Rng,
    /// Free-running clock, used while Clock is unpatched. It starts full, so
    /// the first step plays at once.
    free: f32,
    /// Clocks into the walk; `None` until the first one after a reset.
    tick: Option<u32>,
    pos: usize,
    glide: OnePole,
    /// False until the first sample, which jumps straight to the step.
    settled: bool,
    step_left: u32,
    end_left: u32,
}

impl State {
    pub fn new(seed: u64) -> Self {
        Self {
            clock: Gate::default(),
            reset: Gate::default(),
            rng: Rng::new(seed),
            free: 1.0,
            tick: None,
            pos: 0,
            glide: OnePole::default(),
            settled: false,
            step_left: 0,
            end_left: 0,
        }
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let (steps, rest) = io.params.split_at(STEPS);
        let &[length, direction, glide, rate, depth] = rest else {
            return;
        };
        let patched = io.connected[0];
        if self.reset.update(io.inputs[1]) == Edge::Rose {
            self.tick = None;
            // Free-running, step 1 comes round again at once.
            self.free = 1.0;
        }
        let ticked = if patched {
            self.clock.update(io.inputs[0]) == Edge::Rose
        } else {
            let hz = SLOWEST_HZ * (FASTEST_HZ / SLOWEST_HZ).powf(rate);
            self.free += hz / io.sample_rate;
            let due = self.free >= 1.0;
            if due {
                self.free -= self.free.floor();
            }
            due
        };
        let len = 1 + (length * (STEPS - 1) as f32).round() as u32;
        let direction = Direction::from_param(direction);
        if ticked {
            let cycle = if direction == Direction::Pendulum {
                (2 * (len - 1)).max(1)
            } else {
                len
            };
            let mut next = self.tick.map_or(0, |t| t + 1);
            let wrapped = self.tick.is_some() && next >= cycle;
            if next >= cycle {
                next = 0;
            }
            self.tick = Some(next);
            self.pos = match direction {
                Direction::Forward => next,
                Direction::Back => len - 1 - next,
                Direction::Pendulum if next < len => next,
                Direction::Pendulum => 2 * (len - 1) - next,
                Direction::Random => ((self.rng.unit() * len as f32) as u32).min(len - 1),
            } as usize;
            self.step_left = (PULSE_SECONDS * io.sample_rate) as u32;
            if wrapped {
                self.end_left = self.step_left;
            }
        }
        let value = steps[self.pos.min(len as usize - 1)] * 2.0 - 1.0;
        let coeff = if self.settled {
            smoothing(glide * glide * LONGEST_GLIDE, io.sample_rate)
        } else {
            1.0
        };
        self.settled = true;
        io.outputs[0] = self.glide.process(value, coeff) * (CV_MAX / 2.0) * depth;
        for (out, left) in io.outputs[1..]
            .iter_mut()
            .zip([&mut self.step_left, &mut self.end_left])
        {
            *out = if *left > 0 {
                *left -= 1;
                GATE_HIGH
            } else {
                0.0
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::{Dsp, Io};

    const SR: f32 = 48_000.0;
    /// Steps at 0, 1/7 ... 1, so a step's output volts say which one played.
    const RAMP: [f32; 8] = [
        0.0,
        1.0 / 7.0,
        2.0 / 7.0,
        3.0 / 7.0,
        4.0 / 7.0,
        5.0 / 7.0,
        6.0 / 7.0,
        1.0,
    ];

    /// `[steps.., length, direction, glide, rate, depth]`.
    fn params(length: f32, direction: f32, glide: f32) -> [f32; 13] {
        let mut p = [0.0; 13];
        p[..8].copy_from_slice(&RAMP);
        p[8..].copy_from_slice(&[length, direction, glide, 0.5, 1.0]);
        p
    }

    /// Frames are `[cv, step, end]`. `clock` gives the Clock voltage per
    /// sample, or `None` to leave it unpatched; `reset` likewise for Reset.
    fn render(
        seed: u64,
        params: &[f32; 13],
        samples: usize,
        clock: Option<&dyn Fn(usize) -> f32>,
        reset: &dyn Fn(usize) -> f32,
    ) -> Vec<[f32; 3]> {
        let mut tread = State::new(seed);
        (0..samples)
            .map(|i| {
                let mut out = [0.0; 3];
                tread.process(&mut Io {
                    sample_rate: SR,
                    tempo: 120.0,
                    params,
                    inputs: &[clock.map_or(0.0, |f| f(i)), reset(i)],
                    connected: &[clock.is_some(), true],
                    outputs: &mut out,
                });
                out
            })
            .collect()
    }

    /// A pulse every 1000 samples, the first at sample 100.
    fn clock(i: usize) -> f32 {
        if (i + 900) % 1000 < 50 { 10.0 } else { 0.0 }
    }

    fn none(_: usize) -> f32 {
        0.0
    }

    /// Which step plays after each of the first `n` clock pulses.
    fn walk(params: &[f32; 13], n: usize, seed: u64) -> Vec<usize> {
        let frames = render(seed, params, 1000 * n + 500, Some(&clock), &none);
        (0..n)
            .map(|k| {
                let volts = frames[100 + 1000 * k + 500][0];
                ((volts / 5.0 + 1.0) / 2.0 * 7.0).round() as usize
            })
            .collect()
    }

    #[test]
    fn walks_forward_and_wraps() {
        assert_eq!(
            walk(&params(1.0, 0.0, 0.0), 10, 1),
            [0, 1, 2, 3, 4, 5, 6, 7, 0, 1]
        );
    }

    #[test]
    fn length_cuts_the_walk_short() {
        // 3 of 8 steps.
        assert_eq!(
            walk(&params(2.0 / 7.0, 0.0, 0.0), 7, 1),
            [0, 1, 2, 0, 1, 2, 0]
        );
        assert_eq!(walk(&params(0.0, 0.0, 0.0), 3, 1), [0, 0, 0]);
    }

    #[test]
    fn walks_back_and_there_and_back() {
        // Length 4.
        assert_eq!(
            walk(&params(3.0 / 7.0, 0.3, 0.0), 9, 1),
            [3, 2, 1, 0, 3, 2, 1, 0, 3]
        );
        assert_eq!(
            walk(&params(3.0 / 7.0, 0.6, 0.0), 9, 1),
            [0, 1, 2, 3, 2, 1, 0, 1, 2]
        );
    }

    #[test]
    fn random_stays_inside_the_length_and_follows_the_seed() {
        let p = params(3.0 / 7.0, 1.0, 0.0);
        let a = walk(&p, 40, 5);
        assert!(a.iter().all(|&s| s < 4), "{a:?}");
        assert!((0..4).all(|s| a.contains(&s)), "{a:?}");
        assert_eq!(a, walk(&p, 40, 5));
        assert_ne!(a, walk(&p, 40, 6));
    }

    #[test]
    fn pulses_on_every_step_and_at_the_end_of_each_walk() {
        let frames = render(1, &params(3.0 / 7.0, 0.0, 0.0), 12_000, Some(&clock), &none);
        let rises = |lane: usize| -> Vec<usize> {
            (1..frames.len())
                .filter(|&i| frames[i][lane] > 5.0 && frames[i - 1][lane] < 5.0)
                .collect()
        };
        assert_eq!(rises(1).len(), 12);
        // The walk finishes as step 1 comes round again: clocks 5, 9, ... (0-based 4, 8).
        assert_eq!(rises(2), [4100, 8100]);
        // A pendulum over 4 steps is 6 clocks long.
        let frames = render(1, &params(3.0 / 7.0, 0.6, 0.0), 14_000, Some(&clock), &none);
        let ends: Vec<_> = (1..frames.len())
            .filter(|&i| frames[i][2] > 5.0 && frames[i - 1][2] < 5.0)
            .collect();
        assert_eq!(ends, [6100, 12_100]);
    }

    #[test]
    fn reset_goes_back_to_the_first_step() {
        let reset = |i: usize| {
            if (3_500..3_600).contains(&i) {
                10.0
            } else {
                0.0
            }
        };
        let frames = render(1, &params(1.0, 0.0, 0.0), 8_000, Some(&clock), &reset);
        let step = |i: usize| ((frames[i][0] / 5.0 + 1.0) / 2.0 * 7.0).round() as usize;
        assert_eq!(step(3_400), 3);
        // The pulse at 4100 plays step 1 rather than step 5.
        assert_eq!(step(4_500), 0);
        assert_eq!(step(5_500), 1);
    }

    #[test]
    fn free_runs_at_the_rate_when_clock_is_unpatched() {
        // 0.05 * 400^0.5 = 1 Hz: a step at once, then one a second.
        let frames = render(
            1,
            &params(1.0, 0.0, 0.0),
            10 * SR as usize + 24_000,
            None,
            &none,
        );
        let rises: Vec<usize> = (1..frames.len())
            .filter(|&i| frames[i][1] > 5.0 && frames[i - 1][1] < 5.0)
            .collect();
        // The step at the very start has no rising edge to count.
        assert_eq!(rises.len(), 10, "{rises:?}");
        let gap = (rises[9] - rises[0]) as f32 / 9.0;
        assert!((gap - SR).abs() < 0.005 * SR, "{gap}");
    }

    #[test]
    fn glide_slides_between_steps_and_zero_jumps() {
        let jump = render(1, &params(1.0, 0.0, 0.0), 2_000, Some(&clock), &none);
        assert!(jump[1_200][0] > jump[50][0] + 1.0);
        assert_eq!(jump[1_200][0], jump[1_900][0]);
        let slid = render(1, &params(1.0, 0.0, 0.5), 2_000, Some(&clock), &none);
        // Just after the pulse it has hardly moved; by the end it is nearly there.
        assert!(slid[1_120][0] - slid[1_090][0] > 0.0);
        assert!(slid[1_120][0] < jump[1_120][0] - 0.1);
        assert!(slid[1_900][0] > slid[1_120][0]);
    }

    #[test]
    fn depth_scales_the_output() {
        let mut p = params(1.0, 0.0, 0.0);
        p[12] = 0.5;
        // After the eighth pulse the walk sits on step 8, at +5 V.
        let frames = render(1, &p, 8_000, Some(&clock), &none);
        assert!(
            (frames[7_600][0] - 2.5).abs() < 1e-5,
            "{}",
            frames[7_600][0]
        );
        p[12] = 0.0;
        let frames = render(1, &p, 8_000, Some(&clock), &none);
        assert!(frames.iter().all(|f| f[0] == 0.0));
    }

    #[test]
    fn stays_finite_and_bounded_at_the_extremes() {
        for bits in 0..32u32 {
            let mut p = [0.5; 13];
            for (k, v) in p[8..].iter_mut().enumerate() {
                *v = (bits >> k & 1) as f32;
            }
            for (k, v) in p[..8].iter_mut().enumerate() {
                *v = (k % 2) as f32;
            }
            for patched in [true, false] {
                let clock: &dyn Fn(usize) -> f32 = &|i| if i % 37 < 3 { 10.0 } else { 0.0 };
                let frames = render(1, &p, 5_000, patched.then_some(clock), &|i| {
                    if i % 900 < 5 { 10.0 } else { 0.0 }
                });
                assert!(
                    frames
                        .iter()
                        .all(|f| f.iter().all(|v| v.is_finite()) && f[0].abs() <= 5.0),
                    "{p:?}"
                );
            }
        }
    }
}
