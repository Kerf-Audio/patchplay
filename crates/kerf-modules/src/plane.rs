use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, Svf};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Plane",
    band: Band::Shape,
    hp: 8,
    summary: "Multimode filter",
    params: &[
        param("cutoff", "Cutoff", 0.45, "Where the filter starts cutting."),
        param(
            "res",
            "Res",
            0.3,
            "Boost at the cutoff. High settings whistle.",
        ),
        param(
            "mode",
            "Mode",
            0.0,
            "Low-pass, band-pass, high-pass, notch.",
        ),
        param("drive", "Drive", 0.2, "Saturates the input."),
    ],
    inputs: &[
        jack("in", "In", Audio, "The sound to filter."),
        jack(
            "cutoff",
            "Cutoff",
            Cv,
            "Moves the cutoff. An envelope here makes each note bloom.",
        ),
        jack("voct", "V/Oct", Pitch, "Makes the cutoff follow the notes."),
    ],
    outputs: &[jack("out", "Out", Audio, "The filtered sound.")],
};

#[derive(Default)]
pub struct State {
    svf: Svf,
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[cutoff, res, mode, drive] = io.params else {
            return;
        };
        let &[x, cv, voct] = io.inputs else {
            return;
        };
        let gain = 1.0 + drive * 7.0;
        let x = (x / AUDIO_VOLTS * gain).tanh();
        // Knob: 20 Hz to 20 kHz. CV: half an octave per volt. V/Oct: 1 V per octave.
        let octaves = cutoff * 10.0 + cv * 0.5 + voct;
        let hz = (20.0 * octaves.exp2()).clamp(20.0, io.sample_rate * 0.45);
        let q = 0.5 + res * res * 20.0;
        let o = self.svf.process(x, hz, q, io.sample_rate);
        let y = match (mode * 3.0).round() as u8 {
            0 => o.low,
            1 => o.band,
            2 => o.high,
            _ => o.low + o.high,
        };
        io.outputs[0] = y * AUDIO_VOLTS;
    }
}
