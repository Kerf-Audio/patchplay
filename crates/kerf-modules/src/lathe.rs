mod tables;

use std::f32::consts::TAU;

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, CV_MAX, Phase, lerp, volts_to_hz};

use self::tables::{BANK, Bank, FRAMES, TABLES};
use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Lathe",
    band: Band::Voices,
    hp: 10,
    summary: "Wavetable voice",
    params: &[
        param("tune", "Tune", 0.5, "Coarse pitch, ±2 octaves."),
        param("table", "Table", 0.3, "Picks one of 32 wavetables."),
        param(
            "turn",
            "Turn",
            0.4,
            "Position in the table. Move it and the timbre turns, like wood on a lathe.",
        ),
        param(
            "warp",
            "Warp",
            0.2,
            "Bends the waveform for extra harmonics.",
        ),
    ],
    inputs: &[
        jack("voct", "V/Oct", Pitch, "Pitch input."),
        jack(
            "turn",
            "Turn",
            Cv,
            "Moves Turn. An LFO here makes the tone evolve.",
        ),
    ],
    outputs: &[jack("out", "Out", Audio, "The wavetable voice.")],
};

/// Deepest phase bend, in cycles, when Warp is full.
const MAX_BEND: f32 = 0.8;

pub struct State {
    bank: &'static Bank,
    phase: Phase,
}

impl State {
    pub fn new() -> Self {
        Self {
            bank: &BANK,
            phase: Phase::default(),
        }
    }

    /// Cubic (Catmull-Rom) read of one cycle at phase `t`.
    fn read(cycle: &[f32], t: f32) -> f32 {
        let mask = cycle.len() - 1;
        let x = t * cycle.len() as f32;
        let i = x as usize;
        let f = x - i as f32;
        let p = |o: usize| cycle[(i + o) & mask];
        let (p0, p1, p2, p3) = (p(mask), p(0), p(1), p(2));
        let c1 = 0.5 * (p2 - p0);
        let c2 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
        let c3 = 0.5 * (p3 - p0) + 1.5 * (p1 - p2);
        p1 + f * (c1 + f * (c2 + f * c3))
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[tune, table, turn, warp] = io.params else {
            return;
        };
        let &[voct, turn_cv] = io.inputs else {
            return;
        };
        let volts = voct + (tune - 0.5) * 4.0;
        let dt = (volts_to_hz(volts) / io.sample_rate).min(0.45);
        let t = self.phase.advance(dt);

        // Bending the phase spreads harmonics upwards; leave room for them.
        let bend = MAX_BEND * warp;
        let mip = Bank::mip_for(dt * (1.0 + 1.5 * bend));
        let t = t + bend * (TAU * t).sin() / TAU;
        let t = t - t.floor();

        let table = (table * (TABLES - 1) as f32).round() as usize;
        let position = ((turn + turn_cv / CV_MAX).clamp(0.0, 1.0)) * (FRAMES - 1) as f32;
        let frame = (position as usize).min(FRAMES - 2);
        let mix = position - frame as f32;
        let a = Self::read(self.bank.cycle(mip, table, frame), t);
        let b = Self::read(self.bank.cycle(mip, table, frame + 1), t);
        io.outputs[0] = lerp(a, b, mix) * AUDIO_VOLTS;
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::rig::{SR, peak, render, rms, tone};

    /// Renders one second at the given settings and V/Oct.
    fn run(params: [f32; 4], voct: f32, turn_cv: f32) -> Vec<f32> {
        let mut lathe = State::new();
        let out = render(
            &mut lathe,
            &params,
            &[true, true],
            1,
            SR as usize,
            |_, i| {
                i[0] = voct;
                i[1] = turn_cv;
            },
        );
        out.into_iter().next().unwrap()
    }

    fn rising_crossings(x: &[f32]) -> usize {
        x.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count()
    }

    #[test]
    fn plays_the_pitch_it_is_given() {
        // Table 0 at Turn 0 is a sine.
        let c4 = run([0.5, 0.0, 0.0, 0.0], 0.0, 0.0);
        let hz = rising_crossings(&c4) as f32;
        assert!((hz - 261.6).abs() <= 1.5, "C4 played {hz} Hz");
        let c5 = run([0.5, 0.0, 0.0, 0.0], 1.0, 0.0);
        let hz = rising_crossings(&c5) as f32;
        assert!((hz - 523.3).abs() <= 1.5, "C5 played {hz} Hz");
        assert!((rms(&c4) - 0.97 * 5.0 / 2f32.sqrt()).abs() < 0.1);
    }

    #[test]
    fn turning_changes_the_timbre_and_never_clips() {
        let sine = run([0.5, 0.0, 0.0, 0.0], 0.0, 0.0);
        let saw = run([0.5, 0.0, 1.0, 0.0], 0.0, 0.0);
        let cv = run([0.5, 0.0, 0.0, 0.0], 0.0, 10.0);
        assert!(tone(&sine, 523.3) < 0.01);
        assert!(tone(&saw, 523.3) > 0.5, "the saw has an octave");
        assert_eq!(saw, cv, "Turn CV adds to the knob");
        for table in 0..32 {
            for turn in [0.0, 0.4, 1.0] {
                let out = run([0.5, table as f32 / 31.0, turn, 0.5], 0.5, 0.0);
                let p = peak(&out);
                assert!(p > 0.5 && p <= 5.0, "table {table} turn {turn}: peak {p} V");
                assert!(rms(&out) > 0.15, "table {table} turn {turn} is quiet");
            }
        }
    }

    #[test]
    fn high_notes_do_not_fold_back() {
        // At 3.3 kHz a saw's 8th harmonic (26.4 kHz) would fold to 21.6 kHz,
        // between the 6th and 7th. Nothing may land between harmonics.
        let out = run([0.5, 0.0, 1.0, 0.5], 3.66, 0.0);
        let hz = 261.63 * 3.66f32.exp2();
        let fundamental = tone(&out, hz);
        let worst = (1..=6)
            .map(|k| tone(&out, hz * (k as f32 + 0.5)))
            .fold(0.0, f32::max);
        assert!(
            worst < fundamental * 0.02,
            "alias {worst} vs fundamental {fundamental}"
        );
    }
}
