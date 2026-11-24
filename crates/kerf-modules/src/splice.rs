use std::f32::consts::TAU;

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, CV_MAX, OnePole, Phase, lerp, smoothing};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Splice",
    band: Band::Shape,
    hp: 8,
    summary: "FM, AM, ring and sync modulator",
    params: &[
        param(
            "mode",
            "Mode",
            2.0 / 3.0,
            "FM bends the carrier's pitch, AM swells its volume, ring keeps only the sum and difference tones, sync restarts the carrier's cycle on every modulator cycle.",
        ),
        param(
            "amount",
            "Amount",
            0.7,
            "How hard the modulator works: FM depth, AM depth, ring blend or how far sync resets.",
        ),
        param(
            "coarse",
            "Coarse",
            0.4,
            "Pitch of the built-in modulator as a ratio of the carrier, from a quarter up to 8 times. Used when nothing is patched into Mod.",
        ),
        param(
            "fine",
            "Fine",
            0.5,
            "Nudges the built-in modulator up or down by a semitone. Off the whole ratios it turns clangorous.",
        ),
        param("mix", "Mix", 1.0, "Dry carrier to modulated sound."),
        param(
            "level",
            "Level",
            0.5,
            "Volume of the modulated sound. Half is unity.",
        ),
    ],
    inputs: &[
        jack("carrier", "Carrier", Audio, "The sound to modulate."),
        jack(
            "mod",
            "Mod",
            Audio,
            "The sound that modulates it. Leave empty to use the built-in sine.",
        ),
        jack("amount", "Amount", Cv, "Moves Amount."),
    ],
    outputs: &[jack("out", "Out", Audio, "The modulated sound.")],
};

/// Built-in modulator ratios for Coarse.
const RATIOS: [f32; 11] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];
/// FM swings the carrier's delay this far either way at full Amount.
const MAX_DEPTH_SECONDS: f32 = 0.003;
/// Sync never reaches back further than this: carriers below about 40 Hz
/// restart from where they were 25 ms ago.
const MAX_SYNC_SECONDS: f32 = 0.025;
/// A zero crossing counts only after the signal has been this far below zero.
const HYSTERESIS: f32 = 0.02;
/// Carrier pitch the built-in modulator follows is limited to this range.
const MIN_TRACK_HZ: f32 = 30.0;
/// Where the output limiter starts to round off, as a fraction of full scale.
const KNEE: f32 = 0.95;
const AMOUNT_SECONDS: f32 = 0.005;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Fm,
    Am,
    Ring,
    Sync,
}

impl Mode {
    fn from_param(p: f32) -> Self {
        match (p * 3.0).round() as u8 {
            0 => Self::Fm,
            1 => Self::Am,
            2 => Self::Ring,
            _ => Self::Sync,
        }
    }
}

/// The recent past of the carrier, read at fractional delays.
#[derive(Debug, Clone)]
struct History {
    buffer: Vec<f32>,
    /// Index of the latest sample.
    latest: usize,
}

impl History {
    fn new(len: usize) -> Self {
        Self {
            buffer: vec![0.0; len],
            latest: 0,
        }
    }

    fn push(&mut self, x: f32) {
        self.latest = (self.latest + 1) % self.buffer.len();
        self.buffer[self.latest] = x;
    }

    /// The sample pushed `n` pushes ago (0 is the latest).
    fn age(&self, n: usize) -> f32 {
        let len = self.buffer.len();
        self.buffer[(self.latest + len - n % len) % len]
    }

    /// Linear read, `0 <= d <= len - 2`.
    fn linear(&self, d: f32) -> f32 {
        let d = d.clamp(0.0, (self.buffer.len() - 2) as f32);
        let whole = d as usize;
        lerp(self.age(whole), self.age(whole + 1), d - whole as f32)
    }

