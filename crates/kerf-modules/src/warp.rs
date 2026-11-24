use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{OnePole, Rng, smoothing};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Warp",
    band: Band::Motion,
    hp: 4,
    summary: "Drift and jitter",
    params: &[
        param(
            "amount",
            "Amount",
            0.3,
            "How far the pitch wanders. Small settings are a gentle human wobble; the top is wild. 0 passes In through untouched.",
        ),
        param(
            "speed",
            "Speed",
            0.4,
            "How quickly it wanders, from a slow drift over many seconds to a fast shake.",
        ),
        param(
            "rough",
            "Rough",
            0.3,
            "Smooth wandering at 0, abrupt little jumps at 1.",
        ),
        param(
            "chaos",
            "Chaos",
            0.3,
            "Mixes between slow drift at 0 and quick jitter at 1.",
        ),
    ],
    inputs: &[jack(
        "in",
        "In",
        Pitch,
        "A pitch to wobble. 1 V = 1 octave. Leave it empty to use Noise alone.",
    )],
    outputs: &[
        jack("out", "Out", Pitch, "In with the wandering added."),
        jack(
            "noise",
            "Noise",
            Cv,
            "The wandering on its own, in volts, above and below 0 V.",
        ),
    ],
};

/// Largest wander in volts (octaves, as a pitch) at Amount 1.
const MOST_VOLTS: f32 = 2.0;
/// Drift picks per second at the bottom and top of the Speed knob.
const SLOWEST_HZ: f32 = 0.03;
const FASTEST_HZ: f32 = 18.0;
/// How many times faster the jitter picks than the drift.
const JITTER_RATIO: f32 = 12.0;

/// One layer of random wander: a new value is picked at a steady rate and
/// the output eases towards it. Always inside `-1..1`.
struct Layer {
    /// Starts full, so the first value is picked at once.
    phase: f32,
    target: f32,
    out: OnePole,
    /// False until the first sample, which jumps straight to its value.
    started: bool,
}

impl Layer {
    fn new() -> Self {
        Self {
            phase: 1.0,
            target: 0.0,
            out: OnePole::default(),
            started: false,
        }
    }

    /// `hz` picks per second; `rough` shortens the ease from half a pick
    /// down to nearly nothing.
    fn process(&mut self, hz: f32, rough: f32, rng: &mut Rng, sample_rate: f32) -> f32 {
        self.phase += hz / sample_rate;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            self.target = rng.unit() * 2.0 - 1.0;
        }
        let seconds = 0.5 * (1.0 - rough) / hz + 0.0005;
        let coeff = if self.started {
            smoothing(seconds, sample_rate)
        } else {
            1.0
        };
        self.started = true;
        self.out.process(self.target, coeff)
    }
}

pub struct State {
    rng: Rng,
    drift: Layer,
    jitter: Layer,
}

