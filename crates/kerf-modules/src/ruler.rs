use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{CV_MAX, Edge, GATE_HIGH, Gate, PULSE_SECONDS, Rng};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Ruler",
    band: Band::Time,
    hp: 12,
    summary: "Four Euclidean trigger lanes",
    params: &[
        param(
            "hits_a",
            "Hits A",
            0.25,
            "How full lane A is: hits spread as evenly as possible over its steps.",
        ),
        param("hits_b", "Hits B", 0.4, "How full lane B is."),
        param("hits_c", "Hits C", 0.7, "How full lane C is."),
        param("hits_d", "Hits D", 0.15, "How full lane D is."),
        param("steps_a", "Steps A", 1.0, "Length of lane A, 1–16 steps."),
        param(
            "steps_b",
            "Steps B",
            0.73,
            "Length of lane B. Lanes of different lengths drift against each other.",
        ),
        param("steps_c", "Steps C", 0.4, "Length of lane C."),
        param("steps_d", "Steps D", 0.27, "Length of lane D."),
        param(
            "rotate",
            "Rotate",
            0.0,
            "Slides every pattern later in time.",
        ),
        param(
            "chance",
            "Chance",
            0.9,
            "Probability that each hit on lanes B to D plays. Lane A always plays, so it can hold a straight beat.",
        ),
    ],
    inputs: &[
        jack("clock", "Clock", Clock, "Advances one step per pulse."),
        jack("rotate", "Rotate", Cv, "Moves Rotate from a voltage."),
    ],
    outputs: &[
        jack("a", "A", Gate, "Lane A triggers."),
        jack("b", "B", Gate, "Lane B triggers."),
        jack("c", "C", Gate, "Lane C triggers."),
        jack("d", "D", Gate, "Lane D triggers."),
    ],
};

pub struct State {
    /// Steps taken since the start; each lane wraps it at its own length.
    step: Option<u32>,
    clock: Gate,
    rng: Rng,
    pulse_left: [u32; 4],
}

impl State {
    pub fn new(seed: u64) -> Self {
        Self {
            step: None,
            clock: Gate::default(),
            rng: Rng::new(seed),
            pulse_left: [0; 4],
        }
    }
}

/// Whether `step` of a Euclidean pattern with `hits` over `len` is a hit.
/// The first hit lands on step 0.
fn euclid(step: u32, hits: u32, len: u32) -> bool {
    (step * hits) % len < hits
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[ha, hb, hc, hd, sa, sb, sc, sd, rotate, chance] = io.params else {
            return;
        };
        if self.clock.update(io.inputs[0]) == Edge::Rose {
            let step = self.step.map_or(0, |s| s.wrapping_add(1));
            self.step = Some(step);
            let rotate = (rotate + io.inputs[1] / CV_MAX).clamp(0.0, 1.0);
            let pulse = (PULSE_SECONDS * io.sample_rate) as u32;
            for (i, (hits, steps)) in [(ha, sa), (hb, sb), (hc, sc), (hd, sd)]
                .into_iter()
                .enumerate()
            {
                let len = 1 + (steps * 15.0).round() as u32;
                let hits = (hits * len as f32).round() as u32;
                let shift = (rotate * len as f32).round() as u32;
                let pos = (step % len + len - shift % len) % len;
                if euclid(pos, hits, len) && (i == 0 || self.rng.unit() < chance) {
                    self.pulse_left[i] = pulse;
                }
            }
        }
        for (out, left) in io.outputs.iter_mut().zip(&mut self.pulse_left) {
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
    use super::{State, euclid};
    use crate::{Dsp, Io};

    #[test]
    fn euclid_spreads_hits_evenly() {
        let pattern: String = (0..8)
            .map(|s| if euclid(s, 3, 8) { 'x' } else { '.' })
            .collect();
        assert_eq!(pattern, "x..x..x.");
    }

    #[test]
    fn chance_thins_lanes_b_to_d_but_never_lane_a() {
        // Every lane: 4 hits over 16 steps. Chance 0.
        let params = [0.25, 0.25, 0.25, 0.25, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0];
        let mut ruler = State::new(7);
        let mut hits = [0; 4];
        let mut was_high = [false; 4];
        for sample in 0..16 * 100 {
            let clock = if sample % 100 < 50 { 10.0 } else { 0.0 };
            let mut outputs = [0.0; 4];
            ruler.process(&mut Io {
                sample_rate: 48_000.0,
                tempo: 120.0,
                params: &params,
                inputs: &[clock, 0.0],
                connected: &[true, false],
                outputs: &mut outputs,
            });
            for lane in 0..4 {
                let high = outputs[lane] > 5.0;
                hits[lane] += usize::from(high && !was_high[lane]);
                was_high[lane] = high;
            }
        }
        assert_eq!(hits, [4, 0, 0, 0]);
    }
}