    /// Four-point Hermite read, `1 <= d <= len - 3`.
    fn cubic(&self, d: f32) -> f32 {
        let d = d.clamp(1.0, (self.buffer.len() - 3) as f32);
        let whole = d as usize;
        let t = d - whole as f32;
        let (y0, y1, y2, y3) = (
            self.age(whole - 1),
            self.age(whole),
            self.age(whole + 1),
            self.age(whole + 2),
        );
        let c1 = 0.5 * (y2 - y0);
        let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
        ((c3 * t + c2) * t + c1) * t + y1
    }
}

/// Follows a signal's pitch by timing rising zero crossings, to a fraction
/// of a sample.
#[derive(Debug, Clone)]
struct Pitch {
    prev: f32,
    armed: bool,
    /// Samples since the last rising crossing.
    since: f32,
    /// Smoothed period in samples; 0 until one has been heard.
    period: f32,
    strays: u8,
}

impl Pitch {
    fn new() -> Self {
        Self {
            prev: 0.0,
            armed: false,
            since: 0.0,
            period: 0.0,
            strays: 0,
        }
    }

    fn update(&mut self, x: f32, sample_rate: f32) {
        self.since += 1.0;
        if x < -HYSTERESIS {
            self.armed = true;
        }
        if self.armed && x >= 0.0 && self.prev < 0.0 {
            let fraction = -self.prev / (x - self.prev);
            let period = self.since - 1.0 + fraction;
            self.since = 1.0 - fraction;
            self.armed = false;
            let hz = sample_rate / period;
            if (MIN_TRACK_HZ..0.2 * sample_rate).contains(&hz) {
                let near = self.period > 0.0 && (period / self.period - 1.0).abs() < 0.3;
                if near {
                    self.period += 0.2 * (period - self.period);
                    self.strays = 0;
                } else {
                    self.strays += 1;
                    if self.period == 0.0 || self.strays >= 3 {
                        self.period = period;
                        self.strays = 0;
                    }
                }
            }
        }
        self.prev = x;
    }

    /// Hz, or `None` before the first full cycle.
    fn hz(&self, sample_rate: f32) -> Option<f32> {
        (self.period > 0.0).then(|| sample_rate / self.period)
    }
}

/// Rounds off anything above [`KNEE`] towards full scale, so the output stays
/// inside it at any Level.
fn limit(x: f32) -> f32 {
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        x.signum() * (KNEE + (1.0 - KNEE) * ((a - KNEE) / (1.0 - KNEE)).tanh())
    }
}

/// FM reads the carrier this many samples back at rest, so the delay can
/// swing both ways without going negative.
fn fm_centre(sample_rate: f32) -> f32 {
    (MAX_DEPTH_SECONDS * sample_rate).ceil() + 2.0
}

#[derive(Debug, Clone)]
pub struct State {
    history: History,
    pitch: Pitch,
    osc: Phase,
    amount: OnePole,
    mod_prev: f32,
    mod_armed: bool,
    /// How far back sync reads, in samples: the carrier's age at the last
    /// modulator cycle start.
    reset: f32,
}

