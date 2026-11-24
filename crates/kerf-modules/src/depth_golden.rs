//! Motion modules that gained a Depth knob scale their CV outputs by it. At its
//! default of 1 the multiply is exact, so nothing else needs pinning (bit
//! hashes of the output would differ between platforms' maths libraries).

use kerf_core::Catalogue;

use crate::{Builtin, Io, instantiate};

const RATE: f32 = 48_000.0;

/// Two seconds of busy input with each param but Depth at `0.37`. Returns every output sample, frame by frame.
fn render(kind: &str, depth: f32) -> Vec<Vec<f32>> {
    let d = Builtin.get(kind).expect("a catalogue kind");
    let params: Vec<f32> = d
        .params
        .iter()
        .map(|p| if p.key == "depth" { depth } else { 0.37 })
        .collect();
    let connected = vec![true; d.inputs.len()];
    let mut inputs = vec![0.0; d.inputs.len()];
    let mut outputs = vec![0.0; d.outputs.len()];
    let mut dsp = instantiate(kind, 3, RATE).expect("DSP exists");
    let mut frames = Vec::new();
    for i in 0..2 * RATE as usize {
        let t = i as f32 / RATE;
        for (k, v) in inputs.iter_mut().enumerate() {
            let pulse = if (6.0 * t + k as f32 * 0.11).fract() < 0.25 {
                9.0
            } else {
                0.0
            };
            let hum = 3.0 * (std::f32::consts::TAU * (130.0 + 91.0 * k as f32) * t).sin();
            *v = pulse + hum;
        }
        dsp.process(&mut Io {
            sample_rate: RATE,
            tempo: 120.0,
            params: &params,
            inputs: &inputs,
            connected: &connected,
            outputs: &mut outputs,
        });
        frames.push(outputs.clone());
    }
    frames
}

/// Which outputs Depth scales, and the voltage a scaled output rests at when
/// Depth is 0 (0 V for most, the 5 V centre for Spokeshave's LFOs).
#[test]
fn depth_scales_the_cv_outputs_towards_their_rest_voltage() {
    let scaled: [(&str, &[usize], f32); 5] = [
        ("Bevel", &[0, 1], 0.0),
        ("Burl", &[0, 1], 0.0),
        ("Level", &[0], 0.0),
        ("Jig", &[0, 1, 2, 3], 0.0),
        ("Spokeshave", &[0, 1, 2, 3], 5.0),
    ];
    for (kind, lanes, rest) in scaled {
        let full = render(kind, 1.0);
        let half = render(kind, 0.5);
        let off = render(kind, 0.0);
        let mut moved = false;
        for ((f, h), o) in full.iter().zip(&half).zip(&off) {
            for &lane in lanes {
                let want = rest + (f[lane] - rest) * 0.5;
                assert!((h[lane] - want).abs() < 1e-4, "{kind} lane {lane}");
                assert!((o[lane] - rest).abs() < 1e-4, "{kind} lane {lane} at 0");
                moved |= (f[lane] - rest).abs() > 0.5;
            }
        }
        assert!(moved, "{kind} never left its rest voltage at full depth");
    }
}
