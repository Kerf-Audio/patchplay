use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{
    AUDIO_VOLTS, CV_MAX, Edge, Gate, OnePole, Phase, Rng, flush, lerp, lowpass_coeff, smoothing,
};

use crate::delay_line::DelayLine;
use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Bandsaw",
    band: Band::Space,
    hp: 8,
    summary: "Tape delay",
    params: &[
        param(
            "time",
            "Time",
            0.375,
            "Delay time, synced when Clock is patched.",
        ),
        param(
            "feedback",
            "Feedback",
            0.45,
            "How many echoes. Near the top it builds up on itself.",
        ),
        param(
            "wear",
            "Wear",
            0.3,
            "Tape wobble and darkening on each echo.",
        ),
        param("mix", "Mix", 0.3, "Dry to wet."),
    ],
    inputs: &[
        jack("in", "In", Audio, "The sound to echo."),
        jack("clock", "Clock", Clock, "Syncs the delay time to tempo."),
        jack(
            "time",
            "Time",
            Cv,
            "Moves the delay time; fast moves bend pitch.",
        ),
    ],
    outputs: &[
        jack("l", "L", Audio, "Left."),
        jack("r", "R", Audio, "Right."),
    ],
};

const MAX_SECONDS: f32 = 2.0;
const MIN_SECONDS: f32 = 0.02;
/// Clock pulses per echo for each division of the Time knob.
const DIVISIONS: [f32; 8] = [1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0];
/// The right head sits at this fraction of the left head's delay.
const RIGHT_HEAD: f32 = 0.75;
/// Loop gain at full Feedback: above 1, so the tape saturation, not the
/// knob, sets the level of a runaway echo.
const MAX_LOOP_GAIN: f32 = 1.1;
const WOW_HZ: f32 = 0.6;
const FLUTTER_HZ: f32 = 7.3;
/// Wow and flutter depth at full Wear, in seconds.
const WOW_DEPTH: f32 = 0.0025;
const FLUTTER_DEPTH: f32 = 0.0002;
/// High-cut in the loop: bright at no Wear, dull at full Wear.
const BRIGHT_HZ: f32 = 12_000.0;
const DULL_HZ: f32 = 2_200.0;
/// Time glides this long between settings, bending the pitch like tape.
const GLIDE_SECONDS: f32 = 0.08;
/// A measured clock period must move by more than this share before the
/// delay follows it, so sample-count jitter never bends the pitch.
const PERIOD_TOLERANCE: f32 = 0.02;

#[derive(Debug, Clone)]
pub struct State {
    line: DelayLine,
    /// Smoothed delay in seconds; `None` until the first sample.
    delay: Option<f32>,
    clock: Gate,
    /// Samples since the last clock pulse, the gap before it, and the pulse
    /// period in use: the mean of the last two gaps, so a swung clock
    /// (long, short, long, ...) gives one steady period instead of
    /// retargeting the delay, and sweeping its pitch, on every pulse.
    since_pulse: u32,
    last_gap: Option<u32>,
    period: Option<u32>,
    wow: Phase,
    flutter: Phase,
    drift: OnePole,
    rng: Rng,
    loop_lp: OnePole,
}

impl State {
    pub fn new(seed: u64, sample_rate: f32) -> Self {
        let mut rng = Rng::new(seed);
        let mut wow = Phase::default();
        let mut flutter = Phase::default();
        wow.advance(rng.unit());
        flutter.advance(rng.unit());
        Self {
            line: DelayLine::new(((MAX_SECONDS + 0.05) * sample_rate) as usize),
            delay: None,
            clock: Gate::default(),
            since_pulse: 0,
            last_gap: None,
            period: None,
            wow,
            flutter,
            drift: OnePole::default(),
            rng,
            loop_lp: OnePole::default(),
        }
    }

