use std::f32::consts::TAU;

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{
    AUDIO_VOLTS, CV_MAX, Gate, OnePole, Phase, Rng, flush, lerp, lowpass_coeff, smoothing,
};

use crate::delay_line::DelayLine;
use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Dado",
    band: Band::Space,
    hp: 12,
    summary: "Dub echo",
    params: &[
        param(
            "time",
            "Time",
            0.68,
            "Echo spacing. With Sync on it picks a note length instead: 1/16, 1/8 triplet, 1/8, dotted 1/8, 1/4, dotted 1/4, 1/2 or a bar.",
        ),
        param(
            "sync",
            "Sync",
            0.0,
            "Off: Time is free. On: Time picks a note length at the patch tempo.",
        ),
        param(
            "feedback",
            "Feedback",
            0.5,
            "How many echoes. Past about 85 percent the echoes grow into a steady howl that the loop's saturation keeps in check.",
        ),
        param(
            "tone",
            "Tone",
            0.5,
            "Low-pass in the loop. Each echo comes back duller than the last.",
        ),
        param(
            "thin",
            "Thin",
            0.2,
            "High-pass in the loop. Each echo loses a little more bass.",
        ),
        param(
            "wobble",
            "Wobble",
            0.25,
            "Slow wow and fast flutter on the delay time, for a loose, worn feel.",
        ),
        param(
            "drive",
            "Drive",
            0.2,
            "Saturation in the loop. Echoes grow thicker and more crushed as they repeat.",
        ),
        param("mix", "Mix", 0.35, "Dry to echoes."),
    ],
    inputs: &[
        jack("in_l", "In L", Audio, "Left in, or mono."),
        jack("in_r", "In R", Audio, "Right in."),
        jack(
            "throw",
            "Throw",
            Gate,
            "When patched, sound only goes into the echoes while this is high. Echoes already running keep going.",
        ),
        jack(
            "time",
            "Time",
            Cv,
            "Moves Time. Sweep it to make the echoes dive and climb in pitch.",
        ),
    ],
    outputs: &[
        jack("l", "L", Audio, "Left."),
        jack("r", "R", Audio, "Right."),
    ],
};

const MAX_SECONDS: f32 = 1.5;
const MIN_SECONDS: f32 = 0.02;
/// Note lengths in beats for each division of Time when Sync is on.
const DIVISIONS: [f32; 8] = [0.25, 1.0 / 3.0, 0.5, 0.75, 1.0, 1.5, 2.0, 4.0];
/// Loop gain at full Feedback: above 1, so the loop saturation, not the
/// knob, sets the level of a runaway echo.
const MAX_LOOP_GAIN: f32 = 1.15;
/// Time eases towards its target in about this long.
const GLIDE_SECONDS: f32 = 0.08;
/// The delay never moves faster than this, in seconds of delay per second,
/// so a sweep bends echoes down or up but never plays them backwards.
const MAX_SLEW: f32 = 0.8;
const WOW_HZ: [f32; 2] = [0.45, 0.83];
const FLUTTER_HZ: f32 = 6.9;
/// Wow and flutter depth at full Wobble, in seconds.
const WOW_DEPTH: f32 = 0.004;
const FLUTTER_DEPTH: f32 = 0.000_35;
/// Loop low-pass: dark at no Tone, bright at full Tone.
const DARK_HZ: f32 = 500.0;
const BRIGHT_HZ: f32 = 10_000.0;
/// Loop high-pass: out of the way at no Thin, thin at full Thin.
const LOW_CUT_MIN_HZ: f32 = 20.0;
const LOW_CUT_MAX_HZ: f32 = 800.0;
/// Pre-gain into the loop saturation at full Drive, minus one.
const MAX_DRIVE_GAIN: f32 = 3.0;
const THROW_OPEN_SECONDS: f32 = 0.002;
const THROW_CLOSE_SECONDS: f32 = 0.025;

/// Free delay in seconds for a Time position in `0..1`.
fn free_seconds(pos: f32) -> f32 {
    MIN_SECONDS * (MAX_SECONDS / MIN_SECONDS).powf(pos)
}

