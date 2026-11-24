use std::f32::consts::TAU;

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{CV_MAX, Edge, GATE_HIGH, Gate, Rng, flush};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Chatter",
    band: Band::Motion,
    hp: 4,
    summary: "A shaken spring",
    params: &[
        param(
            "energy",
            "Energy",
            0.5,
            "How hard each kick hits the spring. Bigger kicks swing it further.",
        ),
        param(
            "tension",
            "Tension",
            0.5,
            "How tight the spring is. Tight springs shake quickly, loose ones swing slowly.",
        ),
        param(
            "damping",
            "Damping",
            0.4,
            "How quickly it settles. Low keeps it ringing, high stops it dead.",
        ),
        param(
            "rate",
            "Rate",
            0.3,
            "How often it gets kicked at random. 0 means only the Kick jack moves it.",
        ),
        param(
            "depth",
            "Depth",
            1.0,
            "Scales Position and Velocity. 0 holds both at 0 V.",
        ),
    ],
    inputs: &[jack(
        "kick",
        "Kick",
        Gate,
        "Knocks the spring once on every pulse.",
    )],
    outputs: &[
        jack(
            "position",
            "Position",
            Cv,
            "Where the spring is, above and below 0 V. Never past 5 V either way.",
        ),
        jack(
            "velocity",
            "Velocity",
            Cv,
            "How fast it is moving, above and below 0 V. Leads Position by a quarter of a swing.",
        ),
        jack(
            "moving",
            "Moving",
            Gate,
            "High while the spring is moving hard.",
        ),
    ],
};

/// Natural frequency of the spring in hertz at the bottom and top of Tension.
const SLOWEST_HZ: f32 = 0.3;
const FASTEST_HZ: f32 = 60.0;
/// Damping ratio at the bottom and top of Damping. The bottom still decays,
/// so random kicks cannot build energy for ever; the top is well past dead.
const LOOSEST: f32 = 0.03;
const TIGHTEST: f32 = 1.5;
/// Random kicks per second at the top of Rate; the knob is squared.
const MOST_KICKS_HZ: f32 = 40.0;
/// Moving switches on above this speed (as a fraction of the stops) and off below the lower one.
const MOVING_ON: f32 = 0.3;
const MOVING_OFF: f32 = 0.15;
/// The spring stops at ±1 and bounces off them, losing half its speed.
const STOP: f32 = 1.0;
const MOST_SPEED: f32 = 2.0;

pub struct State {
    kick: Gate,
    rng: Rng,
    /// Position, `-1..1`.
    x: f32,
    /// Speed in positions per radian, so a swing of any frequency has the
    /// same speed as size. `-2..2`.
    u: f32,
    moving: bool,
}

