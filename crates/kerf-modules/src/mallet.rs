use std::f32::consts::{PI, TAU};

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, Edge, Gate, lerp};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Mallet",
    band: Band::Voices,
    hp: 10,
    summary: "Modal percussion",
    params: &[
        param("material", "Material", 0.3, "Wood, glass, metal, skin."),
        param("size", "Size", 0.5, "Bigger objects ring lower."),
        param(
            "strike",
            "Strike",
            0.5,
            "Where and how hard the mallet hits.",
        ),
        param("damp", "Damp", 0.4, "How quickly the ringing dies away."),
    ],
    inputs: &[
        jack("trig", "Trig", Gate, "Strikes the object."),
        jack("voct", "V/Oct", Pitch, "Tunes the object."),
    ],
    outputs: &[jack("out", "Out", Audio, "The struck sound.")],
};

const MODES: usize = 8;

/// Mode frequencies relative to the lowest, for wood (a tuned bar), glass
/// (a free bar), metal (a bell) and skin (a circular membrane). Material
/// slides along them.
const RATIOS: [[f32; MODES]; 4] = [
    [1.0, 3.9, 9.2, 15.4, 22.8, 31.1, 40.6, 51.0],
    [1.0, 2.76, 5.40, 8.93, 13.34, 18.64, 24.8, 31.9],
    [1.0, 2.0, 2.38, 3.0, 4.0, 5.02, 5.32, 8.14],
    [1.0, 1.59, 2.14, 2.30, 2.65, 2.92, 3.16, 3.50],
];

/// Seconds for the lowest mode to fall by 60 dB at Damp 0.5.
const RING: [f32; 4] = [0.3, 1.5, 3.0, 0.4];

/// Modes above this fraction of the sample rate are left out.
const CEILING: f32 = 0.45;
/// Overall level; the modes add to a few volts at most.
const GAIN: f32 = 0.45;

/// Moves along the four materials.
fn along<T: Copy>(table: &[T; 4], x: f32, mix: impl Fn(T, T, f32) -> T) -> T {
    let x = x.clamp(0.0, 1.0) * 3.0;
    let i = (x as usize).min(2);
    mix(table[i], table[i + 1], x - i as f32)
}

/// What the resonators were last tuned to.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tuning {
    volts: f32,
    material: f32,
    damp: f32,
}

pub struct State {
    trig: Gate,
    tuning: Option<Tuning>,
    ratios: [f32; MODES],
    /// Per mode: damped rotation `(r·cos ω, r·sin ω)`, zero for modes above
    /// the ceiling.
    rotation: [(f32, f32); MODES],
    /// Per mode: the ringing phasor.
    ring: [(f32, f32); MODES],
}

impl Default for State {
    fn default() -> Self {
        Self {
            trig: Gate::default(),
            tuning: None,
            ratios: [0.0; MODES],
            rotation: [(0.0, 0.0); MODES],
            ring: [(0.0, 0.0); MODES],
        }
    }
}

impl State {
    fn retune(&mut self, tuning: Tuning, sr: f32) {
        self.tuning = Some(tuning);
        let hz = 261.625_58 * tuning.volts.exp2();
        let blend = |a: [f32; MODES], b: [f32; MODES], t: f32| -> [f32; MODES] {
            std::array::from_fn(|m| a[m] * (b[m] / a[m]).powf(t))
        };
        self.ratios = along(&RATIOS, tuning.material, blend);
        // Damp 0.5 is the stated ring; each step of 0.25 doubles or halves it.
        let ring = along(&RING, tuning.material, lerp) * 4f32.powf(1.0 - 2.0 * tuning.damp);
        for m in 0..MODES {
            let mode_hz = hz * self.ratios[m];
            self.rotation[m] = if mode_hz < CEILING * sr {
                // Upper modes die faster.
                let seconds = ring / self.ratios[m].sqrt();
                let r = (-6.91 / (seconds * sr)).exp();
                let w = TAU * mode_hz / sr;
                (r * w.cos(), r * w.sin())
            } else {
                (0.0, 0.0)
            };
        }
    }