impl State {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Rng::new(seed),
            drift: Layer::new(),
            jitter: Layer::new(),
        }
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[amount, speed, rough, chaos] = io.params else {
            return;
        };
        let hz = SLOWEST_HZ * (FASTEST_HZ / SLOWEST_HZ).powf(speed);
        let drift = self.drift.process(hz, rough, &mut self.rng, io.sample_rate);
        let jitter = self.jitter.process(
            (hz * JITTER_RATIO).min(io.sample_rate * 0.25),
            rough,
            &mut self.rng,
            io.sample_rate,
        );
        let wander = drift + (jitter - drift) * chaos;
        // Cubed, so the low end of the knob has fine control.
        let noise = wander * amount * amount * amount * MOST_VOLTS;
        io.outputs[0] = io.inputs[0] + noise;
        io.outputs[1] = noise;
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::{Dsp, Io};

    const SR: f32 = 48_000.0;

    /// Frames are `[out, noise]`; `input` gives the In voltage per sample.
    fn render(
        seed: u64,
        params: [f32; 4],
        samples: usize,
        input: &dyn Fn(usize) -> f32,
    ) -> Vec<[f32; 2]> {
        let mut warp = State::new(seed);
        (0..samples)
            .map(|i| {
                let mut out = [0.0; 2];
                warp.process(&mut Io {
                    sample_rate: SR,
                    tempo: 120.0,
                    params: &params,
                    inputs: &[input(i)],
                    connected: &[true],
                    outputs: &mut out,
                });
                out
            })
            .collect()
    }

    fn quiet(_: usize) -> f32 {
        0.0
    }

    /// Mean absolute change per sample of the noise: how busy it is.
    fn busyness(frames: &[[f32; 2]]) -> f32 {
        let total: f32 = frames.windows(2).map(|w| (w[1][1] - w[0][1]).abs()).sum();
        total / frames.len() as f32
    }

    #[test]
    fn out_is_in_plus_noise() {
        let input = |i: usize| 0.25 + (i as f32 * 0.001).sin();
        let frames = render(1, [0.6, 0.5, 0.5, 0.5], 20_000, &input);
        for (i, f) in frames.iter().enumerate() {
            assert!((f[0] - (input(i) + f[1])).abs() < 1e-6);
        }
        assert!(frames.iter().any(|f| f[1].abs() > 0.01));
    }

    #[test]
    fn zero_amount_passes_the_input_untouched() {
        let input = |i: usize| (i as f32 * 0.01).sin();
        let frames = render(1, [0.0, 0.5, 0.5, 0.5], 5_000, &input);
        assert!(
            frames
                .iter()
                .enumerate()
                .all(|(i, f)| f[0] == input(i) && f[1] == 0.0)
        );
    }

    #[test]
    fn same_seed_same_wander() {
        let run = |seed| render(seed, [0.5, 0.5, 0.5, 0.5], 24_000, &quiet);
        assert_eq!(run(7), run(7));
        assert_ne!(run(7), run(8));
    }

    #[test]
    fn amount_sets_the_size_cubed() {
        let peak = |amount| {
            render(2, [amount, 0.6, 0.5, 0.5], 4 * SR as usize, &quiet)
                .iter()
                .fold(0f32, |p, f| p.max(f[1].abs()))
        };
        for amount in [0.2, 0.5, 1.0] {
            let limit = amount * amount * amount * 2.0;
            let got = peak(amount);
            // Wanders over a good part of its range, never past it.
            assert!(
                got <= limit && got > 0.3 * limit,
                "{amount}: {got} of {limit}"
            );
        }
    }

    #[test]
    fn a_gentle_setting_is_a_few_cents() {
        // Amount 0.2 is at most 0.016 V: under 20 cents.
        let frames = render(3, [0.2, 0.4, 0.3, 0.3], 10 * SR as usize, &quiet);
        let peak_cents = frames.iter().fold(0f32, |p, f| p.max(f[1].abs())) * 1200.0;
        assert!(peak_cents < 20.0 && peak_cents > 2.0, "{peak_cents}");
    }

    #[test]
    fn speed_and_chaos_make_it_busier() {
        let slow = busyness(&render(4, [0.8, 0.0, 0.5, 0.0], 4 * SR as usize, &quiet));
        let fast = busyness(&render(4, [0.8, 1.0, 0.5, 0.0], 4 * SR as usize, &quiet));
        assert!(fast > 20.0 * slow, "{slow} {fast}");
        let drift = busyness(&render(4, [0.8, 0.5, 0.5, 0.0], 4 * SR as usize, &quiet));
        let jitter = busyness(&render(4, [0.8, 0.5, 0.5, 1.0], 4 * SR as usize, &quiet));
        assert!(jitter > 4.0 * drift, "{drift} {jitter}");
    }

    #[test]
    fn rough_makes_it_jumpier() {
        let jumps = |rough| {
            render(5, [0.8, 0.5, rough, 1.0], 4 * SR as usize, &quiet)
                .windows(2)
                .fold(0f32, |m, w| m.max((w[1][1] - w[0][1]).abs()))
        };
        assert!(
            jumps(1.0) > 10.0 * jumps(0.0),
            "{} {}",
            jumps(0.0),
            jumps(1.0)
        );
    }

    #[test]
    fn stays_finite_and_bounded_at_the_extremes() {
        for bits in 0..16u32 {
            let params: [f32; 4] = std::array::from_fn(|k| (bits >> k & 1) as f32);
            let frames = render(1, params, 20_000, &|i| (i % 100) as f32 * 0.1);
            assert!(
                frames
                    .iter()
                    .all(|f| f.iter().all(|v| v.is_finite()) && f[1].abs() <= 2.0),
                "{params:?}"
            );
        }
    }
}
