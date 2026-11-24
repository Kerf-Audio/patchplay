use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{CV_MAX, Edge, GATE_HIGH, Gate, OnePole, Rng, smoothing};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Dovetail",
    band: Band::Time,
    hp: 14,
    summary: "Two interlocking step lanes",
    params: &[
        param("steps_a", "Steps A", 0.5, "Length of lane A, 1–16 steps."),
        param(
            "steps_b",
            "Steps B",
            0.38,
            "Length of lane B. Different lengths drift against each other.",
        ),
        param(
            "interlock",
            "Interlock",
            0.6,
            "How lane B fits lane A: 0 plays both freely, 1 lets B play only where A rests.",
        ),
        param(
            "chance",
            "Chance",
            0.85,
            "Probability that each step plays.",
        ),
        param(
            "glide",
            "Glide",
            0.15,
            "Slides pitch from one step to the next.",
        ),
        param(
            "range",
            "Range",
            0.4,
            "Spread of the step pitches, up to 3 octaves.",
        ),
    ],
    inputs: &[
        jack("clock", "Clock", Clock, "Advances one step per pulse."),
        jack("reset", "Reset", Gate, "Back to step 1."),
        jack(
            "transpose",
            "Transpose",
            Pitch,
            "Added to every step. 1 V = 1 octave.",
        ),
    ],
    outputs: &[
        jack("pitch", "Pitch", Pitch, "Pitch of the current step."),
        jack("gate", "Gate", Gate, "High while a step plays."),
        jack("accent", "Accent", Cv, "Per-step accent level."),
    ],
};

/// Minor pentatonic, in semitones. Phase 3 styles bring their own scales.
const SCALE: [f32; 5] = [0.0, 3.0, 5.0, 7.0, 10.0];

#[derive(Debug, Clone, Copy, Default)]
struct Step {
    on: bool,
    /// Where in the pitch range this step sits, `0..1`.
    degree: f32,
    accent: f32,
}

pub struct State {
    lanes: [[Step; 16]; 2],
    index: [usize; 2],
    rng: Rng,
    clock: Gate,
    reset: Gate,
    /// Samples since the last clock; `u32::MAX` until the first one.
    since_clock: u32,
    /// Last clock interval in samples; sets the gate length.
    interval: u32,
    gate_left: u32,
    target: f32,
    glide: OnePole,
    accent: f32,
}

impl State {
    /// The step pattern comes from `seed`, so the same patch plays the same
    /// sequence every time.
    pub fn new(seed: u64, sample_rate: f32) -> Self {
        let mut rng = Rng::new(seed);
        let mut lanes = [[Step::default(); 16]; 2];
        for (lane, density) in lanes.iter_mut().zip([0.65, 0.5]) {
            for step in lane.iter_mut() {
                *step = Step {
                    on: rng.unit() < density,
                    degree: rng.unit(),
                    accent: if rng.unit() < 0.3 { 1.0 } else { 0.4 },
                };
            }
        }
        lanes[0][0].on = true;
        Self {
            lanes,
            index: [usize::MAX; 2],
            rng,
            clock: Gate::default(),
            reset: Gate::default(),
            since_clock: u32::MAX,
            interval: (sample_rate * 0.125) as u32,
            gate_left: 0,
            target: 0.0,
            glide: OnePole::default(),
            accent: 0.0,
        }
    }
}

fn length(param: f32) -> usize {
    1 + (param * 15.0).round() as usize
}

fn pitch(degree: f32, range: f32) -> f32 {
    let notes = 1 + (range * 3.0 * SCALE.len() as f32).round() as usize;
    let i = ((degree * notes as f32) as usize).min(notes - 1);
    (i / SCALE.len()) as f32 + SCALE[i % SCALE.len()] / 12.0
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[steps_a, steps_b, interlock, chance, glide, range] = io.params else {
            return;
        };
        if self.reset.update(io.inputs[1]) == Edge::Rose {
            self.index = [usize::MAX; 2];
        }
        self.since_clock = self.since_clock.saturating_add(1);
        if self.clock.update(io.inputs[0]) == Edge::Rose {
            if self.since_clock != u32::MAX {
                self.interval = self.since_clock;
            }
            self.since_clock = 0;
            for (index, len) in self
                .index
                .iter_mut()
                .zip([length(steps_a), length(steps_b)])
            {
                *index = index.wrapping_add(1) % len;
            }
            let a = self.lanes[0][self.index[0]];
            let b = self.lanes[1][self.index[1]];
            let a_plays = a.on && self.rng.unit() < chance;
            let b_allowed = !a_plays || self.rng.unit() >= interlock;
            let b_plays = b.on && b_allowed && self.rng.unit() < chance;
            let step = if b_plays {
                Some(b)
            } else if a_plays {
                Some(a)
            } else {
                None
            };
            if let Some(step) = step {
                self.target = pitch(step.degree, range);
                self.accent = step.accent * CV_MAX;
                self.gate_left = self.interval / 2;
            }
        }
        let coeff = smoothing(glide * glide * 0.25, io.sample_rate);
        io.outputs[0] = self.glide.process(self.target, coeff) + io.inputs[2];
        io.outputs[1] = if self.gate_left > 0 {
            self.gate_left -= 1;
            GATE_HIGH
        } else {
            0.0
        };
        io.outputs[2] = self.accent;
    }
}