impl State {
    pub fn new(sample_rate: f32) -> Self {
        let longest = MAX_SYNC_SECONDS.max(2.0 * MAX_DEPTH_SECONDS) * sample_rate;
        Self {
            history: History::new(longest.ceil() as usize + 8),
            pitch: Pitch::new(),
            osc: Phase::default(),
            amount: OnePole::default(),
            mod_prev: 0.0,
            mod_armed: false,
            reset: 0.0,
        }
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[mode, amount, coarse, fine, mix, level] = io.params else {
            return;
        };
        let sr = io.sample_rate;
        let carrier = io.inputs[0] / AUDIO_VOLTS;
        self.history.push(carrier);
        self.pitch.update(carrier, sr);

        let modulator = if io.connected[1] {
            io.inputs[1] / AUDIO_VOLTS
        } else {
            let ratio = RATIOS[(coarse * (RATIOS.len() - 1) as f32).round() as usize];
            let nudge = ((fine - 0.5) * 2.0 / 12.0).exp2();
            let hz = self.pitch.hz(sr).unwrap_or(261.625_58) * ratio * nudge;
            (TAU * self.osc.advance(hz.min(0.45 * sr) / sr)).sin()
        };
        let cycle_start = self.mod_armed && modulator >= 0.0 && self.mod_prev < 0.0;
        if modulator < -HYSTERESIS {
            self.mod_armed = true;
        } else if cycle_start {
            self.mod_armed = false;
        }
        self.mod_prev = modulator;

        let amount = self.amount.process(
            (amount + io.inputs[2] / CV_MAX).clamp(0.0, 1.0),
            smoothing(AMOUNT_SECONDS, sr),
        );
        let (dry, wet) = match Mode::from_param(mode) {
            Mode::Fm => {
                let centre = fm_centre(sr);
                let depth = amount * amount * MAX_DEPTH_SECONDS * sr;
                (
                    self.history.age(centre as usize),
                    self.history.cubic(centre + depth * modulator),
                )
            }
            Mode::Am => (carrier, carrier * (1.0 - amount * 0.5 * (1.0 - modulator))),
            Mode::Ring => (carrier, carrier * lerp(1.0, modulator, amount)),
            Mode::Sync => {
                if cycle_start {
                    self.reset = self.pitch.since.min(MAX_SYNC_SECONDS * sr);
                }
                (carrier, self.history.linear(amount * self.reset))
            }
        };
        let wet = limit(wet * level * 2.0);
        io.outputs[0] = lerp(dry, wet, mix) * AUDIO_VOLTS;
    }
}

#[cfg(test)]
mod tests {
    use super::{State, fm_centre};
    use crate::rig::{SR, peak, render, sine, tone};

    const FM: f32 = 0.0;
    const AM: f32 = 1.0 / 3.0;
    const RING: f32 = 2.0 / 3.0;
    const SYNC: f32 = 1.0;
    /// Coarse knob positions for ratios 1.5 and 1.0.
    const RATIO_1_5: f32 = 0.4;
    const RATIO_1: f32 = 0.3;

    /// Mode, Amount, Coarse, Fine, Mix, Level.
    type Params = [f32; 6];

    fn run(
        params: Params,
        connected: &[bool; 3],
        frames: usize,
        input: impl FnMut(usize, &mut [f32]),
    ) -> Vec<f32> {
        render(&mut State::new(SR), &params, connected, 1, frames, input).remove(0)
    }

    const BOTH: [bool; 3] = [true, true, false];
    const CARRIER_ONLY: [bool; 3] = [true, false, false];

    /// Window of one tenth of a second after the first half second: whole
    /// cycles of every test frequency here.
    fn window(x: &[f32]) -> &[f32] {
        &x[24_000..28_800]
    }

    fn two_sines(
        f_carrier: f32,
        v_carrier: f32,
        f_mod: f32,
        v_mod: f32,
    ) -> impl FnMut(usize, &mut [f32]) {
        move |i, inp| {
            inp[0] = sine(i, f_carrier, v_carrier);
            inp[1] = sine(i, f_mod, v_mod);
        }
    }

    #[test]
    fn ring_mod_keeps_only_the_sum_and_difference_tones() {
        let out = run(
            [RING, 1.0, 0.0, 0.5, 1.0, 0.5],
            &BOTH,
            30_000,
            two_sines(1_000.0, 4.0, 300.0, 3.0),
        );
        let w = window(&out);
        // (4/5)(3/5) of 5 V, split between two tones.
        for hz in [700.0, 1_300.0] {
            let found = tone(w, hz);
            assert!((found - 1.2).abs() < 0.05, "{hz} Hz at {found} V");
        }
        for hz in [300.0, 1_000.0] {
            assert!(tone(w, hz) < 0.05, "{hz} Hz leaks through");
        }
    }

