use std::f32::consts::{LN_10, TAU};

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, OnePole, Phase, flush, lerp, lowpass_coeff, smoothing};

use crate::delay_line::DelayLine;
use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Kiln",
    band: Band::Space,
    hp: 8,
    summary: "Reverb",
    params: &[
        param("size", "Size", 0.6, "Small room to cathedral."),
        param("heat", "Heat", 0.4, "Damping. Hotter is darker."),
        param("bloom", "Bloom", 0.3, "How slowly the tail swells in."),
        param("mix", "Mix", 0.25, "Dry to wet."),
    ],
    inputs: &[
        jack("in_l", "In L", Audio, "Left in, or mono."),
        jack("in_r", "In R", Audio, "Right in."),
    ],
    outputs: &[
        jack("l", "L", Audio, "Left."),
        jack("r", "R", Audio, "Right."),
    ],
};

/// Eight-line feedback delay network: lengths in milliseconds at Size 1/1.5,
/// mutually prime-ish so the echoes do not line up.
const LINES: usize = 8;
const LINE_MS: [f32; LINES] = [31.1, 37.3, 43.9, 51.7, 59.3, 67.1, 73.9, 81.7];
const MIN_SCALE: f32 = 0.3;
const MAX_SCALE: f32 = 1.5;
const MIN_RT60: f32 = 0.4;
const MAX_RT60: f32 = 12.0;
/// Damping cutoff: bright at no Heat, dark at full Heat.
const BRIGHT_HZ: f32 = 14_000.0;
const DARK_HZ: f32 = 1_200.0;
/// Slow modulation of the line lengths, which keeps the tail from ringing.
const MOD_HZ: f32 = 0.23;
const MOD_SECONDS: f32 = 0.0005;
/// Input diffusers, in milliseconds: two per channel.
const DIFFUSER_MS: [f32; 4] = [4.8, 7.7, 5.3, 8.3];
const DIFFUSER_GAIN: f32 = 0.6;
const SIZE_GLIDE_SECONDS: f32 = 0.05;
/// How loud the input must be to fully duck the wet signal, as a fraction
/// of full scale.
const BLOOM_SENSE: f32 = 4.0;
const MAX_BLOOM_SECONDS: f32 = 0.85;

/// Schroeder all-pass: spreads an input over time without colouring it.
#[derive(Debug, Clone)]
struct Diffuser {
    line: DelayLine,
    delay: f32,
}