    /// Target delay in seconds from the Time knob (plus CV) and the clock.
    fn target(&mut self, io: &Io<'_>, time: f32) -> f32 {
        let sr = io.sample_rate;
        if io.connected[1] {
            self.since_pulse = self.since_pulse.saturating_add(1);
            if self.clock.update(io.inputs[1]) == Edge::Rose {
                let gap = self.since_pulse;
                if gap > 1 && gap < (MAX_SECONDS * sr) as u32 {
                    let mean = self.last_gap.map_or(gap, |last| (last + gap) / 2);
                    let moved = self.period.is_none_or(|p| {
                        (mean as f32 - p as f32).abs() > PERIOD_TOLERANCE * p as f32
                    });
                    if moved {
                        self.period = Some(mean);
                    }
                    self.last_gap = Some(gap);
                }
                self.since_pulse = 0;
            }
            let pulse = self.period.map_or(60.0 / io.tempo / 4.0, |p| p as f32 / sr);
            let step = ((time * DIVISIONS.len() as f32) as usize).min(DIVISIONS.len() - 1);
            let mut seconds = pulse * DIVISIONS[step];
            while seconds > MAX_SECONDS {
                seconds *= 0.5;
            }
            seconds.max(MIN_SECONDS)
        } else {
            MIN_SECONDS * (MAX_SECONDS / MIN_SECONDS).powf(time)
        }
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[time, feedback, wear, mix] = io.params else {
            return;
        };
        let sr = io.sample_rate;
        let target = self.target(io, (time + io.inputs[2] / CV_MAX).clamp(0.0, 1.0));
        let delay = self.delay.unwrap_or(target);
        let delay = delay + smoothing(GLIDE_SECONDS, sr) * (target - delay);
        self.delay = Some(delay);

        let wow = (self.wow.advance(WOW_HZ / sr) * std::f32::consts::TAU).sin();
        let flutter = (self.flutter.advance(FLUTTER_HZ / sr) * std::f32::consts::TAU).sin();
        let jitter = self
            .drift
            .process(self.rng.unit() - 0.5, lowpass_coeff(40.0, sr));
        let wobble = wear * (WOW_DEPTH * wow + FLUTTER_DEPTH * (flutter + 6.0 * jitter));
        let left = self.line.tap((delay + wobble) * sr);
        let right = self.line.tap((delay * RIGHT_HEAD + wobble) * sr);

        let cutoff = BRIGHT_HZ * (DULL_HZ / BRIGHT_HZ).powf(wear);
        let darkened = self.loop_lp.process(left, lowpass_coeff(cutoff, sr));
        let dry = io.inputs[0];
        let tape_drive = 1.0 + 1.5 * wear;
        let recorded = (dry / AUDIO_VOLTS + feedback * MAX_LOOP_GAIN * darkened) * tape_drive;
        self.line.push(flush(recorded.tanh() / tape_drive));

        io.outputs[0] = lerp(dry, left * AUDIO_VOLTS, mix);
        io.outputs[1] = lerp(dry, right * AUDIO_VOLTS, mix);
    }
}

#[cfg(test)]
mod tests {
    use super::State;
    use crate::rig::{SR, peak, render, rms, sine};

    /// Time knob for a free delay of 0.2 s.
    const TIME_0_2: f32 = 0.5;

    fn delay(seed: u64) -> State {
        State::new(seed, SR)
    }

    fn impulse(params: [f32; 4], frames: usize) -> Vec<Vec<f32>> {
        render(
            &mut delay(1),
            &params,
            &[true, false, false],
            2,
            frames,
            |i, inp| {
                inp[0] = if i == 100 { 5.0 } else { 0.0 };
            },
        )
    }

    fn argmax(x: &[f32]) -> usize {
        (0..x.len())
            .max_by(|&a, &b| x[a].abs().total_cmp(&x[b].abs()))
            .unwrap()
    }

    #[test]
    fn echo_arrives_after_the_set_time() {
        let out = impulse([TIME_0_2, 0.0, 0.0, 1.0], 20_000);
        let echo = argmax(&out[0][1_000..]) + 1_000;
        let expected = 100 + (0.2 * SR) as usize;
        assert!(
            echo.abs_diff(expected) <= 2,
            "echo at {echo}, expected {expected}"
        );
        let right = argmax(&out[1][1_000..]) + 1_000;
        let expected = 100 + (0.15 * SR) as usize;
        assert!(right.abs_diff(expected) <= 2, "right echo at {right}");
    }

    #[test]
    fn feedback_repeats_get_quieter_and_darker() {
        let out = impulse([TIME_0_2, 0.5, 0.5, 1.0], 40_000);
        let first = peak(&out[0][9_000..10_500]);
        let second = peak(&out[0][18_500..20_500]);
        assert!(
            second > 0.05 * first && second < 0.8 * first,
            "{first} then {second}"
        );
    }

