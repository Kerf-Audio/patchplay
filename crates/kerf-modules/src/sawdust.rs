use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, Rng, lerp};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Sawdust",
    band: Band::Voices,
    hp: 4,
    summary: "Noise",
    params: &[
        param("colour", "Colour", 0.5, "Dark rumble to bright hiss."),
        param(
            "crackle",
            "Crackle",
            0.3,
            "Density of the vinyl-like crackle.",
        ),
    ],
    inputs: &[],
    outputs: &[
        jack("noise", "Noise", Audio, "Coloured noise."),
        jack("dust", "Dust", Audio, "Random crackles."),
    ],
};

/// RMS of every colour, as a fraction of full scale (±5 V).
const LEVEL: f32 = 0.3;
/// Standard deviation of uniform noise in `-1..1`.
const WHITE_RMS: f32 = 0.577_35;
/// RMS of the three-pole pink filter below for uniform input in `-1..1`
/// (measured over a long run).
const PINK_RMS: f32 = 1.71;
/// Brown noise is white through a leaky integrator with this corner.
const BROWN_CORNER_HZ: f32 = 12.0;
/// Pops decay with this time constant.
const POP_SECONDS: f32 = 0.0005;
/// Pops per second when Crackle is full.
const MAX_POPS: f32 = 2000.0;

pub struct State {
    noise: Rng,
    dust: Rng,
    brown: f32,
    brown_leak: f32,
    brown_gain: f32,
    pink: [f32; 3],
    pop: f32,
    pop_decay: f32,
}

impl State {
    pub fn new(seed: u64, sample_rate: f32) -> Self {
        let brown_leak = (-std::f32::consts::TAU * BROWN_CORNER_HZ / sample_rate).exp();
        // Variance of a leaky integrator: gain² · σ² / (1 − leak²).
        let brown_gain = LEVEL * (1.0 - brown_leak * brown_leak).sqrt() / WHITE_RMS;
        Self {
            noise: Rng::new(seed),
            dust: Rng::new(seed ^ 0xD057),
            brown: 0.0,
            brown_leak,
            brown_gain,
            pink: [0.0; 3],
            pop: 0.0,
            pop_decay: (-1.0 / (POP_SECONDS * sample_rate)).exp(),
        }
    }

    /// Pink noise (Paul Kellet's three-pole filter) at `LEVEL` RMS.
    fn pink(&mut self, white: f32) -> f32 {
        let [a, b, c] = &mut self.pink;
        *a = 0.99765 * *a + white * 0.099_046;
        *b = 0.963 * *b + white * 0.296_516_4;
        *c = 0.57 * *c + white * 1.052_691_3;
        (*a + *b + *c + white * 0.1848) * (LEVEL / PINK_RMS)
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[colour, crackle] = io.params else {
            return;
        };
        let white = self.noise.unit() * 2.0 - 1.0;
        self.brown = self.brown * self.brown_leak + white * self.brown_gain;
        let pink = self.pink(white);
        let white = white * (LEVEL / WHITE_RMS);
        // Dark to bright: brown, pink, white.
        let tilted = if colour < 0.5 {
            lerp(self.brown, pink, colour * 2.0)
        } else {
            lerp(pink, white, colour * 2.0 - 1.0)
        };

        let pops_per_second = MAX_POPS * crackle.powf(3.5);
        if self.dust.unit() < pops_per_second / io.sample_rate {
            let sign = if self.dust.next_u64() & 1 == 0 {
                1.0
            } else {
                -1.0
            };
            let u = self.dust.unit();
            self.pop += sign * (0.2 + 0.8 * u * u);
        }
        let pop = self.pop.clamp(-1.0, 1.0);
        self.pop = if self.pop.abs() < 1e-6 {
            0.0
        } else {
            self.pop * self.pop_decay
        };

        io.outputs[0] = tilted.tanh() * AUDIO_VOLTS;
        io.outputs[1] = pop * AUDIO_VOLTS;
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::rig::{SR, peak, render, rms};

    fn run(seed: u64, params: [f32; 2], seconds: f32) -> Vec<Vec<f32>> {
        let mut dust = State::new(seed, SR);
        render(
            &mut dust,
            &params,
            &[],
            2,
            (seconds * SR) as usize,
            |_, _| {},
        )
    }

    /// Correlation between neighbouring samples: near 0 for white, towards
    /// 1 for darker noise.
    fn lag_one(x: &[f32]) -> f32 {
        let dot: f32 = x.windows(2).map(|w| w[0] * w[1]).sum();
        dot / x.iter().map(|v| v * v).sum::<f32>()
    }

    #[test]
    fn colour_runs_from_rumble_to_hiss_at_one_level() {
        let mut last = 1.0;
        for colour in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let out = run(1, [colour, 0.0], 4.0);
            let noise = &out[0][SR as usize..];
            let level = rms(noise) / 5.0;
            assert!(
                (level - 0.3).abs() < 0.06,
                "colour {colour}: {level} of full scale"
            );
            assert!(peak(noise) <= 5.0);
            let corr = lag_one(noise);
            assert!(corr < last, "colour {colour} is not brighter than the last");
            last = corr;
        }
        assert!(last.abs() < 0.05, "white noise is uncorrelated");
    }

    #[test]
    fn crackle_density_follows_the_knob() {
        assert_eq!(peak(&run(1, [0.5, 0.0], 2.0)[1]), 0.0, "no crackle at zero");
        let count = |crackle: f32| {
            let dust = &run(1, [0.5, crackle], 4.0)[1];
            dust.windows(2)
                .filter(|w| w[0].abs() < 0.1 && w[1].abs() >= 0.1)
                .count() as f32
                / 4.0
        };
        let sparse = count(0.3);
        let dense = count(0.5);
        let expected = |c: f32| 2000.0 * c.powf(3.5);
        for (found, knob) in [(sparse, 0.3), (dense, 0.5)] {
            assert!(
                (found / expected(knob) - 1.0).abs() < 0.35,
                "crackle {knob}: {found} pops/s"
            );
        }
        assert!(dense > sparse * 4.0);
        assert!(peak(&run(1, [0.5, 1.0], 1.0)[1]) <= 5.0);
    }

    #[test]
    fn same_seed_same_noise() {
        let a = run(7, [0.5, 0.5], 0.2);
        assert_eq!(a, run(7, [0.5, 0.5], 0.2));
        assert_ne!(a, run(8, [0.5, 0.5], 0.2));
    }
}