impl Diffuser {
    fn new(seconds: f32, sample_rate: f32) -> Self {
        let delay = (seconds * sample_rate).round().max(1.0);
        Self {
            line: DelayLine::new(delay as usize),
            delay,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let echo = self.line.tap(self.delay);
        let v = x + DIFFUSER_GAIN * echo;
        self.line.push(flush(v));
        echo - DIFFUSER_GAIN * v
    }
}

/// In-place Walsh-Hadamard transform, scaled to be energy-preserving.
fn hadamard(x: &mut [f32; LINES]) {
    let mut half = 1;
    while half < LINES {
        for start in (0..LINES).step_by(half * 2) {
            for i in start..start + half {
                let (a, b) = (x[i], x[i + half]);
                x[i] = a + b;
                x[i + half] = a - b;
            }
        }
        half *= 2;
    }
    let scale = (LINES as f32).sqrt().recip();
    x.iter_mut().for_each(|v| *v *= scale);
}

#[derive(Debug, Clone)]
pub struct State {
    lines: [DelayLine; LINES],
    damping: [OnePole; LINES],
    diffusers: [Diffuser; 4],
    lfo: Phase,
    /// Smoothed Size; `None` until the first sample.
    size: Option<f32>,
    /// Recent input loudness, which holds the wet signal down (Bloom).
    duck: f32,
}

impl State {
    pub fn new(sample_rate: f32) -> Self {
        let longest = LINE_MS[LINES - 1] * 1e-3 * MAX_SCALE + MOD_SECONDS;
        Self {
            lines: std::array::from_fn(|_| DelayLine::new((longest * sample_rate) as usize + 1)),
            damping: [OnePole::default(); LINES],
            diffusers: DIFFUSER_MS.map(|ms| Diffuser::new(ms * 1e-3, sample_rate)),
            lfo: Phase::default(),
            size: None,
            duck: 0.0,
        }
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[size_knob, heat, bloom, mix] = io.params else {
            return;
        };
        let sr = io.sample_rate;
        let size = self.size.unwrap_or(size_knob);
        let size = size + smoothing(SIZE_GLIDE_SECONDS, sr) * (size_knob - size);
        self.size = Some(size);

        let dry_l = io.inputs[0];
        let dry_r = if io.connected[1] { io.inputs[1] } else { dry_l };
        let scale = lerp(MIN_SCALE, MAX_SCALE, size);
        let rt60 = MIN_RT60 * (MAX_RT60 / MIN_RT60).powf(size);
        let decay_per_second = -3.0 * LN_10 / rt60;
        let damp = lowpass_coeff(BRIGHT_HZ * (DARK_HZ / BRIGHT_HZ).powf(heat), sr);

        let [d0, d1, d2, d3] = &mut self.diffusers;
        let in_l = d1.process(d0.process(dry_l / AUDIO_VOLTS));
        let in_r = d3.process(d2.process(dry_r / AUDIO_VOLTS));

        // Keep the stationary level near the input's whatever the decay time.
        let mean_ms = LINE_MS.iter().sum::<f32>() / LINES as f32;
        let mean_loop = (decay_per_second * mean_ms * 1e-3 * scale).exp();
        let inject = (2.0 * (1.0 - mean_loop * mean_loop)).sqrt();

        let lfo = self.lfo.advance(MOD_HZ / sr);
        let mut returned = [0.0; LINES];
        let (mut wet_l, mut wet_r) = (0.0, 0.0);
        for (i, ((line, lp), out)) in self
            .lines
            .iter()
            .zip(self.damping.iter_mut())
            .zip(returned.iter_mut())
            .enumerate()
        {
            let seconds = LINE_MS[i] * 1e-3 * scale;
            let wobble = MOD_SECONDS * ((lfo + i as f32 / LINES as f32) * TAU).sin();
            let tapped = lp.process(line.tap((seconds + wobble) * sr), damp);
            *out = tapped * (decay_per_second * seconds).exp();
            let signed = if i % 4 < 2 { tapped } else { -tapped };
            if i % 2 == 0 {
                wet_l += signed;
            } else {
                wet_r += signed;
            }
        }
        hadamard(&mut returned);
        for (i, (line, fed)) in self.lines.iter_mut().zip(returned).enumerate() {
            let injected = inject * if i % 2 == 0 { in_l } else { in_r };
            line.push(flush(fed + injected));
        }

        // Bloom: loud input holds the wet signal down; it swells in as the
        // input fades.
        let loudness = (in_l.abs().max(in_r.abs()) * BLOOM_SENSE).min(1.0);
        if loudness > self.duck {
            self.duck = loudness;
        } else {
            let decay = smoothing(0.05 + bloom * MAX_BLOOM_SECONDS, sr);
            self.duck = flush(self.duck - decay * self.duck);
        }
        let swell = 1.0 - bloom * self.duck;

        io.outputs[0] = lerp(dry_l, (0.5 * wet_l * swell).tanh() * AUDIO_VOLTS, mix);
        io.outputs[1] = lerp(dry_r, (0.5 * wet_r * swell).tanh() * AUDIO_VOLTS, mix);
    }
}

#[cfg(test)]
mod tests {
    use kerf_dsp::Rng;

    use super::State;
    use crate::rig::{SR, peak, render, rms, sine};

    const CONNECTED: [bool; 2] = [true, false];