impl State {
    pub fn new(seed: u64) -> Self {
        Self {
            kick: Gate::default(),
            rng: Rng::new(seed),
            x: 0.0,
            u: 0.0,
            moving: false,
        }
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[energy, tension, damping, rate, depth] = io.params else {
            return;
        };
        let rng = &mut self.rng;
        if self.kick.update(io.inputs[0]) == Edge::Rose {
            self.u += energy;
        }
        let kicks_per_sample = rate * rate * MOST_KICKS_HZ / io.sample_rate;
        if rng.unit() < kicks_per_sample {
            let size = 0.3 + 0.7 * rng.unit();
            let sign = if rng.unit() < 0.5 { -1.0 } else { 1.0 };
            self.u += sign * size * energy;
        }
        // Radians per sample, held under 0.5 so the integrator is stable at
        // any sample rate.
        let w =
            (TAU * SLOWEST_HZ * (FASTEST_HZ / SLOWEST_HZ).powf(tension) / io.sample_rate).min(0.5);
        let zeta = LOOSEST * (TIGHTEST / LOOSEST).powf(damping);
        // Semi-implicit Euler: speed from the old position, position from the new speed.
        self.u += w * (-self.x - 2.0 * zeta * self.u);
        self.x += w * self.u;
        if self.x.abs() > STOP {
            if self.x * self.u > 0.0 {
                self.u *= -0.5;
            }
            self.x = self.x.clamp(-STOP, STOP);
        }
        self.u = self.u.clamp(-MOST_SPEED, MOST_SPEED);
        if !(self.x.is_finite() && self.u.is_finite()) {
            self.x = 0.0;
            self.u = 0.0;
        }
        self.x = flush(self.x);
        self.u = flush(self.u);
        let speed = self.u.abs();
        if speed > MOVING_ON {
            self.moving = true;
        } else if speed < MOVING_OFF {
            self.moving = false;
        }
        let half = CV_MAX / 2.0;
        io.outputs[0] = self.x * half * depth;
        io.outputs[1] = self.u.clamp(-1.0, 1.0) * half * depth;
        io.outputs[2] = if self.moving { GATE_HIGH } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::{Dsp, Io};

    const SR: f32 = 48_000.0;

    /// Tension that puts the spring at 5 Hz: 0.3 * 200^t.
    fn five_hz() -> f32 {
        (5.0f32 / 0.3).ln() / 200f32.ln()
    }

    /// Frames are `[position, velocity, moving]`; `kick` gives the Kick voltage per sample.
    fn render_at(
        sample_rate: f32,
        seed: u64,
        params: [f32; 5],
        samples: usize,
        kick: &dyn Fn(usize) -> f32,
    ) -> Vec<[f32; 3]> {
        let mut chatter = State::new(seed);
        (0..samples)
            .map(|i| {
                let mut out = [0.0; 3];
                chatter.process(&mut Io {
                    sample_rate,
                    tempo: 120.0,
                    params: &params,
                    inputs: &[kick(i)],
                    connected: &[true],
                    outputs: &mut out,
                });
                out
            })
            .collect()
    }

    fn render(
        seed: u64,
        params: [f32; 5],
        samples: usize,
        kick: &dyn Fn(usize) -> f32,
    ) -> Vec<[f32; 3]> {
        render_at(SR, seed, params, samples, kick)
    }

    /// One kick at sample 100.
    fn once(i: usize) -> f32 {
        if (100..200).contains(&i) { 10.0 } else { 0.0 }
    }

    fn never(_: usize) -> f32 {
        0.0
    }

    fn peak(frames: &[[f32; 3]], lane: usize) -> f32 {
        frames.iter().fold(0f32, |p, f| p.max(f[lane].abs()))
    }

    #[test]
    fn rests_at_zero_until_something_kicks_it() {
        let frames = render(1, [1.0, 0.5, 0.0, 0.0, 1.0], 48_000, &never);
        assert!(frames.iter().all(|f| *f == [0.0; 3]));
    }

    #[test]
    fn a_kick_rings_at_the_tension() {
        // Loosest damping at 5 Hz: about 10 swings in two seconds.
        let frames = render(1, [0.5, five_hz(), 0.0, 0.0, 1.0], 2 * SR as usize, &once);
        let rises = frames
            .windows(2)
            .filter(|w| w[0][0] < 0.0 && w[1][0] >= 0.0)
            .count();
        assert!((9..=10).contains(&rises), "{rises}");
        assert!(peak(&frames, 0) > 2.0);
    }

    #[test]
    fn energy_sets_the_size_of_the_swing() {
        let size = |energy| {
            peak(
                &render(1, [energy, five_hz(), 0.4, 0.0, 1.0], 24_000, &once),
                0,
            )
        };
        assert!(size(0.2) < 0.5 * size(0.8), "{} {}", size(0.2), size(0.8));
        assert!(size(0.0) == 0.0);
    }

    #[test]
    fn damping_decides_how_long_it_rings() {
        let tail = |damping| {
            let frames = render(
                1,
                [0.6, five_hz(), damping, 0.0, 1.0],
                2 * SR as usize,
                &once,
            );
            peak(&frames[SR as usize..], 0)
        };
        assert!(tail(0.0) > 0.5, "{}", tail(0.0));
        assert!(tail(0.6) < 0.1 * tail(0.0));
        // Right at the top it is past critical: it creeps back without crossing zero.
        let frames = render(1, [0.6, five_hz(), 1.0, 0.0, 1.0], 4 * SR as usize, &once);
        assert!(frames.iter().all(|f| f[0] >= 0.0));
    }

    #[test]
    fn velocity_leads_position() {
        // Velocity is zero as position turns round at the top of the first swing,
        // and positive on the way up before it.
        let frames = render(1, [0.5, five_hz(), 0.0, 0.0, 1.0], SR as usize / 2, &once);
        let first_peak = (1..frames.len())
            .find(|&i| frames[i][0] < frames[i - 1][0])
            .unwrap();
        assert!(
            frames[first_peak][1].abs() < 0.3,
            "{}",
            frames[first_peak][1]
        );
        assert!(peak(&frames, 1) > 1.0);
        assert!(frames[100][1] >= 0.0 && frames[1000][1] > 0.0);
    }

    #[test]
    fn moving_is_high_while_it_swings_hard_and_low_once_settled() {
        let frames = render(1, [0.8, five_hz(), 0.5, 0.0, 1.0], 2 * SR as usize, &once);
        assert_eq!(frames[300][2], 10.0);
        assert_eq!(frames[2 * SR as usize - 1][2], 0.0);
        assert!(frames.iter().all(|f| f[2] == 0.0 || f[2] == 10.0));
    }

    #[test]
    fn random_kicks_move_it_without_a_trigger_and_follow_the_seed() {
        let params = [0.7, 0.5, 0.4, 0.5, 1.0];
        let a = render(3, params, 48_000, &never);
        assert!(peak(&a, 0) > 0.5);
        assert_eq!(a, render(3, params, 48_000, &never));
        assert_ne!(a, render(4, params, 48_000, &never));
    }

    #[test]
    fn depth_scales_position_and_velocity_but_not_the_gate() {
        let full = render(1, [0.5, five_hz(), 0.2, 0.0, 1.0], 24_000, &once);
        let half = render(1, [0.5, five_hz(), 0.2, 0.0, 0.5], 24_000, &once);
        for (f, h) in full.iter().zip(&half) {
            assert!((f[0] * 0.5 - h[0]).abs() < 1e-6 && (f[1] * 0.5 - h[1]).abs() < 1e-6);
            assert_eq!(f[2], h[2]);
        }
        let flat = render(1, [0.5, five_hz(), 0.2, 0.0, 0.0], 24_000, &once);
        assert!(flat.iter().all(|f| f[0] == 0.0 && f[1] == 0.0));
    }

    #[test]
    fn never_blows_up_at_any_setting_or_sample_rate() {
        // Every knob at each end and in the middle, kicked hard and often.
        let ends = [0.0, 0.5, 1.0];
        for sample_rate in [8_000.0, 48_000.0, 192_000.0] {
            for a in ends {
                for b in ends {
                    for c in ends {
                        for d in ends {
                            let frames = render_at(
                                sample_rate,
                                1,
                                [a, b, c, d, 1.0],
                                (sample_rate * 0.5) as usize,
                                &|i| if i % 50 < 10 { 10.0 } else { 0.0 },
                            );
                            let ok = frames.iter().all(|f| {
                                f.iter().all(|v| v.is_finite())
                                    && f[0].abs() <= 5.0
                                    && f[1].abs() <= 5.0
                            });
                            assert!(ok, "{sample_rate} {a} {b} {c} {d}");
                        }
                    }
                }
            }
        }
    }
}