    #[test]
    fn am_leaves_the_carrier_with_a_sideband_each_side() {
        let out = run(
            [AM, 1.0, 0.0, 0.5, 1.0, 0.5],
            &BOTH,
            30_000,
            two_sines(1_000.0, 3.0, 100.0, 4.0),
        );
        let w = window(&out);
        // c (1 + m) / 2: carrier at half, each sideband at a quarter of c m.
        assert!(
            (tone(w, 1_000.0) - 1.5).abs() < 0.05,
            "carrier {}",
            tone(w, 1_000.0)
        );
        for hz in [900.0, 1_100.0] {
            assert!(
                (tone(w, hz) - 0.6).abs() < 0.05,
                "{hz} Hz at {}",
                tone(w, hz)
            );
        }
        assert!(tone(w, 800.0) < 0.05);
        // No amount, no change.
        let flat = run(
            [AM, 0.0, 0.0, 0.5, 1.0, 0.5],
            &BOTH,
            30_000,
            two_sines(1_000.0, 3.0, 100.0, 4.0),
        );
        assert!(tone(window(&flat), 1_100.0) < 0.01);
    }

    #[test]
    fn fm_adds_sidebands_and_stays_bounded() {
        let sidebands = |amount: f32| {
            let out = run(
                [FM, amount, 0.0, 0.5, 1.0, 0.5],
                &BOTH,
                30_000,
                two_sines(1_000.0, 3.0, 100.0, 5.0),
            );
            let w = window(&out);
            (tone(w, 1_000.0), tone(w, 1_100.0), tone(w, 900.0))
        };
        let (carrier, upper, lower) = sidebands(0.2);
        // Index 0.75: J0 = 0.86 and J1 = 0.35 of 3 V.
        assert!((carrier - 2.6).abs() < 0.2, "carrier {carrier}");
        assert!(
            (upper - 1.05).abs() < 0.15 && (lower - 1.05).abs() < 0.15,
            "{lower} {upper}"
        );
        let (_, upper, _) = sidebands(0.0);
        assert!(upper < 0.01, "sideband with no amount");

        // Full scale carrier, noisy full scale modulator, everything up.
        let noise = |i: usize, inp: &mut [f32]| {
            let x = (i as u32).wrapping_mul(2_654_435_761) as f32 / u32::MAX as f32;
            inp[0] = sine(i, 440.0, 5.0);
            inp[1] = (x * 2.0 - 1.0) * 5.0;
            inp[2] = 10.0;
        };
        let out = run(
            [FM, 1.0, 0.0, 0.5, 1.0, 1.0],
            &[true, true, true],
            20_000,
            noise,
        );
        assert!(peak(&out) <= 5.0, "peak {}", peak(&out));
    }

    #[test]
    fn mix_zero_passes_the_carrier() {
        for mode in [AM, RING, SYNC] {
            let out = run(
                [mode, 1.0, 0.0, 0.5, 0.0, 0.5],
                &BOTH,
                10_000,
                two_sines(440.0, 3.0, 110.0, 4.0),
            );
            for i in [0, 1, 500, 9_999] {
                assert!((out[i] - sine(i, 440.0, 3.0)).abs() < 1e-5, "mode {mode}");
            }
        }
        // FM reads the carrier from a fixed delay so it can swing both ways.
        let out = run(
            [FM, 1.0, 0.0, 0.5, 0.0, 0.5],
            &BOTH,
            10_000,
            two_sines(440.0, 3.0, 110.0, 4.0),
        );
        let lag = fm_centre(SR) as usize;
        for i in [lag, lag + 1, 5_000, 9_999] {
            assert!((out[i] - sine(i - lag, 440.0, 3.0)).abs() < 1e-5);
        }
    }

    #[test]
    fn the_built_in_modulator_keeps_its_ratio_to_the_carrier() {
        for carrier_hz in [440.0, 220.0] {
            let out = run(
                [RING, 1.0, RATIO_1_5, 0.5, 1.0, 0.5],
                &CARRIER_ONLY,
                30_000,
                |i, inp| {
                    inp[0] = sine(i, carrier_hz, 3.0);
                },
            );
            let w = window(&out);
            // Modulator at 1.5 times the carrier: tones at 0.5 and 2.5 times.
            for hz in [0.5 * carrier_hz, 2.5 * carrier_hz] {
                let found = tone(w, hz);
                assert!((found - 1.5).abs() < 0.2, "{hz} Hz at {found} V");
            }
            assert!(tone(w, carrier_hz) < 0.1, "carrier leaks");
        }
        // Same ratio, nudged up a semitone by Fine: the tones move.
        let out = run(
            [RING, 1.0, RATIO_1, 1.0, 1.0, 0.5],
            &CARRIER_ONLY,
            30_000,
            |i, inp| {
                inp[0] = sine(i, 440.0, 3.0);
            },
        );
        let up = 440.0 * 2f32.powf(1.0 / 12.0);
        assert!(tone(window(&out), 440.0 + up) > 1.2, "fine did not retune");
    }

