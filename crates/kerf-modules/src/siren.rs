use std::f32::consts::TAU;

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{
    AUDIO_VOLTS, CV_MAX, Edge, GATE_HIGH, Gate, OnePole, Phase, Rng, Svf, lerp, poly_blep,
    smoothing,
};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Siren",
    band: Band::Voices,
    hp: 12,
    summary: "Dub siren and bleep machine",
    params: &[
        param(
            "pitch",
            "Pitch",
            0.5,
            "Base pitch, from about 46 Hz to 1.5 kHz. The LFO and the V/Oct input are added on top.",
        ),
        param(
            "range",
            "Range",
            0.4,
            "How far the LFO pushes the pitch up, from nothing to 4 octaves. A down-saw starts this far above Pitch and dives back to it.",
        ),
        param(
            "rate",
            "Rate",
            0.45,
            "LFO speed, from one cycle every 10 seconds to 30 times a second. Ignored while Sync is on.",
        ),
        param(
            "sync",
            "Sync",
            0.0,
            "Off at the bottom. Turn up to lock the LFO to the patch tempo: one cycle takes 2 bars, 1 bar, 2 beats, 1 beat, a half beat, a quarter beat or an eighth of a beat.",
        ),
        param(
            "shape",
            "Shape",
            0.0,
            "The LFO wave: down-saw for the classic pitch dive, up-saw, triangle, square for bleeps, or sample-and-hold for random steps.",
        ),
        param(
            "wave",
            "Wave",
            0.0,
            "The oscillator wave: square, triangle, saw or sine.",
        ),
        param(
            "release",
            "Release",
            0.5,
            "How long the siren keeps sounding after the gate drops, from 10 ms to 15 seconds. The LFO keeps running and the pitch keeps falling as it fades.",
        ),
        param(
            "cutoff",
            "Cutoff",
            0.85,
            "Low-pass filter cutoff, from 40 Hz to 18 kHz.",
        ),
        param(
            "resonance",
            "Resonance",
            0.2,
            "Emphasis at the cutoff. High settings whistle but stay under control.",
        ),
        param(
            "drive",
            "Drive",
            0.2,
            "Pushes the oscillator into soft clipping for a grittier, fatter tone.",
        ),
    ],
    inputs: &[
        jack(
            "gate",
            "Gate",
            Gate,
            "Hold it high to sound the siren. When it drops the sound falls away over Release. Every rise restarts the LFO, so each bleep repeats exactly. Leave it unpatched and the siren sounds all the time.",
        ),
        jack("voct", "V/Oct", Pitch, "Pitch input. 1 V up = 1 octave up."),
        jack(
            "rate",
            "Rate",
            Cv,
            "Speeds the LFO up or slows it down. While Sync is on it picks a shorter or longer note length instead.",
        ),
    ],
    outputs: &[
        jack("out", "Out", Audio, "The siren."),
        jack("lfo", "LFO", Cv, "The pitch LFO, 0 to 10 V."),
    ],
};

/// Free-running LFO cycles per second at the bottom and top of the Rate knob.
const SLOWEST_HZ: f32 = 0.1;
const FASTEST_HZ: f32 = 30.0;
/// Beats in one LFO cycle for each Sync position above Off, slowest first.
const SYNC_BEATS: [f32; 7] = [8.0, 4.0, 2.0, 1.0, 0.5, 0.25, 0.125];
/// Octaves the LFO adds at full Range.
const MAX_RANGE_OCTAVES: f32 = 4.0;
/// Octaves the pitch falls over a whole release.
const RELEASE_DIVE_OCTAVES: f32 = 1.0;
/// The gate rises into the sound over this long, so it never clicks.
const ATTACK_SECONDS: f32 = 0.002;
/// Release time at the bottom and top of the knob.
const SHORTEST_RELEASE: f32 = 0.01;
const LONGEST_RELEASE: f32 = 15.0;
/// Time the Wave and Cutoff knobs take to settle, so turning them never clicks.
const KNOB_SECONDS: f32 = 0.005;
/// Filter cutoff at the bottom and top of the knob.
const LOWEST_CUTOFF: f32 = 40.0;
const HIGHEST_CUTOFF: f32 = 18_000.0;
/// Peak level of the oscillator, leaving headroom for the filter to ring.
const LEVEL: f32 = 0.7;
/// Pitch limits in volts from C4, so the oscillator stays below Nyquist.
const LOWEST_VOLTS: f32 = -6.0;
const HIGHEST_VOLTS: f32 = 5.5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Stage {
    #[default]
    Idle,
    Held,
    Release,
}

