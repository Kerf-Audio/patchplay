use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{CV_MAX, Edge, GATE_HIGH, Gate, OnePole, PULSE_SECONDS, Phase, Rng, smoothing};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Burl",
    band: Band::Motion,
    hp: 6,
    summary: "Random voltages",
    params: &[
        param(
            "rate",
            "Rate",
            0.4,
            "How often a new random value is picked, when nothing is patched into Trig.",
        ),
        param(
            "smooth",
            "Smooth",
            0.6,
            "How slowly Drift glides to each new value.",
        ),
        param("bias", "Bias", 0.5, "Pushes the random values up or down."),
        param(
            "depth",
            "Depth",
            1.0,
            "Scales Stepped and Drift. 0 holds both at 0 V.",
        ),
    ],
    inputs: &[jack(
        "trig",
        "Trig",
        Gate,
        "Picks a new value on every pulse.",
    )],
    outputs: &[
        jack(
            "stepped",
            "Stepped",
            Cv,
            "Holds each random value until the next.",
        ),
        jack(
            "drift",
            "Drift",
            Cv,
            "Glides smoothly between random values.",
        ),
        jack(
            "coin",
            "Coin",
            Gate,
            "Passes each trigger on, or not, at random.",
        ),
    ],
};

/// Free-running picks per second at the bottom and top of the Rate knob.
const SLOWEST_HZ: f32 = 0.05;
const FASTEST_HZ: f32 = 20.0;
/// Time Drift takes to settle on a new value at the bottom and top of Smooth.
const QUICKEST_GLIDE: f32 = 0.001;
const SLOWEST_GLIDE: f32 = 5.0;

pub struct State {
    trig: Gate,
    /// Free-running clock, used while Trig is unpatched.
    clock: Phase,
    rng: Rng,
    /// The value being held, `0..1`; `None` until the first sample.
    held: Option<f32>,
    drift: OnePole,
    pulse_left: u32,
}

