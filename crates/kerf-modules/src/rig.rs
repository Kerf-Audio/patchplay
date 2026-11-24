//! Offline render helpers for module DSP tests.

use crate::{Dsp, Io};

pub const SR: f32 = 48_000.0;
pub const TEMPO: f32 = 120.0;

/// Runs `dsp` for `frames` samples at [`SR`]. `connected` sets how many
/// inputs there are and which are patched; `input` fills each sample's
/// input volts. Returns one vector per output.
pub fn render(
    dsp: &mut dyn Dsp,
    params: &[f32],
    connected: &[bool],
    outputs: usize,
    frames: usize,
    mut input: impl FnMut(usize, &mut [f32]),
) -> Vec<Vec<f32>> {
    let mut inputs = vec![0.0; connected.len()];
    let mut out = vec![0.0; outputs];
    let mut rendered = vec![Vec::with_capacity(frames); outputs];
    for i in 0..frames {
        inputs.fill(0.0);
        input(i, &mut inputs);
        dsp.process(&mut Io {
            sample_rate: SR,
            tempo: TEMPO,
            params,
            inputs: &inputs,
            connected,
            outputs: &mut out,
        });
        for (channel, &v) in rendered.iter_mut().zip(&out) {
            assert!(v.is_finite(), "non-finite output at sample {i}");
            channel.push(v);
        }
    }
    rendered
}

pub fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, v| m.max(v.abs()))
}

pub fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// Amplitude of the sine component at `hz` (single-bin DFT).
pub fn tone(x: &[f32], hz: f32) -> f32 {
    let w = std::f32::consts::TAU * hz / SR;
    let (mut re, mut im) = (0.0, 0.0);
    for (i, v) in x.iter().enumerate() {
        let (s, c) = (w * i as f32).sin_cos();
        re += v * c;
        im += v * s;
    }
    2.0 * re.hypot(im) / x.len() as f32
}

/// A sine of `volts` peak at `hz`, sample `i`.
pub fn sine(i: usize, hz: f32, volts: f32) -> f32 {
    volts * (std::f32::consts::TAU * hz * i as f32 / SR).sin()
}