pub struct State {
    rng: Rng,
    gate: Gate,
    stage: Stage,
    /// Loudness, `0..1`.
    level: f32,
    /// Where the release began, and how far through it we are, `0..1`.
    release_from: f32,
    release_at: f32,
    lfo: Phase,
    /// The sample-and-hold value for this LFO cycle, `0..1`.
    held: f32,
    osc: Phase,
    wave: OnePole,
    cutoff: OnePole,
    knob_coeff: f32,
    /// False until the first sample, which takes Wave and Cutoff as they are.
    settled: bool,
    filter: Svf,
}

impl State {
    pub fn new(seed: u64, sample_rate: f32) -> Self {
        let mut rng = Rng::new(seed);
        let held = rng.unit();
        Self {
            rng,
            gate: Gate::default(),
            stage: Stage::Idle,
            level: 0.0,
            release_from: 0.0,
            release_at: 0.0,
            lfo: Phase::default(),
            held,
            osc: Phase::default(),
            wave: OnePole::default(),
            cutoff: OnePole::default(),
            knob_coeff: smoothing(KNOB_SECONDS, sample_rate),
            settled: false,
            filter: Svf::default(),
        }
    }
}

/// Down-saw, up-saw, triangle, square or the held random value at `phase`,
/// as `0..1`. Every wave but the random one starts at its own low or high
/// point so a restart is audible.
fn lfo_wave(index: usize, phase: f32, held: f32) -> f32 {
    match index {
        0 => 1.0 - phase,
        1 => phase,
        2 => 1.0 - (2.0 * phase - 1.0).abs(),
        3 => {
            if phase < 0.5 {
                1.0
            } else {
                0.0
            }
        }
        _ => held,
    }
}

/// Square, triangle, saw and sine at phase `t`, all in `-1..1`.
fn oscillators(t: f32, dt: f32) -> [f32; 4] {
    let square =
        if t < 0.5 { 1.0 } else { -1.0 } + poly_blep(t, dt) - poly_blep((t + 0.5) % 1.0, dt);
    let triangle = 1.0 - 4.0 * (t - 0.5).abs();
    let saw = 2.0 * t - 1.0 - poly_blep(t, dt);
    [square, triangle, saw, (TAU * t).sin()]
}

/// Passes `x` unchanged up to 0.8, then bends it smoothly towards 1.
fn soft_limit(x: f32) -> f32 {
    let a = x.abs();
    if a <= 0.8 {
        x
    } else {
        x.signum() * (0.8 + 0.2 * ((a - 0.8) / 0.2).tanh())
    }
}