impl State {
    pub fn new(seed: u64) -> Self {
        Self {
            trig: Gate::default(),
            clock: Phase::default(),
            rng: Rng::new(seed),
            held: None,
            drift: OnePole::default(),
            pulse_left: 0,
        }
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[rate, smooth, bias, depth] = io.params else {
            return;
        };
        let triggered = if io.connected[0] {
            self.trig.update(io.inputs[0]) == Edge::Rose
        } else {
            let hz = SLOWEST_HZ * (FASTEST_HZ / SLOWEST_HZ).powf(rate);
            let dt = hz / io.sample_rate;
            self.clock.advance(dt) < dt
        };
        let first = self.held.is_none();
        if triggered || first {
            // Bias 0.5 leaves the values even; the ends crowd them towards 0 or 1.
            let skew = 4f32.powf(1.0 - 2.0 * bias);
            self.held = Some(self.rng.unit().powf(skew));
            if triggered && self.rng.unit() < 0.5 {
                self.pulse_left = (PULSE_SECONDS * io.sample_rate) as u32;
            }
        }
        let held = self.held.unwrap_or_default();
        let glide = if first {
            1.0
        } else {
            let seconds = QUICKEST_GLIDE * (SLOWEST_GLIDE / QUICKEST_GLIDE).powf(smooth);
            smoothing(seconds, io.sample_rate)
        };
        io.outputs[0] = held * CV_MAX * depth;
        io.outputs[1] = self.drift.process(held, glide) * CV_MAX * depth;
        io.outputs[2] = if self.pulse_left > 0 {
            self.pulse_left -= 1;
            GATE_HIGH
        } else {
            0.0
        };
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::{Dsp, Io};

    const SR: f32 = 48_000.0;

    /// Renders `samples` frames; `trig` gives the Trig voltage per sample,
    /// or `None` to leave it unpatched. Frames are `[stepped, drift, coin]`.
    fn render(
        seed: u64,
        params: [f32; 3],
        samples: usize,
        trig: Option<&dyn Fn(usize) -> f32>,
    ) -> Vec<[f32; 3]> {
        let mut burl = State::new(seed);
        (0..samples)
            .map(|i| {
                let mut out = [0.0; 3];
                burl.process(&mut Io {
                    sample_rate: SR,
                    tempo: 120.0,
                    params: &[params[0], params[1], params[2], 1.0],
                    inputs: &[trig.map_or(0.0, |f| f(i))],
                    connected: &[trig.is_some()],
                    outputs: &mut out,
                });
                out
            })
            .collect()
    }

    /// A 10 Hz pulse train: one pulse every 4800 samples, starting at 100.
    fn pulses(i: usize) -> f32 {
        if (i + 4700) % 4800 < 240 { 10.0 } else { 0.0 }
    }

    fn changes(frames: &[[f32; 3]], lane: usize) -> Vec<usize> {
        (1..frames.len())
            .filter(|&i| frames[i][lane] != frames[i - 1][lane])
            .collect()
    }

    #[test]
    fn stepped_holds_between_triggers() {
        let frames = render(1, [0.5, 0.5, 0.5], 48_000, Some(&pulses));
        let changed = changes(&frames, 0);
        assert_eq!(changed.len(), 10);
        assert!(
            changed.iter().all(|i| (i + 4700) % 4800 == 0),
            "{changed:?}"
        );
        assert!(frames.iter().all(|f| (0.0..=10.0).contains(&f[0])));
    }

    #[test]
    fn free_runs_at_the_rate_when_trig_is_unpatched() {
        // 0.05 * 400^0.5 = 1 Hz. The extra half second keeps the tenth pick in.
        let frames = render(1, [0.5, 0.5, 0.5], 10 * SR as usize + 24_000, None);
        let changed = changes(&frames, 0);
        assert_eq!(changed.len(), 10, "{changed:?}");
        let gap = (changed[9] - changed[0]) as f32 / 9.0;
        assert!((gap - SR).abs() < 0.005 * SR, "{gap}");
    }

    #[test]
    fn same_seed_same_sequence() {
        let run = |seed| render(seed, [0.5, 0.5, 0.5], 24_000, Some(&pulses));
        assert_eq!(run(7), run(7));
        assert_ne!(run(7), run(8));
    }

    #[test]
    fn smooth_sets_how_slowly_drift_glides() {
        let quick = render(3, [0.5, 0.0, 0.5], 48_000, Some(&pulses));
        // 83 ms after each pulse Drift has arrived at Stepped.
        for i in (100 + 4000..48_000).step_by(4800) {
            assert!((quick[i][1] - quick[i][0]).abs() < 0.01, "sample {i}");
        }
        let slow = render(3, [0.5, 1.0, 0.5], 48_000, Some(&pulses));
        let steepest = slow.windows(2).map(|w| (w[1][1] - w[0][1]).abs());
        assert!(steepest.fold(0.0, f32::max) < 0.001);
        assert!(slow.iter().all(|f| (0.0..=10.0).contains(&f[1])));
    }

    #[test]
    fn bias_pushes_values_up_or_down() {
        let mean = |bias| {
            let frames = render(5, [0.5, 0.0, bias], 400 * 4800, Some(&pulses));
            let held: Vec<f32> = changes(&frames, 0).iter().map(|&i| frames[i][0]).collect();
            held.iter().sum::<f32>() / held.len() as f32
        };
        assert!((mean(0.5) - 5.0).abs() < 0.5);
        assert!(mean(1.0) > 7.0);
        assert!(mean(0.0) < 3.0);
    }

    #[test]
    fn coin_passes_about_half_the_triggers() {
        let frames = render(9, [0.5, 0.5, 0.5], 400 * 4800, Some(&pulses));
        let passed = (1..frames.len())
            .filter(|&i| frames[i - 1][2] < 1.0 && frames[i][2] > 1.0)
            .count();
        assert!((150..250).contains(&passed), "{passed} of 400");
    }
}