/// Synced delay in seconds, halved until it fits the line.
fn synced_seconds(pos: f32, tempo: f32) -> f32 {
    let step = ((pos * DIVISIONS.len() as f32) as usize).min(DIVISIONS.len() - 1);
    let mut seconds = DIVISIONS[step] * 60.0 / tempo.max(1.0);
    while seconds > MAX_SECONDS {
        seconds *= 0.5;
    }
    seconds.max(MIN_SECONDS)
}

/// What the loop does this sample, shared by both sides.
#[derive(Debug, Clone, Copy)]
struct Loop {
    sample_rate: f32,
    /// Delay time in seconds.
    delay: f32,
    feedback: f32,
    low_pass: f32,
    low_cut: f32,
    drive: f32,
}

#[derive(Debug, Clone)]
struct Side {
    line: DelayLine,
    low_pass: [OnePole; 2],
    low_cut: OnePole,
    drift: OnePole,
    /// This side's wobble is the other's shifted by this many cycles, which
    /// widens the echoes slightly.
    skew: f32,
}

impl Side {
    fn new(sample_rate: f32, skew: f32) -> Self {
        Self {
            line: DelayLine::new(((MAX_SECONDS + 2.0 * WOW_DEPTH) * sample_rate) as usize + 1),
            low_pass: [OnePole::default(); 2],
            low_cut: OnePole::default(),
            drift: OnePole::default(),
            skew,
        }
    }

    /// Runs the loop once. `sent` and the result are normalised (1 is 5 V).
    fn step(
        &mut self,
        sent: f32,
        wobble: f32,
        lp: &Loop,
        wobble_phases: [f32; 3],
        jitter: f32,
    ) -> f32 {
        let [wow_a, wow_b, flutter] = wobble_phases;
        let slow =
            0.7 * (TAU * (wow_a + self.skew)).sin() + 0.3 * (TAU * (wow_b + 1.7 * self.skew)).sin();
        let fast = (TAU * (flutter + 3.1 * self.skew)).sin()
            + 6.0
                * self
                    .drift
                    .process(jitter, lowpass_coeff(30.0, lp.sample_rate));
        let moved = wobble * (WOW_DEPTH * slow + FLUTTER_DEPTH * fast);
        let echo = self.line.tap((lp.delay + moved) * lp.sample_rate);

        let first = self.low_pass[0].process(echo, lp.low_pass);
        let dark = self.low_pass[1].process(first, lp.low_pass);
        let thin = dark - self.low_cut.process(dark, lp.low_cut);
        let recorded = ((sent + thin * lp.feedback) * lp.drive).tanh() / lp.drive;
        self.line.push(flush(recorded));
        // The loop saturates at 1/drive; this puts the level back.
        echo * lp.drive.sqrt()
    }
}

#[derive(Debug, Clone)]
pub struct State {
    sides: [Side; 2],
    /// Smoothed delay in seconds; `None` until the first sample.
    delay: Option<f32>,
    throw: Gate,
    send: OnePole,
    /// False until the first sample, which sets the send without easing.
    started: bool,
    wow: [Phase; 2],
    flutter: Phase,
    rng: Rng,
}