fn release_seconds(p: f32) -> f32 {
    SHORTEST_RELEASE * (LONGEST_RELEASE / SHORTEST_RELEASE).powf(p)
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[
            pitch,
            range,
            rate,
            sync,
            shape,
            wave,
            release,
            cutoff,
            resonance,
            drive,
        ] = io.params
        else {
            return;
        };
        let &[gate_in, voct, rate_in] = io.inputs else {
            return;
        };
        let sr = io.sample_rate;

        // An unpatched gate is held high.
        let gate = if io.connected[0] { gate_in } else { GATE_HIGH };
        match self.gate.update(gate) {
            Edge::Rose => {
                self.stage = Stage::Held;
                self.lfo = Phase::default();
                self.held = self.rng.unit();
            }
            Edge::Fell if self.stage == Stage::Held => {
                self.stage = Stage::Release;
                self.release_from = self.level;
                self.release_at = 0.0;
            }
            _ => {}
        }
        match self.stage {
            Stage::Idle => self.level = 0.0,
            Stage::Held => self.level = (self.level + 1.0 / (ATTACK_SECONDS * sr)).min(1.0),
            Stage::Release => {
                self.release_at += 1.0 / (release_seconds(release) * sr);
                if self.release_at >= 1.0 {
                    self.stage = Stage::Idle;
                    self.level = 0.0;
                } else {
                    let left = 1.0 - self.release_at;
                    self.level = self.release_from * left * left;
                }
            }
        }
        let dive = if self.stage == Stage::Release {
            self.release_at
        } else {
            0.0
        };

        // The LFO.
        let cv = rate_in / CV_MAX;
        let synced = (sync * SYNC_BEATS.len() as f32).round() as i32;
        let hz = if synced > 0 {
            // The CV walks the note length up and down the Sync positions.
            let at = (synced as f32 + cv * SYNC_BEATS.len() as f32).round();
            let beats = SYNC_BEATS[(at as i32).clamp(1, SYNC_BEATS.len() as i32) as usize - 1];
            io.tempo.max(1.0) / 60.0 / beats
        } else {
            SLOWEST_HZ * (FASTEST_HZ / SLOWEST_HZ).powf((rate + cv).clamp(0.0, 1.0))
        };
        let lfo_dt = (hz / sr).min(0.5);
        let phase = self.lfo.advance(lfo_dt);
        if phase < lfo_dt {
            self.held = self.rng.unit();
        }
        let index = (shape.clamp(0.0, 1.0) * 4.0).round() as usize;
        let lfo = lfo_wave(index, phase, self.held);

        // The oscillator.
        let volts = (pitch - 0.5) * 5.0 + voct + range * MAX_RANGE_OCTAVES * lfo
            - dive * RELEASE_DIVE_OCTAVES;
        let dt = (kerf_dsp::volts_to_hz(volts.clamp(LOWEST_VOLTS, HIGHEST_VOLTS)) / sr).min(0.45);
        let waves = oscillators(self.osc.advance(dt), dt);
        let coeff = if self.settled { self.knob_coeff } else { 1.0 };
        self.settled = true;
        let at = self.wave.process(wave.clamp(0.0, 1.0) * 3.0, coeff);
        let low = (at as usize).min(2);
        let tone = lerp(waves[low], waves[low + 1], at - low as f32) * LEVEL;

        // Drive, then the amplitude, then the filter.
        let drive = drive.clamp(0.0, 1.0);
        let gain = 1.0 + 11.0 * drive;
        let driven = lerp(tone, (gain * tone).tanh() / gain.tanh(), drive);
        let knob = self.cutoff.process(cutoff.clamp(0.0, 1.0), coeff);
        let hz = LOWEST_CUTOFF * (HIGHEST_CUTOFF / LOWEST_CUTOFF).powf(knob);
        let resonance = resonance.clamp(0.0, 1.0);
        let q = 0.6 + resonance * resonance * 9.4;
        let filtered = self.filter.process(driven * self.level, hz, q, sr).low;

        io.outputs[0] = soft_limit(filtered) * AUDIO_VOLTS;
        io.outputs[1] = lfo * CV_MAX;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::{SR, peak, render, rms, tone};

    /// Pitch, range, rate, sync, shape, wave, release, cutoff, resonance, drive.
    type Knobs = [f32; 10];

    const PITCH: usize = 0;
    const RANGE: usize = 1;
    const RATE: usize = 2;
    const SYNC: usize = 3;
    const SHAPE: usize = 4;
    const WAVE: usize = 5;
    const RELEASE: usize = 6;
    const CUTOFF: usize = 7;
    const RESONANCE: usize = 8;
    const DRIVE: usize = 9;

    /// A clean sine dive: down-saw at 0.5 Hz, 2 octaves over 261.6 Hz, filter
    /// open, no drive.
    fn dive_knobs() -> Knobs {
        let mut k = [0.0; 10];
        k[PITCH] = 0.5;
        k[RANGE] = 0.5;
        k[RATE] = rate_knob(0.5);
        k[WAVE] = 1.0;
        k[RELEASE] = release_knob(0.5);
        k[CUTOFF] = 1.0;
        k
    }

    fn rate_knob(hz: f32) -> f32 {
        (hz / SLOWEST_HZ).ln() / (FASTEST_HZ / SLOWEST_HZ).ln()
    }

    fn release_knob(seconds: f32) -> f32 {
        (seconds / SHORTEST_RELEASE).ln() / (LONGEST_RELEASE / SHORTEST_RELEASE).ln()
    }

    fn samples(seconds: f32) -> usize {
        (seconds * SR) as usize
    }

    /// Renders `seconds` of Siren and returns (out, lfo). `gate` gives the
    /// gate volts at a time in seconds, or `None` to leave it unpatched.
    fn run(
        seed: u64,
        knobs: &Knobs,
        seconds: f32,
        gate: Option<&dyn Fn(f32) -> f32>,
        extra: impl Fn(f32) -> (f32, f32),
    ) -> (Vec<f32>, Vec<f32>) {
        let mut siren = State::new(seed, SR);
        let connected = [gate.is_some(), true, true];
        let mut out = render(
            &mut siren,
            knobs,
            &connected,
            2,
            samples(seconds),
            |i, inputs| {
                let t = i as f32 / SR;
                let (voct, rate) = extra(t);
                inputs[0] = gate.map_or(0.0, |g| g(t));
                inputs[1] = voct;
                inputs[2] = rate;
            },
        );
        let lfo = out.pop().unwrap();
        (out.pop().unwrap(), lfo)
    }

    fn held(_: f32) -> f32 {
        GATE_HIGH
    }

    fn plain(knobs: &Knobs, seconds: f32) -> (Vec<f32>, Vec<f32>) {
        run(1, knobs, seconds, Some(&held), |_| (0.0, 0.0))
    }

    /// Rising zero crossings per second between `from` and `to` seconds.
    fn crossings(x: &[f32], from: f32, to: f32) -> f32 {
        let window = &x[samples(from)..samples(to)];
        let n = window
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        n as f32 / (to - from)
    }

    #[test]
    fn a_down_saw_dives_and_an_up_saw_climbs() {
        let mut knobs = dive_knobs();
        let (out, lfo) = plain(&knobs, 2.0);
        assert!(
            lfo[1] > 9.9 * 0.99 && lfo[samples(1.9)] < 1.5,
            "the LFO falls"
        );
        let windows = [(0.05, 0.3), (0.55, 0.8), (1.05, 1.3), (1.55, 1.8)];
        let down: Vec<f32> = windows
            .iter()
            .map(|&(a, b)| crossings(&out, a, b))
            .collect();
        assert!(down.windows(2).all(|w| w[0] > w[1] * 1.2), "{down:?}");
        assert!(down[0] > 2.2 * down[3], "{down:?}");
        // Two octaves above the base at the start, back down to it at the end.
        assert!((down[0] / 261.6 - 3.4).abs() < 0.9, "{down:?}");
        assert!(down[3] < 261.6 * 1.7, "{down:?}");

        knobs[SHAPE] = 0.25;
        let (out, _) = plain(&knobs, 2.0);
        let up: Vec<f32> = windows
            .iter()
            .map(|&(a, b)| crossings(&out, a, b))
            .collect();
        assert!(up.windows(2).all(|w| w[0] < w[1] / 1.2), "{up:?}");
        assert!(up[3] > 2.2 * up[0], "{up:?}");
    }

    /// Samples between LFO restarts (jumps up from the bottom of a down-saw).
    fn lfo_periods(lfo: &[f32]) -> Vec<usize> {
        let wraps: Vec<usize> = (1..lfo.len())
            .filter(|&i| lfo[i] - lfo[i - 1] > 5.0)
            .collect();
        wraps.windows(2).map(|w| w[1] - w[0]).collect()
    }

    /// Within 0.2% (the f32 phase accumulator drifts a little) or 2 samples.
    fn near(period: usize, expected: f32) -> bool {
        (period as f32 - expected).abs() <= (expected * 0.002).max(2.0)
    }

    #[test]
    fn rate_sets_the_lfo_period() {
        let mut knobs = dive_knobs();
        for hz in [1.0f32, 2.0, 6.0] {
            knobs[RATE] = rate_knob(hz);
            let (_, lfo) = plain(&knobs, 3.0);
            let periods = lfo_periods(&lfo);
            assert!(!periods.is_empty());
            for p in periods {
                assert!(near(p, SR / hz), "{hz} Hz gave a period of {p} samples");
            }
        }

        // The rate input moves the knob by a tenth of its travel per volt.
        knobs[RATE] = rate_knob(1.0);
        let (_, lfo) = run(1, &knobs, 3.0, Some(&held), |_| (0.0, 2.0));
        let faster = SLOWEST_HZ * (FASTEST_HZ / SLOWEST_HZ).powf(rate_knob(1.0) + 0.2);
        let periods = lfo_periods(&lfo);
        assert!(!periods.is_empty());
        for p in periods {
            assert!(near(p, SR / faster), "{p} vs {}", SR / faster);
        }
    }

    #[test]
    fn sync_locks_the_lfo_to_the_tempo_and_ignores_rate() {
        let mut knobs = dive_knobs();
        // Position 4 of 7 is one beat: 2 Hz at the rig's 120 BPM.
        knobs[SYNC] = 4.0 / 7.0;
        for rate in [0.1, 0.9] {
            knobs[RATE] = rate;
            let (_, lfo) = plain(&knobs, 3.0);
            let periods = lfo_periods(&lfo);
            assert!(!periods.is_empty());
            for p in periods {
                assert!(near(p, SR / 2.0), "{p}");
            }
        }
        // A volt on the rate input moves the note length one step shorter.
        let (_, lfo) = run(1, &knobs, 3.0, Some(&held), |_| (0.0, 10.0 / 7.0));
        for p in lfo_periods(&lfo) {
            assert!(near(p, SR / 4.0), "{p}");
        }
    }

    #[test]
    fn shape_picks_the_lfo_wave() {
        let mut knobs = dive_knobs();
        knobs[RATE] = rate_knob(2.0);
        // Triangle: rises and falls, spanning the whole range.
        knobs[SHAPE] = 0.5;
        let (_, lfo) = plain(&knobs, 1.0);
        let (lo, hi) = lfo
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
        assert!(lo < 0.1 && hi > 9.9, "{lo} {hi}");
        let rising = lfo.windows(2).filter(|w| w[1] > w[0]).count();
        let falling = lfo.windows(2).filter(|w| w[1] < w[0]).count();
        assert!((rising as f32 / falling as f32 - 1.0).abs() < 0.05);
        // Square: two levels only.
        knobs[SHAPE] = 0.75;
        let (_, lfo) = plain(&knobs, 1.0);
        assert!(lfo.iter().all(|&v| v == 0.0 || v == 10.0));
        assert!(lfo.contains(&0.0) && lfo.contains(&10.0));
        // Sample-and-hold: flat steps at many different levels.
        knobs[SHAPE] = 1.0;
        let (_, lfo) = plain(&knobs, 4.0);
        let mut levels: Vec<u32> = lfo.iter().map(|v| (v * 1000.0) as u32).collect();
        levels.dedup();
        assert!(
            (6..=12).contains(&levels.len()),
            "{} steps in 8 cycles",
            levels.len()
        );
        assert!(lfo.iter().all(|v| (0.0..=10.0).contains(v)));
    }

    #[test]
    fn the_release_tail_decays_to_silence() {
        let mut knobs = dive_knobs();
        knobs[RANGE] = 0.0;
        knobs[RELEASE] = release_knob(0.5);
        let gate = |t: f32| if t < 0.3 { GATE_HIGH } else { 0.0 };
        let (out, _) = run(1, &knobs, 1.6, Some(&gate), |_| (0.0, 0.0));
        let held = peak(&out[samples(0.1)..samples(0.3)]);
        assert!(held > 3.0, "{held}");
        // The gate drops at 0.3 s; the tail ends at 0.8 s.
        let tail: Vec<f32> = [(0.3, 0.4), (0.5, 0.6), (0.7, 0.8)]
            .iter()
            .map(|&(a, b)| peak(&out[samples(a)..samples(b)]))
            .collect();
        assert!(
            tail[0] > 1.5 && tail[0] > tail[1] && tail[1] > tail[2],
            "{tail:?}"
        );
        assert!(tail[2] < 0.5, "{tail:?}");
        assert!(peak(&out[samples(0.9)..]) < 0.01);
        // The pitch keeps falling while it fades.
        let early = crossings(&out, 0.3, 0.4);
        let late = crossings(&out, 0.6, 0.7);
        assert!(late < early * 0.8, "{early} then {late}");

        // A long release is still ringing a second after the gate drops.
        knobs[RELEASE] = release_knob(4.0);
        let (out, _) = run(1, &knobs, 2.0, Some(&gate), |_| (0.0, 0.0));
        assert!(peak(&out[samples(1.2)..samples(1.4)]) > 0.5);
        // A short one is gone almost at once.
        knobs[RELEASE] = 0.0;
        let (out, _) = run(1, &knobs, 1.0, Some(&gate), |_| (0.0, 0.0));
        assert!(peak(&out[samples(0.35)..]) < 0.01);
    }

    #[test]
    fn a_gate_rise_restarts_the_dive() {
        let mut knobs = dive_knobs();
        knobs[RELEASE] = 1.0;
        // Held for a second, a 30 ms gap, then held again.
        let gate = |t: f32| {
            if (1.0..1.03).contains(&t) {
                0.0
            } else {
                GATE_HIGH
            }
        };
        let (out, lfo) = run(1, &knobs, 2.0, Some(&gate), |_| (0.0, 0.0));
        let before = crossings(&out, 0.9, 1.0);
        let after = crossings(&out, 1.05, 1.15);
        assert!(after > before * 1.5, "{before} Hz then {after} Hz");
        // The LFO was well into its fall, and starts again from the top.
        assert!(lfo[samples(1.02)] < 6.0);
        assert!(lfo[samples(1.03) + 2] > 9.8, "{}", lfo[samples(1.03) + 2]);
    }

    #[test]
    fn an_unpatched_gate_sounds_all_the_time() {
        let (out, _) = run(1, &dive_knobs(), 1.0, None, |_| (0.0, 0.0));
        assert!(rms(&out[samples(0.1)..]) > 1.0);
        // Patching a low gate silences it.
        let (out, _) = run(1, &dive_knobs(), 1.0, Some(&|_| 0.0), |_| (0.0, 0.0));
        assert_eq!(peak(&out), 0.0);
    }

    #[test]
    fn gates_do_not_click() {
        let mut knobs = dive_knobs();
        // A sine at the base pitch has the steepest legitimate slope; the
        // amplitude ramps must not add a step on top of it.
        knobs[RANGE] = 0.0;
        knobs[RELEASE] = 0.0;
        let gate = |t: f32| match t {
            t if t < 0.1 => 0.0,
            t if t < 0.3 => GATE_HIGH,
            t if t < 0.4 => 0.0,
            t if t < 0.5 => GATE_HIGH,
            t if t < 0.5005 => 0.0,
            _ => GATE_HIGH,
        };
        let (out, _) = run(1, &knobs, 0.7, Some(&gate), |_| (0.0, 0.0));
        let step = out
            .windows(2)
            .fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
        assert!(step < 0.3, "largest step {step} V");
        assert!(peak(&out) > 3.0);
    }

    #[test]
    fn wave_picks_the_oscillator() {
        let mut knobs = dive_knobs();
        knobs[RANGE] = 0.0;
        let f = 261.625_58;
        // The fundamental and one harmonic of each wave.
        let harmonic = |wave: f32, n: f32| {
            let mut knobs = knobs;
            knobs[WAVE] = wave;
            let (out, _) = plain(&knobs, 0.8);
            let x = &out[samples(0.2)..];
            (tone(x, f), tone(x, n * f))
        };
        let (sine, sine3) = harmonic(1.0, 3.0);
        assert!((sine - 3.5).abs() < 0.3, "{sine}");
        assert!(sine3 < 0.05, "{sine3}");
        let (square, square3) = harmonic(0.0, 3.0);
        assert!(
            (square3 / square - 1.0 / 3.0).abs() < 0.06,
            "{square} {square3}"
        );
        let (saw, saw2) = harmonic(2.0 / 3.0, 2.0);
        assert!((saw2 / saw - 0.5).abs() < 0.08, "{saw} {saw2}");
    }

    #[test]
    fn the_filter_and_drive_shape_the_tone() {
        let mut knobs = dive_knobs();
        knobs[RANGE] = 0.0;
        knobs[WAVE] = 2.0 / 3.0;
        let level = |knobs: &Knobs| rms(&plain(knobs, 0.6).0[samples(0.2)..]);
        let open = level(&knobs);
        knobs[CUTOFF] = 0.0;
        assert!(level(&knobs) < open * 0.2);
        // Resonance on a cutoff at the pitch boosts it.
        knobs[CUTOFF] = (261.6 / LOWEST_CUTOFF).ln() / (HIGHEST_CUTOFF / LOWEST_CUTOFF).ln();
        let plain_q = level(&knobs);
        knobs[RESONANCE] = 0.8;
        assert!(level(&knobs) > plain_q * 1.5);
        // Drive squashes a sine towards a square, raising its RMS.
        let mut knobs = dive_knobs();
        knobs[RANGE] = 0.0;
        let clean = level(&knobs);
        knobs[DRIVE] = 1.0;
        assert!(level(&knobs) > clean * 1.15);
    }

    #[test]
    fn extremes_stay_finite_and_bounded() {
        let mut rng = Rng::new(99);
        let mut pick = || [0.0, 0.5, 1.0][(rng.next_u64() % 3) as usize];
        for case in 0..60 {
            let knobs: Knobs = std::array::from_fn(|_| pick());
            let mut siren = State::new(case, SR);
            let big = if case % 2 == 0 { 100.0 } else { -100.0 };
            let out = render(
                &mut siren,
                &knobs,
                &[true; 3],
                2,
                samples(0.3),
                |i, inputs| {
                    inputs[0] = if (i / 1500) % 2 == 0 { GATE_HIGH } else { 0.0 };
                    inputs[1] = big * (i as f32 / 14_400.0).sin();
                    inputs[2] = big;
                },
            );
            assert!(peak(&out[0]) <= AUDIO_VOLTS, "{knobs:?}");
            assert!(
                out[1].iter().all(|v| (0.0..=CV_MAX).contains(v)),
                "{knobs:?}"
            );
        }
    }

    #[test]
    fn same_seed_same_siren() {
        let mut knobs = dive_knobs();
        knobs[SHAPE] = 1.0;
        let go = |seed| run(seed, &knobs, 1.0, Some(&held), |_| (0.0, 0.0));
        assert_eq!(go(7), go(7));
        assert_ne!(go(7).1, go(8).1);
    }
}
