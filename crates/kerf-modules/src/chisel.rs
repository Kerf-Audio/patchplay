use std::f32::consts::TAU;

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, CV_MAX, Phase, lerp, poly_blep, volts_to_hz};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Chisel",
    band: Band::Voices,
    hp: 10,
    summary: "Analogue-style oscillator",
    params: &[
        param("tune", "Tune", 0.5, "Coarse pitch, ±2 octaves."),
        param("fine", "Fine", 0.5, "Fine pitch, ±1 semitone."),
        param(
            "shape",
            "Shape",
            0.2,
            "Morphs the Saw output towards a softer triangle.",
        ),
        param("pw", "PW", 0.5, "Pulse width of the Square output."),
        param("fm", "FM", 0.0, "How much the FM input bends the pitch."),
    ],
    inputs: &[
        jack("voct", "V/Oct", Pitch, "Pitch input. 1 V up = 1 octave up."),
        jack(
            "fm",
            "FM",
            Audio,
            "Patch audio here for metallic, bell-like tones.",
        ),
        jack("pw", "PW", Cv, "Moves the pulse width."),
    ],
    outputs: &[
        jack("saw", "Saw", Audio, "Bright and buzzy."),
        jack("square", "Square", Audio, "Hollow, woody."),
        jack("sine", "Sine", Audio, "Pure and round."),
        jack("sub", "Sub", Audio, "One octave down, for weight."),
    ],
};

#[derive(Default)]
pub struct State {
    phase: Phase,
    sub: Phase,
}

/// Band-limited square: +1 below `width`, −1 above.
fn square(t: f32, dt: f32, width: f32) -> f32 {
    let naive = if t < width { 1.0 } else { -1.0 };
    naive + poly_blep(t, dt) - poly_blep((t + 1.0 - width) % 1.0, dt)
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[tune, fine, shape, pw, fm] = io.params else {
            return;
        };
        let &[voct, fm_in, pw_in] = io.inputs else {
            return;
        };
        let volts = voct + (tune - 0.5) * 4.0 + (fine - 0.5) / 6.0 + fm_in / AUDIO_VOLTS * fm * 2.0;
        let dt = (volts_to_hz(volts) / io.sample_rate).min(0.45);
        let t = self.phase.advance(dt);
        let saw = 2.0 * t - 1.0 - poly_blep(t, dt);
        let triangle = 1.0 - 4.0 * (t - 0.5).abs();
        let width = (pw + pw_in / CV_MAX).clamp(0.05, 0.95);
        let s = self.sub.advance(dt * 0.5);
        let out = [
            lerp(saw, triangle, shape),
            square(t, dt, width),
            (TAU * t).sin(),
            square(s, dt * 0.5, 0.5),
        ];
        for (o, v) in io.outputs.iter_mut().zip(out) {
            *o = v * AUDIO_VOLTS;
        }
    }
}
