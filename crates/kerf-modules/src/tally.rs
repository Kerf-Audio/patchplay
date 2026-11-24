use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{Edge, GATE_HIGH, Gate, PULSE_SECONDS};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Tally",
    band: Band::Time,
    hp: 6,
    summary: "Clock and transport",
    params: &[
        param(
            "tempo",
            "Tempo",
            0.5,
            "Nudges the patch tempo by up to ±10%. Centre is exact.",
        ),
        param(
            "swing",
            "Swing",
            0.1,
            "Delays every second step. 0 is straight, 1 is a heavy shuffle.",
        ),
        param(
            "divide",
            "Divide",
            0.5,
            "Sets the speed of the ×2 and ÷4 outputs relative to the beat.",
        ),
    ],
    inputs: &[
        jack(
            "run",
            "Run",
            Gate,
            "High = playing. Leave empty to follow the transport.",
        ),
        jack(
            "reset",
            "Reset",
            Gate,
            "A pulse here jumps back to the first beat.",
        ),
    ],
    outputs: &[
        jack("clock", "Clock", Clock, "One pulse per 16th note."),
        jack("x2", "×2", Clock, "Twice as fast as Clock."),
        jack("div4", "÷4", Clock, "One pulse per beat."),
        jack("bar", "Bar", Gate, "One pulse at the start of every bar."),
    ],
};

#[derive(Default)]
pub struct State {
    /// Position in 16th notes since the start or the last reset.
    position: f64,
    /// Index of the next 16th to fire.
    next: u64,
    /// Index of the next fast-clock pulse.
    next_fast: u64,
    /// Samples left on each output's pulse.
    pulses: [u32; 4],
    run: Gate,
    reset: Gate,
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[nudge, swing, divide] = io.params else {
            return;
        };
        if self.reset.update(io.inputs[1]) == Edge::Rose {
            self.position = 0.0;
            self.next = 0;
            self.next_fast = 0;
        }
        self.run.update(io.inputs[0]);
        if !io.connected[0] || self.run.is_high() {
            let pulse = (PULSE_SECONDS * io.sample_rate) as u32;
            // 0, 1, 2: the fast output runs ×1/×2/×4, the slow one ÷2/÷4/÷8.
            let speed = (divide * 2.0).round() as u32;
            let onset = self.next as f64
                + if self.next % 2 == 1 {
                    f64::from(swing) * 0.5
                } else {
                    0.0
                };
            if self.position >= onset {
                self.pulses[0] = pulse;
                if self.next % (2 << speed) == 0 {
                    self.pulses[2] = pulse;
                }
                if self.next % 16 == 0 {
                    self.pulses[3] = pulse;
                }
                self.next += 1;
            }
            if self.position >= self.next_fast as f64 / f64::from(1u32 << speed) {
                self.pulses[1] = pulse;
                self.next_fast += 1;
            }
            let bpm = io.tempo * (1.0 + (nudge - 0.5) * 0.2);
            self.position += f64::from(bpm * 4.0 / (60.0 * io.sample_rate));
        }
        for (out, left) in io.outputs.iter_mut().zip(&mut self.pulses) {
            *out = if *left > 0 {
                *left -= 1;
                GATE_HIGH
            } else {
                0.0
            };
        }
    }
}