    #[test]
    fn sync_restarts_the_carrier_with_the_modulator() {
        let drive = two_sines(1_000.0, 4.0, 130.0, 3.0);
        let out = run([SYNC, 1.0, 0.0, 0.5, 1.0, 0.5], &BOTH, 30_000, drive);
        assert!(peak(&out) <= 4.0 + 1e-3);
        let w = window(&out);
        assert!(tone(w, 1_000.0) < 3.8, "carrier untouched");
        // Cycles restarted at 130 Hz make the sound a harmonic of 130 Hz, so
        // there is energy at the multiples either side of the carrier.
        let off: f32 = [780.0, 910.0, 1_040.0, 1_170.0]
            .iter()
            .map(|&hz| tone(w, hz))
            .sum();
        assert!(off > 0.4, "no sync sidebands ({off})");
        let none = run(
            [SYNC, 0.0, 0.0, 0.5, 1.0, 0.5],
            &BOTH,
            10_000,
            two_sines(1_000.0, 4.0, 130.0, 3.0),
        );
        for (i, v) in none.iter().enumerate().skip(100) {
            assert!((v - sine(i, 1_000.0, 4.0)).abs() < 1e-3);
        }
    }

    #[test]
    fn level_scales_the_modulated_sound_and_stays_inside_full_scale() {
        let at = |level: f32| {
            run(
                [RING, 1.0, 0.0, 0.5, 1.0, level],
                &BOTH,
                10_000,
                two_sines(300.0, 2.0, 500.0, 2.0),
            )
        };
        assert_eq!(peak(&at(0.0)), 0.0);
        let (half, full) = (peak(&at(0.5)), peak(&at(1.0)));
        assert!((full / half - 2.0).abs() < 0.05, "{half} then {full}");
        let loud = run(
            [RING, 1.0, 0.0, 0.5, 1.0, 1.0],
            &BOTH,
            10_000,
            two_sines(300.0, 5.0, 500.0, 5.0),
        );
        assert!(peak(&loud) <= 5.0);
    }

    #[test]
    fn every_mode_stays_finite_and_bounded_at_extreme_settings() {
        // `render` rejects non-finite samples.
        let noise = |i: usize, inp: &mut [f32]| {
            let x = (i as u32).wrapping_mul(2_654_435_761) as f32 / u32::MAX as f32;
            let y = (i as u32).wrapping_mul(40_503) as f32 / u32::MAX as f32;
            inp[0] = (x * 2.0 - 1.0) * 5.0;
            inp[1] = (y * 2.0 - 1.0) * 5.0;
            inp[2] = if i % 7_000 < 3_500 { 10.0 } else { 0.0 };
        };
        for mode in [FM, AM, RING, SYNC] {
            for (coarse, fine, patched) in [(0.0, 0.0, true), (1.0, 1.0, false), (0.5, 0.5, false)]
            {
                let params = [mode, 1.0, coarse, fine, 1.0, 1.0];
                let out = run(params, &[true, patched, true], 30_000, noise);
                assert!(peak(&out) <= 5.0, "mode {mode} peak {}", peak(&out));
            }
        }
        // Silence in, silence out: nothing to track, nothing divides by zero.
        let quiet = run(
            [RING, 1.0, 1.0, 1.0, 1.0, 1.0],
            &CARRIER_ONLY,
            10_000,
            |_, _| {},
        );
        assert_eq!(peak(&quiet), 0.0);
    }
}
