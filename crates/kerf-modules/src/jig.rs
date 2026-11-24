use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{CV_MAX, OnePole, smoothing};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Jig",
    band: Band::Motion,
    hp: 8,
    summary: "Four macros",
    params: &[
        param("m1", "M1", 0.5, "Macro 1. Patch its output to anything."),
        param("m2", "M2", 0.5, "Macro 2."),
        param("m3", "M3", 0.5, "Macro 3."),
        param("m4", "M4", 0.5, "Macro 4."),
        param(
            "depth",
            "Depth",
            1.0,
            "Scales all four outputs. 0 holds them at 0 V.",
        ),
    ],
    inputs: &[],
    outputs: &[
        jack("m1", "M1", Cv, "Macro 1 as a voltage."),
        jack("m2", "M2", Cv, "Macro 2."),
        jack("m3", "M3", Cv, "Macro 3."),
        jack("m4", "M4", Cv, "Macro 4."),
    ],
};

/// Time the outputs take to follow a macro that has been turned or morphed.
const GLIDE_SECONDS: f32 = 0.01;

pub struct State {
    macros: [OnePole; 4],
    coeff: f32,
    /// False until the first sample, which jumps straight to the macros.
    settled: bool,
}

impl State {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            macros: [OnePole::default(); 4],
            coeff: smoothing(GLIDE_SECONDS, sample_rate),
            settled: false,
        }
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let coeff = if self.settled { self.coeff } else { 1.0 };
        self.settled = true;
        let (macros, rest) = io.params.split_at(4);
        let depth = rest[0];
        for ((out, smoother), &value) in io.outputs.iter_mut().zip(&mut self.macros).zip(macros) {
            *out = smoother.process(value, coeff) * CV_MAX * depth;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::{Dsp, Io};

    const SR: f32 = 48_000.0;

    fn step(jig: &mut State, params: &[f32; 4]) -> [f32; 4] {
        step_with_depth(jig, params, 1.0)
    }

    fn step_with_depth(jig: &mut State, params: &[f32; 4], depth: f32) -> [f32; 4] {
        let mut out = [0.0; 4];
        let all = [params[0], params[1], params[2], params[3], depth];
        jig.process(&mut Io {
            sample_rate: SR,
            tempo: 120.0,
            params: &all,
            inputs: &[],
            connected: &[],
            outputs: &mut out,
        });
        out
    }

    #[test]
    fn outputs_equal_the_macros_from_the_first_sample() {
        let mut jig = State::new(SR);
        let out = step(&mut jig, &[0.0, 0.25, 0.5, 1.0]);
        assert_eq!(out, [0.0, 2.5, 5.0, 10.0]);
    }

    #[test]
    fn depth_scales_every_output() {
        let mut jig = State::new(SR);
        let out = step_with_depth(&mut jig, &[0.0, 0.25, 0.5, 1.0], 0.5);
        assert_eq!(out, [0.0, 1.25, 2.5, 5.0]);
        let off = step_with_depth(&mut State::new(SR), &[1.0; 4], 0.0);
        assert_eq!(off, [0.0; 4]);
    }

    #[test]
    fn turned_macros_glide_rather_than_jump() {
        let mut jig = State::new(SR);
        step(&mut jig, &[0.0; 4]);
        let first = step(&mut jig, &[1.0; 4]);
        assert!(first.iter().all(|&v| v > 0.0 && v < 0.1), "{first:?}");
        // 100 ms is ten time constants.
        let mut last = first;
        for _ in 0..4800 {
            last = step(&mut jig, &[1.0; 4]);
        }
        assert!(last.iter().all(|&v| (v - 10.0).abs() < 0.01), "{last:?}");
    }
}
