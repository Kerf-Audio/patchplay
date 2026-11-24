use std::f32::consts::FRAC_PI_4;

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, Gate, OnePole, Phase, Rng, Svf, poly_blep, smoothing, volts_to_hz};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Ply",
    band: Band::Voices,
    hp: 10,
    summary: "Stacked, detuned voices",
    params: &[
        param(
            "layers",
            "Layers",
            0.6,
            "How many detuned copies are stacked, 1–9.",
        ),
        param(
            "detune",
            "Detune",
            0.35,
            "How far apart the layers are tuned.",
        ),
        param(
            "width",
            "Width",
            0.7,
            "Spreads the layers across the stereo field.",
        ),
        param("tone", "Tone", 0.5, "Darkens or brightens the stack."),
    ],
    inputs: &[
        jack(
            "voct",
            "V/Oct",
            Pitch,
            "Pitch of the voice, for example from Fret.",
        ),
        jack(
            "gate",
            "Gate",
            Gate,
            "Opens the voice. Left unpatched, the voice drones.",
        ),
    ],
    outputs: &[
        jack("l", "L", Audio, "Left."),
        jack("r", "R", Audio, "Right."),
    ],
};

const MAX_LAYERS: usize = 9;
/// Spread at full Detune, in cents either side of the note.
const MAX_CENTS: f32 = 60.0;

pub struct State {
    phases: [Phase; MAX_LAYERS],
    /// Left and right gain of each layer for the current layer count and
    /// width.
    pans: [[f32; 2]; MAX_LAYERS],
    layout: (usize, f32),
    gate: Gate,
    envelope: OnePole,
    attack: f32,
    release: f32,
    filters: [Svf; 2],
}

impl State {
    pub fn new(seed: u64, sample_rate: f32) -> Self {
        let mut rng = Rng::new(seed);
        let mut phases = [Phase::default(); MAX_LAYERS];
        for p in &mut phases {
            p.advance(rng.unit());
        }
        Self {
            phases,
            pans: [[0.0; 2]; MAX_LAYERS],
            layout: (0, -1.0),
            gate: Gate::default(),
            envelope: OnePole::default(),
            attack: smoothing(0.003, sample_rate),
            release: smoothing(0.08, sample_rate),
            filters: [Svf::default(); 2],
        }
    }

    /// Position of layer `i` of `n` across the stack, `-1..=1`.
    fn spread(i: usize, n: usize) -> f32 {
        if n == 1 {
            0.0
        } else {
            i as f32 / (n - 1) as f32 * 2.0 - 1.0
        }
    }

    /// Equal-power pans, lowest layer leftmost at full width.
    fn lay_out(&mut self, n: usize, width: f32) {
        if self.layout == (n, width) {
            return;
        }
        self.layout = (n, width);
        for (i, pan) in self.pans.iter_mut().enumerate().take(n) {
            let angle = (Self::spread(i, n) * width + 1.0) * FRAC_PI_4;
            *pan = [angle.cos(), angle.sin()];
        }
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[layers, detune, width, tone] = io.params else {
            return;
        };
        let &[voct, gate] = io.inputs else {
            return;
        };
        let sr = io.sample_rate;
        let n = 1 + (layers * (MAX_LAYERS - 1) as f32).round() as usize;
        self.lay_out(n, width);

        self.gate.update(gate);
        let open = !io.connected[1] || self.gate.is_high();
        let coeff = if open { self.attack } else { self.release };
        let level = self.envelope.process(f32::from(u8::from(open)), coeff);

        let hz = volts_to_hz(voct);
        let spread_octaves = detune.powf(1.5) * MAX_CENTS / 1200.0;
        let (mut l, mut r) = (0.0, 0.0);
        for (i, (phase, pan)) in self.phases.iter_mut().zip(&self.pans).enumerate().take(n) {
            let dt = (hz * (Self::spread(i, n) * spread_octaves).exp2() / sr).min(0.45);
            let t = phase.advance(dt);
            let saw = 2.0 * t - 1.0 - poly_blep(t, dt);
            l += saw * pan[0];
            r += saw * pan[1];
        }

        let cutoff = (hz * (1.0 + tone * 5.0).exp2()).min(18_000.0);
        let scale = 0.7 / (n as f32).sqrt() * level;
        let l = self.filters[0].process(l * scale, cutoff, 0.7, sr).low;
        let r = self.filters[1].process(r * scale, cutoff, 0.7, sr).low;
        io.outputs[0] = l.tanh() * AUDIO_VOLTS;
        io.outputs[1] = r.tanh() * AUDIO_VOLTS;
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::rig::{SR, peak, render, rms};

    /// Renders `seconds` of both channels with the gate at `gate` volts, or
    /// unpatched when `None`.
    fn run(seed: u64, params: [f32; 4], gate: Option<f32>, seconds: f32) -> Vec<Vec<f32>> {
        let mut ply = State::new(seed, SR);
        render(
            &mut ply,
            &params,
            &[true, gate.is_some()],
            2,
            (seconds * SR) as usize,
            |_, i| i[1] = gate.unwrap_or(0.0),
        )
    }

    fn rising_crossings(x: &[f32]) -> usize {
        x.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count()
    }

    #[test]
    fn one_layer_plays_the_note() {
        // One layer, tone fully open: a plain saw at C4.
        let out = run(1, [0.0, 0.0, 0.0, 1.0], None, 1.0);
        let hz = rising_crossings(&out[0]) as f32;
        assert!((hz - 261.6).abs() <= 2.0, "played {hz} Hz");
        assert!(peak(&out[0]) > 2.0 && peak(&out[0]) <= 5.0);
    }

    #[test]
    fn stack_stays_in_range_and_has_the_same_weight_at_any_size() {
        let mut levels = Vec::new();
        for layers in [0.0, 0.5, 1.0] {
            let out = run(2, [layers, 0.35, 0.7, 0.5], None, 1.0);
            assert!(peak(&out[0]) <= 5.0 && peak(&out[1]) <= 5.0);
            levels.push(rms(&out[0]));
        }
        let (lo, hi) = levels
            .iter()
            .fold((f32::MAX, 0.0f32), |(a, b), &v| (a.min(v), b.max(v)));
        assert!(lo > 0.8 && hi / lo < 1.4, "levels {levels:?}");
    }

    #[test]
    fn width_spreads_the_layers_and_seed_sets_the_phases() {
        let mono = run(3, [1.0, 0.35, 0.0, 0.5], None, 0.2);
        assert_eq!(mono[0], mono[1]);
        let wide = run(3, [1.0, 0.35, 1.0, 0.5], None, 0.2);
        assert_ne!(wide[0], wide[1]);
        assert_eq!(wide, run(3, [1.0, 0.35, 1.0, 0.5], None, 0.2));
        assert_ne!(wide, run(4, [1.0, 0.35, 1.0, 0.5], None, 0.2));
    }

    #[test]
    fn gate_opens_and_closes_the_voice() {
        let params = [0.6, 0.35, 0.7, 0.5];
        let closed = run(5, params, Some(0.0), 0.5);
        assert!(peak(&closed[0]) < 0.001, "a closed gate is silent");
        let open = run(5, params, Some(10.0), 0.5);
        assert!(rms(&open[0][(SR * 0.1) as usize..]) > 0.8);

        let mut ply = State::new(5, SR);
        let out = render(&mut ply, &params, &[true, true], 2, SR as usize, |i, v| {
            v[1] = if i < SR as usize / 2 { 10.0 } else { 0.0 };
        });
        assert!(peak(&out[0][(SR * 0.9) as usize..]) < 0.05, "released");
    }
}
