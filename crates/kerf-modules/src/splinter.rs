use std::f32::consts::{FRAC_PI_4, PI, SQRT_2};
use std::sync::Arc;

use kerf_core::descriptor::{Band, JackKind::*, ModuleDescriptor, jack, param};
use kerf_dsp::{AUDIO_VOLTS, CV_MAX, Edge, Gate, OnePole, Rng, lerp, smoothing};

use crate::{Dsp, Io};

pub static DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    kind: "Splinter",
    band: Band::Voices,
    hp: 16,
    summary: "Granular sampler",
    params: &[
        param(
            "position",
            "Position",
            0.3,
            "Where in the sample the grains start reading, from its start to its end.",
        ),
        param(
            "spray",
            "Spray",
            0.1,
            "Scatters each grain's start around Position. Full spray covers the whole sample.",
        ),
        param(
            "size",
            "Size",
            0.45,
            "Grain length, from 5 ms to 500 ms. Short grains buzz, long ones keep the sample recognisable.",
        ),
        param(
            "density",
            "Density",
            0.6,
            "Grains per second, from 1 to 60. The level is eased down as grains overlap.",
        ),
        param(
            "pitch",
            "Pitch",
            0.5,
            "Transposes the grains, ±24 semitones. Centre is the sample's own pitch.",
        ),
        param(
            "scatter",
            "Scatter",
            0.0,
            "Random pitch change per grain, up to an octave either way.",
        ),
        param(
            "reverse",
            "Reverse",
            0.0,
            "Chance that a grain plays backwards. Full is always.",
        ),
        param(
            "shape",
            "Shape",
            0.25,
            "Grain window: soft bell at the left, flat with short fades in the middle, sharp attack and long fall at the right.",
        ),
        param(
            "spread",
            "Spread",
            0.4,
            "Scatters grains across the stereo field.",
        ),
        param("level", "Level", 0.8, "Output level."),
        param(
            "hold",
            "Hold",
            0.0,
            "Above 0.5 the read point stays where it was, ignoring Position and its input.",
        ),
    ],
    inputs: &[
        jack("voct", "V/Oct", Pitch, "Repitches the grains."),
        jack(
            "position",
            "Position",
            Cv,
            "Moves Position. A slow LFO here scans through the sample.",
        ),
        jack(
            "trig",
            "Trig",
            Gate,
            "A pulse fires a burst of eight grains at the Density rate. With Trig patched and no Gate, grains play only after a pulse.",
        ),
        jack(
            "gate",
            "Gate",
            Gate,
            "Patched, grains stream while it is high and restart on each rising edge. Unpatched, they stream all the time.",
        ),
    ],
    outputs: &[
        jack("l", "L", Audio, "Left."),
        jack("r", "R", Audio, "Right."),
    ],
};

/// Grains that can sound at once; a new grain that finds the pool full is skipped.
const MAX_GRAINS: usize = 48;
/// Grains in the burst a trigger fires.
const BURST: u32 = 8;
/// Grain length range, in seconds, across the Size knob.
const MIN_SIZE: f32 = 0.005;
const MAX_SIZE: f32 = 0.5;
/// Grains per second across the Density knob.
const MIN_DENSITY: f32 = 1.0;
const MAX_DENSITY: f32 = 60.0;
/// Total range of the Pitch knob, in octaves.
const PITCH_RANGE: f32 = 4.0;
/// Pitch scatter at full, in octaves either way.
const SCATTER_OCTAVES: f32 = 1.0;
/// The most any grain is transposed, in octaves, whatever the inputs add up to.
const MAX_OCTAVES: f32 = 6.0;
/// How far the stream's grain spacing wanders, as a fraction of the spacing.
const JITTER: f32 = 0.3;
/// Gain changes settle in about this long, in seconds.
const GAIN_SECONDS: f32 = 0.005;
/// Window table length, and how many Shape settings it is rebuilt for.
const WINDOW_LEN: usize = 512;
const SHAPE_STEPS: f32 = 128.0;

