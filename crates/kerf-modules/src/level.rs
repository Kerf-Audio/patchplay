use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, CV_MAX, GATE_HIGH, OnePole, smoothing};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Level",
    band: Band::Motion,
    hp: 4,
    summary: "Envelope follower",
    params: &[
        param(
            "attack",
            "Attack",
            0.1,
            "How quickly it follows a sound getting louder.",
        ),
        param(
            "release",
            "Release",
            0.4,
            "How quickly it lets go when the sound gets quieter.",
        ),
        param(
            "gain",
            "Gain",
            0.5,
            "Scales the output, and so how loud the sound must be to switch Over on.",
        ),
        param(
            "depth",
            "Depth",
            1.0,
            "Scales Env. 0 holds it at 0 V. Over still follows the sound.",
        ),
    ],
    inputs: &[jack("in", "In", Audio, "The sound to follow.")],
    outputs: &[
        jack("env", "Env", Cv, "Rises and falls with the loudness of In."),
        jack("over", "Over", Gate, "High while the sound is loud."),
    ],
};

/// Over switches on at this fraction of full scale on Env and off at the lower one.
const OVER_ON: f32 = 0.5;
const OVER_OFF: f32 = 0.35;

#[derive(Default)]
pub struct State {
    /// Smoothed absolute level of the input, 1.0 at full scale (5 V).
    env: OnePole,
    over: bool,
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[attack, release, gain, depth] = io.params else {
            return;
        };
        let level = io.inputs[0].abs() / AUDIO_VOLTS;
        let seconds = if level > self.env.value() {
            0.001 * 1000f32.powf(attack)
        } else {
            0.005 * 2000f32.powf(release)
        };
        let followed = self.env.process(level, smoothing(seconds, io.sample_rate));
        // Unity gain at the knob's centre, 8× at the top, ⅛ at the bottom.
        let env = (followed * ((gain - 0.5) * 6.0).exp2()).clamp(0.0, 1.0);
        if env > OVER_ON {
            self.over = true;
        } else if env < OVER_OFF {
            self.over = false;
        }
        io.outputs[0] = env * CV_MAX * depth;
        io.outputs[1] = if self.over { GATE_HIGH } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::State;
    use crate::{Dsp, Io};

    const SR: f32 = 48_000.0;

    /// 440 Hz sine of `volts` peak for the first `loud` seconds, then
    /// silence, rendered for `total` seconds. Frames are `[env, over]`.
    fn run(params: [f32; 3], volts: f32, loud: f32, total: f32) -> Vec<[f32; 2]> {
        run_with(&mut State::default(), params, volts, loud, total)
    }

    fn run_with(
        level: &mut State,
        params: [f32; 3],
        volts: f32,
        loud: f32,
        total: f32,
    ) -> Vec<[f32; 2]> {
        (0..(total * SR) as usize)
            .map(|i| {
                let t = i as f32 / SR;
                let x = if t < loud {
                    volts * (TAU * 440.0 * t).sin()
                } else {
                    0.0
                };
                let mut out = [0.0; 2];
                level.process(&mut Io {
                    sample_rate: SR,
                    tempo: 120.0,
                    params: &[params[0], params[1], params[2], 1.0],
                    inputs: &[x],
                    connected: &[true],
                    outputs: &mut out,
                });
                out
            })
            .collect()
    }

    #[test]
    fn follows_a_loud_sound_and_lets_go_in_silence() {
        let frames = run([0.1, 0.4, 0.5], 5.0, 1.0, 4.0);
        let loud = frames[(0.9 * SR) as usize];
        assert!(loud[0] > 7.0 && loud[0] <= 10.0, "env {}", loud[0]);
        assert_eq!(loud[1], 10.0);
        let silent = frames[(3.9 * SR) as usize];
        assert!(silent[0] < 0.1 && silent[1] == 0.0, "{silent:?}");
    }

    #[test]
    fn over_needs_a_loud_enough_sound_and_gain_lowers_the_bar() {
        let quiet = run([0.1, 0.4, 0.5], 0.5, 1.0, 1.0);
        assert!(quiet.iter().all(|f| f[1] == 0.0));
        let boosted = run([0.1, 0.4, 1.0], 0.5, 1.0, 1.0);
        assert_eq!(boosted[(0.9 * SR) as usize][1], 10.0);
    }

    #[test]
    fn attack_and_release_set_the_speed() {
        let fast_attack = run([0.0, 1.0, 0.5], 5.0, 0.5, 1.0);
        let slow_attack = run([1.0, 0.0, 0.5], 5.0, 0.5, 1.0);
        let at = |frames: &[[f32; 2]], seconds: f32| frames[(seconds * SR) as usize][0];
        // 50 ms in: the fast attack is up, the slow one has barely moved.
        assert!(at(&fast_attack, 0.05) > 7.0 && at(&slow_attack, 0.05) < 1.0);
        // 0.4 s after the sound stops: the slow release is still high, the fast one gone.
        assert!(at(&fast_attack, 0.9) > 7.0 && at(&slow_attack, 0.9) < 0.1);
    }

    #[test]
    fn the_follower_settles_to_exact_silence() {
        let mut level = State::default();
        run_with(&mut level, [0.0, 0.0, 0.5], 5.0, 0.1, 2.0);
        assert_eq!(level.env.value(), 0.0, "stuck at {:e}", level.env.value());
    }
}
