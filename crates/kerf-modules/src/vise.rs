use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, OnePole, db_to_gain, smoothing};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Vise",
    band: Band::Shape,
    hp: 6,
    summary: "Compressor",
    params: &[
        param(
            "squeeze",
            "Squeeze",
            0.4,
            "How hard loud parts are pushed down.",
        ),
        param("speed", "Speed", 0.5, "Slow and smooth to fast and grabby."),
        param("makeup", "Makeup", 0.3, "Gain added after squeezing."),
    ],
    inputs: &[
        jack("in", "In", Audio, "The sound."),
        jack(
            "side",
            "Side",
            Audio,
            "Another sound that triggers the squeeze, for pumping.",
        ),
    ],
    outputs: &[jack("out", "Out", Audio, "The compressed sound.")],
};

/// Width of the soft knee around the threshold, in dB.
const KNEE_DB: f32 = 6.0;
const MAX_MAKEUP_DB: f32 = 18.0;
/// Detector floor, so silence does not take the logarithm of zero.
const FLOOR: f32 = 1e-6;

/// Log-interpolates between `slow` (at 0) and `fast` (at 1).
fn between(slow: f32, fast: f32, t: f32) -> f32 {
    slow * (fast / slow).powf(t)
}

#[derive(Debug, Clone, Default)]
pub struct State {
    /// Smoothed gain reduction in dB, never negative.
    reduction: OnePole,
}

impl State {
    /// Target gain reduction in dB for a detector level in dBFS.
    fn target(level_db: f32, threshold_db: f32, ratio: f32) -> f32 {
        let over = level_db - threshold_db;
        let slope = 1.0 - 1.0 / ratio;
        if over <= -KNEE_DB / 2.0 {
            0.0
        } else if over >= KNEE_DB / 2.0 {
            over * slope
        } else {
            slope * (over + KNEE_DB / 2.0).powi(2) / (2.0 * KNEE_DB)
        }
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[squeeze, speed, makeup] = io.params else {
            return;
        };
        let sr = io.sample_rate;
        let detector = if io.connected[1] {
            io.inputs[1]
        } else {
            io.inputs[0]
        };
        let level_db = 20.0 * (detector.abs() / AUDIO_VOLTS).max(FLOOR).log10();
        let target = Self::target(level_db, -6.0 - 30.0 * squeeze, 1.5 + 8.5 * squeeze);
        let seconds = if target > self.reduction.value() {
            between(0.03, 0.0005, speed)
        } else {
            between(0.4, 0.04, speed)
        };
        let reduction = self.reduction.process(target, smoothing(seconds, sr));
        io.outputs[0] = io.inputs[0] * db_to_gain(makeup * MAX_MAKEUP_DB - reduction);
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::rig::{render, rms, sine};

    const F: f32 = 220.0;

    /// Output RMS over the last half second of a steady sine.
    fn settled(volts: f32, side: Option<f32>, params: [f32; 3]) -> f32 {
        let n = 48_000;
        let connected = [true, side.is_some()];
        let out = render(
            &mut State::default(),
            &params,
            &connected,
            1,
            n,
            |i, inp| {
                inp[0] = sine(i, F, volts);
                inp[1] = side.map_or(0.0, |v| sine(i, F, v));
            },
        );
        rms(&out[0][n / 2..])
    }

    #[test]
    fn loud_bursts_are_pushed_down_relative_to_quiet_ones() {
        let params = [0.6, 0.5, 0.0];
        let quiet = settled(0.2, None, params);
        let loud = settled(4.0, None, params);
        assert!(
            loud / quiet < 10.0,
            "ratio {} (input ratio 20)",
            loud / quiet
        );
        assert!(loud < 4.0 / 2f32.sqrt());
    }

    #[test]
    fn quiet_sound_below_threshold_passes_untouched() {
        let out = settled(0.2, None, [0.4, 0.5, 0.0]);
        assert!((out - 0.2 / 2f32.sqrt()).abs() < 0.002, "{out}");
    }

    #[test]
    fn makeup_raises_the_level() {
        let flat = settled(0.2, None, [0.4, 0.5, 0.0]);
        let made_up = settled(0.2, None, [0.4, 0.5, 0.5]);
        assert!(made_up > 2.0 * flat);
    }

    #[test]
    fn sidechain_ducks_the_input() {
        let params = [0.6, 0.5, 0.0];
        let alone = settled(1.0, None, params);
        let ducked = settled(1.0, Some(4.0), params);
        assert!(ducked < 0.5 * alone, "{ducked} vs {alone}");
    }

    #[test]
    fn gain_reduction_settles_to_exact_silence() {
        let mut vise = State::default();
        render(
            &mut vise,
            &[0.6, 1.0, 0.0],
            &[true, false],
            1,
            7 * 48_000,
            |i, inp| {
                inp[0] = if i < 24_000 { sine(i, F, 4.0) } else { 0.0 };
            },
        );
        assert_eq!(vise.reduction.value(), 0.0);
    }
}