/// Grain length in seconds for the Size knob.
fn size_seconds(knob: f32) -> f32 {
    MIN_SIZE * (MAX_SIZE / MIN_SIZE).powf(knob)
}

/// Grains per second for the Density knob.
fn density_hz(knob: f32) -> f32 {
    MIN_DENSITY * (MAX_DENSITY / MIN_DENSITY).powf(knob)
}

/// Fills `table` with the grain window for `shape`: raised-cosine fades in and out around a flat
/// top. Both fades take half the grain at 0 (a Hann window), a tenth in the middle, and at 1 a
/// hundredth in and nine tenths out.
fn fill_window(table: &mut [f32; WINDOW_LEN + 1], shape: f32) {
    let (fade_in, fade_out) = if shape < 0.5 {
        let f = lerp(0.5, 0.1, shape * 2.0);
        (f, f)
    } else {
        let t = (shape - 0.5) * 2.0;
        (lerp(0.1, 0.01, t), lerp(0.1, 0.9, t))
    };
    for (i, w) in table.iter_mut().enumerate() {
        let t = i as f32 / WINDOW_LEN as f32;
        *w = if t < fade_in {
            0.5 - 0.5 * (PI * t / fade_in).cos()
        } else if t > 1.0 - fade_out {
            0.5 + 0.5 * (PI * (t - (1.0 - fade_out)) / fade_out).cos()
        } else {
            1.0
        };
    }
}

/// The window at `phase` in `0..1` through the grain.
fn window_at(table: &[f32; WINDOW_LEN + 1], phase: f32) -> f32 {
    let x = phase * WINDOW_LEN as f32;
    let i = (x as usize).min(WINDOW_LEN - 1);
    lerp(table[i], table[i + 1], x - i as f32)
}

/// Cubic (Catmull-Rom) read at a fractional index; outside the sample is silence.
fn read(sample: &[f32], at: f64) -> f32 {
    let floor = at.floor();
    let f = (at - floor) as f32;
    let i = floor as isize;
    let p = |o: isize| {
        let k = i + o;
        if k < 0 || k >= sample.len() as isize {
            0.0
        } else {
            sample[k as usize]
        }
    };
    let (y0, y1, y2, y3) = (p(-1), p(0), p(1), p(2));
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    y1 + f * (c1 + f * (c2 + f * c3))
}

#[derive(Clone, Copy)]
struct Grain {
    active: bool,
    /// Read point, in samples (fractional), and its change per output sample.
    pos: f64,
    step: f64,
    /// Output samples played, and in all.
    age: u32,
    len: u32,
    left: f32,
    right: f32,
}

impl Grain {
    const IDLE: Self = Self {
        active: false,
        pos: 0.0,
        step: 0.0,
        age: 0,
        len: 1,
        left: 0.0,
        right: 0.0,
    };
}

/// What a new grain is made from, worked out once per sample from the knobs and jacks.
struct Cloud {
    /// Read point, 0..1.
    read: f32,
    spray: f32,
    /// Grain length in samples.
    len: f32,
    /// Knob and V/Oct transposition together, in octaves.
    octaves: f32,
    scatter: f32,
    reverse: f32,
    spread: f32,
}

pub struct State {
    /// Mono, at the engine's sample rate.
    sample: Arc<[f32]>,
    rng: Rng,
    grains: [Grain; MAX_GRAINS],
    window: [f32; WINDOW_LEN + 1],
    /// Shape step the window was built for.
    shape_step: u32,
    trig: Gate,
    gate: Gate,
    /// Samples until the stream's next grain, and until the burst's.
    next: f32,
    burst_next: f32,
    burst_left: u32,
    /// The read point Hold keeps.
    frozen: Option<f32>,
    gain: OnePole,
    gain_primed: bool,
}

