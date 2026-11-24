use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{CV_MAX, Edge, GATE_HIGH, Gate, PULSE_SECONDS, lerp};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Bevel",
    band: Band::Motion,
    hp: 6,
    summary: "Envelope",
    params: &[
        param(
            "attack",
            "Attack",
            0.05,
            "Time to rise after the gate opens.",
        ),
        param("decay", "Decay", 0.35, "Time to fall to the sustain level."),
        param(
            "sustain",
            "Sustain",
            0.4,
            "Level held while the gate stays high.",
        ),
        param(
            "release",
            "Release",
            0.3,
            "Time to fall to zero after the gate closes.",
        ),
        param(
            "curve",
            "Curve",
            0.4,
            "Straight lines at 0.5, snappy below, swelling above.",
        ),
        param(
            "depth",
            "Depth",
            1.0,
            "Scales Env and Inverted. 0 holds both at 0 V.",
        ),
    ],
    inputs: &[jack(
        "gate",
        "Gate",
        Gate,
        "Starts the envelope; holding it keeps sustain.",
    )],
    outputs: &[
        jack("env", "Env", Cv, "The envelope, 0 to 10 V."),
        jack("inverted", "Inverted", Cv, "The envelope upside down."),
        jack("end", "End", Gate, "Pulses when the envelope finishes."),
    ],
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Stage {
    #[default]
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Default)]
pub struct State {
    stage: Stage,
    value: f32,
    /// Value when the current stage began, so retriggers do not click.
    start: f32,
    elapsed: f32,
    gate: Gate,
    end_left: u32,
}

impl State {
    fn enter(&mut self, stage: Stage) {
        self.stage = stage;
        self.start = self.value;
        self.elapsed = 0.0;
    }
}

/// Segment shape: `k` = 1 is a straight line, above 1 snappy, below swelling.
fn shape(progress: f32, k: f32) -> f32 {
    1.0 - (1.0 - progress.min(1.0)).powf(k)
}

/// 1 ms to 2 s.
fn attack_seconds(p: f32) -> f32 {
    0.001 * 2000f32.powf(p)
}

/// 5 ms to 5 s.
fn fall_seconds(p: f32) -> f32 {
    0.005 * 1000f32.powf(p)
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[attack, decay, sustain, release, curve, depth] = io.params else {
            return;
        };
        match self.gate.update(io.inputs[0]) {
            Edge::Rose => self.enter(Stage::Attack),
            Edge::Fell if self.stage != Stage::Idle && self.stage != Stage::Release => {
                self.enter(Stage::Release)
            }
            _ => {}
        }
        let k = ((0.5 - curve) * 4.0).exp2();
        self.elapsed += 1.0 / io.sample_rate;
        match self.stage {
            Stage::Idle => self.value = 0.0,
            Stage::Attack => {
                let p = self.elapsed / attack_seconds(attack);
                self.value = lerp(self.start, 1.0, shape(p, k));
                if p >= 1.0 {
                    self.enter(Stage::Decay);
                }
            }
            Stage::Decay => {
                let p = self.elapsed / fall_seconds(decay);
                self.value = lerp(self.start, sustain, shape(p, k));
                if p >= 1.0 {
                    self.enter(Stage::Sustain);
                }
            }
            Stage::Sustain => self.value = sustain,
            Stage::Release => {
                let p = self.elapsed / fall_seconds(release);
                self.value = lerp(self.start, 0.0, shape(p, k));
                if p >= 1.0 {
                    self.enter(Stage::Idle);
                    self.end_left = (PULSE_SECONDS * io.sample_rate) as u32;
                }
            }
        }
        io.outputs[0] = self.value * CV_MAX * depth;
        io.outputs[1] = (1.0 - self.value) * CV_MAX * depth;
        io.outputs[2] = if self.end_left > 0 {
            self.end_left -= 1;
            GATE_HIGH
        } else {
            0.0
        };
    }
}
