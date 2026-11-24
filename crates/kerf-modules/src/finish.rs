use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, OnePole, db_to_gain, flush, smoothing};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Finish",
    band: Band::Out,
    hp: 6,
    summary: "Master output",
    params: &[
        param("volume", "Volume", 0.7, "Master level."),
        param("glue", "Glue", 0.3, "Gentle bus compression."),
        param(
            "ceiling",
            "Ceiling",
            0.8,
            "Limiter ceiling; nothing goes louder than this.",
        ),
    ],
    inputs: &[
        jack("l", "L", Audio, "Left to speakers."),
        jack("r", "R", Audio, "Right to speakers."),
    ],
    outputs: &[],
};

/// Compression starts at about −9 dBFS.
const GLUE_THRESHOLD: f32 = 0.35;

#[derive(Default)]
pub struct State {
    glue_env: OnePole,
    limit_env: f32,
    out: [f32; 2],
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[volume, glue, ceiling] = io.params else {
            return;
        };
        let sr = io.sample_rate;
        let gain = (volume / 0.7).powi(2) / AUDIO_VOLTS;
        let l = io.inputs[0] * gain;
        // An unpatched R copies L, so one cable gives a centred mono mix.
        let r = if io.connected[1] {
            io.inputs[1]
        } else {
            io.inputs[0]
        } * gain;

        let level = l.abs().max(r.abs());
        let seconds = if level > self.glue_env.value() {
            0.01
        } else {
            0.15
        };
        let glue_env = self.glue_env.process(level, smoothing(seconds, sr));
        let ratio = 1.0 + glue * 3.0;
        let glue_gain = if glue_env > GLUE_THRESHOLD {
            (GLUE_THRESHOLD / glue_env).powf(1.0 - 1.0 / ratio)
        } else {
            1.0
        };
        let (l, r) = (l * glue_gain, r * glue_gain);

        // Instant-attack peak limiter: the output never passes the ceiling.
        let ceiling = db_to_gain(-12.0 + ceiling * 12.0);
        let peak = l.abs().max(r.abs());
        self.limit_env = flush(peak.max(self.limit_env * (1.0 - smoothing(0.1, sr))));
        let limit = if self.limit_env > ceiling {
            ceiling / self.limit_env
        } else {
            1.0
        };
        self.out = [l * limit, r * limit];
    }

    fn master(&self) -> Option<[f32; 2]> {
        Some(self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::rig::{render, sine};

    #[test]
    fn the_envelopes_settle_to_exact_silence() {
        let mut finish = State::default();
        render(
            &mut finish,
            &[0.7, 1.0, 1.0],
            &[true, true],
            0,
            12 * 48_000,
            |i, inp| {
                if i < 4_800 {
                    inp[0] = sine(i, 220.0, 5.0);
                    inp[1] = inp[0];
                }
            },
        );
        assert_eq!(finish.glue_env.value(), 0.0);
        assert_eq!(finish.limit_env, 0.0);
    }
}