    #[test]
    fn a_clock_sets_the_delay_to_a_division_of_its_pulse() {
        // Pulses every 1000 samples; knob 0.375 is 4 pulses = 4000 samples.
        let params = [0.375, 0.0, 0.0, 1.0];
        let out = render(
            &mut delay(1),
            &params,
            &[true, true, false],
            2,
            60_000,
            |i, inp| {
                inp[0] = if i == 48_000 { 5.0 } else { 0.0 };
                inp[1] = if i % 1_000 < 50 { 10.0 } else { 0.0 };
            },
        );
        let echo = argmax(&out[0][48_500..]) + 48_500;
        assert!(echo.abs_diff(52_000) <= 2, "echo at {echo}");
    }

    #[test]
    fn a_swung_clock_holds_the_pitch_of_the_echo_steady() {
        // Swung pulses: gaps of 1300 then 700 samples, mean 1000. Before the
        // fix the delay retargeted on every pulse and the echo of a held tone
        // swept in pitch (a whoosh on noisy material).
        let swung = |i: usize| {
            let pos = i % 2_000;
            pos < 50 || (1_300..1_350).contains(&pos)
        };
        let out = render(
            &mut delay(1),
            &[0.375, 0.0, 0.0, 1.0],
            &[true, true, false],
            2,
            96_000,
            |i, inp| {
                inp[0] = sine(i, 1_000.0, 3.0);
                inp[1] = if swung(i) { 10.0 } else { 0.0 };
            },
        );
        let tones: Vec<f32> = out[0][48_000..]
            .chunks(2_400)
            .map(|c| crate::rig::tone(c, 1_000.0))
            .collect();
        let (lo, hi) = tones
            .iter()
            .fold((f32::MAX, 0.0f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        assert!(hi - lo < 0.05, "echo pitch varies {lo}..{hi}");
    }

    #[test]
    fn runaway_feedback_stays_bounded_and_normal_feedback_dies() {
        let burst = |i: usize, inp: &mut [f32]| {
            inp[0] = if i < 24_000 { sine(i, 330.0, 5.0) } else { 0.0 };
        };
        let frames = 20 * SR as usize;
        for wear in [0.0, 1.0] {
            let out = render(
                &mut delay(2),
                &[0.2, 1.0, wear, 1.0],
                &[true, false, false],
                2,
                frames,
                burst,
            );
            assert!(peak(&out[0]) <= 5.0 && peak(&out[1]) <= 5.0);
            assert!(
                rms(&out[0][frames - 4_800..]) > 0.1,
                "runaway loop should sustain"
            );
        }
        let out = render(
            &mut delay(2),
            &[0.2, 0.45, 0.3, 1.0],
            &[true, false, false],
            2,
            frames,
            burst,
        );
        assert!(peak(&out[0][frames - 4_800..]) < 1e-3);
    }

    #[test]
    fn wear_wobbles_the_pitch_of_a_held_tone() {
        let tone_in = |i: usize, inp: &mut [f32]| inp[0] = sine(i, 1_000.0, 3.0);
        let steady = render(
            &mut delay(3),
            &[TIME_0_2, 0.0, 0.0, 1.0],
            &[true, false, false],
            2,
            96_000,
            tone_in,
        );
        let worn = render(
            &mut delay(3),
            &[TIME_0_2, 0.0, 1.0, 1.0],
            &[true, false, false],
            2,
            96_000,
            tone_in,
        );
        let a = &steady[0][48_000..];
        let b = &worn[0][48_000..];
        let spread = |x: &[f32]| {
            x.chunks(4_800)
                .map(|c| crate::rig::tone(c, 1_000.0))
                .fold((f32::MAX, 0.0f32), |(lo, hi), v| (lo.min(v), hi.max(v)))
        };
        let (lo_a, hi_a) = spread(a);
        let (lo_b, hi_b) = spread(b);
        assert!(hi_a - lo_a < 0.05, "steady tone varies {lo_a}..{hi_a}");
        assert!(hi_b - lo_b > 0.3, "worn tone varies {lo_b}..{hi_b}");
    }
}