    fn hall(params: [f32; 4], seconds: f32, input: impl FnMut(usize, &mut [f32])) -> Vec<Vec<f32>> {
        render(
            &mut State::new(SR),
            &params,
            &CONNECTED,
            2,
            (seconds * SR) as usize,
            input,
        )
    }

    fn clap(i: usize, inp: &mut [f32]) {
        inp[0] = if i == 0 { 5.0 } else { 0.0 };
    }

    fn tone_burst(i: usize, inp: &mut [f32]) {
        inp[0] = if i < 9_600 { sine(i, 440.0, 5.0) } else { 0.0 };
    }

    #[test]
    fn the_tail_decays_to_silence_at_every_extreme() {
        for params in [
            [1.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 0.0, 1.0],
            [0.0, 0.0, 0.0, 1.0],
            [0.6, 0.4, 0.3, 1.0],
        ] {
            let out = hall(params, 40.0, tone_burst);
            let tail = (35.0 * SR) as usize;
            let settled = peak(&out[0][tail..]).max(peak(&out[1][tail..]));
            assert!(settled < 1e-4, "{params:?} still sounds at {settled} V");
            let early = peak(&out[0][..3 * SR as usize]);
            assert!(early > 0.05, "{params:?} makes no tail ({early} V)");
        }
    }

    #[test]
    fn a_hot_signal_at_full_size_never_leaves_range() {
        let mut rng = Rng::new(3);
        let noise: Vec<f32> = (0..(10.0 * SR) as usize)
            .map(|_| (rng.unit() * 2.0 - 1.0) * 5.0)
            .collect();
        let out = hall([1.0, 0.0, 0.0, 1.0], 12.0, |i, inp| {
            inp[0] = noise.get(i).copied().unwrap_or(0.0)
        });
        assert!(peak(&out[0]) <= 5.0 && peak(&out[1]) <= 5.0);
        assert!(rms(&out[0][(8.0 * SR) as usize..(10.0 * SR) as usize]) > 0.5);
    }

    #[test]
    fn a_mono_input_comes_out_in_stereo() {
        let out = hall([0.6, 0.4, 0.0, 1.0], 1.0, clap);
        let apart: f32 = out[0].iter().zip(&out[1]).map(|(l, r)| (l - r).abs()).sum();
        assert!(apart > 1.0, "channels are identical");
    }

    #[test]
    fn mix_zero_is_dry_only() {
        let out = hall([0.6, 0.4, 0.0, 0.0], 0.1, |i, inp| {
            inp[0] = if i % 100 == 0 { 3.0 } else { 0.0 }
        });
        assert_eq!(out[0][100], 3.0);
        assert_eq!(out[1][100], 3.0);
        assert_eq!(out[0][50], 0.0);
    }

    #[test]
    fn bloom_holds_the_wet_signal_down_under_loud_input() {
        let mut rng = Rng::new(5);
        let noise: Vec<f32> = (0..24_000)
            .map(|_| (rng.unit() * 2.0 - 1.0) * 4.0)
            .collect();
        let burst = |i: usize, inp: &mut [f32]| inp[0] = noise.get(i).copied().unwrap_or(0.0);
        let plain = hall([0.6, 0.4, 0.0, 1.0], 0.5, burst);
        let bloomed = hall([0.6, 0.4, 1.0, 1.0], 0.5, burst);
        let window = 4_800..24_000;
        assert!(rms(&bloomed[0][window.clone()]) < 0.3 * rms(&plain[0][window]));
    }

    #[test]
    fn the_recursive_state_settles_to_exact_silence() {
        let mut kiln = State::new(SR);
        render(
            &mut kiln,
            &[0.0, 0.0, 0.0, 1.0],
            &CONNECTED,
            2,
            (8.0 * SR) as usize,
            tone_burst,
        );
        assert_eq!(kiln.duck, 0.0);
        assert!(kiln.damping.iter().all(|d| d.value() == 0.0));
    }
}
