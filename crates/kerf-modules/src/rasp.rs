use std::f32::consts::FRAC_PI_2;

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, CV_MAX, OnePole, db_to_gain, lerp, lowpass_coeff, smoothing};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Rasp",
    band: Band::Shape,
    hp: 6,
    summary: "Drive and wavefolder",
    params: &[
        param("drive", "Drive", 0.3, "Gain into the clipper."),
        param(
            "fold",
            "Fold",
            0.1,
            "Folds loud peaks back for rich harmonics.",
        ),
        param("bias", "Bias", 0.5, "Uneven clipping, for a warmer edge."),
        param("mix", "Mix", 1.0, "Dry to fully driven."),
    ],
    inputs: &[
        jack("in", "In", Audio, "The sound."),
        jack("drive", "Drive", Cv, "Moves Drive."),
    ],
    outputs: &[jack("out", "Out", Audio, "The driven sound.")],
};

/// Below this input step the antiderivative quotient is ill-conditioned and
/// the shaper is evaluated at the midpoint instead.
const ADAA_EPS: f32 = 1e-4;
/// Drive range in dB: 0 up to `MAX_DRIVE_DB`.
const MAX_DRIVE_DB: f32 = 36.0;
/// How far a full Fold turn multiplies the folder's input.
const MAX_FOLD_GAIN: f32 = 4.0;
/// Cutoff of the DC blocker that removes what Bias adds.
const DC_HZ: f32 = 12.0;

/// A memoryless shaper with a closed-form antiderivative, evaluated with
/// first-order antiderivative anti-aliasing (ADAA).
#[derive(Debug, Clone, Copy, Default)]
struct Adaa {
    x1: f32,
    f1: f32,
}

impl Adaa {
    fn process(
        &mut self,
        x: f32,
        shape: impl Fn(f32) -> f32,
        antiderivative: impl Fn(f32) -> f32,
    ) -> f32 {
        let f = antiderivative(x);
        let dx = x - self.x1;
        let y = if dx.abs() < ADAA_EPS {
            shape(0.5 * (x + self.x1))
        } else {
            (f - self.f1) / dx
        };
        self.x1 = x;
        self.f1 = f;
        y
    }
}

/// `ln(cosh(x))`, the antiderivative of `tanh`, without overflow.
fn ln_cosh(x: f32) -> f32 {
    let a = x.abs();
    a + (-2.0 * a).exp().ln_1p() - std::f32::consts::LN_2
}

#[derive(Debug, Clone, Default)]
pub struct State {
    clip: Adaa,
    fold: Adaa,
    drive: OnePole,
    fold_amount: OnePole,
    bias: OnePole,
    dc: OnePole,
}

impl State {
    /// One wet sample from a normalised (`±1`) input.
    fn shape(&mut self, x: f32, drive: f32, fold: f32, bias: f32, sr: f32) -> f32 {
        let gain = db_to_gain(drive * MAX_DRIVE_DB);
        let offset = (bias - 0.5) * 2.0;
        let clipped = self
            .clip
            .process(x * gain + offset * 0.5 * gain.sqrt(), f32::tanh, ln_cosh);
        let k = FRAC_PI_2 * (1.0 + MAX_FOLD_GAIN * fold);
        let blend = (fold * 4.0).min(1.0);
        let folded = self.fold.process(
            clipped,
            |u| lerp(u, (k * u).sin(), blend),
            |u| lerp(0.5 * u * u, -(k * u).cos() / k, blend),
        );
        // The blocker only removes what the signal leaves behind, so dividing
        // by one plus that keeps a step inside full scale; the clamp absorbs
        // rounding in the ADAA quotient.
        let dc = self.dc.process(folded, lowpass_coeff(DC_HZ, sr));
        ((folded - dc) / (1.0 + dc.abs())).clamp(-1.0, 1.0)
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[drive, fold, bias, mix] = io.params else {
            return;
        };
        let smooth = smoothing(0.005, io.sample_rate);
        let drive = self
            .drive
            .process((drive + io.inputs[1] / CV_MAX).clamp(0.0, 1.0), smooth);
        let fold = self.fold_amount.process(fold, smooth);
        let bias = self.bias.process(bias, smooth);
        let dry = io.inputs[0];
        let wet = self.shape(dry / AUDIO_VOLTS, drive, fold, bias, io.sample_rate) * AUDIO_VOLTS;
        io.outputs[0] = lerp(dry, wet, mix);
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::rig::{peak, render, sine, tone};

    const F: f32 = 440.0;

    fn run(params: [f32; 4]) -> Vec<f32> {
        let n = 48_000;
        let out = render(
            &mut State::default(),
            &params,
            &[true, false],
            1,
            n,
            |i, inp| {
                inp[0] = sine(i, F, 3.0);
            },
        );
        out[0][n / 2..].to_vec()
    }

    /// Summed amplitude of the odd harmonics from the third to the ninth.
    fn odd_harmonics(x: &[f32]) -> f32 {
        (3..=9).step_by(2).map(|h| tone(x, h as f32 * F)).sum()
    }

    #[test]
    fn drive_adds_harmonics_and_stays_in_range() {
        let clean = run([0.0, 0.0, 0.5, 1.0]);
        assert!(odd_harmonics(&clean) < 0.15);
        for params in [
            [0.6, 0.0, 0.5, 1.0],
            [0.6, 0.8, 0.5, 1.0],
            [1.0, 1.0, 1.0, 1.0],
        ] {
            let out = run(params);
            assert!(peak(&out) <= 5.0, "{params:?} peaks at {}", peak(&out));
            assert!(odd_harmonics(&out) > 0.3, "{params:?} adds no harmonics");
        }
    }

    #[test]
    fn bias_adds_even_harmonics_without_dc() {
        let out = run([0.6, 0.0, 0.9, 1.0]);
        assert!(tone(&out, 2.0 * F) > 0.1);
        let mean = out.iter().sum::<f32>() / out.len() as f32;
        assert!(mean.abs() < 0.05, "dc {mean}");
    }

    #[test]
    fn mix_zero_passes_the_dry_signal() {
        let out = run([1.0, 1.0, 1.0, 0.0]);
        assert!((tone(&out, F) - 3.0).abs() < 0.01);
        assert!(tone(&out, 3.0 * F) < 0.01);
    }

    #[test]
    fn ln_cosh_survives_huge_input() {
        assert!((super::ln_cosh(1000.0) - (1000.0 - std::f32::consts::LN_2)).abs() < 1e-2);
    }
}