    /// Excites every mode. Strike low is a soft mallet at the centre; high
    /// is a hard one near the edge.
    fn strike(&mut self, strike: f32) {
        let position = 0.5 - 0.35 * strike;
        let softness = 1.0 - strike;
        let mut weights = [0.0f32; MODES];
        for (m, w) in weights.iter_mut().enumerate() {
            if self.rotation[m].0 != 0.0 || self.rotation[m].1 != 0.0 {
                let node = 0.25 + 0.75 * (PI * position * (m + 1) as f32).sin().abs();
                *w = node * (-0.35 * (self.ratios[m] - 1.0) * softness).exp();
            }
        }
        let norm = GAIN / weights.iter().map(|w| w * w).sum::<f32>().sqrt().max(1e-6);
        for (ring, w) in self.ring.iter_mut().zip(weights) {
            ring.0 += w * norm;
        }
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[material, size, strike, damp] = io.params else {
            return;
        };
        let &[trig, voct] = io.inputs else {
            return;
        };
        let tuning = Tuning {
            volts: voct + (0.5 - size) * 4.0,
            material,
            damp,
        };
        if self.tuning != Some(tuning) {
            self.retune(tuning, io.sample_rate);
        }
        if self.trig.update(trig) == Edge::Rose {
            self.strike(strike);
        }
        let mut sum = 0.0;
        for (ring, &(rc, rs)) in self.ring.iter_mut().zip(&self.rotation) {
            let (c, s) = *ring;
            *ring = if c * c + s * s < 1e-20 {
                (0.0, 0.0)
            } else {
                (rc * c - rs * s, rs * c + rc * s)
            };
            sum += ring.1;
        }
        io.outputs[0] = sum.tanh() * AUDIO_VOLTS;
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::rig::{SR, peak, render, rms, tone};

    /// A single strike at the start of `seconds` of audio.
    fn hit(params: [f32; 4], voct: f32, seconds: f32) -> Vec<f32> {
        let mut mallet = State::default();
        let out = render(
            &mut mallet,
            &params,
            &[true, true],
            1,
            (seconds * SR) as usize,
            |i, v| {
                v[0] = if i < 240 { 10.0 } else { 0.0 };
                v[1] = voct;
            },
        );
        out.into_iter().next().unwrap()
    }

    #[test]
    fn rings_at_its_pitch_for_every_material() {
        // Size 0.5 is C4. Soft strike so the lowest mode leads.
        for material in [0.0, 0.33, 0.66, 1.0] {
            let out = hit([material, 0.5, 0.0, 0.4], 0.0, 0.5);
            let at_pitch = tone(&out[2400..], 261.6);
            let off_pitch = tone(&out[2400..], 261.6 * 1.2);
            assert!(
                at_pitch > off_pitch * 3.0,
                "material {material}: {at_pitch} at pitch, {off_pitch} off"
            );
            let octave = hit([material, 0.5, 0.0, 0.4], 1.0, 0.5);
            assert!(tone(&octave[2400..], 523.3) > tone(&octave[2400..], 261.6) * 3.0);
        }
    }

    #[test]
    fn hits_at_a_sane_level_and_dies_away() {
        for material in [0.0, 0.33, 0.66, 1.0] {
            let out = hit([material, 0.5, 0.5, 0.4], 0.0, 8.0);
            let p = peak(&out);
            assert!(p > 1.0 && p <= 5.0, "material {material}: peak {p} V");
            let tail = peak(&out[(7.0 * SR) as usize..]);
            assert!(tail < 0.01, "material {material} still rings at {tail} V");
        }
    }

    #[test]
    fn damp_shortens_the_ring() {
        let loose = hit([0.5, 0.5, 0.5, 0.0], 0.0, 2.0);
        let tight = hit([0.5, 0.5, 0.5, 1.0], 0.0, 2.0);
        let late = (SR * 0.5) as usize..;
        assert!(rms(&tight[late.clone()]) * 20.0 < rms(&loose[late]));
    }

    #[test]
    fn harder_strikes_are_brighter() {
        let soft = hit([0.66, 0.5, 0.0, 0.4], 0.0, 0.3);
        let hard = hit([0.66, 0.5, 1.0, 0.4], 0.0, 0.3);
        let brightness = |x: &[f32]| {
            let diff: Vec<f32> = x.windows(2).map(|w| w[1] - w[0]).collect();
            rms(&diff) / rms(x)
        };
        assert!(brightness(&hard) > brightness(&soft) * 1.5);
    }

    #[test]
    fn silent_until_struck_and_modes_above_nyquist_stay_out() {
        let mut mallet = State::default();
        let quiet = render(
            &mut mallet,
            &[0.0, 0.0, 1.0, 0.4],
            &[true, true],
            1,
            4800,
            |_, v| {
                v[1] = 3.0;
            },
        );
        assert_eq!(peak(&quiet[0]), 0.0);
        // Size 0 is two octaves up, voct 3 more: the upper modes are past
        // 0.45 sr and must not wrap round or blow up.
        let high = hit([0.0, 0.0, 1.0, 0.4], 3.0, 0.2);
        assert!(peak(&high) <= 5.0);
    }
}