impl State {
    pub fn new(seed: u64, sample: Arc<[f32]>) -> Self {
        Self {
            sample,
            rng: Rng::new(seed),
            grains: [Grain::IDLE; MAX_GRAINS],
            window: [0.0; WINDOW_LEN + 1],
            shape_step: u32::MAX,
            trig: Gate::default(),
            gate: Gate::default(),
            next: 0.0,
            burst_next: 0.0,
            burst_left: 0,
            frozen: None,
            gain: OnePole::default(),
            gain_primed: false,
        }
    }

    /// Starts one grain. The random draws are the same however the knobs are set, so turning
    /// Spray up does not reshuffle the pitches.
    fn spawn(&mut self, cloud: &Cloud) {
        let draws = [
            self.rng.unit(),
            self.rng.unit(),
            self.rng.unit(),
            self.rng.unit(),
        ];
        let Some(grain) = self.grains.iter_mut().find(|g| !g.active) else {
            return;
        };
        let bipolar = |u: f32| u * 2.0 - 1.0;
        let at = (cloud.read + bipolar(draws[0]) * 0.5 * cloud.spray).clamp(0.0, 1.0);
        let octaves = (cloud.octaves + bipolar(draws[1]) * cloud.scatter * SCATTER_OCTAVES)
            .clamp(-MAX_OCTAVES, MAX_OCTAVES);
        let pan = bipolar(draws[3]) * cloud.spread;
        let angle = (pan + 1.0) * FRAC_PI_4;
        let len = (cloud.len.round() as u32).max(1);
        let step = f64::from(octaves).exp2();
        let start = f64::from(at) * (self.sample.len() - 1) as f64;
        // A backwards grain covers the stretch a forward one would, from its far end.
        let (pos, step) = if draws[2] < cloud.reverse {
            (start + step * f64::from(len), -step)
        } else {
            (start, step)
        };
        *grain = Grain {
            active: true,
            pos,
            step,
            age: 0,
            len,
            left: angle.cos() * SQRT_2,
            right: angle.sin() * SQRT_2,
        };
    }
}