impl State {
    pub fn new(seed: u64, sample_rate: f32) -> Self {
        let mut rng = Rng::new(seed);
        let mut wow = [Phase::default(); 2];
        let mut flutter = Phase::default();
        for p in &mut wow {
            p.advance(rng.unit());
        }
        flutter.advance(rng.unit());
        Self {
            sides: [Side::new(sample_rate, 0.0), Side::new(sample_rate, 0.31)],
            delay: None,
            throw: Gate::default(),
            send: OnePole::default(),
            started: false,
            wow,
            flutter,
            rng,
        }
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[time, sync, feedback, tone, thin, wobble, drive, mix] = io.params else {
            return;
        };
        let sr = io.sample_rate;

        let pos = (time + io.inputs[3] / CV_MAX).clamp(0.0, 1.0);
        let target = if sync >= 0.5 {
            synced_seconds(pos, io.tempo)
        } else {
            free_seconds(pos)
        };
        let delay = self.delay.unwrap_or(target);
        let step =
            (smoothing(GLIDE_SECONDS, sr) * (target - delay)).clamp(-MAX_SLEW / sr, MAX_SLEW / sr);
        let delay = delay + step;
        self.delay = Some(delay);

        self.throw.update(io.inputs[2]);
        let open = if io.connected[2] && !self.throw.is_high() {
            0.0
        } else {
            1.0
        };
        let seconds = if open > self.send.value() {
            THROW_OPEN_SECONDS
        } else {
            THROW_CLOSE_SECONDS
        };
        // The first sample sets the send outright instead of easing in from shut.
        let coeff = if self.started {
            smoothing(seconds, sr)
        } else {
            1.0
        };
        self.started = true;
        let send = self.send.process(open, coeff);

        let phases = [
            self.wow[0].advance(WOW_HZ[0] / sr),
            self.wow[1].advance(WOW_HZ[1] / sr),
            self.flutter.advance(FLUTTER_HZ / sr),
        ];
        let lp = Loop {
            sample_rate: sr,
            delay,
            feedback: feedback * MAX_LOOP_GAIN,
            low_pass: lowpass_coeff(DARK_HZ * (BRIGHT_HZ / DARK_HZ).powf(tone), sr),
            low_cut: lowpass_coeff(
                LOW_CUT_MIN_HZ * (LOW_CUT_MAX_HZ / LOW_CUT_MIN_HZ).powf(thin),
                sr,
            ),
            drive: 1.0 + MAX_DRIVE_GAIN * drive,
        };

        let dry_l = io.inputs[0];
        let dry_r = if io.connected[1] { io.inputs[1] } else { dry_l };
        for (side, (dry, out)) in self
            .sides
            .iter_mut()
            .zip([dry_l, dry_r].into_iter().zip(io.outputs.iter_mut()))
        {
            let jitter = self.rng.unit() - 0.5;
            let wet = side.step(dry / AUDIO_VOLTS * send, wobble, &lp, phases, jitter);
            *out = lerp(dry, wet * AUDIO_VOLTS, mix);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{State, free_seconds};
    use crate::rig::{SR, peak, render, rms, sine, tone};

    /// Time, Sync, Feedback, Tone, Thin, Wobble, Drive, Mix.
    type Params = [f32; 8];

    /// Bright, clean, wet only: `[time, sync, feedback]` set by the test.
    fn clean(time: f32, sync: f32, feedback: f32) -> Params {
        [time, sync, feedback, 1.0, 0.0, 0.0, 0.0, 1.0]
    }

    const MONO: [bool; 4] = [true, false, false, false];

    fn run(
        params: Params,
        connected: &[bool; 4],
        frames: usize,
        input: impl FnMut(usize, &mut [f32]),
    ) -> Vec<Vec<f32>> {
        render(&mut State::new(1, SR), &params, connected, 2, frames, input)
    }

    fn click(i: usize, inp: &mut [f32]) {
        inp[0] = if i == 100 { 1.0 } else { 0.0 };
    }

    /// Index of the biggest sample in `x[from..to]`.
    fn loudest(x: &[f32], from: usize, to: usize) -> usize {
        (from..to)
            .max_by(|&a, &b| x[a].abs().total_cmp(&x[b].abs()))
            .unwrap()
    }

    #[test]
    fn echoes_land_one_synced_beat_apart() {
        // Knob 0.55 is a quarter note: 0.5 s at the rig's 120 BPM.
        let out = run(clean(0.55, 1.0, 0.5), &MONO, 100_000, click);
        for (k, tol) in [(1usize, 3usize), (2, 6), (3, 9)] {
            let expected = 100 + 24_000 * k;
            let found = loudest(&out[0], expected - 2_000, expected + 2_000);
            assert!(
                found.abs_diff(expected) <= tol,
                "echo {k} at {found}, expected {expected}"
            );
        }
    }

    #[test]
    fn free_time_follows_the_knob() {
        let out = run(clean(0.5, 0.0, 0.0), &MONO, 20_000, click);
        let expected = 100 + (free_seconds(0.5) * SR).round() as usize;
        let found = loudest(&out[0], 1_000, 20_000);
        assert!(
            found.abs_diff(expected) <= 2,
            "echo at {found}, expected {expected}"
        );
        assert!(free_seconds(0.0) < 0.03 && free_seconds(1.0) > 1.4);
    }

    #[test]
    fn feedback_repeats_fade_and_none_means_one_echo() {
        let out = run(clean(0.55, 1.0, 0.5), &MONO, 110_000, click);
        let peaks: Vec<f32> = (1..=4)
            .map(|k| {
                let at = 100 + 24_000 * k;
                peak(&out[0][at - 100..at + 100])
            })
            .collect();
        assert!(peaks[0] > 0.1, "first echo {}", peaks[0]);
        for pair in peaks.windows(2) {
            assert!(
                pair[1] < 0.7 * pair[0] && pair[1] > 0.05 * pair[0],
                "{peaks:?}"
            );
        }
        let single = run(clean(0.55, 1.0, 0.0), &MONO, 110_000, click);
        assert!(
            peak(&single[0][24_000 + 2_000..]) < 1e-4,
            "feedback 0 still repeats"
        );
    }

    #[test]
    fn max_feedback_stays_bounded_and_normal_feedback_dies() {
        let burst = |i: usize, inp: &mut [f32]| {
            inp[0] = if i < 24_000 { sine(i, 330.0, 5.0) } else { 0.0 };
        };
        let frames = 20 * SR as usize;
        for (tone, drive) in [(1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.5, 0.0)] {
            let params = [0.4, 0.0, 1.0, tone, 0.0, 0.5, drive, 1.0];
            let out = run(params, &MONO, frames, burst);
            assert!(
                peak(&out[0]) <= 5.0 && peak(&out[1]) <= 5.0,
                "tone {tone} drive {drive}"
            );
        }
        let howl = run(
            [0.4, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            &MONO,
            frames,
            burst,
        );
        assert!(
            rms(&howl[0][frames - 4_800..]) > 0.1,
            "full feedback should sustain"
        );
        let dying = run(
            [0.4, 0.0, 0.5, 0.5, 0.2, 0.2, 0.2, 1.0],
            &MONO,
            frames,
            burst,
        );
        assert!(peak(&dying[0][frames - 4_800..]) < 1e-3);
    }

    #[test]
    fn extreme_settings_never_produce_nan_or_runaway() {
        // `render` rejects non-finite samples.
        let noise = |i: usize, inp: &mut [f32]| {
            let x = (i as u32).wrapping_mul(2_654_435_761) as f32 / u32::MAX as f32;
            inp[0] = (x * 2.0 - 1.0) * 5.0;
            inp[1] = -inp[0];
            inp[3] = if i % 20_000 < 10_000 { 10.0 } else { 0.0 };
        };
        for corner in 0..64u32 {
            let bit = |n: u32| ((corner >> n) & 1) as f32;
            let params = [bit(0), bit(1), 1.0, bit(2), bit(3), bit(4), bit(5), 1.0];
            let out = run(params, &[true, true, false, true], 40_000, noise);
            assert!(peak(&out[0]) <= 5.0 && peak(&out[1]) <= 5.0, "{params:?}");
        }
    }

    #[test]
    fn mix_zero_is_the_dry_signal() {
        let input = |i: usize, inp: &mut [f32]| {
            inp[0] = sine(i, 440.0, 3.0);
            inp[1] = sine(i, 330.0, 2.0);
        };
        let params = [0.6, 0.0, 0.9, 0.5, 0.2, 1.0, 0.5, 0.0];
        let out = run(params, &[true, true, false, false], 20_000, input);
        for i in [0, 1, 777, 10_000, 19_999] {
            assert!((out[0][i] - sine(i, 440.0, 3.0)).abs() < 1e-5);
            assert!((out[1][i] - sine(i, 330.0, 2.0)).abs() < 1e-5);
        }
    }

    #[test]
    fn a_mono_input_comes_out_on_both_sides() {
        let out = run(clean(0.55, 1.0, 0.5), &MONO, 30_000, click);
        let (l, r) = (&out[0], &out[1]);
        assert!(peak(&l[24_000..]) > 0.1 && peak(&r[24_000..]) > 0.1);
    }

    #[test]
    fn throw_opens_the_send_only_while_high() {
        let patched = [true, false, true, false];
        let shut = run(clean(0.55, 1.0, 0.5), &patched, 60_000, click);
        assert!(
            peak(&shut[0][1_000..]) < 1e-6,
            "echo got through a closed send"
        );
        let thrown = run(clean(0.55, 1.0, 0.5), &patched, 60_000, |i, inp| {
            click(i, inp);
            inp[2] = if i < 1_000 { 10.0 } else { 0.0 };
        });
        let at = 100 + 24_000;
        assert!(
            peak(&thrown[0][at - 50..at + 50]) > 0.1,
            "thrown sound never echoed"
        );
        // The second echo is still there once the gate has closed.
        assert!(peak(&thrown[0][at + 24_000 - 100..at + 24_000 + 100]) > 0.01);
    }

    #[test]
    fn sweeping_time_bends_pitch_without_clicks() {
        let out = run(
            [0.1, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            &[true, false, false, true],
            120_000,
            |i, inp| {
                inp[0] = sine(i, 200.0, 3.0);
                inp[3] = if i >= 48_000 { 10.0 } else { 0.0 };
            },
        );
        let worst = out[0]
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        assert!(worst < 0.2, "largest step between samples {worst} V");
        // While the delay grows the echo plays slower: a lower pitch.
        let before = tone(&out[0][40_000..44_800], 200.0);
        let during = tone(&out[0][50_000..54_800], 200.0);
        assert!(
            before > 1.0 && during < 0.5 * before,
            "{before} then {during}"
        );
    }

    #[test]
    fn wobble_moves_the_pitch_of_a_held_tone() {
        let held = |i: usize, inp: &mut [f32]| inp[0] = sine(i, 1_000.0, 3.0);
        let spread = |wobble: f32| {
            let params = [0.5, 0.0, 0.0, 1.0, 0.0, wobble, 0.0, 1.0];
            let out = run(params, &MONO, 96_000, held);
            out[0][48_000..]
                .chunks(4_800)
                .map(|c| tone(c, 1_000.0))
                .fold((f32::MAX, 0.0f32), |(lo, hi), v| (lo.min(v), hi.max(v)))
        };
        let (lo, hi) = spread(0.0);
        assert!(hi - lo < 0.05, "steady tone varies {lo}..{hi}");
        let (lo, hi) = spread(1.0);
        assert!(hi - lo > 0.3, "wobbly tone varies {lo}..{hi}");
    }

    #[test]
    fn the_same_seed_gives_the_same_wobble() {
        let params = [0.5, 0.0, 0.6, 0.5, 0.2, 1.0, 0.3, 0.5];
        let held = |i: usize, inp: &mut [f32]| inp[0] = sine(i, 440.0, 3.0);
        let a = run(params, &MONO, 20_000, held);
        let b = run(params, &MONO, 20_000, held);
        assert_eq!(a, b);
        let c = render(&mut State::new(2, SR), &params, &MONO, 2, 20_000, held);
        assert_ne!(a, c);
    }

    #[test]
    fn the_loop_settles_to_exact_silence() {
        let mut dado = State::new(1, SR);
        render(
            &mut dado,
            &[0.2, 0.0, 0.3, 0.5, 0.2, 0.0, 0.2, 1.0],
            &MONO,
            2,
            (10.0 * SR) as usize,
            click,
        );
        let out = render(
            &mut dado,
            &[0.2, 0.0, 0.3, 0.5, 0.2, 0.0, 0.2, 1.0],
            &MONO,
            2,
            100,
            |_, _| {},
        );
        assert_eq!(peak(&out[0]), 0.0);
    }
}
