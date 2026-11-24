use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{CV_MAX, OnePole, lerp, lowpass_coeff, smoothing};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Clamp",
    band: Band::Shape,
    hp: 6,
    summary: "VCA and low-pass gate",
    params: &[
        param(
            "level",
            "Level",
            0.0,
            "Base level when nothing is patched into CV.",
        ),
        param(
            "response",
            "Response",
            0.2,
            "Clean VCA at 0, plucky low-pass gate at 1.",
        ),
    ],
    inputs: &[
        jack("in", "In", Audio, "The sound."),
        jack(
            "cv",
            "CV",
            Cv,
            "Opens the clamp. Patch an envelope to turn a drone into notes.",
        ),
    ],
    outputs: &[jack("out", "Out", Audio, "The shaped sound.")],
};

#[derive(Default)]
pub struct State {
    /// Current opening, `0..1`, slewed like a vactrol.
    gain: OnePole,
    lowpass: OnePole,
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[level, response] = io.params else {
            return;
        };
        let &[x, cv] = io.inputs else {
            return;
        };
        let target = (level + cv / CV_MAX).clamp(0.0, 1.0);
        let seconds = if target > self.gain.value() {
            0.001
        } else {
            0.002 + response * 0.25
        };
        let gain = self
            .gain
            .process(target, smoothing(seconds, io.sample_rate));
        let vca = x * gain;
        // The gate's filter opens with its level: 40 Hz closed, 20 kHz open.
        let hz = 40.0 * (gain * 9.0).exp2();
        let lpg = self.lowpass.process(vca, lowpass_coeff(hz, io.sample_rate));
        io.outputs[0] = lerp(vca, lpg, response);
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::rig::{SR, render, sine};

    #[test]
    fn a_closed_clamp_settles_to_exact_silence() {
        let mut clamp = State::default();
        render(
            &mut clamp,
            &[0.0, 0.0],
            &[true, true],
            1,
            2 * SR as usize,
            |i, inp| {
                inp[0] = sine(i, 220.0, 3.0);
                inp[1] = if i < 4800 { 10.0 } else { 0.0 };
            },
        );
        assert_eq!(clamp.gain.value(), 0.0);
        assert_eq!(clamp.lowpass.value(), 0.0);
    }
}