impl Dsp for State {
    fn process(&mut self, io: &mut Io<'_>) {
        let &[
            position,
            spray,
            size,
            density,
            pitch,
            scatter,
            reverse,
            shape,
            spread,
            level,
            hold,
        ] = io.params
        else {
            return;
        };
        let &[voct, position_cv, trig, gate] = io.inputs else {
            return;
        };
        let sr = io.sample_rate;
        let trig_edge = self.trig.update(trig);
        let gate_edge = self.gate.update(gate);
        if self.sample.is_empty() {
            io.outputs[..2].fill(0.0);
            return;
        }

        let live = (position + position_cv / CV_MAX).clamp(0.0, 1.0);
        let read_point = if hold > 0.5 {
            *self.frozen.get_or_insert(live)
        } else {
            self.frozen = None;
            live
        };

        let shape_step = (shape.clamp(0.0, 1.0) * SHAPE_STEPS).round() as u32;
        if shape_step != self.shape_step {
            self.shape_step = shape_step;
            fill_window(&mut self.window, shape_step as f32 / SHAPE_STEPS);
        }

        let size_s = size_seconds(size.clamp(0.0, 1.0));
        let rate = density_hz(density.clamp(0.0, 1.0));
        let cloud = Cloud {
            read: read_point,
            spray: spray.clamp(0.0, 1.0),
            len: size_s * sr,
            octaves: (pitch - 0.5) * PITCH_RANGE + voct,
            scatter: scatter.clamp(0.0, 1.0),
            reverse: reverse.clamp(0.0, 1.0),
            spread: spread.clamp(0.0, 1.0),
        };
        let interval = sr / rate;

        let (trig_patched, gate_patched) = (io.connected[2], io.connected[3]);
        let streaming = if gate_patched {
            self.gate.is_high()
        } else {
            !trig_patched
        };
        if gate_edge == Edge::Rose {
            self.next = 0.0;
        }
        if trig_edge == Edge::Rose {
            self.burst_left = BURST;
            self.burst_next = 0.0;
        }
        if streaming {
            self.next -= 1.0;
            if self.next <= 0.0 {
                self.spawn(&cloud);
                self.next += interval * (1.0 + (self.rng.unit() - 0.5) * JITTER);
            }
        }
        if self.burst_left > 0 {
            self.burst_next -= 1.0;
            if self.burst_next <= 0.0 {
                self.spawn(&cloud);
                self.burst_left -= 1;
                self.burst_next += interval;
            }
        }

        let (mut l, mut r) = (0.0, 0.0);
        let len = self.sample.len() as f64;
        for g in self.grains.iter_mut().filter(|g| g.active) {
            let w = window_at(&self.window, g.age as f32 / g.len as f32);
            let y = read(&self.sample, g.pos) * w;
            l += y * g.left;
            r += y * g.right;
            g.pos += g.step;
            g.age += 1;
            let gone = if g.step > 0.0 {
                g.pos > len
            } else {
                g.pos < -1.0
            };
            g.active = g.age < g.len && !gone;
        }

        // Grains that overlap add as uncorrelated sounds, so ease off by the square root of how
        // many overlap on average.
        let overlap = (rate * size_s).max(1.0);
        let target = level.clamp(0.0, 1.0) / overlap.sqrt();
        let coeff = if self.gain_primed {
            smoothing(GAIN_SECONDS, sr)
        } else {
            self.gain_primed = true;
            1.0
        };
        let gain = self.gain.process(target, coeff) * AUDIO_VOLTS;
        io.outputs[0] = (l * gain).clamp(-AUDIO_VOLTS, AUDIO_VOLTS);
        io.outputs[1] = (r * gain).clamp(-AUDIO_VOLTS, AUDIO_VOLTS);
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;
    use std::sync::Arc;

    use kerf_dsp::Rng;

    use super::{MAX_DENSITY, MAX_SIZE, MIN_DENSITY, MIN_SIZE, State};
    use crate::rig::{SR, peak, render, rms, tone};

    const POSITION: usize = 0;
    const SPRAY: usize = 1;
    const SIZE: usize = 2;
    const DENSITY: usize = 3;
    const PITCH: usize = 4;
    const SCATTER: usize = 5;
    const REVERSE: usize = 6;
    const SHAPE: usize = 7;
    const SPREAD: usize = 8;
    const HOLD: usize = 10;

    /// Sparse, flat-topped, mono grains at full level, one a second: easy to measure. Overrides
    /// are `(param, value)`.
    fn params(set: &[(usize, f32)]) -> [f32; 11] {
        let mut p = [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.5, 0.0, 1.0, 0.0];
        for &(i, v) in set {
            p[i] = v;
        }
        p
    }

    fn size_knob(seconds: f32) -> f32 {
        (seconds / MIN_SIZE).ln() / (MAX_SIZE / MIN_SIZE).ln()
    }

    fn density_knob(hz: f32) -> f32 {
        (hz / MIN_DENSITY).ln() / (MAX_DENSITY / MIN_DENSITY).ln()
    }

    fn dc(n: usize) -> Arc<[f32]> {
        vec![1.0; n].into()
    }

    fn ramp(n: usize) -> Arc<[f32]> {
        (0..n).map(|i| i as f32 / n as f32).collect()
    }

    fn sine(n: usize, hz: f32) -> Arc<[f32]> {
        (0..n).map(|i| (TAU * hz * i as f32 / SR).sin()).collect()
    }

    fn noise(n: usize) -> Arc<[f32]> {
        let mut rng = Rng::new(7);
        (0..n).map(|_| rng.unit() * 1.8 - 0.9).collect()
    }

    /// Free-running grains (neither trig nor gate patched), no CV.
    fn run(sample: &Arc<[f32]>, seed: u64, p: [f32; 11], frames: usize) -> Vec<Vec<f32>> {
        let mut splinter = State::new(seed, sample.clone());
        render(&mut splinter, &p, &[false; 4], 2, frames, |_, _| {})
    }

    /// Times the output leaves exact silence.
    fn onsets(x: &[f32]) -> usize {
        usize::from(x.first().is_some_and(|&v| v != 0.0))
            + x.windows(2).filter(|w| w[0] == 0.0 && w[1] != 0.0).count()
    }

    #[test]
    fn grain_count_follows_density() {
        let seconds = 4.0;
        let frames = (seconds * SR) as usize;
        for hz in [3.0, 10.0, 30.0] {
            let out = run(
                &dc(48_000),
                1,
                params(&[(DENSITY, density_knob(hz))]),
                frames,
            );
            let n = onsets(&out[0]) as f32;
            let expected = hz * seconds;
            assert!(
                (n - expected).abs() <= 0.06 * expected + 1.0,
                "{n} grains at {hz} per second, expected about {expected}"
            );
        }
    }

    #[test]
    fn size_sets_the_grain_length() {
        for seconds in [0.005, 0.05, 0.2] {
            let out = run(
                &dc(48_000),
                1,
                params(&[(SIZE, size_knob(seconds))]),
                30_000,
            );
            let x = &out[0];
            let first = x.iter().position(|&v| v != 0.0).unwrap();
            let run = x[first..].iter().take_while(|&&v| v != 0.0).count();
            let expected = (seconds * SR).round() as usize;
            // The window is zero on the first sample, so the sound is one shorter.
            assert!(
                run.abs_diff(expected - 1) <= 2,
                "{run} samples for {seconds} s, expected {expected}"
            );
        }
    }

    #[test]
    fn pitch_knob_and_v_oct_change_the_read_speed() {
        // A 1 kHz sine, read from its start by one flat-topped 100 ms grain.
        let sample = sine(48_000, 1000.0);
        let base = [(POSITION, 0.0), (SIZE, size_knob(0.1))];
        let heard = |extra: &[(usize, f32)]| {
            let mut set = base.to_vec();
            set.extend_from_slice(extra);
            run(&sample, 1, params(&set), 4_800)[0][1000..4000].to_vec()
        };
        let plain = heard(&[]);
        assert!(tone(&plain, 1000.0) > 4.0 && tone(&plain, 2000.0) < 0.5);
        let up = heard(&[(PITCH, 0.75)]);
        assert!(
            tone(&up, 2000.0) > 4.0 && tone(&up, 1000.0) < 0.5,
            "+12 doubles"
        );
        let down = heard(&[(PITCH, 0.25)]);
        assert!(
            tone(&down, 500.0) > 4.0 && tone(&down, 1000.0) < 0.5,
            "-12 halves"
        );

        let mut splinter = State::new(1, sample.clone());
        let by_cv = render(
            &mut splinter,
            &params(&base),
            &[true, false, false, false],
            2,
            4_800,
            |_, v| v[0] = 1.0,
        );
        assert_eq!(by_cv[0][1000..4000], up[..], "1 V is an octave");
    }

    #[test]
    fn position_picks_the_region_and_spray_scatters_it() {
        // On a ramp the sound is as loud as the read point is far along.
        let sample = ramp(48_000);
        let loud = |set: &[(usize, f32)]| {
            let mut all = vec![(DENSITY, density_knob(10.0))];
            all.extend_from_slice(set);
            let out = run(&sample, 1, params(&all), 96_000);
            out[0].iter().map(|v| v.abs()).sum::<f32>() / out[0].len() as f32
        };
        let early = loud(&[(POSITION, 0.2)]);
        let late = loud(&[(POSITION, 0.8)]);
        assert!(
            (3.8..4.2).contains(&(late / early)),
            "late {late} early {early}"
        );

        let mut splinter = State::new(1, sample.clone());
        let p = params(&[(POSITION, 0.0), (DENSITY, density_knob(10.0))]);
        let cv = render(
            &mut splinter,
            &p,
            &[false, true, false, false],
            2,
            96_000,
            |_, v| v[1] = 8.0,
        );
        let by_cv = cv[0].iter().map(|v| v.abs()).sum::<f32>() / cv[0].len() as f32;
        assert!((by_cv / late - 1.0).abs() < 0.01, "cv {by_cv} knob {late}");

        let tight = run(&sample, 1, params(&[(DENSITY, density_knob(10.0))]), 96_000);
        let sprayed = run(
            &sample,
            1,
            params(&[(DENSITY, density_knob(10.0)), (SPRAY, 0.4)]),
            96_000,
        );
        assert!(peak(&tight[0]) < 2.7, "peak {}", peak(&tight[0]));
        assert!(peak(&sprayed[0]) > 3.1, "peak {}", peak(&sprayed[0]));
    }

    #[test]
    fn hold_keeps_the_read_point() {
        let sample = ramp(48_000);
        let sweep = |hold: f32, cv: bool| {
            let mut splinter = State::new(1, sample.clone());
            let p = params(&[(HOLD, hold), (DENSITY, density_knob(10.0))]);
            render(
                &mut splinter,
                &p,
                &[false, true, false, false],
                2,
                96_000,
                |i, v| v[1] = if cv { 10.0 * i as f32 / 96_000.0 } else { 0.0 },
            )
        };
        assert_eq!(sweep(1.0, true), sweep(1.0, false));
        assert_ne!(sweep(0.0, true), sweep(0.0, false));
    }

    #[test]
    fn shape_runs_from_a_bell_to_a_sharp_attack() {
        let size = size_knob(0.1);
        let peak_at = |shape: f32| {
            let out = run(
                &dc(48_000),
                1,
                params(&[(SIZE, size), (SHAPE, shape)]),
                4_800,
            );
            let x = &out[0];
            let top = peak(x);
            let first = x.iter().position(|&v| v > top * 0.99).unwrap();
            (first, rms(x))
        };
        let (bell_top, bell_rms) = peak_at(0.0);
        let (_, flat_rms) = peak_at(0.5);
        let (sharp_top, _) = peak_at(1.0);
        assert!((2200..2600).contains(&bell_top), "bell tops at {bell_top}");
        assert!(flat_rms > bell_rms * 1.2, "flat {flat_rms} bell {bell_rms}");
        assert!(sharp_top < 800, "sharp tops at {sharp_top}");
    }

    #[test]
    fn reverse_chance_plays_grains_backwards() {
        // On a ramp a forward grain climbs and a backward one falls.
        let sample = ramp(48_000);
        let at = |reverse: f32| {
            let out = run(
                &sample,
                1,
                params(&[(SIZE, size_knob(0.1)), (REVERSE, reverse)]),
                4_800,
            );
            (out[0][1440], out[0][3360])
        };
        let (a, b) = at(0.0);
        assert!(b > a, "forward {a} then {b}");
        let (a, b) = at(1.0);
        assert!(a > b, "backward {a} then {b}");
    }

    #[test]
    fn the_same_seed_gives_the_same_cloud() {
        let sample = noise(48_000);
        let p = params(&[
            (SPRAY, 0.5),
            (SIZE, 0.4),
            (DENSITY, 0.8),
            (SCATTER, 0.5),
            (REVERSE, 0.5),
            (SPREAD, 1.0),
        ]);
        let a = run(&sample, 5, p, 48_000);
        assert_eq!(a, run(&sample, 5, p, 48_000));
        assert_ne!(a, run(&sample, 6, p, 48_000));
        assert_ne!(a[0], a[1], "spread puts grains in different places");
    }

    #[test]
    fn mono_when_spread_is_zero() {
        let out = run(
            &noise(48_000),
            1,
            params(&[(SIZE, 0.4), (DENSITY, 0.8)]),
            24_000,
        );
        assert_eq!(out[0], out[1]);
    }

    #[test]
    fn silent_with_nothing_to_play_or_nothing_asking() {
        let nothing: Arc<[f32]> = Vec::new().into();
        let out = run(&nothing, 1, params(&[(DENSITY, 1.0)]), 4_800);
        assert_eq!(peak(&out[0]) + peak(&out[1]), 0.0);

        // Trig patched and never pulsed; gate patched and low.
        for connected in [[false, false, true, false], [false, false, false, true]] {
            let mut splinter = State::new(1, dc(48_000));
            let out = render(
                &mut splinter,
                &params(&[(DENSITY, 1.0)]),
                &connected,
                2,
                24_000,
                |_, _| {},
            );
            assert_eq!(peak(&out[0]), 0.0, "{connected:?}");
        }
    }

    #[test]
    fn a_trigger_fires_a_burst_of_eight() {
        let mut splinter = State::new(1, dc(48_000));
        let p = params(&[(DENSITY, density_knob(10.0))]);
        let out = render(
            &mut splinter,
            &p,
            &[false, false, true, false],
            2,
            60_000,
            |i, v| v[2] = if (1000..1100).contains(&i) { 10.0 } else { 0.0 },
        );
        let x = &out[0];
        assert_eq!(peak(&x[..1000]), 0.0);
        assert_eq!(onsets(x), 8);
    }

    #[test]
    fn a_gate_runs_the_stream_and_restarts_it() {
        let mut splinter = State::new(1, dc(48_000));
        let p = params(&[(DENSITY, density_knob(10.0))]);
        let out = render(
            &mut splinter,
            &p,
            &[false, false, false, true],
            2,
            140_000,
            |i, v| {
                v[3] = if (20_000..70_000).contains(&i) {
                    10.0
                } else {
                    0.0
                }
            },
        );
        let x = &out[0];
        assert_eq!(peak(&x[..20_000]), 0.0);
        assert!(x[20_001] != 0.0, "a grain starts on the rising edge");
        assert!(onsets(&x[20_000..70_300]) >= 9);
        assert_eq!(peak(&x[70_300..]), 0.0);
    }

    #[test]
    fn level_stays_steady_as_grains_pile_up() {
        let sample = noise(96_000);
        let loud = |density: f32, size: f32| {
            let p = params(&[
                (SPRAY, 1.0),
                (DENSITY, density_knob(density)),
                (SIZE, size_knob(size)),
                (SHAPE, 0.0),
            ]);
            rms(&run(&sample, 1, p, 96_000)[0][48_000..])
        };
        let few = loud(20.0, 0.1);
        let many = loud(60.0, 0.45);
        assert!((0.6..1.5).contains(&(many / few)), "few {few} many {many}");
        assert!(peak(&run(&sample, 1, params(&[(DENSITY, 1.0), (SIZE, 1.0)]), 96_000)[0]) <= 5.0);
    }

    #[test]
    fn extremes_stay_finite_and_bounded() {
        let samples = [dc(1), sine(2, 1.0), noise(5_000), dc(48_000)];
        let mut rng = Rng::new(3);
        for sample in &samples {
            for round in 0..24 {
                let mut p = [0.0; 11];
                for v in &mut p {
                    *v = if rng.unit() < 0.5 { 0.0 } else { 1.0 };
                }
                p[HOLD] = if round % 3 == 0 { 1.0 } else { 0.0 };
                let mut splinter = State::new(round, sample.clone());
                let out = render(&mut splinter, &p, &[true; 4], 2, 12_000, |i, v| {
                    v[0] = [-60.0, 60.0, 0.0][round as usize % 3];
                    v[1] = [-100.0, 100.0][i / 3000 % 2];
                    v[2] = if i % 700 < 100 { 10.0 } else { 0.0 };
                    v[3] = if i % 5000 < 3000 { 10.0 } else { 0.0 };
                });
                assert!(
                    peak(&out[0]).max(peak(&out[1])) <= 5.0,
                    "round {round} on {} samples",
                    sample.len()
                );
            }
        }
    }
}
